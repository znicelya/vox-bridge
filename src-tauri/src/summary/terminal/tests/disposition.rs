use super::*;

#[tokio::test]
async fn terminal_failed_automatic_manual_retry_and_output_deletion_preserve_permanent_identity() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let session = h.completed_session(&["retained final"]).await;
    let (delivery, _) = start(&h, 0).await;
    let automatic = h.native_versions(session).unwrap()[0].clone();
    assert_eq!(automatic.error, Some(Code::Interrupted));
    let marker = h
        .store
        .transaction(|tx| permanent_job(tx, &format!("auto-summary:{session}")))
        .unwrap()
        .unwrap();
    let manual = h.generate(session, None).await.unwrap();
    assert_eq!(h.wait(manual.id).await.status, JobStatus::Succeeded);
    let permanent: Option<String> = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT permanent_key FROM intelligence_jobs WHERE id=?1",
                [manual.id.to_string()],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert!(permanent.is_none());
    assert_ne!(manual.id, marker.id);
    h.delete(automatic.id).await.unwrap();
    pending(&h, session, delivery.delivery_id().unwrap(), None);
    h.service
        .after_terminal_owned(
            session,
            local(&delivery, LocalRetryReason::Interrupted),
            h.process.clone(),
        )
        .await
        .unwrap();
    let versions = h.native_versions(session).unwrap();
    let existing = h
        .store
        .transaction(|tx| permanent_job(tx, &format!("auto-summary:{session}")))
        .unwrap()
        .unwrap();
    close(&h, &delivery).await;
    assert_eq!(existing.id, marker.id);
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, 2);
    assert_eq!(versions[0].status, JobStatus::Succeeded);
    assert_eq!(
        h.llm.calls.lock().unwrap().len(),
        1,
        "only the explicit manual retry may call the model"
    );
}

#[tokio::test]
async fn terminal_live_preflight_orders_empty_text_then_missing_configuration_then_actual_exclusion_busy()
 {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let port = ready.terminal_port();
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let empty = created_session(&h, &[]);
    commit(&h, &port, empty);
    assert_eq!(
        versions(&h, empty, 1).await[0].error,
        Some(Code::NoFinalText)
    );
    assert!(h.llm.calls.lock().unwrap().is_empty());

    h.llm.block.store(true, Ordering::SeqCst);
    let missing = created_session(&h, &["final text without selected automatic profile"]);
    let manual = h.generate(missing, Some(h.profile.id)).await.unwrap();
    h.llm_entered().await;
    commit(&h, &port, missing);
    let missing_versions = versions(&h, missing, 2).await;
    assert_eq!(
        missing_versions[1].error,
        Some(Code::Configuration),
        "missing automatic profile precedes the real unfinished manual exclusion"
    );
    h.llm.release.notify_one();
    assert_eq!(h.wait(manual.id).await.status, JobStatus::Succeeded);

    enabled(&h, true).await;
    let busy = created_session(&h, &["valid profile and retained final"]);
    let manual = h.generate(busy, None).await.unwrap();
    h.llm_entered().await;
    commit(&h, &port, busy);
    let busy_versions = versions(&h, busy, 2).await;
    assert_eq!(busy_versions[1].error, Some(Code::Busy));
    h.llm.release.notify_one();
    assert_eq!(h.wait(manual.id).await.status, JobStatus::Succeeded);
    close(&h, &delivery).await;
    assert_eq!(
        h.llm.calls.lock().unwrap().len(),
        2,
        "both calls belong to explicitly admitted manual jobs"
    );
}

#[tokio::test]
async fn terminal_counter_exhaustion_retains_truthful_journal_and_no_version_marker_or_call() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let session = h.completed_session(&["retained final"]).await;
    h.store
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO summary_version_counters VALUES(?1,4294967295)",
                [session.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    h.jobs.shutdown().await.unwrap();
    let delivery = h.service.delivery_owner();
    let result = delivery
        .start_owned(
            h.process.clone(),
            0,
            h.service.core.recovery.ready().unwrap(),
        )
        .await;
    assert!(matches!(
        result,
        Err(AppError::Intelligence(Code::ResourceLimit))
    ));
    let pending = journal(&h, session);
    assert_eq!(pending.0, 1);
    assert_eq!(pending.2.as_deref(), Some("resource_limit"));
    assert!(delivery.require_epoch(0).is_err());
    assert!(h.jobs.admission_fence(JobKind::Summary).is_err());
    let (key, audio) = audio_close(&h, &delivery).await;
    let closed = delivery
        .close_final_facade_owned(h.process.clone(), key, audio)
        .await
        .unwrap()
        .outcome();
    assert!(matches!(
        closed,
        Err(AppError::Intelligence(Code::ResourceLimit))
    ));
    assert!(
        h.store
            .transaction(|tx| permanent_job(tx, &format!("auto-summary:{session}")))
            .unwrap()
            .is_none()
    );
    assert!(h.native_versions(session).unwrap().is_empty());
    assert!(h.llm.calls.lock().unwrap().is_empty());
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
}
