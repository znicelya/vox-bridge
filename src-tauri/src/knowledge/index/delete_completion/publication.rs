//! Validate a complete source cohort before deleting any domain row.
use super::*;
use crate::intelligence::types::JobStatus;
use journal::{
    Anchor, Authority, Certificate, CompletionKind, Header, Leaf, Publication, WorkPage,
};

pub(super) enum Scope<'a> {
    Source(&'a str, Uuid),
    Document(&'a DeleteInput),
}
pub(super) fn job_ids(tx: &Transaction<'_>, after: &str) -> AppResult<Vec<Uuid>> {
    let mut q = tx.prepare("SELECT id FROM intelligence_jobs WHERE id>?1 ORDER BY id LIMIT 200")?;
    let mut rows = q.query([after])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let raw = state::text(r, 0, 36).map_err(|_| repair())?;
        let id = Uuid::parse_str(raw).map_err(|_| repair())?;
        if id.to_string() != raw {
            return Err(repair());
        }
        out.push(id);
    }
    Ok(out)
}
pub(super) fn relevant(
    tx: &Transaction<'_>,
    id: Uuid,
    scope: &Scope<'_>,
) -> AppResult<Option<job_proof::DeleteJob>> {
    let (kind, object, input) = {
        let mut q =
            tx.prepare("SELECT kind,object_id,payload_json FROM intelligence_jobs WHERE id=?1")?;
        let mut rows = q.query([id.to_string()])?;
        let r = rows.next()?.ok_or_else(repair)?;
        let kind = state::text(r, 0, 32).map_err(|_| repair())?.to_owned();
        let object = state::optional_text(r, 1, 36)
            .map_err(|_| repair())?
            .map(str::to_owned);
        let input = match state::text(r, 2, 64 * 1024) {
            Ok(raw) => serde_json::from_str::<serde_json::Value>(raw)
                .ok()
                .and_then(|value| value.get("input").cloned()),
            Err(_) => None,
        };
        (kind, object, input)
    };
    let work: bool = match scope {
        Scope::Source(identity, generation) => tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_index_work_items WHERE operation_id=?1 \
             AND identity=?2 AND generation=?3 AND document_id IS NOT NULL \
             AND location_sha256 IS NULL AND previous_vector_id IS NULL)",
            params![id.to_string(), identity, generation.to_string()], |r| r.get(0))?,
        Scope::Document(target) => tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_index_work_items WHERE operation_id=?1 AND document_id=?2 \
             AND location_sha256 IS NULL AND previous_vector_id IS NULL)",
            params![id.to_string(), target.document_id.to_string()], |r| r.get(0))?,
    };
    let live_object: bool = match scope {
        Scope::Source(identity, _) => tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents \
            WHERE id IS ?1 AND status='deleting' AND index_identity=?2)",
            params![object, identity],
            |r| r.get(0),
        )?,
        Scope::Document(target) => {
            object.as_deref() == Some(target.document_id.to_string().as_str())
        }
    };
    let decoded = input
        .clone()
        .and_then(|input| serde_json::from_value::<DeleteInput>(input).ok());
    let delete_shape = input.as_ref().is_some_and(|input| {
        input.get("document").is_some()
            || input.get("operation_id").is_some()
            || input.get("generation").is_some()
    });
    if kind != "delete_document" && !work && !delete_shape {
        return Ok(None);
    }
    let matches = decoded.as_ref().is_some_and(|input| match scope {
        Scope::Source(identity, generation) => {
            input.identity.as_deref() == Some(*identity) && input.generation == Some(*generation)
        }
        Scope::Document(target) => input.document_id == target.document_id,
    });
    if matches!(scope, Scope::Source(_, _)) && decoded.is_some() && !matches && !work {
        return Ok(None);
    }
    if !matches && !work && !live_object {
        if decoded.is_some() || matches!(scope, Scope::Document(_)) || kind != "delete_document" {
            return Ok(None);
        }
        // Unknown deletion payloads cannot become unreferenced by failed parsing.
        return Err(repair());
    }
    let job = job_proof::delete(tx, id)?.ok_or_else(repair)?;
    if journal::load(tx, id)?.is_some() {
        return Ok(None);
    }
    if !present_document(tx, job.input.document_id)? {
        if job.status == JobStatus::Succeeded && work_count(tx, id)? == 0 {
            return Ok(None);
        }
        return Err(repair());
    }
    match scope {
        Scope::Source(identity, generation)
            if job.input.identity.as_deref() != Some(*identity)
                || job.input.generation != Some(*generation) =>
        {
            return Err(repair());
        }
        Scope::Document(target)
            if job.input.identity != target.identity
                || job.input.generation != target.generation
                || job.input.document != target.document =>
        {
            return Err(repair());
        }
        _ => {}
    }
    delete_work::require_document(tx, &job.input)?;
    Ok(Some(job))
}
pub(super) fn orphan_work(tx: &Transaction<'_>, scope: &Scope<'_>) -> AppResult<()> {
    let missing: bool = match scope {
        Scope::Source(identity, generation) => tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_index_work_items w \
            WHERE w.identity=?1 AND w.generation=?2 AND w.document_id IS NOT NULL AND w.location_sha256 IS NULL \
            AND w.previous_vector_id IS NULL AND NOT EXISTS(SELECT 1 FROM intelligence_jobs j WHERE j.id=w.operation_id))",
            params![identity, generation.to_string()], |r| r.get(0))?,
        Scope::Document(input) => tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_index_work_items w \
            WHERE w.document_id=?1 AND w.location_sha256 IS NULL AND w.previous_vector_id IS NULL \
            AND NOT EXISTS(SELECT 1 FROM intelligence_jobs j WHERE j.id=w.operation_id))",
            [input.document_id.to_string()], |r| r.get(0))?,
    };
    if missing {
        return Err(repair());
    }
    Ok(())
}
pub(crate) fn preflight(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    directory: &files::ManagedDirectory,
) -> AppResult<delete_work::DeleteWorkProof> {
    let scope = Scope::Document(input);
    orphan_work(tx, &scope)?;
    let own = delete_work::capture(tx, input)?;
    let mut after = String::new();
    let mut found = false;
    loop {
        let jobs = job_ids(tx, &after)?;
        if jobs.is_empty() {
            break;
        }
        for id in jobs {
            after = id.to_string();
            if let Some(job) = relevant(tx, id, &scope)? {
                delete_work::capture(tx, &job.input)?;
                files::capture(tx, &job.input, directory, false)?;
                found |= id == input.operation_id;
            }
        }
    }
    if !found {
        return Err(repair());
    }
    Ok(own)
}
pub(crate) fn publish_delete(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    receipt: Option<&super::super::owner::PersistedReceipt>,
    directory: &files::ManagedDirectory,
) -> AppResult<()> {
    let authority = match receipt {
        Some(receipt) => Authority {
            id: receipt.intent().body().operation_id,
            kind: CompletionKind::Delete,
            identity: input.identity.clone(),
            source: input.generation,
            source_generation: Some(receipt.intent().body().descriptor.clone()),
            intent_json: Some(receipt.intent().body().encode()?),
            seal_json: Some(receipt.seal_json().into()),
            fresh_count: None,
        },
        None => Authority {
            id: input.operation_id,
            kind: CompletionKind::Unindexed,
            identity: None,
            source: None,
            source_generation: None,
            intent_json: None,
            seal_json: None,
            fresh_count: None,
        },
    };
    authority.validate()?;
    build(tx, authority, Scope::Document(input), directory)
}
pub(crate) fn publish_fresh(
    tx: &Transaction<'_>,
    receipt: &super::super::owner::PersistedReceipt,
    source: Uuid,
    count: u32,
    kind: CompletionKind,
    directory: &files::ManagedDirectory,
) -> AppResult<()> {
    let intent = receipt.intent().body();
    let source_generation = source_descriptor(tx, &intent.identity, source)?;
    let authority = Authority {
        id: intent.operation_id,
        kind,
        identity: Some(intent.identity.clone()),
        source: Some(source),
        source_generation: Some(source_generation),
        intent_json: Some(intent.encode()?),
        seal_json: Some(receipt.seal_json().into()),
        fresh_count: Some(count),
    };
    authority.validate()?;
    build(
        tx,
        authority,
        Scope::Source(&intent.identity, source),
        directory,
    )
}
fn source_descriptor(tx: &Transaction<'_>, identity: &str, source: Uuid) -> AppResult<Generation> {
    let mut q = tx.prepare("SELECT seal_json,intent_json FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2")?;
    let mut rows = q.query(params![identity, source.to_string()])?;
    let r = rows.next()?.ok_or_else(repair)?;
    let descriptor = match state::optional_text(r, 0, 65536).map_err(|_| repair())? {
        Some(raw) => super::super::Seal::decode(raw)?.descriptor,
        None => {
            super::super::seal::Intent::decode(state::text(r, 1, 16384).map_err(|_| repair())?)?
                .descriptor
        }
    };
    if descriptor.key()? != identity || descriptor.generation != source {
        return Err(repair());
    }
    Ok(descriptor)
}
fn target_key(publication: Uuid, document: Uuid) -> String {
    format!("{}.{}", key("target", publication), document)
}
fn build(
    tx: &Transaction<'_>,
    authority: Authority,
    scope: Scope<'_>,
    directory: &files::ManagedDirectory,
) -> AppResult<()> {
    orphan_work(tx, &scope)?;
    let mut after = String::new();
    let mut count = 0u64;
    let mut leaves = Vec::new();
    // Batch hashes are staged as bounded records. The final root is streamed
    // from those records once the complete cardinality is known.
    loop {
        let jobs = job_ids(tx, &after)?;
        if jobs.is_empty() {
            break;
        }
        for id in jobs {
            after = id.to_string();
            let Some(job) = relevant(tx, id, &scope)? else {
                continue;
            };
            let target = target_key(authority.id, job.input.document_id);
            let prior_target: Option<super::super::DocumentFence> = read(tx, &target)?;
            match &prior_target {
                Some(existing) if *existing != job.input.document => return Err(repair()),
                None if matches!(scope, Scope::Source(_, _)) => {
                    // One fresh receipt cannot complete this target's operation
                    // bound to a different source. Check all its unresolved
                    // peers once before staging any domain deletion authority.
                    preflight(tx, &job.input, directory)?;
                }
                _ => {}
            }
            let proof = delete_work::capture(tx, &job.input)?;
            let file_proof = files::capture(tx, &job.input, directory, true)?;
            if let Some(identity) = &authority.identity {
                let absent: bool = if authority.kind == CompletionKind::Delete {
                    true
                } else {
                    let seal = super::super::Seal::decode(
                        authority.seal_json.as_deref().ok_or_else(repair)?,
                    )?;
                    !tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_generation_mappings WHERE identity=?1 \
                        AND generation=?2 AND document_id=?3)", params![identity, seal.descriptor.generation.to_string(), job.input.document_id.to_string()], |r| r.get::<_, bool>(0))?
                };
                if !absent {
                    return Err(repair());
                }
            }
            let header = Header {
                version: 1,
                input: job.input.clone(),
                payload_sha256: job.proof.payload_hash().into(),
                count: proof.count,
                work_root: proof.root.clone(),
                shared_root: proof.shared_root(&job.input)?,
                pages: proof.pages.len() as u32,
                file_count: file_proof.count,
                file_root: file_proof.root,
                file_pages: file_proof.pages,
                file_directory_sha256: file_proof.directory_sha256,
                publication: authority.id,
                authority_sha256: hash(&authority)?,
                ordinal: count,
            };
            for (index, (association, semantic)) in proof
                .pages
                .into_iter()
                .zip(proof.association_pages)
                .enumerate()
            {
                let page = WorkPage {
                    version: 1,
                    operation: id,
                    index: index as u32,
                    count: (proof.count - index as u32 * delete_work::PAGE_SIZE)
                        .min(delete_work::PAGE_SIZE),
                    association_sha256: association,
                    semantic_sha256: semantic,
                    acknowledged: false,
                };
                insert(tx, &page_key("page", id, index as u64), &page)?;
            }
            if prior_target.is_none() {
                insert(tx, &target, &job.input.document)?
            }
            leaves.push(Leaf {
                operation: id,
                header_sha256: hash(&header)?,
            });
            insert(
                tx,
                &key("operation", id),
                &Certificate {
                    header,
                    files_cleaned: false,
                    acknowledged: false,
                    consumed: false,
                },
            )?;
            count = count.checked_add(1).ok_or_else(repair)?;
            if leaves.len() as u64 == ANCHOR_PAGE {
                let anchor = Anchor {
                    version: 1,
                    publication: authority.id,
                    index: (count - 1) / ANCHOR_PAGE,
                    leaves: std::mem::take(&mut leaves),
                };
                insert(tx, &page_key("anchor", authority.id, anchor.index), &anchor)?;
            }
        }
    }
    if !leaves.is_empty() {
        let anchor = Anchor {
            version: 1,
            publication: authority.id,
            index: count / ANCHOR_PAGE,
            leaves,
        };
        insert(tx, &page_key("anchor", authority.id, anchor.index), &anchor)?;
    }
    ensure_targets(tx, &authority, &scope)?;
    if count == 0 {
        return Ok(());
    }
    let batches = count.div_ceil(ANCHOR_PAGE);
    let mut root = journal::publication_root(&authority, count, batches)?;
    for index in 0..batches {
        let batch: Anchor =
            read(tx, &page_key("anchor", authority.id, index))?.ok_or_else(repair)?;
        field(&mut root, hash(&batch)?.as_bytes());
    }
    insert(
        tx,
        &key("publication", authority.id),
        &Publication {
            version: 1,
            source_removed: authority.source.is_none(),
            authority: authority.clone(),
            count,
            batches,
            root: finish(root),
        },
    )?;
    delete_targets(tx, &authority)
}
fn ensure_targets(tx: &Transaction<'_>, authority: &Authority, scope: &Scope<'_>) -> AppResult<()> {
    let mut after = String::new();
    loop {
        let ids = {
            let (identity, document) = match scope {
                Scope::Source(identity, _) => (Some(*identity), None),
                Scope::Document(input) => (None, Some(input.document_id.to_string())),
            };
            let mut q = tx.prepare(
                "SELECT id FROM documents WHERE status='deleting' AND id>?1 \
                AND ((?2 IS NOT NULL AND index_identity=?2) OR id IS ?3) ORDER BY id LIMIT 200",
            )?;
            let mut rows = q.query(params![after, identity, document])?;
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
            let fence: super::super::DocumentFence =
                read(tx, &target_key(authority.id, id))?.ok_or_else(repair)?;
            if id.to_string() != raw || fence.id != id || !fence.matches(tx, "deleting")? {
                return Err(repair());
            }
        }
    }
}
fn delete_targets(tx: &Transaction<'_>, authority: &Authority) -> AppResult<()> {
    let prefix = format!("{}.", key("target", authority.id));
    let mut after = prefix.clone();
    loop {
        let targets = {
            let mut q = tx.prepare("SELECT key,value_json FROM intelligence_meta WHERE key LIKE ?1 AND key>?2 ORDER BY key LIMIT 200")?;
            let mut rows = q.query(params![format!("{prefix}%"), after])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                let key = state::text(r, 0, 160).map_err(|_| repair())?.to_owned();
                let raw = state::text(r, 1, 1024).map_err(|_| repair())?;
                let fence: super::super::DocumentFence =
                    serde_json::from_str(raw).map_err(|_| repair())?;
                if encode(&fence)? != raw || key != target_key(authority.id, fence.id) {
                    return Err(repair());
                }
                out.push((key, fence));
            }
            out
        };
        if targets.is_empty() {
            return Ok(());
        }
        for (key, fence) in targets {
            after = key.clone();
            if !fence.matches(tx, "deleting")? {
                return Err(repair());
            }
            tx.execute(
                "UPDATE knowledge_managed_files SET state='delete_pending' WHERE document_id=?1",
                [fence.id.to_string()],
            )?;
            if tx.execute(
                "DELETE FROM documents WHERE id=?1 AND version=?2",
                params![fence.id.to_string(), fence.version],
            )? != 1
            {
                return Err(repair());
            }
            tx.execute("DELETE FROM intelligence_meta WHERE key=?1", [key])?;
        }
    }
}
