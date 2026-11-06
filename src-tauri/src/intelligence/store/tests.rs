use super::*;
use crate::intelligence::test_support::*;
#[test]
fn automatic_identity_survives_terminal_reopen_and_full_capacity() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let id = h
        .store
        .transaction(|tx| admit_job(tx, automatic(session)))
        .unwrap()
        .view()
        .id;
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    for _ in 0..32 {
        let other = h.session();
        h.store
            .transaction(|tx| admit_job(tx, manual(other)))
            .unwrap();
    }
    let reopened = LocalStore::open(h.temp_dir.path()).unwrap();
    let same = reopened
        .transaction(|tx| admit_job(tx, automatic(session)))
        .unwrap();
    assert!(matches!(same, JobAdmissionResult::Existing(_)));
    assert_eq!(same.view().id, id);
    assert_eq!(reopened.list_jobs(None, 200).unwrap().rows.len(), 33);
}
#[test]
fn manual_collision_and_terminal_release_are_transactional() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let one = h
        .store
        .transaction(|tx| admit_job(tx, manual(session)))
        .unwrap()
        .view()
        .id;
    assert!(matches!(
        h.store.transaction(|tx| admit_job(tx, manual(session))),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='cancelled' WHERE id=?1",
                [one.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let two = h
        .store
        .transaction(|tx| admit_job(tx, manual(session)))
        .unwrap()
        .view()
        .id;
    assert_ne!(one, two);
}

#[test]
fn every_terminal_status_releases_manual_but_retains_automatic_identity() {
    for status in ["succeeded", "failed", "cancelled", "interrupted"] {
        let h = IntelligenceHarness::new();
        let session = h.session();
        let first = h
            .store
            .transaction(|tx| admit_job(tx, automatic(session)))
            .unwrap()
            .view()
            .id;
        h.store
            .transaction(|tx| {
                tx.execute("UPDATE intelligence_jobs SET status=?1", [status])?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            h.store
                .transaction(|tx| admit_job(tx, automatic(session)))
                .unwrap()
                .view()
                .id,
            first
        );
        let next = h
            .store
            .transaction(|tx| admit_job(tx, manual(session)))
            .unwrap()
            .view()
            .id;
        assert_ne!(next, first);
        let key: Option<String> = h
            .store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT permanent_key FROM intelligence_jobs WHERE id=?1",
                    [next.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(key, None);
    }
}
#[test]
fn automatic_failure_markers_bypass_full_capacity_but_validate_identity_and_error() {
    let h = IntelligenceHarness::new();
    for _ in 0..32 {
        let session = h.session();
        h.store
            .transaction(|tx| admit_job(tx, manual(session)))
            .unwrap();
    }
    let session = h.session();
    assert!(matches!(
        h.store.transaction(|tx| admit_job(tx, manual(session))),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    for code in [
        IntelligenceErrorCode::Configuration,
        IntelligenceErrorCode::Busy,
        IntelligenceErrorCode::NoFinalText,
        IntelligenceErrorCode::Interrupted,
    ] {
        let session = h.session();
        let result = h
            .store
            .transaction(|tx| {
                admit_job(
                    tx,
                    JobDraft {
                        initial_failure: Some(code),
                        ..automatic(session)
                    },
                )
            })
            .unwrap();
        assert_eq!(result.view().status, JobStatus::Failed);
        assert_eq!(result.view().error, Some(code));
    }
    for code in [
        IntelligenceErrorCode::Network,
        IntelligenceErrorCode::Cancelled,
    ] {
        assert!(
            h.store
                .transaction(|tx| admit_job(
                    tx,
                    JobDraft {
                        initial_failure: Some(code),
                        ..automatic(session)
                    }
                ))
                .is_err()
        );
    }
    let other = h.session();
    assert!(
        h.store
            .transaction(|tx| admit_job(
                tx,
                JobDraft {
                    session_id: Some(other),
                    ..automatic(session)
                }
            ))
            .is_err()
    );
    assert!(
        h.store
            .transaction(|tx| admit_job(
                tx,
                JobDraft {
                    kind: JobKind::Research,
                    ..automatic(session)
                }
            ))
            .is_err()
    );
    assert!(
        h.store
            .transaction(|tx| admit_job(
                tx,
                JobDraft {
                    initial_failure: Some(IntelligenceErrorCode::Busy),
                    ..manual(session)
                }
            ))
            .is_err()
    );
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 36);
}
#[test]
fn concurrent_manual_admission_has_one_winner_and_transaction_rollback_has_no_row() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let mut threads = Vec::new();
    for _ in 0..2 {
        let store = h.store.clone();
        threads.push(std::thread::spawn(move || {
            store.transaction(|tx| admit_job(tx, manual(session)))
        }));
    }
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
    let other = h.session();
    let result: AppResult<()> = h.store.transaction(|tx| {
        admit_job(tx, manual(other))?;
        Err(AppError::Intelligence(IntelligenceErrorCode::Storage))
    });
    assert!(result.is_err());
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
}
#[test]
fn manual_scope_is_exact_and_all_document_mutations_share_exclusion() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    for key in [
        "".into(),
        "x".repeat(161),
        "summary:wrong".into(),
        "非ASCII".into(),
    ] {
        assert!(
            h.store
                .transaction(|tx| admit_job(
                    tx,
                    JobDraft {
                        admission: JobAdmission::Manual { exclusion_key: key },
                        ..manual(session)
                    }
                ))
                .is_err()
        );
    }
    let object = Uuid::new_v4();
    for (i, kind) in [JobKind::Import, JobKind::Reindex, JobKind::DeleteDocument]
        .into_iter()
        .enumerate()
    {
        let result = h.store.transaction(|tx| {
            admit_job(
                tx,
                JobDraft {
                    kind,
                    object_id: Some(object),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{object}"),
                    },
                    ..manual(session)
                },
            )
        });
        assert_eq!(result.is_ok(), i == 0);
    }
}
#[test]
fn paging_is_exclusive_bounded_and_payload_is_versioned_and_bounded() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    for value in [
        serde_json::json!({}),
        serde_json::json!({"version":2,"profiles":[],"input":{}}),
        serde_json::json!({"version":1,"profiles":[],"input":"x".repeat(65536)}),
    ] {
        assert!(
            h.store
                .transaction(|tx| admit_job(
                    tx,
                    JobDraft {
                        payload: value,
                        ..manual(session)
                    }
                ))
                .is_err()
        );
    }
    for _ in 0..3 {
        let other = h.session();
        h.store
            .transaction(|tx| admit_job(tx, manual(other)))
            .unwrap();
    }
    let first = h.store.list_jobs(None, 2).unwrap();
    assert_eq!(first.rows.len(), 2);
    assert!(first.next_cursor.is_some());
    let second = h.store.list_jobs(first.next_cursor, 2).unwrap();
    assert_eq!(second.rows.len(), 1);
    assert_eq!(second.next_cursor, None);
    assert!(!first.rows.iter().any(|r| r.id == second.rows[0].id));
    for limit in [0, 201] {
        assert!(h.store.list_jobs(None, limit).is_err());
    }
    assert!(h.store.list_jobs(Some(-1), 1).is_err());
    assert_eq!(
        h.store.get_job(second.rows[0].id).unwrap(),
        Some(second.rows[0].clone())
    );
}

#[test]
fn snapshot_payload_caps_three_unique_profiles_and_rejects_four_or_duplicates() {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let profiles: Vec<_> = (0..3).map(|_| test_llm_profile()).collect();
    let mut fourth = profiles.clone();
    fourth.push(test_llm_profile());
    for invalid in [fourth, vec![profiles[0].clone(), profiles[0].clone()]] {
        assert!(
            h.store
                .transaction(|tx| admit_job(
                    tx,
                    JobDraft {
                        payload: payload(invalid),
                        ..manual(session)
                    }
                ))
                .is_err()
        );
    }
    h.store
        .transaction(|tx| {
            admit_job(
                tx,
                JobDraft {
                    payload: payload(profiles),
                    ..manual(session)
                },
            )
        })
        .unwrap();
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
}
