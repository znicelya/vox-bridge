use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_entered_config_survives_real_ten_second_timeout_and_blocks_next_admission_until_late_commit()
 {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let port = ready.terminal_port();
    let baseline = h.config.summary_selection_entries_for_tests();
    let (entered, release) = h.config.hold_next_summary_selection_for_tests();
    let first = created_session(&h, &["first retained final"]);
    commit(&h, &port, first);
    entered.await.unwrap();
    let second = created_session(&h, &["second retained final"]);
    commit(&h, &port, second);
    // Exercise the actual accepted ConfigIo deadline, not a shortened observer.
    tokio::time::sleep(Duration::from_millis(10_100)).await;
    let entries = h.config.summary_selection_entries_for_tests();
    let owned_child = delivery.port.control.child.available_permits() == 0;
    let before = (
        h.native_versions(first).unwrap(),
        h.native_versions(second).unwrap(),
    );
    let (key, audio) = audio_close(&h, &delivery).await;
    let mut closing = Box::pin(delivery.close_final_facade_owned(h.process.clone(), key, audio));
    let held = tokio::time::timeout(Duration::from_millis(30), closing.as_mut())
        .await
        .is_err();
    release.send(()).unwrap();
    let result = closing.await.unwrap().outcome();
    h.jobs.shutdown().await.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
    assert_eq!(
        entries,
        baseline + 1,
        "an observer timeout cannot dispatch a second entered configuration selection"
    );
    assert!(owned_child && held && before.0.is_empty() && before.1.is_empty());
    assert!(
        matches!(result, Err(AppError::Intelligence(Code::Configuration))),
        "the original observer failure stays independent of the late positive join"
    );
    for session in [first, second] {
        let versions = h.native_versions(session).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].error, Some(Code::Interrupted));
        assert_eq!(journal(&h, session), (0, None, None));
    }
    assert!(h.llm.calls.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_entered_config_observer_loss_retains_original_child_and_local_late_commit() {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let factory = ChildFactory {
        process: Arc::downgrade(&h.process),
        slot: delivery.port.control.child.clone(),
        closed: AtomicBool::new(false),
    };
    let child = factory.enter().await.unwrap();
    let retained = Arc::downgrade(&child);
    let port = ready.terminal_port();
    let first = created_session(&h, &["retained final"]);
    commit(&h, &port, first);
    let attempt = AutomaticAttempt::Live {
        delivery_id: delivery.delivery_id().unwrap(),
        job_fence: h.jobs.admission_fence(JobKind::Summary).unwrap(),
    };
    let baseline = h.config.summary_selection_entries_for_tests();
    let (entered, release) = h.config.hold_next_summary_selection_for_tests();
    let admission = delivery.port.clone();
    let observer = tokio::spawn(async move { admission.admit(first, attempt, child, false).await });
    entered.await.unwrap();
    observer.abort();
    assert!(matches!(observer.await, Err(error) if error.is_cancelled()));
    let second = created_session(&h, &["another retained final"]);
    commit(&h, &port, second);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let entries = h.config.summary_selection_entries_for_tests();
    let child_held = retained.upgrade().is_some();
    let (key, audio) = audio_close(&h, &delivery).await;
    let mut closing = Box::pin(delivery.close_final_facade_owned(h.process.clone(), key, audio));
    let held = tokio::time::timeout(Duration::from_millis(30), closing.as_mut())
        .await
        .is_err();
    release.send(()).unwrap();
    closing.await.unwrap().outcome().unwrap();
    h.jobs.shutdown().await.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
    assert!(held && child_held);
    assert_eq!(entries, baseline + 1);
    assert!(
        retained.upgrade().is_none(),
        "the original child ends only after its actual entered ConfigIo closure releases"
    );
    for session in [first, second] {
        let versions = h.native_versions(session).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].error, Some(Code::Interrupted));
    }
    assert!(h.llm.calls.lock().unwrap().is_empty());
}
