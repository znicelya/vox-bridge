use super::*;
use crate::{
    providers::{
        ProviderEvent,
        fixture::{FixtureScript, FixtureStep},
    },
    session::test_support::{RuntimeHarness, final_text, metadata, wait_phase},
};

// Losing the supervisor does not end the actor's issued SQLite write. Even
// the error observer must wait for its real actor handle before returning.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_supervisor_keeps_actual_actor_sql_join_owned_until_external_release() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "owned final"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let final_event = h.provider.delay_final(SessionDirection::SourceToTarget);
    let (native_done, release_native) = h.audio.delay_actual_join("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let event_entered =
        tokio::time::timeout(Duration::from_secs(2), final_event.wait_entered()).await;
    let (entered, waiting) = oneshot::channel();
    let (released, release_observed) = oneshot::channel();
    let blocker = std::thread::spawn({
        let store = h.store.clone();
        move || {
            let result = store.transaction(|_| {
                let _ = entered.send(());
                std::thread::sleep(Duration::from_millis(450));
                Ok(())
            });
            let _ = released.send(());
            result
        }
    });
    waiting.await.unwrap();
    let (sql_entered, sql_waiting) = oneshot::channel();
    *h.store.oo1_transcript_entry.lock().unwrap() = Some(sql_entered);
    final_event.release();
    let sql_issued = tokio::time::timeout(Duration::from_secs(2), sql_waiting).await;
    run.actual.lock().await.as_ref().unwrap().abort();
    let early = tokio::time::timeout(Duration::from_millis(30), run.join_actual()).await;
    let waited_for_actual_sql = early.is_err();
    release_observed.await.unwrap();
    blocker.join().unwrap().unwrap();
    let outcome = match early {
        Ok(outcome) => outcome,
        Err(_) => run.join_actual().await,
    };
    let native_leaf_joined = matches!(
        tokio::time::timeout(Duration::from_secs(2), native_done).await,
        Ok(Ok(()))
    );
    let released_parent = release_native.send(()).is_ok();
    let key =
        crate::audio::join::AudioCloseKey::new(0, Uuid::new_v4(), h.audio.actual_epoch().unwrap());
    let close = h.audio.begin_close(key).unwrap();
    let native_actual_joined = matches!(
        tokio::time::timeout(Duration::from_secs(2), close.wait()).await,
        Ok(Ok(_))
    );
    let repeated_join = tokio::time::timeout(Duration::from_secs(2), run.join_actual()).await;
    assert!(matches!(event_entered, Ok(true)) && matches!(sql_issued, Ok(Ok(()))));
    assert!(native_leaf_joined && released_parent && native_actual_joined);
    assert!(
        outcome.is_err(),
        "lost supervisor cannot publish normal runtime completion"
    );
    assert!(
        waited_for_actual_sql,
        "supervisor error released its observer before the original actor SQL actually joined"
    );
    assert!(
        matches!(repeated_join, Ok(Err(_))),
        "retrying the actual join must retain the original supervisor failure"
    );
}
