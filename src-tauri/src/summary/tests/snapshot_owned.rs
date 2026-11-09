use crate::{
    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode},
    storage::LocalStore,
};
use std::{fs::File, sync::Arc, time::Duration};
struct Owner {
    _file: File,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
fn owner(dir: &std::path::Path, slots: &Arc<tokio::sync::Semaphore>) -> OperationLease {
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.join("process-owner.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    Arc::new(Owner {
        _file: file,
        _slot: slots.clone().try_acquire_owned().unwrap(),
    })
}
#[tokio::test]
async fn summary_snapshot_dropped_observer_retains_entered_sql_owner_and_slot() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let owner = owner(dir.path(), &slots);
    let weak = Arc::downgrade(&owner);
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let (_cancel, rx) = tokio::sync::watch::channel(false);
    let work = tokio::spawn(async move {
        store
            .with_summary_snapshot_owned(owner, rx, move |tx, scope| {
                scope.check()?;
                tx.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?;
                let _ = entered.send(());
                let _ = gate.recv();
                Ok(())
            })
            .await
    });
    let observed = tokio::time::timeout(Duration::from_secs(2), entry).await;
    if !matches!(observed, Ok(Ok(()))) {
        drop(release);
        let _ = work.await;
        assert!(false, "owned SQL fixture never entered");
        return;
    }
    work.abort();
    assert!(work.await.unwrap_err().is_cancelled());
    let held = weak.upgrade().is_some();
    let retained = slots.available_permits();
    drop(release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while slots.available_permits() != 16 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(held);
    assert_eq!(retained, 15);
    assert_eq!(slots.available_permits(), 16);
}
#[tokio::test]
async fn summary_snapshot_five_second_observation_timeout_keeps_actual_entered_sql_owned() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let owner = owner(dir.path(), &slots);
    let weak = Arc::downgrade(&owner);
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let (_cancel, rx) = tokio::sync::watch::channel(false);
    let work = tokio::spawn(async move {
        store
            .with_summary_snapshot_owned(owner, rx, move |tx, scope| {
                scope.check()?;
                tx.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?;
                let _ = entered.send(());
                let _ = gate.recv();
                Ok(())
            })
            .await
    });
    let observed = tokio::time::timeout(Duration::from_secs(2), entry).await;
    if !matches!(observed, Ok(Ok(()))) {
        drop(release);
        let _ = work.await;
        assert!(false, "owned SQL fixture never entered");
        return;
    }
    let result = tokio::time::timeout(Duration::from_secs(6), work)
        .await
        .unwrap()
        .unwrap();
    let held = weak.upgrade().is_some();
    let retained = slots.available_permits();
    drop(release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while slots.available_permits() != 16 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::Timeout
        ))
    ));
    assert!(held);
    assert_eq!(retained, 15);
    assert_eq!(slots.available_permits(), 16);
}
