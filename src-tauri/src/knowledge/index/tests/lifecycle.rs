use super::super::IndexOwner;
use crate::storage::LocalStore;
use fs2::FileExt;
use std::sync::Arc;
use uuid::Uuid;

struct Release(Option<std::sync::mpsc::SyncSender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
fn block_command(owner: &IndexOwner) -> (std::sync::mpsc::Receiver<()>, Release) {
    let (entered, wait) = std::sync::mpsc::sync_channel(1);
    let (release, receive) = std::sync::mpsc::sync_channel(1);
    *owner.hooks().command.lock().unwrap() = Some(super::super::lifecycle::Gate {
        entered,
        release: receive,
    });
    (wait, Release(Some(release)))
}

fn lock(path: &std::path::Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap()
}
#[tokio::test]
async fn index_lifecycle_owned_idle_pool_holds_process_lock_until_actual_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let file = Arc::new(lock(&dir.path().join("app.lock")));
    file.try_lock_exclusive().unwrap();
    let probe = lock(&dir.path().join("app.lock"));
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    assert!(!dir.path().join("indexes").exists(), "constructor is inert");
    let generation = owner.start_owned(file.clone()).await.unwrap();
    drop(file);
    assert!(
        probe.try_lock_exclusive().is_err(),
        "idle owner independently retains app.lock"
    );
    let again = owner.start_owned(Arc::new(())).await.unwrap();
    assert_eq!(
        again, generation,
        "concurrent/repeated readiness belongs to one generation"
    );
    owner.shutdown().await.unwrap();
    probe.try_lock_exclusive().unwrap();
    FileExt::unlock(&probe).unwrap();
    let next = owner.start_owned(Arc::new(())).await.unwrap();
    assert_ne!(next, generation);
    owner.shutdown().await.unwrap();
}
#[test]
fn index_lifecycle_final_facade_drop_joins_without_tokio_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let file = Arc::new(lock(&dir.path().join("app.lock")));
    file.try_lock_exclusive().unwrap();
    let probe = lock(&dir.path().join("app.lock"));
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(owner.start_owned(file.clone())).unwrap();
    drop(file);
    drop(runtime);
    assert!(
        probe.try_lock_exclusive().is_err(),
        "runtime teardown is not owner completion"
    );
    drop(owner);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if probe.try_lock_exclusive().is_ok() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "final facade did not release after actual joins"
        );
        std::thread::yield_now();
    }
    FileExt::unlock(&probe).unwrap();
}

