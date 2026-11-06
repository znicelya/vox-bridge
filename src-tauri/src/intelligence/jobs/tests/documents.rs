use super::*;
use crate::knowledge::{
    documents::DocumentService,
    parser::DocumentParser,
    types::{DocumentKind, DocumentStatus, TextBlock, TextLocation},
};
use std::path::PathBuf;
struct Reap {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl DocumentParser for Reap {
    async fn parse(
        &self,
        path: PathBuf,
        _: DocumentKind,
        _: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
        self.entered.notify_one();
        self.release.notified().await;
        let text = std::fs::read_to_string(path).map_err(|_| IntelligenceErrorCode::ParseFailed)?;
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
#[tokio::test]
async fn documents_interrupt_publishes_actual_changed_job_beyond_first_history_page() {
    let mut h = IntelligenceHarness::new();
    Arc::get_mut(&mut h.jobs).unwrap().shutdown_timeout = Duration::from_millis(20);
    h.store
        .transaction(|tx| {
            for _ in 0..250 {
                let object = Uuid::new_v4();
                let job = store::admit_job(
                    tx,
                    JobDraft {
                        id: Uuid::new_v4(),
                        kind: JobKind::Import,
                        session_id: None,
                        object_id: Some(object),
                        admission: JobAdmission::Manual {
                            exclusion_key: format!("document:{object}"),
                        },
                        payload: payload(vec![]),
                        initial_failure: None,
                    },
                )?
                .view()
                .clone();
                tx.execute(
                    "UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",
                    [job.id.to_string()],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let docs = DocumentService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        Arc::new(Reap {
            entered: entered.clone(),
            release: release.clone(),
        }),
        h.temp_dir.path().into(),
    );
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let mut events = h.jobs.subscribe();
    let path = h.temp_dir.path().join("source.txt");
    std::fs::write(&path, "preserved until actual reap").unwrap();
    let job = docs.import(path).await.unwrap();
    entered.notified().await;
    let managed = docs
        .get(job.object_id.unwrap())
        .unwrap()
        .unwrap()
        .managed_path
        .unwrap();
    while events.try_recv().is_ok() {}
    let outcome = h.jobs.shutdown().await;
    let mut interrupted = false;
    while let Ok(event) = events.try_recv() {
        if let IntelligenceEvent::JobChanged { job: changed } = event {
            interrupted |= changed.id == job.id && changed.status == JobStatus::Interrupted;
        }
    }
    let retained = managed.exists();
    let unavailable = h.jobs.parser_gate().try_acquire_owned().is_err();
    release.notify_one();
    h.jobs.shutdown().await.unwrap();
    assert!(matches!(
        outcome,
        Err(AppError::Intelligence(IntelligenceErrorCode::Interrupted))
    ));
    assert!(
        interrupted,
        "interruption events must identify this transitioned job, not the first 200 history rows"
    );
    assert!(retained && unavailable);
    assert_eq!(
        h.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    assert_eq!(
        docs.get(job.object_id.unwrap())
            .unwrap()
            .unwrap()
            .summary
            .status,
        DocumentStatus::Failed
    );
    assert!(!managed.exists());
}
