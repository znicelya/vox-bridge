use super::*;
async fn import(h: &KnowledgeHarness, name: &str) -> JobView {
    let path = h.h.temp_dir.path().join(name);
    std::fs::write(&path, "额定电压24V").unwrap();
    let documents = super::super::documents::DocumentService::new(
        h.h.store.clone(),
        h.h.config.clone(),
        h.h.jobs.clone(),
        Arc::new(FixtureParser),
        h.h.temp_dir.path().to_path_buf(),
    );
    let job = documents.import(path).await.unwrap();
    let parent = h.wait(job.id).await;
    assert_eq!(parent.status, JobStatus::Succeeded);
    parent
}
async fn wait_state(h: &KnowledgeHarness, id: Uuid, want: &str) -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let status: String =
                h.h.store
                    .transaction(|tx| {
                        Ok(tx.query_row(
                            "SELECT status FROM documents WHERE id=?1",
                            [id.to_string()],
                            |r| r.get(0),
                        )?)
                    })
                    .unwrap();
            if status == want {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok()
}
#[tokio::test]
async fn knowledge_import_continues_only_committed_success_into_reindex() {
    let h = KnowledgeHarness::with_auto(true).await;
    let parent = import(&h, "pump.md").await;
    let id = parent.object_id.unwrap();
    let ready = wait_state(&h, id, "ready").await;
    h.close().await;
    assert!(
        ready,
        "committed successful Import did not continue into indexing"
    );
    let count: u32 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM intelligence_jobs WHERE \
                    kind='reindex' AND object_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn knowledge_import_without_selected_embedding_stays_parsed_with_deferred_reason() {
    let h = KnowledgeHarness::with_auto(true).await;
    let mut settings = h.h.config.settings().await.unwrap();
    settings.embedding_profile_id = None;
    h.h.config.save_settings(settings).await.unwrap();
    let parent = import(&h, "pump.md").await;
    let id = parent.object_id.unwrap();
    let deferred = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let exists: bool =
                h.h.store
                    .transaction(|tx| {
                        Ok(tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_repairs WHERE document_id=?1 AND \
                    reason='auto_index_deferred')",
                            [id.to_string()],
                            |r| r.get(0),
                        )?)
                    })
                    .unwrap();
            if exists {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    h.close().await;
    assert!(deferred);
    let status: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT status FROM documents WHERE id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(status, "parsed");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn knowledge_manual_reindex_observer_loss_still_wakes_committed_admission() {
    let h = KnowledgeHarness::new().await;
    let first = h.import_text("first", "24V");
    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let id = h.import_text("second", "48V");
    let (entered, wait) = tokio::sync::oneshot::channel();
    let (release, receive) = std::sync::mpsc::channel();
    h.service
        .at_checkpoint(CommitPoint::AdmissionCallback, move || {
            let _ = entered.send(());
            let _ = receive.recv_timeout(std::time::Duration::from_secs(5));
        });
    let observer = {
        let service = h.service.clone();
        tokio::spawn(async move { service.reindex_owned(id, Arc::new(())).await })
    };
    wait.await.unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    let ready = wait_state(&h, id, "ready").await;
    h.close().await;
    assert!(
        ready,
        "entered configuration admission committed after observer loss but did not wake the scheduler"
    );
}
