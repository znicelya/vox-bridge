use super::super::{owner::Runtime, repository::IndexRepository};
use super::persistence::{generation, metadata};
use crate::storage::LocalStore;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn index_records_half_open_ids_tombstones_and_replacement_append() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo, epoch).unwrap();
        runtime.open(&intent).unwrap();
        assert_eq!(
            runtime
                .add(
                    &intent,
                    vec![(vec![1.0; 128], metadata()), (vec![2.0; 128], metadata())]
                )
                .unwrap(),
            vec![0, 1]
        );
        assert!(
            runtime.delete(&intent, vec![0, 2]).is_err(),
            "validate every id before deleting any record"
        );
        assert!(runtime.scan(&intent, 0, 1).unwrap()[0].1.is_some());
        assert_eq!(runtime.delete(&intent, vec![1]).unwrap(), vec![true]);
        assert_eq!(runtime.delete(&intent, vec![1]).unwrap(), vec![false]);
        assert_eq!(
            runtime
                .replace(&intent, 0, vec![3.0; 128], metadata())
                .unwrap(),
            2
        );
        let page = runtime.scan(&intent, 0, 200).unwrap();
        assert_eq!(
            page.iter()
                .map(|r| (r.0, r.1.is_some()))
                .collect::<Vec<_>>(),
            vec![(0, false), (1, false), (2, true)]
        );
        assert!(runtime.scan(&intent, 3, 1).unwrap().is_empty());
        assert!(runtime.scan(&intent, 4, 1).is_err());
        let hits = runtime.search(&intent, vec![1.0; 128], 10).unwrap();
        assert_eq!(hits.iter().map(|r| r.0).collect::<Vec<_>>(), vec![2]);
        assert!(hits[0].1.is_finite());
    })
    .join()
    .unwrap();
}
#[test]
fn index_records_invalid_batch_is_rejected_without_partial_insertion() {
    std::thread::spawn(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = Arc::new(IndexRepository::new(Arc::new(
            LocalStore::in_memory().unwrap(),
        )));
        let epoch = Uuid::new_v4();
        let intent = repo.create(generation(), epoch).unwrap();
        let mut runtime = Runtime::new(dir.path().to_owned(), repo, epoch).unwrap();
        runtime.open(&intent).unwrap();
        for vector in [
            vec![1.0; 127],
            vec![f32::NAN; 128],
            vec![f32::INFINITY; 128],
            vec![0.0; 128],
        ] {
            assert!(
                runtime
                    .add(
                        &intent,
                        vec![(vec![1.0; 128], metadata()), (vector, metadata())]
                    )
                    .is_err()
            );
        }
        assert!(runtime.add(&intent, Vec::new()).is_err());
        assert!(
            runtime
                .add(
                    &intent,
                    (0..201).map(|_| (vec![1.0; 128], metadata())).collect()
                )
                .is_err()
        );
        assert!(runtime.scan(&intent, 0, 200).unwrap().is_empty());
        assert_eq!(
            runtime
                .add(&intent, vec![(vec![1.0; 128], metadata())])
                .unwrap(),
            vec![0]
        );
        assert!(
            runtime
                .replace(&intent, 0, vec![0.0; 128], metadata())
                .is_err()
        );
        assert!(runtime.scan(&intent, 0, 1).unwrap()[0].1.is_some());
        assert!(runtime.scan(&intent, 0, 201).is_err());
        assert!(runtime.search(&intent, vec![0.0; 128], 10).is_err());
        assert!(runtime.search(&intent, vec![1.0; 128], 201).is_err());
    })
    .join()
    .unwrap();
}
