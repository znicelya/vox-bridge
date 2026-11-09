//! Physical retirement, exact page acknowledgments and terminal-only journal GC.
use super::*;
use crate::intelligence::jobs::{JobReapEvidence, JobReapPort};
use crate::intelligence::types::JobStatus;
use journal::{Certificate, CompletionKind};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Consumption {
    version: u32,
    operation: Uuid,
    header_sha256: String,
    reap: JobReapEvidence,
}
fn payload_hash(tx: &Transaction<'_>, operation: Uuid) -> AppResult<String> {
    let mut q = tx.prepare("SELECT payload_json FROM intelligence_jobs WHERE id=?1")?;
    let mut rows = q.query([operation.to_string()])?;
    let row = rows.next()?.ok_or_else(repair)?;
    // The generic scheduler proof uses raw SHA256. Existing knowledge headers
    // keep their domain/parser-framed DocumentJobProof hash unchanged.
    Ok(super::super::seal::digest(
        state::text(row, 0, 64 * 1024).map_err(|_| repair())?,
    ))
}
pub(super) fn validate_consumption(
    tx: &Transaction<'_>,
    value: &Certificate,
    status: JobStatus,
) -> AppResult<()> {
    let receipt: Option<Consumption> = read(tx, &key("consumed", value.header.input.operation_id))?;
    if value.consumed != receipt.is_some() {
        return Err(repair());
    }
    if let Some(receipt) = receipt {
        if receipt.version != 1
            || receipt.operation != value.header.input.operation_id
            || receipt.header_sha256 != hash(&value.header)?
            || !value.acknowledged
            || !receipt.reap.matches(
                receipt.operation,
                &payload_hash(tx, receipt.operation)?,
                status,
            )
        {
            return Err(repair());
        }
    }
    Ok(())
}
fn consume_proved(
    tx: &Transaction<'_>,
    value: &mut Certificate,
    status: JobStatus,
    reap: &JobReapPort,
) -> AppResult<()> {
    if value.consumed || !value.acknowledged {
        return Ok(());
    }
    let Some(proof) = reap.prove(tx, value.header.input.operation_id)? else {
        return Ok(());
    };
    let evidence = proof.evidence();
    if !evidence.matches(
        value.header.input.operation_id,
        &payload_hash(tx, value.header.input.operation_id)?,
        status,
    ) {
        return Err(repair());
    }
    let receipt = Consumption {
        version: 1,
        operation: value.header.input.operation_id,
        header_sha256: hash(&value.header)?,
        reap: evidence,
    };
    insert(tx, &key("consumed", receipt.operation), &receipt)?;
    value.consumed = true;
    update(tx, &key("operation", receipt.operation), value)?;
    Ok(())
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Removed {
    version: u32,
    publication: Uuid,
    authority_sha256: String,
    publication_root: String,
    source: Generation,
}
fn removed(publication: &journal::Publication) -> AppResult<Removed> {
    Ok(Removed {
        version: 1,
        publication: publication.authority.id,
        authority_sha256: hash(&publication.authority)?,
        publication_root: publication.root.clone(),
        source: publication
            .authority
            .source_generation
            .clone()
            .ok_or_else(repair)?,
    })
}
pub(super) fn validate_removal(
    tx: &Transaction<'_>,
    publication: &journal::Publication,
) -> AppResult<()> {
    let receipt: Option<Removed> = read(tx, &key("removed", publication.authority.id))?;
    if publication.authority.source.is_none() {
        if !publication.source_removed || receipt.is_some() {
            return Err(repair());
        }
        return Ok(());
    }
    if publication.source_removed != receipt.is_some() {
        return Err(repair());
    }
    match receipt {
        Some(receipt) if receipt != removed(publication)? => Err(repair()),
        None if publication.source_removed => Err(repair()),
        _ => Ok(()),
    }
}

fn peer_acknowledged(
    tx: &Transaction<'_>,
    certificate: &Certificate,
    page: u32,
) -> AppResult<bool> {
    let publication = journal::verified(tx, certificate.header.publication)?;
    peer_in(&publication, certificate, page)
}
fn peer_in(
    publication: &journal::VerifiedPublication<'_, '_>,
    certificate: &Certificate,
    page: u32,
) -> AppResult<bool> {
    let tx = publication.transaction();
    let header = &certificate.header;
    let expected = journal::work_page(tx, certificate, page)?;
    for ordinal in 0..publication.value.count {
        let peer = publication.load_at(ordinal)?;
        if peer.header.input.operation_id == header.input.operation_id {
            continue;
        }
        if peer.header.input.document == header.input.document
            && peer.header.input.identity == header.input.identity
            && peer.header.input.generation == header.input.generation
            && peer.header.count == header.count
            && peer.header.shared_root == header.shared_root
            && journal::work_page(tx, &peer, page)?.acknowledged
            && journal::work_page(tx, &peer, page)?.semantic_sha256 == expected.semantic_sha256
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn validate_mappings(tx: &Transaction<'_>, certificate: &Certificate) -> AppResult<()> {
    for page in 0..certificate.header.pages {
        if journal::work_page(tx, certificate, page)?.acknowledged {
            continue;
        }
        let mut peer = None;
        for row in delete_work::certified_rows(tx, &certificate.header.input, page)? {
            if row.mapping(tx)? {
                continue;
            }
            if row.conflicting_mapping(tx)? {
                return Err(repair());
            }
            let proved = match peer {
                Some(value) => value,
                None => {
                    let value = peer_acknowledged(tx, certificate, page)?;
                    peer = Some(value);
                    value
                }
            };
            if !proved {
                return Err(repair());
            }
        }
    }
    Ok(())
}
fn source_certificates(tx: &Transaction<'_>, source: &Generation, sql: bool) -> AppResult<()> {
    let key = source.key()?;
    let mut after = String::new();
    loop {
        let ids = journal::scan_publications(tx, &after)?;
        if ids.is_empty() {
            return Ok(());
        }
        for id in ids {
            after = id.to_string();
            let publication = journal::verified(tx, id)?;
            if publication.value.authority.identity.as_deref() != Some(&key)
                || publication.value.authority.source != Some(source.generation)
            {
                continue;
            }
            for batch in 0..publication.value.batches {
                let batch = publication.batch(batch)?;
                for index in 0..batch.len() {
                    let certificate = batch.load(index)?;
                    validate_mappings(tx, &certificate)?;
                    if sql && (!certificate.acknowledged || !publication.value.source_removed) {
                        return Err(repair());
                    }
                }
            }
        }
    }
}
// The token contains no cohort collection. Its constructor has streamed all
// same-source immutable authorities in this transaction, and each later use
// rechecks the exact header/page binding and current mutable flags.
pub(super) struct SourceView<'tx, 'conn> {
    tx: &'tx Transaction<'conn>,
    source: &'tx Generation,
}
impl<'tx, 'conn> SourceView<'tx, 'conn> {
    fn new(tx: &'tx Transaction<'conn>, source: &'tx Generation, sql: bool) -> AppResult<Self> {
        source_certificates(tx, source, sql)?;
        Ok(Self { tx, source })
    }
    pub(super) fn transaction(&self) -> &Transaction<'conn> {
        self.tx
    }
    pub(super) fn source(&self) -> &Generation {
        self.source
    }
    fn certificate(&self, operation: Uuid) -> AppResult<Option<Certificate>> {
        journal::load_in_source(self, operation)
    }
}
pub(crate) fn guard_physical(tx: &Transaction<'_>, source: &Generation) -> AppResult<()> {
    guard_source(tx, source, false)
}
fn guard_source(tx: &Transaction<'_>, source: &Generation, sql: bool) -> AppResult<()> {
    let view = SourceView::new(tx, source, sql)?;
    let mut cursor = String::new();
    loop {
        let ids = journal::scan_operations(tx, &cursor)?;
        if ids.is_empty() {
            break;
        }
        for id in ids {
            cursor = id.to_string();
            let value = journal::stored(tx, id)?.ok_or_else(repair)?;
            if value.header.input.identity.as_deref() == Some(source.key()?.as_str())
                && value.header.input.generation == Some(source.generation)
            {
                let value = view.certificate(id)?.ok_or_else(repair)?;
                validate_mappings(tx, &value)?;
            }
        }
    }
    guard_retained_work(&view)?;
    let identity = source.key()?;
    let scope = publication::Scope::Source(&identity, source.generation);
    publication::orphan_work(tx, &scope)?;
    let mut after = String::new();
    loop {
        let ids = publication::job_ids(tx, &after)?;
        if ids.is_empty() {
            break;
        }
        for id in ids {
            after = id.to_string();
            if let Some(value) = journal::stored(tx, id)? {
                if value.header.input.identity.as_deref() == Some(&identity)
                    && value.header.input.generation == Some(source.generation)
                {
                    view.certificate(id)?.ok_or_else(repair)?;
                    continue;
                }
            }
            if publication::relevant(tx, id, &scope)?.is_some() {
                return Err(repair());
            }
        }
    }
    let uncompleted: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM documents d JOIN knowledge_index_generations g \
        ON g.identity=d.index_identity WHERE d.index_identity=?1 AND d.status='deleting' AND g.active_generation=?2)",
        params![identity, source.generation.to_string()], |r| r.get(0))?;
    if uncompleted {
        return Err(repair());
    }
    Ok(())
}
fn guard_retained_work(view: &SourceView<'_, '_>) -> AppResult<()> {
    let tx = view.transaction();
    let source = view.source();
    let identity = source.key()?;
    let writer = {
        let mut q = tx.prepare("SELECT intent_json FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2 AND dirty=1")?;
        let mut rows = q.query(params![identity, source.generation.to_string()])?;
        match rows.next()? {
            None => None,
            Some(r) => {
                let intent = super::super::seal::Intent::decode(
                    state::text(r, 0, 16384).map_err(|_| repair())?,
                )?;
                if intent.descriptor != *source {
                    return Err(repair());
                }
                Some(intent.operation_id)
            }
        }
    };
    let mut after = String::new();
    loop {
        let ids = {
            let mut q = tx.prepare(
                "SELECT DISTINCT operation_id FROM knowledge_index_work_items WHERE identity=?1 \
                AND generation=?2 AND operation_id>?3 ORDER BY operation_id LIMIT 200",
            )?;
            let mut rows = q.query(params![identity, source.generation.to_string(), after])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                out.push(state::text(r, 0, 36).map_err(|_| repair())?.to_owned())
            }
            out
        };
        if ids.is_empty() {
            return Ok(());
        }
        for raw in ids {
            after = raw.clone();
            let id = Uuid::parse_str(&raw).map_err(|_| repair())?;
            if id.to_string() != raw {
                return Err(repair());
            }
            if writer == Some(id) {
                continue;
            }
            let job = job_proof::delete(tx, id)?.ok_or_else(repair)?;
            if job.input.identity.as_deref() != Some(&identity)
                || job.input.generation != Some(source.generation)
                || view.certificate(id)?.is_none()
            {
                return Err(repair());
            }
        }
    }
}
pub(crate) fn validate_complete(tx: &Transaction<'_>, publication_id: Uuid) -> AppResult<()> {
    let publication = journal::verified(tx, publication_id)?;
    for batch in 0..publication.value.batches {
        let batch = publication.batch(batch)?;
        for index in 0..batch.len() {
            validate_mappings(tx, &batch.load(index)?)?;
        }
    }
    Ok(())
}
pub(crate) fn guard_sql(tx: &Transaction<'_>, source: &Generation) -> AppResult<()> {
    let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_index_generations WHERE identity=?1 AND active_generation=?2)",
        params![source.key()?, source.generation.to_string()], |r| r.get(0))?;
    if active {
        return Err(repair());
    }
    guard_source(tx, source, true)
}
pub(crate) fn retire_writer_work(tx: &Transaction<'_>, source: &Generation) -> AppResult<usize> {
    guard_sql(tx, source)?;
    let identity = source.key()?;
    let generation = source.generation.to_string();
    let writer = {
        let mut q = tx.prepare(
            "SELECT intent_json FROM knowledge_index_generation_seals \
            WHERE identity=?1 AND generation=?2 AND dirty=1",
        )?;
        let mut rows = q.query(params![identity, generation])?;
        let Some(r) = rows.next()? else { return Ok(0) };
        let intent =
            super::super::seal::Intent::decode(state::text(r, 0, 16384).map_err(|_| repair())?)?;
        if intent.descriptor != *source {
            return Err(repair());
        }
        intent.operation_id
    };
    let ordinals = {
        let mut q = tx.prepare(
            "SELECT ordinal FROM knowledge_index_work_items \
            WHERE operation_id=?1 AND identity=?2 AND generation=?3 \
            AND (document_id IS NULL OR location_sha256 IS NOT NULL) ORDER BY ordinal LIMIT 200",
        )?;
        let rows = q.query_map(params![writer.to_string(), identity, generation], |r| {
            r.get::<_, u32>(0)
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for ordinal in &ordinals {
        if tx.execute("DELETE FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal=?2 \
            AND identity=?3 AND generation=?4 AND (document_id IS NULL OR location_sha256 IS NOT NULL)",
            params![writer.to_string(), ordinal, identity, generation])? != 1 { return Err(repair()) }
    }
    Ok(ordinals.len())
}
pub(in crate::knowledge::index) fn mark_source_removed(
    tx: &Transaction<'_>,
    receipt: &super::super::owner::RemovalReceipt,
) -> AppResult<()> {
    let source = receipt.source();
    guard_physical(tx, source)?;
    let identity = source.key()?;
    let mut after = String::new();
    loop {
        let ids = journal::scan_publications(tx, &after)?;
        if ids.is_empty() {
            return Ok(());
        }
        for id in ids {
            after = id.to_string();
            let mut publication = journal::publication(tx, id)?;
            if publication.authority.identity.as_deref() == Some(&identity)
                && publication.authority.source == Some(source.generation)
            {
                let expected = removed(&publication)?;
                if expected.source != *source {
                    return Err(repair());
                }
                match read::<Removed>(tx, &key("removed", id))? {
                    Some(existing) if existing != expected => return Err(repair()),
                    Some(_) => {}
                    None => insert(tx, &key("removed", id), &expected)?,
                }
                publication.source_removed = true;
                update(tx, &key("publication", id), &publication)?;
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn acknowledge(tx: &Transaction<'_>, operation: Uuid, page: u32) -> AppResult<()> {
    let certificate = journal::stored(tx, operation)?.ok_or_else(repair)?;
    let publication = journal::verified(tx, certificate.header.publication)?;
    acknowledge_in(&publication, certificate.header.ordinal, operation, page)
}
pub(crate) fn acknowledge_batch(
    tx: &Transaction<'_>,
    publication_id: Uuid,
    batch: u64,
) -> AppResult<()> {
    let publication = journal::verified(tx, publication_id)?;
    if batch >= publication.value.batches {
        return Err(repair());
    }
    let start = batch.checked_mul(ANCHOR_PAGE).ok_or_else(repair)?;
    for ordinal in start
        ..start
            .checked_add(ANCHOR_PAGE)
            .ok_or_else(repair)?
            .min(publication.value.count)
    {
        let certificate = publication.load_at(ordinal)?;
        for page in 0..certificate.header.pages.max(1) {
            acknowledge_in(
                &publication,
                ordinal,
                certificate.header.input.operation_id,
                page,
            )?;
        }
    }
    Ok(())
}
fn acknowledge_in(
    publication: &journal::VerifiedPublication<'_, '_>,
    ordinal: u64,
    operation: Uuid,
    page: u32,
) -> AppResult<()> {
    let tx = publication.transaction();
    let mut certificate = publication.load_at(ordinal)?;
    if certificate.header.input.operation_id != operation {
        return Err(repair());
    }
    if !certificate.files_cleaned {
        return Err(repair());
    }
    if (certificate.header.pages == 0 && page != 0)
        || (certificate.header.pages != 0 && page >= certificate.header.pages)
    {
        return Err(repair());
    }
    if certificate.acknowledged {
        return Ok(());
    }
    if certificate.header.pages == 0 {
        if page != 0 || work_count(tx, operation)? != 0 {
            return Err(repair());
        }
    } else {
        if page >= certificate.header.pages {
            return Err(repair());
        }
        let mut evidence = journal::work_page(tx, &certificate, page)?;
        if !evidence.acknowledged {
            let rows = delete_work::certified_rows(tx, &certificate.header.input, page)?;
            let mut peer = None;
            for row in rows {
                let proved_absent = if row.mapping(tx)? {
                    false
                } else {
                    if row.conflicting_mapping(tx)? {
                        return Err(repair());
                    }
                    let proved = match peer {
                        Some(value) => value,
                        None => {
                            let value = peer_in(publication, &certificate, page)?;
                            peer = Some(value);
                            value
                        }
                    };
                    if !proved {
                        return Err(repair());
                    }
                    true
                };
                row.acknowledge(tx, operation, proved_absent)?;
            }
            evidence.acknowledged = true;
            update(tx, &page_key("page", operation, u64::from(page)), &evidence)?;
        }
    }
    let mut complete = true;
    for index in 0..certificate.header.pages {
        complete &= journal::work_page(tx, &certificate, index)?.acknowledged
    }
    if complete {
        if work_count(tx, operation)? != 0 {
            return Err(repair());
        }
        certificate.acknowledged = true;
        update(tx, &key("operation", operation), &certificate)?;
    }
    Ok(())
}
fn local_load(tx: &Transaction<'_>, operation: Uuid) -> AppResult<Option<Certificate>> {
    match journal::load(tx, operation) {
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::IndexRepairRequired
            | crate::intelligence::types::IntelligenceErrorCode::Configuration
            | crate::intelligence::types::IntelligenceErrorCode::ResourceLimit,
        )) => Ok(None),
        result => result,
    }
}
pub(crate) fn consume(
    tx: &Transaction<'_>,
    operation: Uuid,
    status: JobStatus,
    awaiting_reap: bool,
    reap: &JobReapPort,
) -> AppResult<()> {
    if awaiting_reap || matches!(status, JobStatus::Queued | JobStatus::Running) {
        return Ok(());
    }
    let Some(mut value) = local_load(tx, operation)? else {
        return Ok(());
    };
    if !value.acknowledged {
        return Ok(());
    }
    let job = job_proof::delete(tx, operation)?.ok_or_else(repair)?;
    if job.status != status {
        return Err(repair());
    }
    consume_proved(tx, &mut value, status, reap)?;
    // Terminal SQL consumption has no current filesystem authority. In
    // particular, fresh startup must retain a genuine prior cleaned receipt.
    Ok(())
}
pub(crate) fn settle_local(
    tx: &Transaction<'_>,
    publication_id: Uuid,
    reap: &JobReapPort,
) -> AppResult<()> {
    let Some(_) = read::<journal::Publication>(tx, &key("publication", publication_id))? else {
        return Ok(());
    };
    let publication = journal::verified(tx, publication_id)?;
    for batch in 0..publication.value.batches {
        let batch = publication.batch(batch)?;
        for index in 0..batch.len() {
            let operation = batch.operation(index)?;
            let mut value = batch.load(index)?;
            let job = job_proof::delete(tx, operation)?.ok_or_else(repair)?;
            if value.acknowledged && !matches!(job.status, JobStatus::Queued | JobStatus::Running) {
                consume_proved(tx, &mut value, job.status, reap)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn collection_ready(tx: &Transaction<'_>, publication_id: Uuid) -> AppResult<bool> {
    if journal::optional_publication(tx, publication_id)?.is_none() {
        return Ok(false);
    }
    let publication = journal::verified(tx, publication_id)?;
    can_collect(&publication)
}
fn can_collect(publication: &journal::VerifiedPublication<'_, '_>) -> AppResult<bool> {
    let tx = publication.transaction();
    let source_absent: bool = match (&publication.value.authority.identity, publication.value.authority.source) {
        (Some(identity), Some(source)) => !tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2)",
            params![identity, source.to_string()], |r| r.get::<_, bool>(0))?,
        _ => true,
    };
    let mut all_succeeded = true;
    for batch in 0..publication.value.batches {
        let batch = publication.batch(batch)?;
        for index in 0..batch.len() {
            let operation = batch.operation(index)?;
            let value = batch.load(index)?;
            if !value.acknowledged || !value.consumed {
                return Ok(false);
            }
            let job = job_proof::delete(tx, operation)?.ok_or_else(repair)?;
            all_succeeded &= job.status == JobStatus::Succeeded;
        }
    }
    Ok(match publication.value.authority.kind {
        CompletionKind::Unindexed => true,
        CompletionKind::Delete => {
            all_succeeded || (publication.value.source_removed && source_absent)
        }
        _ => publication.value.source_removed && source_absent,
    })
}
pub(crate) fn collect(
    tx: &Transaction<'_>,
    publication_id: Uuid,
    receipt: files::CollectionReceipt,
) -> AppResult<()> {
    let publication = journal::verified(tx, publication_id)?;
    receipt.require(&publication.value)?;
    if !can_collect(&publication)? {
        return Ok(());
    }
    // Validate the entire closed cohort above, then collect it atomically.
    // A crash rolls back this whole transaction, including every bounded page.
    for batch in 0..publication.value.batches {
        let batch = publication.batch(batch)?;
        for index in 0..batch.len() {
            let operation = batch.operation(index)?;
            tx.execute("DELETE FROM intelligence_meta WHERE key=?1 OR key LIKE ?2 OR key LIKE ?3 OR key=?4",
                params![key("operation", operation), format!("{}.%", key("page", operation)), format!("{}.%", key("files", operation)), key("cleaned", operation)])?;
            tx.execute(
                "DELETE FROM intelligence_meta WHERE key=?1",
                [key("consumed", operation)],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM intelligence_meta WHERE key LIKE ?1",
        [format!("{}.%", key("anchor", publication_id))],
    )?;
    tx.execute(
        "DELETE FROM intelligence_meta WHERE key=?1",
        [key("removed", publication_id)],
    )?;
    tx.execute(
        "DELETE FROM intelligence_meta WHERE key=?1",
        [key("publication", publication_id)],
    )?;
    Ok(())
}
