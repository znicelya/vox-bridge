use super::{ConfigIo, ConfigStore, ConfigTransaction, ProviderConfig, SecretStore};
use crate::{
    config::secret_store::KeyBackend,
    error::{AppError, AppResult},
};
use fs2::FileExt;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

fn provider(id: Uuid, name: &str) -> ProviderConfig {
    ProviderConfig::new(
        id,
        "volcengine",
        "https://api.example.com",
        None,
        None,
        vec!["zh".into(), "en".into()],
        None,
    )
    .unwrap()
    .with_display_name(name.into())
    .unwrap()
}

fn test_io(data_dir: &Path, gate: Arc<Semaphore>, deadline: Duration) -> ConfigIo {
    let secrets = Arc::new(SecretStore::for_tests(data_dir).unwrap());
    ConfigIo::with_runtime_for_tests(ConfigStore::new(data_dir, secrets).unwrap(), gate, deadline)
}

fn assert_transaction_interface(_transaction: &mut ConfigTransaction<'_>) {}

#[derive(Default)]
struct Latch {
    released: Mutex<bool>,
    changed: Condvar,
}

impl Latch {
    fn wait(&self) {
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.changed.wait(released).unwrap();
        }
    }

    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct LatchReleaseGuard {
    latch: Arc<Latch>,
}

impl LatchReleaseGuard {
    fn new(latch: Arc<Latch>) -> Self {
        Self { latch }
    }

    fn release(&self) {
        self.latch.release();
    }
}

impl Drop for LatchReleaseGuard {
    fn drop(&mut self) {
        self.latch.release();
    }
}

#[test]
fn latch_release_guard_releases_waiters_during_unwind() {
    let latch = Arc::new(Latch::default());
    let waiting = Arc::clone(&latch);
    let (released_tx, released_rx) = std::sync::mpsc::channel();
    let waiter = std::thread::spawn(move || {
        waiting.wait();
        released_tx.send(()).unwrap();
    });

    let unwind = std::panic::catch_unwind({
        let latch = Arc::clone(&latch);
        move || {
            let _release = LatchReleaseGuard::new(latch);
            panic!("injected assertion failure");
        }
    });
    let released = released_rx.recv_timeout(Duration::from_secs(1));
    latch.release();
    waiter.join().unwrap();

    assert!(unwind.is_err());
    assert!(released.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn timed_out_native_jobs_retain_both_capacity_slots_until_they_return() {
    let root = tempfile::tempdir().unwrap();
    let gate = Arc::new(Semaphore::new(2));
    let first_io = test_io(
        &root.path().join("first"),
        Arc::clone(&gate),
        Duration::from_millis(80),
    );
    let second_io = test_io(
        &root.path().join("second"),
        Arc::clone(&gate),
        Duration::from_millis(80),
    );
    let third_io = test_io(
        &root.path().join("third"),
        Arc::clone(&gate),
        Duration::from_millis(80),
    );
    let entered = Arc::new(AtomicUsize::new(0));
    let latch = Arc::new(Latch::default());
    let release_latch = LatchReleaseGuard::new(Arc::clone(&latch));

    let mut calls = Vec::new();
    for io in [first_io, second_io] {
        let entered = Arc::clone(&entered);
        let latch = Arc::clone(&latch);
        calls.push(tokio::spawn(async move {
            io.run(move |_| {
                entered.fetch_add(1, Ordering::SeqCst);
                latch.wait();
                Ok(())
            })
            .await
        }));
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while entered.load(Ordering::SeqCst) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    for call in calls {
        assert!(call.await.unwrap().is_err(), "caller budget must expire");
    }
    assert_eq!(gate.available_permits(), 0);

    let third_started = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&third_started);
    let result = third_io
        .run(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .await;
    assert!(result.is_err());
    assert_eq!(third_started.load(Ordering::SeqCst), 0);

    release_latch.release();
    tokio::time::timeout(Duration::from_secs(2), async {
        while gate.available_permits() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    third_io.run(|_| Ok(())).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_a_job_queued_before_transaction_entry_never_mutates_disk() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Arc::new(Semaphore::new(2));
    let io = test_io(dir.path(), Arc::clone(&gate), Duration::from_secs(2));
    let entered = Arc::new(AtomicUsize::new(0));
    let latch = Arc::new(Latch::default());
    let release_latch = LatchReleaseGuard::new(Arc::clone(&latch));

    let first_io = io.clone();
    let first_entered = Arc::clone(&entered);
    let first_latch = Arc::clone(&latch);
    let first = tokio::spawn(async move {
        first_io
            .run(move |_| {
                first_entered.fetch_add(1, Ordering::SeqCst);
                first_latch.wait();
                Ok(())
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while entered.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let cancelled_mutations = Arc::new(AtomicUsize::new(0));
    let mutations = Arc::clone(&cancelled_mutations);
    let cancelled_io = io.clone();
    let id = Uuid::new_v4();
    let queued = tokio::spawn(async move {
        cancelled_io
            .run(move |transaction| {
                mutations.fetch_add(1, Ordering::SeqCst);
                transaction.save_provider_config(provider(id, "cancelled"))
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while gate.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    assert_eq!(gate.available_permits(), 0);
    release_latch.release();
    first.await.unwrap().unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        while gate.available_permits() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(cancelled_mutations.load(Ordering::SeqCst), 0);
    assert!(
        io.run(move |transaction| transaction.load_provider_config(id))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cross_instance_transactions_are_serialized_for_the_whole_closure() {
    let dir = tempfile::tempdir().unwrap();
    let first = test_io(
        dir.path(),
        Arc::new(Semaphore::new(2)),
        Duration::from_secs(2),
    );
    let second = test_io(
        dir.path(),
        Arc::new(Semaphore::new(2)),
        Duration::from_secs(2),
    );
    let entered = Arc::new(AtomicUsize::new(0));
    let latch = Arc::new(Latch::default());
    let release_latch = LatchReleaseGuard::new(Arc::clone(&latch));

    let count = Arc::clone(&entered);
    let hold = Arc::clone(&latch);
    let first_call = tokio::spawn(async move {
        first
            .run(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                hold.wait();
                Ok(())
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while entered.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let count = Arc::clone(&entered);
    let second_call = tokio::spawn(async move {
        second
            .run(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(entered.load(Ordering::SeqCst), 1);
    release_latch.release();
    first_call.await.unwrap().unwrap();
    second_call.await.unwrap().unwrap();
    assert_eq!(entered.load(Ordering::SeqCst), 2);
}

struct LockOrderKeyring {
    config_lock: PathBuf,
    secret_lock: PathBuf,
}

impl KeyBackend for LockOrderKeyring {
    fn load_or_create(&self, _dek_lock: &Path) -> AppResult<Vec<u8>> {
        for lock_path in [&self.config_lock, &self.secret_lock] {
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(lock_path)
                .map_err(|error| AppError::Configuration(error.to_string()))?;
            if lock.try_lock_exclusive().is_ok() {
                return Err(AppError::Internal(format!(
                    "{} was not retained before DEK access",
                    lock_path.display()
                )));
            }
        }
        Ok(vec![9; 32])
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transaction_secret_methods_hold_config_then_secret_before_dek_without_recursion() {
    let dir = tempfile::tempdir().unwrap();
    let backend: Arc<dyn KeyBackend> = Arc::new(LockOrderKeyring {
        config_lock: dir.path().join("provider-configs.lock"),
        secret_lock: dir.path().join("secrets.lock"),
    });
    let secrets = Arc::new(SecretStore::with_test_backend(dir.path(), backend).unwrap());
    let io = ConfigIo::with_runtime_for_tests(
        ConfigStore::new(dir.path(), secrets).unwrap(),
        Arc::new(Semaphore::new(2)),
        Duration::from_secs(10),
    );

    io.run(|transaction| {
        transaction.secret_set("provider", "api-key", b"private-fixture-value")?;
        assert!(transaction.secret_contains("provider", "api-key")?);
        assert_eq!(
            transaction.secret_get("provider", "api-key")?,
            Some(b"private-fixture-value".to_vec())
        );
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_snapshot_restore_preserves_unrelated_entries_and_absence() {
    let dir = tempfile::tempdir().unwrap();
    let io = test_io(
        dir.path(),
        Arc::new(Semaphore::new(2)),
        Duration::from_secs(10),
    );
    let expected_data_dir = dir.path().to_path_buf();
    io.run(move |transaction| {
        assert_transaction_interface(transaction);
        assert_eq!(transaction.data_dir(), expected_data_dir);
        let config_id = Uuid::new_v4();
        transaction.save_provider_config(provider(config_id, "transaction"))?;
        assert_eq!(transaction.list_provider_configs()?.len(), 1);
        transaction.delete_provider_config(config_id)?;
        assert!(transaction.list_provider_configs()?.is_empty());

        transaction.secret_set("provider", "primary", b"old-private-value")?;
        transaction.secret_set("provider", "unrelated", b"keep-private-value")?;
        let old = transaction.snapshot_secret("provider", "primary")?;
        let absent = transaction.snapshot_secret("provider", "new-key")?;
        transaction.secret_set("provider", "primary", b"replacement-private-value")?;
        transaction.secret_set("provider", "new-key", b"temporary-private-value")?;
        transaction.restore_secret("provider", "primary", old)?;
        transaction.restore_secret("provider", "new-key", absent)?;
        assert_eq!(
            transaction.secret_get("provider", "primary")?,
            Some(b"old-private-value".to_vec())
        );
        assert_eq!(
            transaction.secret_get("provider", "unrelated")?,
            Some(b"keep-private-value".to_vec())
        );
        assert!(!transaction.secret_contains("provider", "new-key")?);
        transaction.secret_delete("provider", "new-key")?;
        Ok(())
    })
    .await
    .unwrap();

    let serialized = std::fs::read_to_string(dir.path().join("secrets.json")).unwrap();
    for plaintext in [
        "old-private-value",
        "keep-private-value",
        "replacement-private-value",
        "temporary-private-value",
    ] {
        assert!(!serialized.contains(plaintext));
    }
}
