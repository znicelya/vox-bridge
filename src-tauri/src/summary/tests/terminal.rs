use super::harness::*;
use crate::{error::AppError, intelligence::types::*, storage::SummaryVersion};
use uuid::Uuid;
fn lure_state(h: &SummaryHarness, session: Uuid) -> Vec<Vec<rusqlite::types::Value>> {
    h.store
        .transaction(|tx| {
            let mut result = Vec::new();
            for table in ["session_summaries", "intelligence_jobs"] {
                let mut query = tx.prepare(&format!(
                    "SELECT * FROM {table} WHERE session_id=?1 ORDER BY id LIMIT 100"
                ))?;
                let count = query.column_count();
                let mut rows = query.query([session.to_string()])?;
                while let Some(row) = rows.next()? {
                    result.push(
                        (0..count)
                            .map(|index| row.get(index))
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                    );
                }
            }
            Ok(result)
        })
        .unwrap()
}

#[tokio::test]
async fn summary_terminal_real_queued_cancel_and_startup_interrupt_copy_only_exact_status_error_on_deleting_native()
 {
    for startup in [false, true] {
        let h = SummaryHarness::new().await;
        h.close().await;
        let session = h.completed_session(&["never dispatched"]).await;
        let job = h.create_stored(session, false).unwrap();
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE session_summaries SET deleting=1 WHERE id=?1",
                    [job.object_id.unwrap().to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        if startup {
            h.jobs.reconcile().unwrap();
        } else {
            h.jobs.cancel(job.id).await.unwrap();
        }
        let expected = if startup {
            JobStatus::Interrupted
        } else {
            JobStatus::Cancelled
        };
        let error = if startup {
            IntelligenceErrorCode::Interrupted
        } else {
            IntelligenceErrorCode::Cancelled
        };
        let durable = h.store.get_job(job.id).unwrap().unwrap();
        let row = &h.native_versions(session).unwrap()[0];
        assert_eq!(durable.status, expected);
        assert_eq!(durable.error, Some(error));
        assert_eq!(row.status, expected);
        assert_eq!(row.error, Some(error));
        assert!(row.deleting);
        assert!(row.content.is_none());
        assert!(row.input_hash.is_none());
        assert!(row.raw_response_hash.is_none());
        assert!(h.raw_responses(job.object_id.unwrap()).unwrap().is_empty());
        assert!(h.coverage(job.object_id.unwrap()).unwrap().is_empty());
        assert!(h.llm.calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn summary_terminal_real_malformed_witness_rolls_back_cancel_and_startup_transition_without_lure_writes()
 {
    for startup in [false, true] {
        let h = SummaryHarness::new().await;
        h.close().await;
        let session = h.completed_session(&[]).await;
        let job = h.create_stored(session, false).unwrap();
        let native = job.object_id.unwrap();
        let other = h.completed_session(&[]).await;
        let lure = h.create_stored(other, false).unwrap();
        h.jobs.cancel(lure.id).await.unwrap();
        h.store
            .create_summary_version(&SummaryVersion {
                id: Uuid::new_v4(),
                session_id: other,
                version: -1,
                created_at: -9,
                status: "original legacy".into(),
                structured_json: r#"{"text":"unmodified"}"#.into(),
            })
            .unwrap();
        let before = lure_state(&h, other);
        let witness = h
            .store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT job_payload_hash FROM session_summaries WHERE id=?1",
                    [native.to_string()],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .unwrap();
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                    rusqlite::params![native.to_string(), "b".repeat(64)],
                )?;
                Ok(())
            })
            .unwrap();
        let result = if startup {
            h.jobs.reconcile()
        } else {
            h.jobs.cancel(job.id).await.map(|_| ())
        };
        let durable = h.store.get_job(job.id).unwrap().unwrap();
        let unchanged = h
            .store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT status,error_code,structured_json FROM session_summaries WHERE id=?1",
                    [native.to_string()],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )?)
            })
            .unwrap();
        assert!(
            matches!(
                result,
                Err(AppError::Intelligence(IntelligenceErrorCode::Storage))
            ),
            "malformed post-transition Native hook must roll back the actual scheduler transaction"
        );
        assert_eq!(durable.status, JobStatus::Queued);
        assert!(durable.error.is_none());
        assert_eq!(unchanged, ("queued".into(), None, "null".into()));
        assert_eq!(lure_state(&h, other), before);
        assert!(h.raw_responses(native).unwrap().is_empty());
        assert!(h.llm.calls.lock().unwrap().is_empty());
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                    rusqlite::params![native.to_string(), witness],
                )?;
                Ok(())
            })
            .unwrap();
        h.jobs.reconcile().unwrap();
    }
}

#[tokio::test]
async fn summary_terminal_missing_native_is_noop_and_changed_kind_bypass_never_writes_foreign_or_legacy()
 {
    for changed_kind in [false, true] {
        let h = SummaryHarness::new().await;
        h.close().await;
        let session = h.completed_session(&[]).await;
        let job = h.create_stored(session, false).unwrap();
        let native = job.object_id.unwrap();
        let other = h.completed_session(&[]).await;
        let lure = h.create_stored(other, false).unwrap();
        h.jobs.cancel(lure.id).await.unwrap();
        h.store
            .create_summary_version(&SummaryVersion {
                id: Uuid::new_v4(),
                session_id: other,
                version: -1,
                created_at: -9,
                status: "legacy".into(),
                structured_json: r#"{"text":"lure"}"#.into(),
            })
            .unwrap();
        let before = lure_state(&h, other);
        h.store
            .transaction(|tx| {
                if changed_kind {
                    tx.execute(
                        "UPDATE intelligence_jobs SET kind='research',object_id=?2 WHERE id=?1",
                        rusqlite::params![job.id.to_string(), lure.object_id.unwrap().to_string()],
                    )?;
                } else {
                    tx.execute(
                        "DELETE FROM session_summaries WHERE id=?1",
                        [native.to_string()],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let cancelled = h.jobs.cancel(job.id).await.unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(lure_state(&h, other), before);
        assert!(h.raw_responses(native).unwrap().is_empty());
        assert!(h.llm.calls.lock().unwrap().is_empty());
        if changed_kind {
            let status = h
                .store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT status FROM session_summaries WHERE id=?1",
                        [native.to_string()],
                        |r| r.get::<_, String>(0),
                    )?)
                })
                .unwrap();
            assert_eq!(
                status, "queued",
                "a bypassed Summary hook cannot be credited with terminal status copy"
            );
        } else {
            assert_eq!(
                h.store.count_summaries(session).unwrap(),
                0,
                "terminal hook must not insert a replacement"
            );
        }
    }
}
