use super::*;
use crate::audio::join::AudioRegistry;
use std::sync::atomic::{AtomicBool, Ordering};

struct Child {
    exit: watch::Receiver<Option<i32>>,
    wait_error: bool,
    kill_error: bool,
    alive: Arc<AtomicBool>,
}
impl Drop for Child {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}
#[async_trait]
impl OwnedChild for Child {
    async fn wait(&mut self) -> ExitResult {
        if self.wait_error {
            self.wait_error = false;
            return Err(AudioFailure::device("injected wait observation error"));
        }
        loop {
            if let Some(code) = *self.exit.borrow_and_update() {
                return Ok(ProcessExit { code: Some(code) });
            }
            self.exit.changed().await.unwrap();
        }
    }
    fn kill(&mut self) -> Result<(), AudioFailure> {
        if self.kill_error {
            Err(AudioFailure::device("injected kill observation error"))
        } else {
            Ok(())
        }
    }
}

// Losing a result observer must not drop the entered child or lose late reap.
#[tokio::test]
async fn actual_audio_reap_survives_wait_and_kill_observation_errors() {
    for wait_error in [true, false] {
        let registry = AudioRegistry::new();
        let parent = registry.stream().unwrap();
        let helpers = parent.helpers();
        let group = helpers.begin().unwrap();
        let (source, actual) = group.leaf().unwrap().enter().unwrap();
        let (exit, rx) = watch::channel(None);
        let alive = Arc::new(AtomicBool::new(true));
        let (control, run) = owner(
            Box::new(Child {
                exit: rx,
                wait_error,
                kill_error: true,
                alive: alive.clone(),
            }),
            Duration::from_millis(10),
        );
        source.run_owned(run);
        let failed = if wait_error {
            control.wait().await
        } else {
            control.terminate().await
        };
        let still_owned = alive.load(Ordering::Acquire);
        let pending = tokio::time::timeout(Duration::from_millis(20), actual.wait())
            .await
            .is_err();
        exit.send_replace(Some(7));
        let reaped = tokio::time::timeout(Duration::from_millis(300), actual.wait())
            .await
            .is_ok();
        let original_error = control.wait().await.is_err();
        drop(control);
        assert!(failed.is_err());
        assert!(
            still_owned,
            "failed wait/kill observation must retain the original child owner"
        );
        assert!(pending, "an observer error is not positive actual exit");
        assert!(
            reaped,
            "late nonzero exit must complete the original actual source"
        );
        assert!(
            original_error,
            "actual reap must not rewrite a failed command result"
        );
        group.join().await.unwrap();
        parent.finish().await.unwrap();
    }
}

#[tokio::test]
async fn actual_audio_exit_waits_for_pipe_descriptor_release() {
    let registry = AudioRegistry::new();
    let parent = registry.stream().unwrap();
    let group = parent.helpers().begin().unwrap();
    let (source, actual) = group.leaf().unwrap().enter().unwrap();
    let (exit, rx) = watch::channel(Some(0));
    let (control, run) = owner(
        Box::new(Child {
            exit: rx,
            wait_error: false,
            kill_error: false,
            alive: Arc::new(AtomicBool::new(true)),
        }),
        Duration::from_millis(10),
    );
    let process = AudioOwnedProcess::attach(
        ProcessPipes {
            stdin: None,
            stdout: Some(Box::pin(std::io::Cursor::new(b"[]"))),
            control,
        },
        actual.clone(),
        &source,
    );
    source.run_owned(run);
    let held = tokio::time::timeout(Duration::from_millis(20), actual.wait())
        .await
        .is_err();
    drop(process);
    actual.wait().await.unwrap();
    group.join().await.unwrap();
    parent.finish().await.unwrap();
    drop(exit);
    assert!(
        held,
        "positive process exit alone cannot release an owned descriptor"
    );
}

struct LostSpawnObserver {
    exit: watch::Sender<Option<i32>>,
    alive: Arc<AtomicBool>,
}
#[async_trait]
impl ProcessSpawner for LostSpawnObserver {
    async fn spawn(&self, _spec: ProcessSpec) -> AppResult<ProcessPipes> {
        Err(audio_error("fixture only supplies actual audio ownership"))
    }
    async fn spawn_audio_owned(
        &self,
        _spec: ProcessSpec,
        execution: AudioLeafExecution,
    ) -> AppResult<AudioOwnedProcess> {
        let (source, actual) = execution.enter()?;
        let (control, run) = owner(
            Box::new(Child {
                exit: self.exit.subscribe(),
                wait_error: false,
                kill_error: true,
                alive: self.alive.clone(),
            }),
            Duration::from_millis(10),
        );
        let _process = AudioOwnedProcess::attach(
            ProcessPipes {
                stdin: None,
                stdout: Some(Box::pin(std::io::Cursor::new(b"[]"))),
                control,
            },
            actual,
            &source,
        );
        source.run_owned(run);
        std::future::pending().await
    }
}

#[tokio::test]
async fn actual_audio_spawn_observer_loss_without_pipes_keeps_the_original_child_and_group() {
    let registry = AudioRegistry::new();
    let parent = registry.inventory().await.unwrap();
    let helpers = parent.helpers();
    let group = helpers.begin().unwrap();
    let (exit, _) = watch::channel(None);
    let alive = Arc::new(AtomicBool::new(true));
    let spawner = LostSpawnObserver {
        exit,
        alive: alive.clone(),
    };
    let failed = output_timed(
        &spawner,
        &[],
        group.leaf().unwrap(),
        Duration::from_millis(10),
    )
    .await
    .is_err();
    drop(group);
    let still_owned = alive.load(Ordering::Acquire);
    let rejected = helpers.begin().is_err();
    let finish = parent.finish();
    tokio::pin!(finish);
    let pending = tokio::time::timeout(Duration::from_millis(20), &mut finish)
        .await
        .is_err();
    spawner.exit.send_replace(Some(7));
    finish.await.unwrap();
    assert!(
        failed && still_owned && rejected && pending,
        "a failed spawn observer returning no pipes must retain its original entered actual group until positive reap"
    );
    assert!(!alive.load(Ordering::Acquire));
}
