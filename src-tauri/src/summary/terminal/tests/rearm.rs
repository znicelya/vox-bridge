use super::*;

#[tokio::test]
async fn terminal_cancelled_actual_closer_cannot_issue_final_drain_proof_after_its_handle_is_removed()
 {
    let h = SummaryHarness::new().await;
    enabled(&h, true).await;
    let (delivery, ready) = start(&h, 0).await;
    let permit = ready.terminal_port().try_reserve();
    let (key, audio) = audio_close(&h, &delivery).await;
    let ack = audio.wait().await.unwrap();
    let session = created_session(&h, &["retained final"]);
    h.store
        .commit_terminal_lifecycle(
            &StoredSessionLifecycle {
                session_id: session,
                status: "completed".into(),
                ended_at: Some(2),
            },
            permit.intent(),
        )
        .unwrap();
    let mut closing = Box::pin(delivery.close_final_facade_owned(h.process.clone(), key, audio));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), closing.as_mut())
            .await
            .is_err()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let sealed = delivery
                .port
                .control
                .state
                .lock()
                .unwrap()
                .seal
                .as_ref()
                .is_some_and(|seal| *seal.borrow());
            if sealed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    {
        let closer = delivery.port.control.closer.lock().await;
        closer.as_ref().unwrap().abort();
    }
    let held = tokio::time::timeout(Duration::from_millis(30), closing.as_mut())
        .await
        .is_err();
    permit.committed(session);
    let joined = closing.await;
    assert!(
        held,
        "a cancelled closer must still retain every pre-issued permit through the actual worker/child drain"
    );
    assert!(matches!(
        joined,
        Err(AppError::Intelligence(Code::Interrupted))
    ));
    assert!(delivery.port.control.worker.lock().await.is_none());
    assert!(delivery.port.control.closer.lock().await.is_none());
    assert_eq!(delivery.port.control.child.available_permits(), 1);
    assert!(delivery.ensure_startable().is_err());
    assert!(delivery.validate_reopen_after_close(&ack).is_err());
    assert!(delivery.reopen_after_close(&ack).is_err());
    let versions = h.native_versions(session).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].error, Some(Code::Interrupted));
    assert_eq!(journal(&h, session), (0, None, None));
    assert!(h.llm.calls.lock().unwrap().is_empty());
    h.jobs.shutdown().await.unwrap();
    h.service.signal_closing().unwrap();
    h.service.drain_actual().await.unwrap();
}

#[tokio::test]
async fn terminal_reopen_rejects_pending_permits_and_wrong_actual_receipt_before_retiring_the_joined_key()
 {
    let h = SummaryHarness::new().await;
    let (delivery, ready) = start(&h, 0).await;
    let permit = ready.terminal_port().try_reserve();
    let (key, audio) = audio_close(&h, &delivery).await;
    let ack = audio.wait().await.unwrap();
    assert!(delivery.validate_reopen_after_close(&ack).is_err());
    let mut closing = Box::pin(delivery.close_join_owned(h.process.clone(), key, audio));
    let held = tokio::time::timeout(Duration::from_millis(30), closing.as_mut())
        .await
        .is_err();
    assert!(delivery.reopen_after_close(&ack).is_err());
    permit.committed(Uuid::new_v4());
    closing.await.unwrap();
    h.jobs.shutdown().await.unwrap();
    let other = SummaryHarness::new().await;
    let (other_delivery, _) = start(&other, 0).await;
    let (_, wrong_audio) = audio_close(&other, &other_delivery).await;
    let wrong = wrong_audio.wait().await.unwrap();
    assert!(delivery.validate_reopen_after_close(&wrong).is_err());
    assert!(delivery.reopen_after_close(&wrong).is_err());
    delivery.validate_reopen_after_close(&ack).unwrap();
    let old = delivery.delivery_id().unwrap();
    delivery.reopen_after_close(&ack).unwrap();
    assert_ne!(delivery.delivery_id().unwrap(), old);
    assert!(
        delivery.require_epoch(0).is_err(),
        "joined retirement is Dormant, not live intelligence readiness"
    );
    assert!(held);
    close(&other, &other_delivery).await;
    close(&h, &delivery).await;
}
