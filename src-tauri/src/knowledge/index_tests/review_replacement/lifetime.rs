use super::flags::{has_certificate, operation_key};
use super::*;
use crate::knowledge::index::delete_completion as completed;

async fn wait_running(h: &KnowledgeHarness, operation: Uuid) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if h.h.store.get_job(operation).unwrap().unwrap().status == JobStatus::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn knowledge_review_terminal_reap_waiter_retains_proof_until_real_handler_join() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let serial = h.service.core.serial.lock().await;
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    wait_running(&h, job.id).await;
    h.service
        .core
        .rebuild_generation(source.clone(), Arc::new(()))
        .await
        .unwrap();
    let before =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap();
    assert!(before.acknowledged && !before.consumed);
    assert!(!generation_path(&h, &source).exists());
    // This is the real scheduler timeout: its handler still owns its context
    // and waits on the serial owner. terminal_tx receives awaiting_reap=true.
    assert!(h.h.jobs.shutdown().await.is_err());
    assert_eq!(
        h.h.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    h.h.store.transaction(|tx| {
        tx.execute_batch(&format!("CREATE TRIGGER forbid_unjoined_collection BEFORE DELETE ON intelligence_meta \
            WHEN OLD.key='{}' BEGIN SELECT RAISE(ABORT,'unjoined handler still owns completion'); END;", operation_key(job.id)))?;
        Ok(())
    }).unwrap();
    let continuation = h.service.core.resume_completions(Arc::new(())).await;
    let retained = has_certificate(&h, job.id);
    let unconsumed =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap()
            .consumed
            == false;
    drop(serial);
    // The second observer joins the actual worker before startup/cleanup may
    // settle its Interrupted history. The injected GC failure stays atomic.
    h.h.jobs.shutdown().await.unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER forbid_unjoined_collection")?;
            Ok(())
        })
        .unwrap();
    h.h.jobs.reconcile().unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!has_certificate(&h, job.id));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
    assert!(
        continuation.is_ok(),
        "a terminal Interrupted row cannot authorize collection before the actual waiting handler joins"
    );
    assert!(retained && unconsumed);
}