#[tokio::test]
async fn index_lifecycle_envelope_outlives_observer_and_retains_only_request_token() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let lease = Arc::new(());
    let weak = Arc::downgrade(&lease);
    let ticket = owner
        .create(super::persistence::generation(), lease.clone())
        .unwrap();
    let (entered, release) = block_command(&owner);
    let pending = owner.enqueue_open(&ticket).unwrap();
    entered
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    ticket.cancel();
    drop(ticket);
    drop(pending);
    drop(lease);
    assert!(
        weak.upgrade().is_some(),
        "entered work owns the issued token after observer loss"
    );
    drop(release);
    owner.shutdown().await.unwrap();
    assert!(
        weak.upgrade().is_none(),
        "cached engine never retains a job child token"
    );
}
#[tokio::test]
async fn index_lifecycle_queue_is_sixteen_and_queued_cancellation_skips_before_entry() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let ticket = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    let (entered, release) = block_command(&owner);
    let first = owner.enqueue_open(&ticket).unwrap();
    entered
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let queued = (0..16)
        .map(|_| owner.enqueue_open(&ticket).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        owner.enqueue_open(&ticket),
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Busy
        ))
    ));
    ticket.cancel();
    drop(release);
    assert_eq!(
        first.observe().await.unwrap(),
        0,
        "cancellation cannot abandon entered synchronous work"
    );
    for pending in queued {
        assert!(matches!(
            pending.observe().await,
            Err(crate::error::AppError::Intelligence(
                crate::intelligence::types::IntelligenceErrorCode::Cancelled
            ))
        ));
    }
    drop(ticket);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_typed_work_roundtrip_publishes_and_reopens_after_owned_join() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let owner = IndexOwner::new(dir.path().join("indexes"), store);
    owner.start_owned(Arc::new(())).await.unwrap();
    let generation = super::persistence::generation();
    let ticket = owner.create(generation.clone(), Arc::new(())).unwrap();
    owner
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    assert_eq!(
        owner
            .add(
                &ticket,
                vec![(vec![1.0; 128], super::persistence::metadata())]
            )
            .await
            .unwrap(),
        vec![0]
    );
    let receipt = owner.persist(&ticket).await.unwrap();
    let ack = owner.publish(&ticket, &receipt).await.unwrap();
    assert!(matches!(
        owner.open_existing(generation.clone(), Arc::new(())),
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Busy
        ))
    ));
    owner.acknowledge(&ticket, ack).await.unwrap();
    drop(ticket);
    owner.shutdown().await.unwrap();
    owner.start_owned(Arc::new(())).await.unwrap();
    let ticket = owner.open_existing(generation, Arc::new(())).unwrap();
    assert_eq!(
        owner
            .enqueue_open(&ticket)
            .unwrap()
            .observe()
            .await
            .unwrap(),
        1
    );
    let receipt = owner.persist(&ticket).await.unwrap();
    owner
        .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
        .await
        .unwrap();
    drop(ticket);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_typed_commands_bound_scan_and_preserve_append_ids() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let ticket = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let axis = |index| {
        let mut vector = vec![0.0; 128];
        vector[index] = 1.0;
        vector
    };
    assert_eq!(
        owner
            .add(
                &ticket,
                vec![
                    (axis(0), super::persistence::metadata()),
                    (axis(1), super::persistence::metadata()),
                    (axis(2), super::persistence::metadata()),
                ],
            )
            .await
            .unwrap(),
        vec![0, 1, 2]
    );
    let first_page = owner.scan(&ticket, 0, 2).await.unwrap();
    assert_eq!(first_page.len(), 2);
    assert_eq!(
        first_page.iter().map(|row| row.0).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(owner.delete(&ticket, vec![1]).await.unwrap(), vec![true]);
    assert_eq!(
        owner
            .replace(&ticket, 0, axis(3), super::persistence::metadata(),)
            .await
            .unwrap(),
        3
    );
    let page = owner.scan(&ticket, 0, 4).await.unwrap();
    assert_eq!(
        page.iter()
            .map(|row| (row.0, row.1.is_some()))
            .collect::<Vec<_>>(),
        [(0, false), (1, false), (2, true), (3, true)]
    );
    let hits = owner.search(&ticket, axis(2), 4).await.unwrap();
    assert_eq!(hits.first().map(|hit| hit.0), Some(2));
    assert!(hits[0].1 > hits[1].1);
    drop(ticket);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_maintenance_copies_full_precision_vectors_without_source_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let owner = IndexOwner::new(dir.path().join("indexes"), store);
    owner.start_owned(Arc::new(())).await.unwrap();
    let source_generation = super::persistence::generation();
    let source = owner
        .create(source_generation.clone(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&source)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let mut first = vec![0.0; 128];
    first[7] = -1.234_567_9;
    let mut last = vec![0.0; 128];
    last[127] = 9.876_543;
    let metadata = vec![
        super::persistence::metadata(),
        super::persistence::metadata(),
    ];
    owner
        .add(
            &source,
            vec![
                (first.clone(), metadata[0].clone()),
                (last.clone(), metadata[1].clone()),
            ],
        )
        .await
        .unwrap();
    owner.persist(&source).await.unwrap();
    let maintenance = owner.begin_maintenance(&source).await.unwrap();
    assert!(matches!(
        owner
            .add(
                &source,
                vec![(last.clone(), super::persistence::metadata())]
            )
            .await,
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Busy
        ))
    ));
    let copied = owner.copy_vectors(&maintenance, 0, 200).await.unwrap();
    assert_eq!(copied.len(), 2);
    assert_eq!(copied[0].0, 0);
    assert_eq!(copied[1].0, 1);
    assert_eq!(
        copied[0]
            .1
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        first
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        copied[1]
            .1
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        last.iter().map(|value| value.to_bits()).collect::<Vec<_>>()
    );

    let mut staging_generation = source_generation;
    staging_generation.generation = Uuid::new_v4();
    let staging = owner.create(staging_generation, Arc::new(())).unwrap();
    owner
        .enqueue_open(&staging)
        .unwrap()
        .observe()
        .await
        .unwrap();
    assert_eq!(
        owner
            .add(
                &staging,
                copied
                    .into_iter()
                    .zip(metadata.into_iter())
                    .map(|((_, vector), metadata)| (vector, metadata))
                    .collect(),
            )
            .await
            .unwrap(),
        vec![0, 1]
    );
    owner.end_maintenance(maintenance).await.unwrap();
    drop(source);
    drop(staging);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_dropped_maintenance_ticket_releases_source_fence() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let source = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&source)
        .unwrap()
        .observe()
        .await
        .unwrap();
    owner
        .add(
            &source,
            vec![(vec![1.0; 128], super::persistence::metadata())],
        )
        .await
        .unwrap();
    owner.persist(&source).await.unwrap();
    let maintenance = owner.begin_maintenance(&source).await.unwrap();
    drop(maintenance);
    let replacement = owner.begin_maintenance(&source).await.unwrap();
    owner.end_maintenance(replacement).await.unwrap();
    drop(source);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_clean_lru_makes_room_before_third_generation_opens() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let mut clean = Vec::new();
    for _ in 0..2 {
        let ticket = owner
            .create(super::persistence::generation(), Arc::new(()))
            .unwrap();
        owner
            .enqueue_open(&ticket)
            .unwrap()
            .observe()
            .await
            .unwrap();
        let receipt = owner.persist(&ticket).await.unwrap();
        owner
            .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
            .await
            .unwrap();
        clean.push(ticket);
    }
    let third = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner.enqueue_open(&third).unwrap().observe().await.unwrap();
    drop(clean);
    drop(third);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_dirty_capacity_returns_busy_without_a_third_engine() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let first = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    let second = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner.enqueue_open(&first).unwrap().observe().await.unwrap();
    owner
        .enqueue_open(&second)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let third = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    assert!(matches!(
        owner.enqueue_open(&third).unwrap().observe().await,
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Busy
        ))
    ));
    drop(first);
    drop(second);
    drop(third);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_failed_close_retains_owner_then_later_observer_retries_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let process = Arc::new(lock(&dir.path().join("app.lock")));
    process.try_lock_exclusive().unwrap();
    let probe = lock(&dir.path().join("app.lock"));
    let owner = IndexOwner::new(dir.path().join("indexes"), store.clone());
    owner.start_owned(process.clone()).await.unwrap();
    drop(process);
    let ticket = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    owner
        .add(
            &ticket,
            vec![(vec![1.0; 128], super::persistence::metadata())],
        )
        .await
        .unwrap();
    let receipt = owner.persist(&ticket).await.unwrap();
    owner
        .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
        .await
        .unwrap();
    drop(ticket);
    store
        .transaction(|tx| {
            tx.execute_batch(
                "CREATE TRIGGER fail_owner_close BEFORE \
                    UPDATE ON knowledge_index_generation_seals \
                    WHEN NEW.dirty=1 AND \
                    json_extract(NEW.intent_json,\
                    '$.operation')='close' BEGIN SELECT \
                    RAISE(ABORT,'close failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        owner.shutdown().await,
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Storage
        ))
    ));
    assert!(
        probe.try_lock_exclusive().is_err(),
        "normal failed close retains real owner/resources for retry"
    );
    assert!(owner.start_owned(Arc::new(())).await.is_err());
    store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER fail_owner_close")?;
            Ok(())
        })
        .unwrap();
    owner.shutdown().await.unwrap();
    probe.try_lock_exclusive().unwrap();
    FileExt::unlock(&probe).unwrap();
}

