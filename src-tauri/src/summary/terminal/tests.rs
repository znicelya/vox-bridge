use super::*;
use crate::{
    audio::runtime::AudioBackend,
    intelligence::types::{IntelligenceSettings, JobStatus},
    providers::{
        ProviderEvent,
        fixture::{FixtureScript, FixtureStep},
    },
    session::test_support::{RuntimeHarness, metadata},
    storage::{StoredSession, StoredSessionLifecycle},
    summary::{SummaryJobRequest, tests::harness::SummaryHarness},
};
mod disposition;
mod entered_config;
mod rearm;

fn created_session(h: &SummaryHarness, finals: &[&str]) -> Uuid {
    let id = Uuid::new_v4();
    h.store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    for (sequence, text) in finals.iter().enumerate() {
        h.insert_final(id, "source_to_target", sequence as i64, text);
    }
    id
}
fn commit(h: &SummaryHarness, port: &SessionTerminalPort, session: Uuid) {
    let reservation = port.try_reserve();
    h.store
        .commit_terminal_lifecycle(
            &StoredSessionLifecycle {
                session_id: session,
                status: "completed".into(),
                ended_at: Some(2),
            },
            reservation.intent(),
        )
        .unwrap();
    reservation.committed(session);
}
async fn versions(
    h: &SummaryHarness,
    session: Uuid,
    count: usize,
) -> Vec<crate::summary::types::NativeSummaryView> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let versions = h.native_versions(session).unwrap();
            if versions.len() == count {
                return versions;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn start(h: &SummaryHarness, epoch: u64) -> (SummaryDeliveryOwner, SummaryDeliveryReady) {
    h.jobs.shutdown().await.unwrap();
    let delivery = h.service.delivery_owner();
    let local = h.service.core.recovery.ready().unwrap();
    let ready = delivery
        .start_owned(h.process.clone(), epoch, local)
        .await
        .unwrap();
    (delivery, ready)
}
async fn audio_close(
    h: &SummaryHarness,
    delivery: &SummaryDeliveryOwner,
) -> (AudioCloseKey, AudioCloseJoin) {
    // No runtime/audio work entered this store fixture. Capture genuine fresh
    // actor and backend sources; no status or empty-active value mints the ack.
    let runtime = RuntimeHarness::live(
        FixtureScript::with_output_audio(
            crate::audio::PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let orchestrator = runtime.orchestrator();
    let key = delivery
        .elect_closing(1, runtime.audio.actual_epoch().unwrap())
        .unwrap();
    let backend = runtime.audio.begin_close(key.clone()).unwrap();
    let session = orchestrator.capture_close_join(key.clone()).unwrap();
    let audio = AudioCloseJoin::start_owned(
        h.process.clone(),
        key.clone(),
        session,
        backend,
        async move { orchestrator.stop_active().await },
    )
    .unwrap();
    (key, audio)
}
async fn close(h: &SummaryHarness, delivery: &SummaryDeliveryOwner) {
    let (key, audio) = audio_close(h, delivery).await;
    let (jobs, delivery) = tokio::join!(
        h.jobs.shutdown(),
        delivery.close_join_owned(h.process.clone(), key, audio)
    );
    jobs.unwrap();
    delivery.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
}
async fn enabled(h: &SummaryHarness, enabled: bool) {
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: enabled,
            summary_profile_id: Some(h.profile.id),
            ..Default::default()
        })
        .await
        .unwrap();
}
fn pending(h: &SummaryHarness, session: Uuid, generation: Uuid, reason: Option<&str>) {
    h.store.transaction(|tx| {
        tx.execute("UPDATE session_lifecycle SET summary_terminal_pending=1,summary_terminal_generation=?2,summary_terminal_error=?3 WHERE session_id=?1", params![session.to_string(), generation.to_string(), reason])?;
        Ok(())
    }).unwrap();
}
fn journal(h: &SummaryHarness, session: Uuid) -> (i64, Option<String>, Option<String>) {
    h.store.transaction(|tx| Ok(tx.query_row("SELECT summary_terminal_pending,summary_terminal_generation,summary_terminal_error FROM session_lifecycle WHERE session_id=?1", [session.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?)).unwrap()
}
fn local(delivery: &SummaryDeliveryOwner, reason: LocalRetryReason) -> AutomaticAttempt {
    let id = delivery.delivery_id().unwrap();
    AutomaticAttempt::LocalRetry {
        delivery_id: id,
        reason,
        local_fence: delivery.port.control.local(id),
    }
}

#[tokio::test]
async fn terminal_final_facade_drop_retains_preissued_permit_until_local_tail_finishes() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let port = ready.terminal_port();
    let reservation = port.try_reserve();
    let session = Uuid::new_v4();
    h.store
        .create_session(&StoredSession {
            id: session,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    h.jobs.shutdown().await.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
    let SummaryHarness {
        store,
        service,
        process,
        dir: _dir,
        ..
    } = h;
    let retained = Arc::downgrade(&process);
    drop(ready);
    drop(port);
    drop(service);
    drop(delivery);
    drop(process);

    let held = tokio::time::timeout(Duration::from_millis(30), async {
        while retained.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_err();
    store
        .commit_terminal_lifecycle(
            &StoredSessionLifecycle {
                session_id: session,
                status: "completed".into(),
                ended_at: Some(2),
            },
            reservation.intent(),
        )
        .unwrap();
    reservation.committed(session);
    tokio::time::timeout(Duration::from_secs(2), async {
        while retained.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect(
        "last facade drop must release delivery only after producer EOF and the owned local tail",
    );
    let rows = store.summary_page(session, None, 100).unwrap().rows;
    let crate::summary::types::SummaryView::Native(row) = &rows[0] else {
        panic!("local automatic disposition must be Native")
    };
    assert!(
        held,
        "a preissued terminal permit retains the actual delivery process owner"
    );
    assert_eq!(row.status, JobStatus::Failed);
    assert_eq!(row.error, Some(Code::Interrupted));
}

#[tokio::test]
async fn terminal_bootstrap_covers_completed_failed_and_old_busy_across_keyset_pages() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let mut ids = Vec::new();
    for index in 0..67 {
        let id = h.completed_session(&[]).await;
        if index % 2 == 1 {
            h.store
                .update_session_lifecycle(&StoredSessionLifecycle {
                    session_id: id,
                    status: "failed".into(),
                    ended_at: Some(23),
                })
                .unwrap();
        }
        if index == 0 {
            pending(&h, id, Uuid::new_v4(), Some("busy"));
        }
        ids.push(id);
    }
    let (delivery, ready) = start(&h, 0).await;
    ready.require_epoch(0).unwrap();
    for id in &ids {
        let versions = h.native_versions(*id).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].status, JobStatus::Failed);
        assert_eq!(
            versions[0].error,
            Some(Code::Interrupted),
            "uncertain startup must preserve Interrupted over empty text/configuration/old Busy"
        );
        assert_eq!(journal(&h, *id), (0, None, None));
    }
    assert!(h.llm.calls.lock().unwrap().is_empty());
    close(&h, &delivery).await;
    let local = h
        .service
        .reconcile_local_owned(h.process.clone(), 1, h.jobs.reap_port())
        .await
        .unwrap();
    delivery
        .start_owned(h.process.clone(), 1, local)
        .await
        .unwrap();
    for id in ids {
        assert_eq!(
            h.native_versions(id).unwrap().len(),
            1,
            "reopen must find permanent Existing"
        );
    }
    close(&h, &delivery).await;
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn terminal_reservation_32_and_33_and_close_waits_late_owned_permit_to_eof() {
    let h = SummaryHarness::new().await;
    let (delivery, ready) = start(&h, 0).await;
    let port = ready.terminal_port();
    let mut reservations = Vec::new();
    for _ in 0..32 {
        let reservation = port.try_reserve();
        assert_eq!(reservation.intent().journal_fields().1, None);
        reservations.push(reservation);
    }
    assert_eq!(port.try_reserve().intent().journal_fields().1, Some("busy"));
    let (key, audio) = audio_close(&h, &delivery).await;
    audio.wait().await.unwrap();
    assert_eq!(
        port.try_reserve().intent().journal_fields().1,
        Some("interrupted")
    );
    let close = delivery.close_join_owned(h.process.clone(), key, audio);
    tokio::pin!(close);
    let held = tokio::time::timeout(Duration::from_millis(30), &mut close)
        .await
        .is_err();
    for reservation in reservations {
        reservation.committed(Uuid::new_v4());
    }
    close.await.unwrap();
    h.jobs.shutdown().await.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
    assert!(
        held,
        "actual audio acknowledgment and empty queue do not settle outstanding OwnedPermits"
    );
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn terminal_closing_overrides_unadmitted_full_queue_busy_with_interrupted() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let port = ready.terminal_port();
    let reservations = (0..32).map(|_| port.try_reserve()).collect::<Vec<_>>();
    let reservation = port.try_reserve();
    assert_eq!(reservation.intent().journal_fields().1, Some("busy"));
    let id = Uuid::new_v4();
    h.store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    h.store
        .commit_terminal_lifecycle(
            &StoredSessionLifecycle {
                session_id: id,
                status: "completed".into(),
                ended_at: Some(2),
            },
            reservation.intent(),
        )
        .unwrap();
    delivery.elect_closing(1, 0).unwrap();
    drop(reservations);
    reservation.committed(id);
    let attempt = delivery
        .port
        .attempt(id, false, h.process.clone())
        .await
        .unwrap();
    h.service
        .after_terminal_owned(id, attempt, h.process.clone())
        .await
        .unwrap();
    let versions = h.native_versions(id).unwrap();
    close(&h, &delivery).await;
    assert_eq!(
        versions[0].error,
        Some(Code::Interrupted),
        "after Closing a not-yet-admitted notice is local Interrupted"
    );
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn terminal_live_attempt_captured_before_closing_is_consumed_as_local_interrupted() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, _) = start(&h, 0).await;
    // This is the same actual child slot used by the worker. Retaining it keeps
    // a rescan from admitting the journal ahead of the captured live attempt.
    let slot = delivery
        .port
        .control
        .child
        .clone()
        .acquire_owned()
        .await
        .unwrap();
    let child: OperationLease = Arc::new(ChildOwner {
        _process: h.process.clone(),
        _slot: slot,
    });
    let session = h.completed_session(&["retained final"]).await;
    let id = delivery.delivery_id().unwrap();
    pending(&h, session, id, None);
    let attempt = AutomaticAttempt::Live {
        delivery_id: id,
        job_fence: h.jobs.admission_fence(JobKind::Summary).unwrap(),
    };
    delivery.elect_closing(1, 0).unwrap();
    let consumed = h
        .service
        .after_terminal_owned(session, attempt, child)
        .await;
    close(&h, &delivery).await;
    let versions = h.native_versions(session).unwrap();
    assert!(
        consumed.is_ok(),
        "Closing must dispose an unadmitted live attempt locally in its actual config/SQL transaction"
    );
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].error, Some(Code::Interrupted));
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn terminal_deleted_session_hint_is_noop_even_when_configuration_is_unreadable() {
    let h = SummaryHarness::new().await;
    let (delivery, _) = start(&h, 0).await;
    let session = h.completed_session(&[]).await;
    h.store.delete_session(session).unwrap();
    let config_path = h.dir.path().join("service-configs.json");
    let original = std::fs::read(&config_path).unwrap();
    std::fs::write(&config_path, b"{unreadable}").unwrap();
    let replay = h
        .service
        .after_terminal_owned(
            session,
            local(&delivery, LocalRetryReason::Interrupted),
            h.process.clone(),
        )
        .await;
    std::fs::write(&config_path, original).unwrap();
    close(&h, &delivery).await;
    assert!(
        replay.is_ok(),
        "a late deleted UUID has no configuration decision or paid work to perform"
    );
    assert!(h.store.get_session_lifecycle(session).unwrap().is_none());
    assert!(h.store.list_jobs(None, 100).unwrap().rows.is_empty());
}

#[tokio::test]
async fn terminal_existing_deleted_output_unreadable_config_keeps_journal_and_ready_closed() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let session = h.completed_session(&["retained final"]).await;
    let original = h
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: Some(h.profile.clone()),
            automatic: true,
            initial_failure: Some(Code::Interrupted),
        })
        .unwrap();
    h.delete(original.object_id.unwrap()).await.unwrap();
    h.jobs.shutdown().await.unwrap();
    let delivery = h.service.delivery_owner();
    pending(&h, session, Uuid::new_v4(), Some("interrupted"));
    let before = journal(&h, session);
    let config_path = h.dir.path().join("service-configs.json");
    let config = std::fs::read(&config_path).unwrap();
    std::fs::write(&config_path, b"{unreadable}").unwrap();
    let startup = delivery
        .start_owned(
            h.process.clone(),
            0,
            h.service.core.recovery.ready().unwrap(),
        )
        .await;
    let replay = h
        .service
        .after_terminal_owned(
            session,
            local(&delivery, LocalRetryReason::Interrupted),
            h.process.clone(),
        )
        .await;
    let after = journal(&h, session);
    let existing = h
        .store
        .transaction(|tx| permanent_job(tx, &format!("auto-summary:{session}")))
        .unwrap()
        .unwrap();
    let counter: i64 = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT last_version FROM summary_version_counters WHERE session_id=?1",
                [session.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let ready = delivery.require_epoch(0).is_ok();
    let jobs = h.jobs.admission_fence(JobKind::Summary).is_ok();
    std::fs::write(&config_path, config).unwrap();
    close(&h, &delivery).await;
    assert!(startup.is_err());
    assert!(replay.is_ok());
    assert_eq!(existing.id, original.id);
    assert_eq!(before, after);
    assert_eq!(counter, 1);
    assert!(
        !ready && !jobs,
        "read-only permanent replay is not bootstrap/job readiness"
    );
    assert!(h.native_versions(session).unwrap().is_empty());
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn terminal_disabled_and_local_failure_precedence_create_no_paid_work() {
    let h = SummaryHarness::new().await;
    enabled(&h, false).await;
    let (delivery, _) = start(&h, 0).await;
    let id = delivery.delivery_id().unwrap();
    let disabled = h.completed_session(&[]).await;
    pending(&h, disabled, id, None);
    h.service
        .after_terminal_owned(
            disabled,
            local(&delivery, LocalRetryReason::Interrupted),
            h.process.clone(),
        )
        .await
        .unwrap();
    assert_eq!(journal(&h, disabled), (0, None, None));
    assert!(h.native_versions(disabled).unwrap().is_empty());
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: true,
            ..Default::default()
        })
        .await
        .unwrap();
    for (reason, expected) in [
        (LocalRetryReason::Busy, Code::Busy),
        (LocalRetryReason::Interrupted, Code::Interrupted),
    ] {
        let session = h.completed_session(&[]).await;
        pending(&h, session, id, None);
        h.service
            .after_terminal_owned(session, local(&delivery, reason), h.process.clone())
            .await
            .unwrap();
        let versions = h.native_versions(session).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].status, JobStatus::Failed);
        assert_eq!(
            versions[0].error,
            Some(expected),
            "forced local failure precedes NoFinalText and missing model"
        );
    }
    close(&h, &delivery).await;
    assert!(h.llm.calls.lock().unwrap().is_empty());
}
