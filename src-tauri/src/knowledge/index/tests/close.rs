use super::{
    super::{
        owner::Runtime,
        preflight::generation_path,
        redb_header::{HeaderSnapshot, StorageState},
        repository::IndexRepository,
    },
    persistence::{generation, metadata},
};
use crate::storage::LocalStore;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn index_close_header_proves_controlled_close_and_rejects_failed_marker_or_slot() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let generation = generation();
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation.clone(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo, epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap();
        runtime.persist(&intent).unwrap();
        let path = generation_path(dir.path(), &generation)
            .unwrap()
            .join("metadata.db");
        let live =
            HeaderSnapshot::read(&path, StorageState::Live).expect("bounded valid live header");
        drop(runtime);
        let closed = HeaderSnapshot::read(&path, StorageState::Closed).unwrap();
        live.validates_close(&closed).unwrap();
        let clean = std::fs::read(&path).unwrap();
        for offset in [9, 12, 24, 31, 64, 192, 176, 319] {
            let mut corrupted = clean.clone();
            corrupted[offset] ^= if offset == 9 { 2 } else { 0x80 };
            std::fs::write(&path, &corrupted).unwrap();
            assert!(
                HeaderSnapshot::read(&path, StorageState::Closed).is_err(),
                "reject corruption at {offset}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), corrupted);
        }
    })
    .join()
    .unwrap();
}

// Launched only by the test below. exit() intentionally skips Rust destructors.
#[test]
fn index_close_live_crash_child() {
    let Some(data) = std::env::var_os("VOXBRIDGE_INDEX_CRASH_FIXTURE") else {
        return;
    };
    let data = std::path::PathBuf::from(data);
    let repo = Arc::new(IndexRepository::new(Arc::new(
        LocalStore::open(&data).unwrap(),
    )));
    let generation = generation();
    let epoch = Uuid::new_v4();
    let intent = repo.create(generation.clone(), epoch).unwrap();
    let mut runtime = Runtime::new(data.join("indexes"), repo.clone(), epoch).unwrap();
    runtime.open(&intent).unwrap();
    assert_eq!(
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap(),
        vec![0]
    );
    let receipt = runtime.persist(&intent).unwrap();
    runtime
        .acknowledge(&repo.publish(&receipt).unwrap())
        .unwrap();
    let descriptor = std::fs::File::create(data.join("fixture-generation.json")).unwrap();
    serde_json::to_writer(&descriptor, &generation).unwrap();
    descriptor.sync_all().unwrap();
    std::process::exit(0);
}
#[test]
fn index_close_exact_published_live_seal_recovers_after_process_exit_without_drop() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "knowledge::index::tests::close::index_close_live_crash_child",
            "--nocapture",
        ])
        .env("VOXBRIDGE_INDEX_CRASH_FIXTURE", dir.path())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("crash fixture exceeded deadline");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(status.success());
    let generation = serde_json::from_reader(
        std::fs::File::open(dir.path().join("fixture-generation.json")).unwrap(),
    )
    .unwrap();
    let repo = Arc::new(IndexRepository::new(Arc::new(
        LocalStore::open(dir.path()).unwrap(),
    )));
    let epoch = Uuid::new_v4();
    let intent = repo.open(generation, epoch).unwrap();
    let mut runtime = Runtime::new(dir.path().join("indexes"), repo, epoch).unwrap();
    assert_eq!(runtime.open(&intent).unwrap(), 1);
    let rows = runtime.scan(&intent, 0, 200).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, 0);
    assert!(rows[0].1.is_some());
}

#[test]
fn index_close_failed_postdrop_marker_or_sync_keeps_dirty_intent_and_no_closed_seal() {
    use super::super::owner::CloseFault;
    for fault in [CloseFault::Marker, CloseFault::Sync] {
        std::thread::spawn(move || {
            let dir=tempfile::tempdir().unwrap();let store=Arc::new(LocalStore::in_memory().unwrap());
            let repo=Arc::new(IndexRepository::new(store.clone()));let generation=generation();let epoch=Uuid::new_v4();
            let intent=repo.create(generation.clone(),epoch).unwrap();let mut runtime=Runtime::new(dir.path().to_owned(),repo.clone(),epoch).unwrap();
            runtime.open(&intent).unwrap();runtime.add(&intent,vec![(vec![1.0;128],metadata())]).unwrap();
            let receipt=runtime.persist(&intent).unwrap();runtime.acknowledge(&repo.publish(&receipt).unwrap()).unwrap();
            runtime.close_fault=Some(fault);
            assert!(runtime.close_generation(&generation).is_err(),"failed post-Drop proof cannot issue a close receipt");
            let row=store.transaction(|tx|Ok(tx.query_row("SELECT dirty,seal_json,json_extract(intent_json,'$.operation') FROM knowledge_index_generation_seals",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?)).unwrap();
            assert_eq!(row.0,1);assert_eq!(row.2,"close");assert_eq!(row.1,receipt.seal_json());
            if matches!(fault,CloseFault::Sync) {
                let close=runtime.close_generation(&generation).unwrap();
                drop(repo.publish(&close).unwrap());
                runtime.shutdown().unwrap();
                assert!(repo.recover_ack(&close).is_ok(),"shutdown recovers an exact lost close acknowledgement");
            } else {
                assert!(runtime.shutdown().is_err());
                assert!(repo.open(generation,Uuid::new_v4()).is_err());
            }
        }).join().unwrap();
    }
}

#[test]
fn index_close_intent_write_failure_retains_live_lock_until_exceptional_final_drop() {
    std::thread::spawn(|| {
        use fs2::FileExt;
        let dir=tempfile::tempdir().unwrap();let store=Arc::new(LocalStore::in_memory().unwrap());
        let repo=Arc::new(IndexRepository::new(store.clone()));let generation=generation();let epoch=Uuid::new_v4();
        let intent=repo.create(generation.clone(),epoch).unwrap();let mut runtime=Runtime::new(dir.path().to_owned(),repo.clone(),epoch).unwrap();
        runtime.open(&intent).unwrap();runtime.add(&intent,vec![(vec![1.0;128],metadata())]).unwrap();
        let receipt=runtime.persist(&intent).unwrap();runtime.acknowledge(&repo.publish(&receipt).unwrap()).unwrap();
        store.transaction(|tx|{tx.execute_batch("CREATE TRIGGER fail_close_intent BEFORE UPDATE ON knowledge_index_generation_seals WHEN NEW.dirty=1 AND json_extract(NEW.intent_json,'$.operation')='close' BEGIN SELECT RAISE(ABORT,'close intent failure'); END;")?;Ok(())}).unwrap();
        assert!(runtime.close_generation(&generation).is_err());
        let path=generation_path(dir.path(),&generation).unwrap();
        let lock=std::fs::OpenOptions::new().read(true).write(true).open(path.join("srvdb.lock")).unwrap();
        assert!(lock.try_lock_exclusive().is_err(),"failed intent retains live cached engine");
        let prior=std::fs::read(path.join("metadata.db")).unwrap();
        drop(runtime);
        lock.try_lock_exclusive().unwrap();FileExt::unlock(&lock).unwrap();
        let changed=std::fs::read(path.join("metadata.db")).unwrap();assert_ne!(prior,changed);
        let epoch=Uuid::new_v4();let open=repo.open(generation,epoch).unwrap();
        let mut restart=Runtime::new(dir.path().to_owned(),repo,epoch).unwrap();
        assert!(restart.open(&open).is_err(),"changed final-Drop bytes are never blessed by old live seal");
        assert_eq!(changed,std::fs::read(path.join("metadata.db")).unwrap());
    }).join().unwrap();
}
