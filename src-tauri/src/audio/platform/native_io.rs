//! A bounded controller around a native, deliberately non-Send owner.
use super::inventory::Lease;
use crate::{
    audio::{AudioFrame, queue::BoundedAudioQueue, runtime::*},
    domain::transcript::{AudioStatistics, RuntimeFailureCode},
    error::AppResult,
};
use async_trait::async_trait;
use std::{
    sync::{Arc, atomic::Ordering, mpsc as sync_mpsc},
    time::Duration,
};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

pub(crate) trait NativeOwnerFactory: Send + Sync {
    fn open_on_owner_thread(
        &self,
        request: &AudioOpenRequest,
        capture: BoundedAudioQueue,
        playback: mpsc::Receiver<AudioFrame>,
        shared: AudioShared,
    ) -> AppResult<Box<dyn NativeOwner>>;
}
pub trait NativeOwner {
    fn stop_capture(&mut self) -> AppResult<()>;
    fn discard_pending(&mut self) -> AppResult<()>;
    fn seal_playback(&mut self) -> AppResult<()>;
    fn playback_drained(&mut self) -> AppResult<Option<PlaybackReport>>;
    fn close(&mut self) -> AppResult<()>;
    fn lost(&self) -> bool {
        false
    }
}
#[derive(Clone, Copy)]
enum Operation {
    Capture,
    Discard,
    Drain,
    Close,
}
type ResultValue = Result<Option<PlaybackReport>, AudioFailure>;
struct Command {
    operation: Operation,
    reply: oneshot::Sender<ResultValue>,
}
#[derive(Clone, Copy)]
pub(crate) struct Deadlines {
    pub open: Duration,
    pub close: Duration,
    pub drain: Duration,
}
impl Default for Deadlines {
    fn default() -> Self {
        Self {
            open: Duration::from_secs(3),
            close: Duration::from_secs(2),
            drain: Duration::from_secs(5),
        }
    }
}
struct NativeControl {
    tx: sync_mpsc::SyncSender<Command>,
    shared: AudioShared,
    capture: BoundedAudioQueue,
    health: watch::Sender<AudioHealth>,
    operations: [Mutex<Option<watch::Receiver<Option<ResultValue>>>>; 4],
    deadlines: Deadlines,
    lease: Option<Arc<Lease>>,
}
impl NativeControl {
    async fn request(&self, operation: Operation) -> AppResult<Option<PlaybackReport>> {
        let index = operation as usize;
        let mut entry = self.operations[index].lock().await;
        let mut result = if let Some(result) = &*entry {
            result.clone()
        } else {
            let (publish, result) = watch::channel(None);
            let (reply, receive) = oneshot::channel();
            if self.tx.try_send(Command { operation, reply }).is_err() {
                return Err(audio_error("native audio owner is unavailable"));
            }
            let shared = self.shared.clone();
            let health = self.health.clone();
            let lease = self.lease.clone();
            let deadline = if matches!(operation, Operation::Drain) {
                self.deadlines.drain
            } else {
                self.deadlines.close
            };
            tokio::spawn(async move {
                let value = match tokio::time::timeout(deadline, receive).await {
                    Ok(Ok(value)) => value,
                    _ => {
                        shared.suppress();
                        health.send_replace(AudioHealth::NeedsRestart);
                        if let Some(lease) = &lease {
                            lease.quarantine();
                        }
                        Err(AudioFailure::new(
                            if matches!(operation, Operation::Drain) {
                                RuntimeFailureCode::PlaybackTimeout
                            } else {
                                RuntimeFailureCode::CleanupTimeout
                            },
                            "native audio acknowledgement timed out",
                        ))
                    }
                };
                publish.send_replace(Some(value));
            });
            *entry = Some(result.clone());
            result
        };
        drop(entry);
        loop {
            if let Some(value) = result.borrow_and_update().clone() {
                return value.map_err(Into::into);
            }
            result
                .changed()
                .await
                .map_err(|_| audio_error("native audio acknowledgement closed"))?;
        }
    }
}
impl Drop for NativeControl {
    fn drop(&mut self) {
        self.shared.suppress();
    }
}
#[async_trait]
impl AudioControl for NativeControl {
    fn suppress_now(&self) {
        self.shared.suppress();
    }
    fn statistics(&self) -> AudioStatistics {
        self.shared.statistics()
    }
    fn playback_overflow_generation(&self) -> u64 {
        self.shared
            .counters
            .playback_overflow_generation
            .load(Ordering::Acquire)
    }
    async fn playback_overflow_changed(&self, observed: u64) {
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.playback_overflow_generation() != observed {
                return;
            }
            changed.await;
        }
    }
    async fn stop_capture(&self) -> AppResult<()> {
        self.shared.stop_capture();
        self.request(Operation::Capture).await.map(|_| ())
    }
    async fn discard_pending(&self) -> AppResult<()> {
        self.shared.suppress();
        self.capture.clear();
        self.request(Operation::Discard).await.map(|_| ())
    }
    async fn drain_playback(&self) -> AppResult<PlaybackReport> {
        self.request(Operation::Drain)
            .await?
            .ok_or_else(|| audio_error("missing native playback drain report"))
    }
    async fn close(&self) -> AppResult<()> {
        self.shared.suppress();
        self.request(Operation::Close).await.map(|_| ())
    }
}
#[cfg(test)]
pub(crate) async fn open_with_factory(
    request: AudioOpenRequest,
    factory: Arc<dyn NativeOwnerFactory>,
) -> AppResult<AudioPorts> {
    open_owned(request, factory, None, Deadlines::default()).await
}
pub(crate) async fn open_owned(
    request: AudioOpenRequest,
    factory: Arc<dyn NativeOwnerFactory>,
    lease: Option<Arc<Lease>>,
    deadlines: Deadlines,
) -> AppResult<AudioPorts> {
    let capture = BoundedAudioQueue::default();
    let shared = AudioShared::new(capture.clone());
    let (playback, input) = mpsc::channel(50);
    let (health, health_rx) = watch::channel(AudioHealth::Ready);
    let (tx, rx) = sync_mpsc::sync_channel::<Command>(8);
    let (ready, ready_rx) = oneshot::channel();
    let control = Arc::new(NativeControl {
        tx,
        shared: shared.clone(),
        capture: capture.clone(),
        health: health.clone(),
        operations: std::array::from_fn(|_| Mutex::new(None)),
        deadlines,
        lease: lease.clone(),
    });
    let owner_capture = capture.clone();
    let owner_lease = lease.clone();
    let owner_shared = shared.clone();
    std::thread::Builder::new()
        .name("vox-native-audio-owner".into())
        .spawn(move || {
            let shared = owner_shared;
            let opened = factory.open_on_owner_thread(
                &request,
                owner_capture.clone(),
                input,
                shared.clone(),
            );
            let mut owner = match opened {
                Ok(owner) => owner,
                Err(error) => {
                    if let Some(lease) = owner_lease {
                        lease.release();
                    }
                    let _ = ready.send(Err(AudioFailure::from_error(error)));
                    return;
                }
            };
            if ready.send(Ok(())).is_err() {
                shared.suppress();
                let result = owner.close();
                drop(owner);
                if let Some(lease) = &owner_lease {
                    if result.is_ok() {
                        lease.release();
                    } else {
                        lease.quarantine();
                    }
                }
                return;
            }
            let mut draining: Option<oneshot::Sender<ResultValue>> = None;
            let mut final_reply = None;
            let mut discarded = false;
            let value = loop {
                if !discarded
                    && (owner.lost() || shared.gates.abort_pending.load(Ordering::Acquire))
                {
                    if owner.lost() {
                        mark_lost(&health);
                    }
                    shared.suppress();
                    owner_capture.clear();
                    let _ = owner.discard_pending();
                    discarded = true;
                }
                match rx.recv_timeout(Duration::from_millis(2)) {
                    Ok(command) => match command.operation {
                        Operation::Capture => {
                            let value = owner
                                .stop_capture()
                                .map(|_| {
                                    owner_capture.close();
                                    None
                                })
                                .map_err(AudioFailure::from_error);
                            let _ = command.reply.send(value);
                        }
                        Operation::Discard => {
                            let _ = command.reply.send(
                                owner
                                    .discard_pending()
                                    .map(|_| None)
                                    .map_err(AudioFailure::from_error),
                            );
                        }
                        Operation::Drain => match owner.seal_playback() {
                            Ok(()) => draining = Some(command.reply),
                            Err(_) => {
                                let _ = command
                                    .reply
                                    .send(Err(AudioFailure::device("native playback seal failed")));
                            }
                        },
                        Operation::Close => {
                            shared.suppress();
                            let value = owner
                                .close()
                                .map(|_| None)
                                .map_err(AudioFailure::from_error);
                            final_reply = Some(command.reply);
                            break value;
                        }
                    },
                    Err(sync_mpsc::RecvTimeoutError::Disconnected) => {
                        shared.suppress();
                        break owner
                            .close()
                            .map(|_| None)
                            .map_err(AudioFailure::from_error);
                    }
                    Err(sync_mpsc::RecvTimeoutError::Timeout) => {}
                }
                if draining.is_some() && shared.gates.abort_pending.load(Ordering::Acquire) {
                    let _ = draining
                        .take()
                        .unwrap()
                        .send(Err(AudioFailure::device("native playback was suppressed")));
                }
                if draining.is_some() {
                    match owner.playback_drained() {
                        Ok(Some(report)) => {
                            let _ = draining.take().unwrap().send(Ok(Some(report)));
                        }
                        Err(_) => {
                            let _ = draining
                                .take()
                                .unwrap()
                                .send(Err(AudioFailure::device("native playback drain failed")));
                        }
                        _ => {}
                    }
                }
            };
            // A native destructor can itself block. Acknowledge/release only after it returns.
            drop(owner);
            owner_capture.close();
            if let Some(lease) = &owner_lease {
                if value.is_ok() {
                    lease.release();
                } else {
                    lease.quarantine();
                }
            }
            if value.is_err() {
                health.send_replace(AudioHealth::NeedsRestart);
            }
            if let Some(reply) = final_reply {
                let _ = reply.send(value);
            }
        })
        .map_err(|_| {
            if let Some(lease) = &lease {
                lease.release();
            }
            audio_error("native audio owner thread could not start")
        })?;
    match tokio::time::timeout(deadlines.open, ready_rx).await {
        Ok(Ok(Ok(()))) => Ok(AudioPorts {
            capture,
            playback,
            health: health_rx,
            control,
        }),
        Ok(Ok(Err(error))) => Err(error.into()),
        _ => {
            shared.suppress();
            if let Some(lease) = lease {
                lease.quarantine();
            }
            Err(AudioFailure::new(
                RuntimeFailureCode::StartupTimeout,
                "native audio open acknowledgement timed out",
            )
            .into())
        }
    }
}
