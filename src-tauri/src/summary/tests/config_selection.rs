use super::harness::*;
use crate::{
    app::CredentialChange,
    intelligence::{config::SummarySelection, types::*},
};
#[tokio::test]
async fn summary_selection_freezes_complete_profile_and_automatic_flag_without_reading_missing_keys()
 {
    let h = SummaryHarness::new().await;
    let id = h.profile.id;
    h.io.run(move |tx| tx.secret_delete("com.voxbridge.intelligence", &format!("{id}/api-key")))
        .await
        .unwrap();
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: false,
            summary_profile_id: Some(id),
            ..Default::default()
        })
        .await
        .unwrap();
    for selection in [
        SummarySelection::Manual { profile_id: None },
        SummarySelection::Manual {
            profile_id: Some(id),
        },
        SummarySelection::Automatic,
    ] {
        let selected = h
            .config
            .scoped(h.process.clone())
            .with_summary_selection_completion(
                selection,
                |_, snapshot| {
                    Ok((
                        snapshot.automatic_summary,
                        snapshot.profile,
                        snapshot.selection_error,
                    ))
                },
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(selected, (false, Some(h.profile.clone()), None));
    }
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let missing = h
        .config
        .scoped(h.process.clone())
        .with_summary_selection_completion(
            SummarySelection::Automatic,
            |_, snapshot| {
                Ok((
                    snapshot.automatic_summary,
                    snapshot.profile,
                    snapshot.selection_error,
                ))
            },
            |_| {},
        )
        .await
        .unwrap();
    h.close().await;
    assert_eq!(
        missing,
        (true, None, Some(IntelligenceErrorCode::Configuration))
    );
}
#[tokio::test]
async fn summary_selection_completion_runs_only_after_actual_admission_commit_and_never_after_rollback()
 {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["all input"]).await;
    let mut explicit = h.profile.clone();
    explicit.id = uuid::Uuid::new_v4();
    explicit.model = Some("explicit-model".into());
    h.config
        .save(SaveModelProfile {
            profile: explicit.clone(),
            credential: CredentialChange::Clear,
        })
        .await
        .unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let completed = seen.clone();
    let store = h.store.clone();
    let expected = explicit.clone();
    let admitted = h
        .config
        .scoped(h.process.clone())
        .with_summary_selection_completion(
            SummarySelection::Manual {
                profile_id: Some(explicit.id),
            },
            move |tx, snapshot| {
                assert_eq!(snapshot.profile, Some(expected));
                crate::storage::LocalStore::create_summary_job_in_tx(
                    tx,
                    crate::summary::SummaryJobRequest {
                        session_id: session,
                        profile: snapshot.profile,
                        automatic: false,
                        initial_failure: None,
                    },
                )
            },
            move |result| {
                assert!(store.get_job(result.view().id).unwrap().is_some());
                assert_eq!(store.count_summaries(session).unwrap(), 1);
                completed.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await
        .unwrap();
    assert_eq!(admitted.view().status, JobStatus::Queued);
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    h.close().await;
    let completed = seen.clone();
    let failed = h
        .config
        .scoped(h.process.clone())
        .with_summary_selection_completion(
            SummarySelection::Manual { profile_id: None },
            move |tx, snapshot| {
                crate::storage::LocalStore::create_summary_job_in_tx(
                    tx,
                    crate::summary::SummaryJobRequest {
                        session_id: session,
                        profile: snapshot.profile,
                        automatic: false,
                        initial_failure: None,
                    },
                )?;
                Err::<(), _>(crate::error::AppError::Intelligence(
                    IntelligenceErrorCode::Storage,
                ))
            },
            move |_| {
                completed.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
    h.close().await;
    assert!(failed.is_err());
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert_eq!(h.store.count_summaries(session).unwrap(), 1);
}
