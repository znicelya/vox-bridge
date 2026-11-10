use super::*;
use crate::storage::LocalStore;
use fs2::FileExt;
use std::sync::atomic::AtomicUsize;

fn process(dir: &std::path::Path) -> Arc<std::fs::File> {
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.join("owned-sql-process.lock"))
        .unwrap();
    file.try_lock_exclusive().unwrap();
    Arc::new(file)
}

// Dropping the async observer must not detach the issued SQL, free its
// process owner, or let another operation replace the retained join handle.
#[tokio::test]
async fn observer_loss_keeps_actual_sql_owned_and_rejects_replacement_until_join() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let sql = Arc::new(SqlSlot::default());
    let owner = process(dir.path());
    let weak = Arc::downgrade(&owner);
    let (entered, waiting) = oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    let observer = tokio::spawn({
        let sql = sql.clone();
        let store = store.clone();
        async move {
            sql.run(owner, move || store.transaction(|tx| {
                entered.send(()).unwrap();
                held.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
                tx.execute_batch("CREATE TABLE actual_owned_sql(value INTEGER); INSERT INTO actual_owned_sql VALUES(7);")?;
                Ok(7)
            })).await
        }
    });
    waiting.await.unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let calls = Arc::new(AtomicUsize::new(0));
    let refused = sql
        .run((), {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await
        .is_err();
    let retained = weak.upgrade().is_some();
    release.send(()).unwrap();
    let drained = sql.drain().await;
    assert!(refused && retained);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        drained.is_err(),
        "lost result must fail closed after the real SQL join"
    );
    assert!(weak.upgrade().is_none());
    assert!(sql.task.lock().await.is_none());
    assert_eq!(
        store
            .transaction(|tx| Ok(tx.query_row(
                "SELECT value FROM actual_owned_sql",
                [],
                |row| row.get::<_, i64>(0)
            )?))
            .unwrap(),
        7
    );
    assert!(sql.run((), || Ok(())).await.is_err());
}

#[tokio::test]
async fn actual_sql_panic_is_joined_and_cannot_publish_normal_completion_or_rearm() {
    let dir = tempfile::tempdir().unwrap();
    let sql = SqlSlot::default();
    let owner = process(dir.path());
    let weak = Arc::downgrade(&owner);
    let result = sql
        .run::<(), _>(owner, || panic!("injected actual SQL panic"))
        .await;
    assert!(result.is_err());
    assert!(sql.drain().await.is_err());
    assert!(weak.upgrade().is_none());
    assert!(sql.task.lock().await.is_none());
    assert!(sql.run((), || Ok(())).await.is_err());
}
