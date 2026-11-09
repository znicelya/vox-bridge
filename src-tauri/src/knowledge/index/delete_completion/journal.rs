use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompletionKind {
    Swap,
    Recovery,
    Ready,
    Delete,
    Unindexed,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Authority {
    pub(crate) id: Uuid,
    pub(crate) kind: CompletionKind,
    pub(crate) identity: Option<String>,
    pub(crate) source: Option<Uuid>,
    pub(crate) source_generation: Option<Generation>,
    pub(crate) intent_json: Option<String>,
    pub(crate) seal_json: Option<String>,
    pub(crate) fresh_count: Option<u32>,
}
impl Authority {
    pub(super) fn validate(&self) -> AppResult<()> {
        if self.kind == CompletionKind::Unindexed {
            if self.identity.is_some()
                || self.source.is_some()
                || self.source_generation.is_some()
                || self.intent_json.is_some()
                || self.seal_json.is_some()
                || self.fresh_count.is_some()
            {
                return Err(repair());
            }
            return Ok(());
        }
        let intent =
            super::super::seal::Intent::decode(self.intent_json.as_deref().ok_or_else(repair)?)?;
        let seal = super::super::Seal::decode(self.seal_json.as_deref().ok_or_else(repair)?)?;
        if intent.operation_id != self.id
            || seal.operation_id != self.id
            || seal.descriptor != intent.descriptor
            || self.identity.as_deref() != Some(&intent.identity)
            || self.source.is_none()
        {
            return Err(repair());
        }
        let source = self.source_generation.as_ref().ok_or_else(repair)?;
        if Some(source.generation) != self.source || source.key()? != intent.identity {
            return Err(repair());
        }
        if self.kind == CompletionKind::Delete {
            if intent.operation != super::super::seal::IntentKind::Open
                || self.source != Some(intent.descriptor.generation)
                || seal.stored_count != intent.expected_stored_count
                || self.fresh_count.is_some()
            {
                return Err(repair());
            }
        } else if intent.operation != super::super::seal::IntentKind::Create
            || intent.prior_seal_sha256.is_some()
            || self.source == Some(intent.descriptor.generation)
            || self.fresh_count.map(u64::from) != Some(seal.stored_count)
        {
            return Err(repair());
        }
        Ok(())
    }
    pub(crate) fn source_descriptor(&self) -> AppResult<Option<Generation>> {
        self.validate()?;
        Ok(self.source_generation.clone())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Publication {
    pub(crate) version: u32,
    pub(crate) authority: Authority,
    pub(crate) count: u64,
    pub(crate) batches: u64,
    pub(crate) root: String,
    pub(crate) source_removed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Leaf {
    pub(super) operation: Uuid,
    pub(super) header_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Anchor {
    pub(super) version: u32,
    pub(super) publication: Uuid,
    pub(super) index: u64,
    pub(super) leaves: Vec<Leaf>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub(crate) version: u32,
    pub(crate) input: DeleteInput,
    pub(crate) payload_sha256: String,
    pub(crate) count: u32,
    pub(crate) work_root: String,
    pub(crate) shared_root: String,
    pub(crate) pages: u32,
    pub(crate) file_count: u32,
    pub(crate) file_root: String,
    pub(crate) file_pages: u32,
    pub(crate) file_directory_sha256: String,
    pub(crate) publication: Uuid,
    pub(crate) authority_sha256: String,
    pub(crate) ordinal: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Certificate {
    pub(crate) header: Header,
    pub(crate) files_cleaned: bool,
    pub(crate) acknowledged: bool,
    pub(crate) consumed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkPage {
    pub(super) version: u32,
    pub(super) operation: Uuid,
    pub(super) index: u32,
    pub(super) count: u32,
    pub(super) association_sha256: String,
    pub(super) semantic_sha256: String,
    pub(super) acknowledged: bool,
}
pub(super) fn publication_root(
    authority: &Authority,
    count: u64,
    batches: u64,
) -> AppResult<Sha256> {
    let mut root = Sha256::new();
    field(&mut root, b"voxbridge-delete-publication-v1");
    field(&mut root, hash(authority)?.as_bytes());
    field(&mut root, &count.to_le_bytes());
    field(&mut root, &batches.to_le_bytes());
    Ok(root)
}
fn publication_record(tx: &Transaction<'_>, id: Uuid) -> AppResult<Publication> {
    let value: Publication = read(tx, &key("publication", id))?.ok_or_else(repair)?;
    value.authority.validate()?;
    if value.version != 1
        || value.authority.id != id
        || value.count == 0
        || value.batches != value.count.div_ceil(ANCHOR_PAGE)
        || !super::super::codec::is_hash(&value.root)
    {
        return Err(repair());
    }
    retirement::validate_removal(tx, &value)?;
    let stored: u64 = tx.query_row(
        "SELECT count(*) FROM intelligence_meta WHERE key LIKE ?1",
        [format!("{}.%", key("anchor", id))],
        |r| r.get(0),
    )?;
    if stored != value.batches {
        return Err(repair());
    }
    Ok(value)
}
pub(crate) fn publication(tx: &Transaction<'_>, id: Uuid) -> AppResult<Publication> {
    let value = publication_record(tx, id)?;
    let mut root = publication_root(&value.authority, value.count, value.batches)?;
    let mut last = None;
    for index in 0..value.batches {
        let batch = anchor(tx, &value, index)?;
        for leaf in &batch.leaves {
            if last.is_some_and(|prior| prior >= leaf.operation) {
                return Err(repair());
            }
            last = Some(leaf.operation);
        }
        field(&mut root, hash(&batch)?.as_bytes());
    }
    if finish(root) != value.root {
        return Err(repair());
    }
    Ok(value)
}
pub(crate) fn optional_publication(
    tx: &Transaction<'_>,
    id: Uuid,
) -> AppResult<Option<Publication>> {
    if read::<Publication>(tx, &key("publication", id))?.is_none() {
        return Ok(None);
    }
    self::publication(tx, id).map(Some)
}
fn anchor(tx: &Transaction<'_>, publication: &Publication, index: u64) -> AppResult<Anchor> {
    let value: Anchor =
        read(tx, &page_key("anchor", publication.authority.id, index))?.ok_or_else(repair)?;
    let expected = publication
        .count
        .checked_sub(index.checked_mul(ANCHOR_PAGE).ok_or_else(repair)?)
        .ok_or_else(repair)?
        .min(ANCHOR_PAGE);
    if value.version != 1
        || value.publication != publication.authority.id
        || value.index != index
        || value.leaves.len() as u64 != expected
        || expected == 0
        || value
            .leaves
            .iter()
            .any(|leaf| !super::super::codec::is_hash(&leaf.header_sha256))
    {
        return Err(repair());
    }
    Ok(value)
}
pub(crate) fn operations(
    tx: &Transaction<'_>,
    publication: Uuid,
    batch: u64,
) -> AppResult<Vec<Uuid>> {
    let value = self::publication(tx, publication)?;
    if batch >= value.batches {
        return Err(repair());
    }
    Ok(anchor(tx, &value, batch)?
        .leaves
        .into_iter()
        .map(|leaf| leaf.operation)
        .collect())
}
pub(super) fn work_page(
    tx: &Transaction<'_>,
    certificate: &Certificate,
    index: u32,
) -> AppResult<WorkPage> {
    let header = &certificate.header;
    let page: WorkPage = read(
        tx,
        &page_key("page", header.input.operation_id, u64::from(index)),
    )?
    .ok_or_else(repair)?;
    let count = header
        .count
        .checked_sub(
            index
                .checked_mul(delete_work::PAGE_SIZE)
                .ok_or_else(repair)?,
        )
        .ok_or_else(repair)?
        .min(delete_work::PAGE_SIZE);
    if page.version != 1
        || page.operation != header.input.operation_id
        || page.index != index
        || page.count != count
        || count == 0
        || !super::super::codec::is_hash(&page.association_sha256)
        || !super::super::codec::is_hash(&page.semantic_sha256)
    {
        return Err(repair());
    }
    Ok(page)
}
// These views cannot escape their SQLite transaction. Only one bounded anchor
// batch is decoded at a time; each certificate and mutable work flag is read
// again when used. The complete ordered root is verified before issuing a view.
pub(super) struct VerifiedPublication<'tx, 'conn> {
    tx: &'tx Transaction<'conn>,
    pub(super) value: Publication,
    cached: std::cell::RefCell<Option<Anchor>>,
}
pub(super) struct VerifiedBatch<'view, 'tx, 'conn> {
    publication: &'view VerifiedPublication<'tx, 'conn>,
    anchor: Anchor,
}
pub(super) fn verified<'tx, 'conn>(
    tx: &'tx Transaction<'conn>,
    id: Uuid,
) -> AppResult<VerifiedPublication<'tx, 'conn>> {
    Ok(VerifiedPublication {
        tx,
        value: publication(tx, id)?,
        cached: std::cell::RefCell::new(None),
    })
}
impl<'tx, 'conn> VerifiedPublication<'tx, 'conn> {
    pub(super) fn transaction(&self) -> &Transaction<'conn> {
        self.tx
    }
    fn leaf_at(&self, ordinal: u64) -> AppResult<Leaf> {
        if ordinal >= self.value.count {
            return Err(repair());
        }
        let index = ordinal / ANCHOR_PAGE;
        let mut cached = self.cached.borrow_mut();
        if cached.as_ref().is_none_or(|batch| batch.index != index) {
            // Drop the old batch before decoding another. This authority and
            // its single bounded immutable batch cannot escape this transaction.
            *cached = None;
            *cached = Some(anchor(self.tx, &self.value, index)?);
        }
        Ok(cached
            .as_ref()
            .ok_or_else(repair)?
            .leaves
            .get((ordinal % ANCHOR_PAGE) as usize)
            .ok_or_else(repair)?
            .clone())
    }
    pub(super) fn load_at(&self, ordinal: u64) -> AppResult<Certificate> {
        let leaf = self.leaf_at(ordinal)?;
        let value = stored(self.tx, leaf.operation)?.ok_or_else(repair)?;
        validate_certificate(self.tx, leaf.operation, value, &self.value, &leaf, ordinal)
    }
    pub(super) fn batch(&self, index: u64) -> AppResult<VerifiedBatch<'_, 'tx, 'conn>> {
        if index >= self.value.batches {
            return Err(repair());
        }
        Ok(VerifiedBatch {
            publication: self,
            anchor: anchor(self.tx, &self.value, index)?,
        })
    }
}
impl VerifiedBatch<'_, '_, '_> {
    pub(super) fn len(&self) -> usize {
        self.anchor.leaves.len()
    }
    pub(super) fn operation(&self, index: usize) -> AppResult<Uuid> {
        Ok(self.anchor.leaves.get(index).ok_or_else(repair)?.operation)
    }
    pub(super) fn load(&self, index: usize) -> AppResult<Certificate> {
        let leaf = self.anchor.leaves.get(index).ok_or_else(repair)?;
        let value = stored(self.publication.tx, leaf.operation)?.ok_or_else(repair)?;
        let ordinal = self
            .anchor
            .index
            .checked_mul(ANCHOR_PAGE)
            .and_then(|start| start.checked_add(index as u64))
            .ok_or_else(repair)?;
        validate_certificate(
            self.publication.tx,
            leaf.operation,
            value,
            &self.publication.value,
            leaf,
            ordinal,
        )
    }
}
pub(super) fn stored(tx: &Transaction<'_>, operation: Uuid) -> AppResult<Option<Certificate>> {
    read(tx, &key("operation", operation))
}
pub(crate) fn load(tx: &Transaction<'_>, operation: Uuid) -> AppResult<Option<Certificate>> {
    let Some(value) = stored(tx, operation)? else {
        return Ok(None);
    };
    let pub_value = self::publication(tx, value.header.publication)?;
    load_member(tx, operation, value, &pub_value).map(Some)
}
pub(crate) fn resume_input(
    tx: &Transaction<'_>,
    operation: Uuid,
) -> AppResult<Option<DeleteInput>> {
    let Some(value) = stored(tx, operation)? else {
        return Ok(None);
    };
    let publication = self::publication(tx, value.header.publication)?;
    let value = load_member(tx, operation, value, &publication)?;
    // Settled history remains retirement authority while the source is active.
    // A verified removed source (including local publication) still needs GC
    // retried after an interrupted or rolled-back collection transaction.
    Ok((!value.consumed || publication.source_removed).then_some(value.header.input))
}
pub(super) fn load_in_source(
    view: &retirement::SourceView<'_, '_>,
    operation: Uuid,
) -> AppResult<Option<Certificate>> {
    let tx = view.transaction();
    let Some(value) = stored(tx, operation)? else {
        return Ok(None);
    };
    let publication = publication_record(tx, value.header.publication)?;
    if publication.authority.source_generation.as_ref() != Some(view.source()) {
        return Err(repair());
    }
    // SourceView already streamed every same-source publication/root in this
    // very transaction. No immutable journal writes occur before its guards end.
    load_member(tx, operation, value, &publication).map(Some)
}
fn load_member(
    tx: &Transaction<'_>,
    operation: Uuid,
    value: Certificate,
    pub_value: &Publication,
) -> AppResult<Certificate> {
    let ordinal = value.header.ordinal;
    if ordinal >= pub_value.count {
        return Err(repair());
    }
    let batch = anchor(tx, pub_value, ordinal / ANCHOR_PAGE)?;
    let leaf = batch
        .leaves
        .get((ordinal % ANCHOR_PAGE) as usize)
        .ok_or_else(repair)?;
    validate_certificate(tx, operation, value, pub_value, leaf, ordinal)
}
fn validate_certificate(
    tx: &Transaction<'_>,
    operation: Uuid,
    value: Certificate,
    pub_value: &Publication,
    leaf: &Leaf,
    ordinal: u64,
) -> AppResult<Certificate> {
    let header = &value.header;
    header.input.validate()?;
    let owner = job_proof::delete(tx, operation)?.ok_or_else(repair)?;
    if header.version != 1
        || header.input.operation_id != operation
        || owner.input != header.input
        || header.payload_sha256 != owner.proof.payload_hash()
        || header.count > 200_000
        || header.pages != header.count.div_ceil(delete_work::PAGE_SIZE)
        || header.file_count > 200_000
        || header.file_pages != header.file_count.div_ceil(files::FILE_PAGE)
        || !super::super::codec::is_hash(&header.file_directory_sha256)
    {
        return Err(repair());
    }
    if header.publication != pub_value.authority.id
        || header.ordinal != ordinal
        || header.ordinal >= pub_value.count
        || header.authority_sha256 != hash(&pub_value.authority)?
        || header.input.identity != pub_value.authority.identity
        || header.input.generation != pub_value.authority.source
    {
        return Err(repair());
    }
    if leaf.operation != operation
        || leaf.header_sha256 != hash(header)?
        || present_document(tx, header.input.document_id)?
    {
        return Err(repair());
    }
    let mut hashes = Vec::new();
    let mut associations = Vec::new();
    let mut remaining = 0u64;
    let mut all_ack = true;
    let stored_pages: u64 = tx.query_row(
        "SELECT count(*) FROM intelligence_meta WHERE key LIKE ?1",
        [format!("{}.%", key("page", operation))],
        |r| r.get(0),
    )?;
    if stored_pages != u64::from(header.pages) {
        return Err(repair());
    }
    for index in 0..header.pages {
        let page = work_page(tx, &value, index)?;
        let rows = delete_work::certified_rows(tx, &header.input, index)?;
        if page.acknowledged {
            if !rows.is_empty() {
                return Err(repair());
            }
        } else {
            if rows.len() as u32 != page.count
                || delete_work::page_hash(&rows) != page.association_sha256
                || delete_work::association_page_hash(&rows)? != page.semantic_sha256
            {
                return Err(repair());
            }
            remaining = remaining
                .checked_add(u64::from(page.count))
                .ok_or_else(repair)?;
            all_ack = false;
        }
        hashes.push(page.association_sha256);
        associations.push(page.semantic_sha256);
    }
    if work_count(tx, operation)? != remaining {
        return Err(repair());
    }
    let proof = delete_work::from_pages(&header.input, header.count, hashes, associations)?;
    if proof.root != header.work_root || proof.shared_root(&header.input)? != header.shared_root {
        return Err(repair());
    }
    files::validate_pages(tx, header)?;
    files::validate_receipt(tx, header, value.files_cleaned)?;
    if value.files_cleaned {
        files::require_metadata_absent(tx, header)?
    }
    if value.acknowledged && (!all_ack || !value.files_cleaned || remaining != 0) {
        return Err(repair());
    }
    retirement::validate_consumption(tx, &value, owner.status)?;
    Ok(value)
}
pub(crate) fn scan_operations(tx: &Transaction<'_>, after: &str) -> AppResult<Vec<Uuid>> {
    scan(tx, "operation", after)
}
pub(crate) fn scan_publications(tx: &Transaction<'_>, after: &str) -> AppResult<Vec<Uuid>> {
    scan(tx, "publication", after)
}
fn scan(tx: &Transaction<'_>, kind: &str, after: &str) -> AppResult<Vec<Uuid>> {
    let mut q = tx.prepare(
        "SELECT key FROM intelligence_meta WHERE key LIKE ?1 AND key>?2 ORDER BY key LIMIT 200",
    )?;
    let prefix = format!("{}.", key_prefix(kind));
    let mut rows = q.query(params![format!("{prefix}%"), format!("{prefix}{after}")])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let raw = state::text(r, 0, 160).map_err(|_| repair())?;
        let suffix = raw.strip_prefix(&prefix).ok_or_else(repair)?;
        let id = Uuid::parse_str(suffix).map_err(|_| repair())?;
        if id.to_string() != suffix || key(kind, id) != raw {
            return Err(repair());
        }
        out.push(id);
    }
    Ok(out)
}
pub(super) fn key_prefix(kind: &str) -> String {
    format!("{PREFIX}.{kind}")
}
