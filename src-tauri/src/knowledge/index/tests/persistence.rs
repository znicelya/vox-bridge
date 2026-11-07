use super::super::{
    codec::ChunkMetadata, owner::Runtime, preflight::generation_path, repository::IndexRepository,
    seal::Generation,
};
use crate::{
    knowledge::{identity::EmbeddingIdentity, types::TextLocation},
    storage::LocalStore,
};
use std::sync::Arc;
use uuid::Uuid;

pub(super) fn generation() -> Generation {
    Generation {
        identity: EmbeddingIdentity {
            profile_id: Uuid::new_v4(),
            base_url: "https://example.test/v1".into(),
            model: "test-embedding".into(),
            dimensions: 128,
            normalization_version: 1,
        },
        requested_dimension: None,
        generation: Uuid::new_v4(),
    }
}
pub(super) fn metadata() -> ChunkMetadata {
    ChunkMetadata::new(
        Uuid::new_v4(),
        1,
        Uuid::new_v4(),
        "a".repeat(64),
        &TextLocation {
            page: None,
            paragraph: 0,
            heading: Some("题".repeat(6000)),
            start_char: 0,
            end_char: 2,
        },
    )
    .unwrap()
}
#[test]
fn index_persistence_real_writer_creates_publishes_and_reopens() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let epoch = Uuid::new_v4();
        let generation = generation();
        let intent = repo
            .create(generation.clone(), epoch)
            .expect("committed creation intent");
        let mut runtime = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        assert_eq!(runtime.open(&intent).unwrap(), 0);
        assert_eq!(
            runtime
                .add(
                    &intent,
                    vec![(vec![1.0; 128], metadata()), (vec![2.0; 128], metadata())]
                )
                .unwrap(),
            vec![0, 1]
        );
        let receipt = runtime.persist(&intent).unwrap();
        let ack = repo.publish(&receipt).unwrap();
        runtime.acknowledge(&ack).unwrap();
        assert!(
            repo.validate(&intent).is_err(),
            "published intent cannot authorize another mutation"
        );
        drop(runtime);
        let epoch = Uuid::new_v4();
        let open = repo.open(generation, epoch).unwrap();
        let mut reopened = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        assert_eq!(reopened.open(&open).unwrap(), 2);
        let page = reopened.scan(&open, 0, 200).unwrap();
        assert_eq!(page.iter().map(|r| r.0).collect::<Vec<_>>(), vec![0, 1]);
        assert!(page.iter().all(|r| r.1.is_some()));
        let receipt = reopened.persist(&open).unwrap();
        reopened
            .acknowledge(&repo.publish(&receipt).unwrap())
            .unwrap();
    })
    .join()
    .unwrap();
}
#[test]
fn index_persistence_dirty_owner_loss_rejects_before_constructor_and_preserves_bytes() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let generation = generation();
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation.clone(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap();
        runtime.persist(&intent).unwrap();
        drop(runtime);
        let path = generation_path(dir.path(), &generation).unwrap();
        let before = std::fs::read(path.join("vectors.bin")).unwrap();
        assert!(repo.open(generation.clone(), Uuid::new_v4()).is_err());
        let mut restarted = Runtime::new(dir.path().to_owned(), repo, Uuid::new_v4()).unwrap();
        assert!(
            restarted.open(&intent).is_err(),
            "retained descriptors cannot reopen dirty bytes in another owner epoch"
        );
        assert_eq!(before, std::fs::read(path.join("vectors.bin")).unwrap());
    })
    .join()
    .unwrap();
}
#[test]
fn index_persistence_changed_sealed_file_is_rejected_without_repairing_it() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let generation = generation();
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation.clone(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap();
        let receipt = runtime.persist(&intent).unwrap();
        runtime
            .acknowledge(&repo.publish(&receipt).unwrap())
            .unwrap();
        drop(runtime);
        let path = generation_path(dir.path(), &generation).unwrap();
        let mut changed = std::fs::read(path.join("vectors.bin")).unwrap();
        changed[24] ^= 1;
        std::fs::write(path.join("vectors.bin"), &changed).unwrap();
        let epoch = Uuid::new_v4();
        let open = repo.open(generation, epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo, epoch).unwrap();
        assert!(runtime.open(&open).is_err());
        assert_eq!(changed, std::fs::read(path.join("vectors.bin")).unwrap());
    })
    .join()
    .unwrap();
}

