use crate::error::AppError;
use crate::intelligence::{
    test_support::IntelligenceHarness,
    types::{IntelligenceErrorCode as Code, JobStatus},
};
use crate::knowledge::{
    MAX_OUTPUT_BYTES, chunk_blocks, chunk_blocks_bounded,
    documents::DocumentService,
    parser::DocumentParser,
    types::{DocumentKind, DocumentStatus, TextBlock, TextLocation},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;

fn block(text_len: usize, heading: String, page: u32) -> TextBlock {
    TextBlock {
        text: "x".repeat(text_len),
        location: TextLocation {
            page: Some(page),
            paragraph: 1,
            heading: Some(heading),
            start_char: 0,
            end_char: text_len as u32,
        },
    }
}

#[test]
fn documents_derived_full_6000_cjk_heading_and_exact_offsets_survive() {
    let heading = "电".repeat(6000);
    let blocks = vec![block(4800, heading.clone(), 2)];
    let chunks = chunk_blocks_bounded(&blocks).unwrap();
    assert_eq!(chunks, chunk_blocks(&blocks).unwrap());
    assert_eq!(
        chunks
            .iter()
            .map(|c| (c.location.start_char, c.location.end_char))
            .collect::<Vec<_>>(),
        vec![(0, 2400), (2080, 4480), (4160, 4800)]
    );
    assert!(
        chunks
            .iter()
            .all(|c| c.location.heading.as_deref() == Some(heading.as_str())
                && c.location.page == Some(2))
    );
}

#[test]
fn documents_derived_safe_probe_accepts_actual_96_chunks_without_conservative_refusal() {
    let heading = "h".repeat(256 * 1024);
    let chunks = chunk_blocks_bounded(&[block(200_000, heading.clone(), 1)]).unwrap();
    assert_eq!(chunks.len(), 96);
    assert!(
        chunks
            .iter()
            .all(|c| c.location.heading.as_deref() == Some(heading.as_str()))
    );
    assert_eq!(chunks.last().unwrap().location.end_char, 200_000);
}

#[test]
fn documents_derived_single_group_over_budget_is_resource_limit() {
    let blocks = vec![block(400_000, "h".repeat(256 * 1024), 1)];
    assert!(serde_json::to_vec(&blocks).unwrap().len() < MAX_OUTPUT_BYTES);
    assert!(matches!(
        chunk_blocks_bounded(&blocks),
        Err(AppError::Intelligence(Code::ResourceLimit))
    ));
    // The established public entry keeps its original policy.
    assert_eq!(
        chunk_blocks(&blocks)
            .unwrap()
            .last()
            .unwrap()
            .location
            .end_char,
        400_000
    );
}

#[test]
fn documents_derived_budget_accumulates_across_later_groups() {
    let blocks = vec![
        block(200_000, "电".repeat(64 * 1024), 1),
        block(200_000, "电".repeat(64 * 1024), 2),
    ];
    for one in &blocks {
        assert!(chunk_blocks_bounded(std::slice::from_ref(one)).is_ok());
    }
    assert!(serde_json::to_vec(&blocks).unwrap().len() < MAX_OUTPUT_BYTES);
    assert!(matches!(
        chunk_blocks_bounded(&blocks),
        Err(AppError::Intelligence(Code::ResourceLimit))
    ));
}

struct Parser;
#[async_trait::async_trait]
impl DocumentParser for Parser {
    async fn parse(
        &self,
        path: PathBuf,
        _: DocumentKind,
        _: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        let text = std::fs::read_to_string(path).map_err(|_| Code::ParseFailed)?;
        if text == "oversized derived heading" {
            return Ok(vec![block(400_000, "h".repeat(256 * 1024), 1)]);
        }
        let end_char = text.chars().count() as u32;
        Ok(vec![TextBlock {
            text,
            location: TextLocation {
                page: None,
                paragraph: 1,
                heading: None,
                start_char: 0,
                end_char,
            },
        }])
    }
}
async fn terminal(h: &IntelligenceHarness, id: uuid::Uuid) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !matches!(
                h.store.get_job(id).unwrap().unwrap().status,
                JobStatus::Queued | JobStatus::Running
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn documents_derived_service_failure_preserves_old_text_source_and_cleans_candidate() {
    let h = IntelligenceHarness::new();
    let docs = DocumentService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        Arc::new(Parser),
        h.temp_dir.path().into(),
    );
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let source = h.temp_dir.path().join("source.txt");
    std::fs::write(&source, "old canonical text").unwrap();
    let old = docs.import(source.clone()).await.unwrap();
    terminal(&h, old.id).await;
    let id = old.object_id.unwrap();
    let retained = docs.get(id).unwrap().unwrap().managed_path.unwrap();
    let replacement = h.temp_dir.path().join("replacement.txt");
    std::fs::write(&replacement, "oversized derived heading").unwrap();
    let new = docs.replace(id, replacement.clone()).await.unwrap();
    terminal(&h, new.id).await;
    let job = h.store.get_job(new.id).unwrap().unwrap();
    let detail = docs.get(id).unwrap().unwrap();
    let chunks: Vec<(String, String)> = h
        .store
        .transaction(|tx| {
            let mut s =
                tx.prepare("SELECT text,index_state FROM document_chunks WHERE document_id=?1")?;
            let rows = s
                .query_map([id.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    h.jobs.shutdown().await.unwrap();
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error, Some(Code::ResourceLimit));
    assert_eq!(job.completed_units, 0);
    assert_eq!(job.total_units, None);
    assert_eq!(detail.summary.status, DocumentStatus::Failed);
    assert_eq!(detail.summary.error, Some(Code::ResourceLimit));
    assert_eq!(detail.summary.version, 2);
    assert_eq!(detail.managed_path, None);
    assert_eq!(chunks, vec![("old canonical text".into(), "stale".into())]);
    assert_eq!(
        std::fs::read_to_string(retained).unwrap(),
        "old canonical text"
    );
    assert!(source.exists());
    assert!(replacement.exists());
    let remaining: u32 = h
        .store
        .transaction(|tx| {
            Ok(
                tx.query_row("SELECT count(*) FROM knowledge_managed_files", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(remaining, 1);
    assert_eq!(
        std::fs::read_dir(h.temp_dir.path().join("knowledge/documents"))
            .unwrap()
            .count(),
        1
    );
}