#[tokio::test]
async fn index_lifecycle_join_spawn_failure_forces_failed_close_without_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let process = Arc::new(lock(&dir.path().join("app.lock")));
    process.try_lock_exclusive().unwrap();
    let probe = lock(&dir.path().join("app.lock"));
    let owner = IndexOwner::new(dir.path().join("indexes"), store.clone());
    owner.start_owned(process.clone()).await.unwrap();
    drop(process);
    let ticket = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    owner
        .add(
            &ticket,
            vec![(vec![1.0; 128], super::persistence::metadata())],
        )
        .await
        .unwrap();
    let receipt = owner.persist(&ticket).await.unwrap();
    owner
        .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
        .await
        .unwrap();
    drop(ticket);
    store
        .transaction(|tx| {
            tx.execute_batch(
                "CREATE TRIGGER fail_join_spawn_close BEFORE \
                    UPDATE ON knowledge_index_generation_seals \
                    WHEN NEW.dirty=1 AND \
                    json_extract(NEW.intent_json,\
                    '$.operation')='close' BEGIN SELECT \
                    RAISE(ABORT,'close failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    owner
        .hooks()
        .fail_join_spawn
        .store(true, std::sync::atomic::Ordering::Release);
    assert!(matches!(
        owner.shutdown().await,
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Storage
        ))
    ));
    probe.try_lock_exclusive().unwrap();
    FileExt::unlock(&probe).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn index_lifecycle_stale_stop_return_does_not_clear_new_generation_transition() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    let first = owner.start_owned(Arc::new(())).await.unwrap();
    owner.set_transition_for_test(Uuid::new_v4(), first);
    let (entered, release) = owner.block_stop_return_for_test();
    let stop_owner = owner.clone();
    let stop = tokio::spawn(async move { stop_owner.shutdown().await });
    entered
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    let second = owner.start_owned(Arc::new(())).await.unwrap();
    assert_ne!(second, first);
    owner.set_transition_for_test(Uuid::new_v4(), second);
    release.send(()).unwrap();
    stop.await.unwrap().unwrap();

    let epochs = owner.transition_epochs_for_test();
    assert!(epochs.contains(&second));
    assert!(!epochs.contains(&first));
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn index_lifecycle_stale_receipt_cannot_publish_after_owner_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let owner = IndexOwner::new(dir.path().join("indexes"), store.clone());
    owner.start_owned(Arc::new(())).await.unwrap();
    let generation = super::persistence::generation();
    let ticket = owner.create(generation, Arc::new(())).unwrap();
    owner
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    owner
        .add(
            &ticket,
            vec![(vec![1.0; 128], super::persistence::metadata())],
        )
        .await
        .unwrap();
    let receipt = owner.persist(&ticket).await.unwrap();
    owner.shutdown().await.unwrap();
    assert!(matches!(
        owner.publish(&ticket, &receipt).await,
        Err(crate::error::AppError::Intelligence(
            crate::intelligence::types::IntelligenceErrorCode::Interrupted
        ))
    ));
    let row = store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT dirty,seal_json,intent_json FROM knowledge_index_generation_seals",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )?)
        })
        .unwrap();
    assert_eq!(row.0, 1);
    assert!(row.1.is_none());
    assert!(row.2.is_some());
}
#[tokio::test]
async fn index_lifecycle_dropped_begin_maintenance_observer_releases_entered_fence() {
    let dir = tempfile::tempdir().unwrap();
    let owner = IndexOwner::new(
        dir.path().join("indexes"),
        Arc::new(LocalStore::in_memory().unwrap()),
    );
    owner.start_owned(Arc::new(())).await.unwrap();
    let source = owner
        .create(super::persistence::generation(), Arc::new(()))
        .unwrap();
    owner
        .enqueue_open(&source)
        .unwrap()
        .observe()
        .await
        .unwrap();
    owner
        .add(
            &source,
            vec![(vec![1.; 128], super::persistence::metadata())],
        )
        .await
        .unwrap();
    owner.persist(&source).await.unwrap();
    let (entered, release) = block_command(&owner);
    let begin = {
        let owner = owner.clone();
        let source = source.clone();
        tokio::spawn(async move { owner.begin_maintenance(&source).await })
    };
    tokio::task::spawn_blocking(move || entered.recv_timeout(std::time::Duration::from_secs(2)))
        .await
        .unwrap()
        .unwrap();
    begin.abort();
    assert!(matches!(begin.await,
Err(error) if error.is_cancelled()));
    drop(release);
    let next = owner.begin_maintenance(&source).await;
    let released = next.is_ok();
    if let Ok(ticket) = next {
        owner.end_maintenance(ticket).await.unwrap();
    }
    owner.shutdown().await.unwrap();
    assert!(
        released,
        "lost begin observer stranded the source maintenance fence"
    );
}