// Characterizes the surprising boundary our seal protocol must account for:
// successful SrvDB::persist does not make redb bytes stable across actual Drop.
#[test]
fn index_persistence_redb_close_changes_preclose_seal_bytes() {
    std::thread::spawn(|| {
        use sha2::{Digest,Sha256};
        let dir=tempfile::tempdir().unwrap();
        let repo=Arc::new(IndexRepository::new(Arc::new(LocalStore::in_memory().unwrap())));
        let generation=generation(); let epoch=Uuid::new_v4();
        let intent=repo.create(generation.clone(),epoch).unwrap();
        let mut runtime=Runtime::new(dir.path().to_owned(),repo,epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime.add(&intent,vec![(vec![1.0;128],metadata())]).unwrap();
        runtime.persist(&intent).unwrap();
        let path=generation_path(dir.path(),&generation).unwrap();
        let before=std::fs::read(path.join("metadata.db")).unwrap();
        let vectors=std::fs::read(path.join("vectors.bin")).unwrap();
        drop(runtime);
        let after=std::fs::read(path.join("metadata.db")).unwrap();
        eprintln!("redb live length={} sha256={:x} god_byte={} slot_versions={}/{}; closed length={} sha256={:x} god_byte={} slot_versions={}/{}",before.len(),Sha256::digest(&before),before[9],before[64],before[192],after.len(),Sha256::digest(&after),after[9],after[64],after[192]);
        assert_ne!(before,after,"a live metadata hash cannot authorize post-Drop bytes");
        assert_eq!(before[9]&2,2,"live writable header requires recovery");
        assert_eq!(after[9]&2,0,"controlled successful close clears recovery flag");
        assert_eq!(vectors,std::fs::read(path.join("vectors.bin")).unwrap());
    }).join().unwrap();
}

#[test]
fn index_persistence_receipt_pins_writer_until_publication_and_recovers_exact_lost_ack() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let generation = generation();
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation.clone(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap();
        let receipt = runtime.persist(&intent).unwrap();
        assert!(
            runtime
                .add(&intent, vec![(vec![2.0; 128], metadata())])
                .is_err(),
            "outstanding receipt fences all subsequent writes"
        );
        let retry = runtime.persist(&intent).unwrap();
        assert_eq!(
            receipt.seal_json(),
            retry.seal_json(),
            "observation retry reuses the receipt without another engine persist"
        );
        drop(repo.publish(&receipt).unwrap());
        runtime
            .acknowledge(&repo.recover_ack(&receipt).unwrap())
            .unwrap();
        assert!(
            repo.publish(&receipt).is_err(),
            "committed intent cannot publish twice"
        );
        let next = repo.open(generation, epoch).unwrap();
        assert!(
            repo.recover_ack(&receipt).is_err(),
            "a newer operation retires the old acknowledgement"
        );
        assert!(repo.validate(&intent).is_err());
        assert!(repo.validate(&next).is_ok());
    })
    .join()
    .unwrap();
}

#[test]
fn index_persistence_post_durability_failure_retries_on_the_same_live_engine() {
    std::thread::spawn(|| {
        use fs2::FileExt;

        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(LocalStore::in_memory().unwrap());
        let repo = Arc::new(IndexRepository::new(store.clone()));
        let epoch = Uuid::new_v4();
        let generation = generation();
        let intent = repo.create(generation.clone(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo.clone(), epoch).unwrap();
        runtime.open(&intent).unwrap();
        runtime
            .add(&intent, vec![(vec![1.0; 128], metadata())])
            .unwrap();
        runtime.fail_once_after_persist = true;
        assert!(runtime.persist(&intent).is_err());
        let (dirty, intent_json) = store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT dirty,intent_json FROM knowledge_index_generation_seals",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(dirty, 1);
        assert!(!intent_json.is_empty());
        let path = generation_path(dir.path(), &generation).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.join("srvdb.lock"))
            .unwrap();
        assert!(
            lock.try_lock_exclusive().is_err(),
            "uncertain persistence must retain the original live engine"
        );
        assert_eq!(runtime.scan(&intent, 0, 1).unwrap().len(), 1);

        let receipt = runtime.persist(&intent).unwrap();
        runtime
            .acknowledge(&repo.publish(&receipt).unwrap())
            .unwrap();
        assert!(repo.validate(&intent).is_err());
        runtime.shutdown().unwrap();
    })
    .join()
    .unwrap();
}
