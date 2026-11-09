//! Core-local epoch capabilities. No status/bool or delivery ID creates Ready.
use super::{
    SummaryCore, operations,
    store::{error, storage},
};
use crate::{
    error::AppResult,
    intelligence::{
        jobs::{JobReapPort, OperationLease},
        types::IntelligenceErrorCode as Code,
    },
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    core: Uuid,
    epoch: u64,
    attempt: Uuid,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Closed,
    Recovering,
    Ready,
    Closing,
}
struct State {
    key: Option<Key>,
    stage: Stage,
}
pub(super) struct RecoveryControl {
    id: Uuid,
    state: Mutex<State>,
    actual: Arc<tokio::sync::Semaphore>,
}
#[derive(Clone)]
pub(crate) struct SummaryReady {
    control: Arc<RecoveryControl>,
    key: Key,
}
#[derive(Clone)]
pub(crate) struct SummaryRecoveryFence {
    control: Arc<RecoveryControl>,
    key: Key,
}
impl RecoveryControl {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: Uuid::new_v4(),
            state: Mutex::new(State {
                key: None,
                stage: Stage::Closed,
            }),
            actual: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
    fn begin(self: &Arc<Self>, epoch: u64) -> AppResult<SummaryRecoveryFence> {
        let mut state = self.state.lock().map_err(|_| storage())?;
        if state.stage == Stage::Recovering {
            return Err(error(Code::Busy));
        }
        if state.key.is_some_and(|key| {
            epoch < key.epoch || epoch == key.epoch && state.stage == Stage::Ready
        }) {
            return Err(error(Code::Interrupted));
        }
        let key = Key {
            core: self.id,
            epoch,
            attempt: Uuid::new_v4(),
        };
        state.key = Some(key);
        state.stage = Stage::Recovering;
        Ok(SummaryRecoveryFence {
            control: self.clone(),
            key,
        })
    }
    pub(super) fn ready(self: &Arc<Self>) -> AppResult<SummaryReady> {
        let state = self.state.lock().map_err(|_| storage())?;
        if state.stage != Stage::Ready {
            return Err(error(Code::Interrupted));
        }
        Ok(SummaryReady {
            control: self.clone(),
            key: state.key.ok_or_else(storage)?,
        })
    }
    pub(super) fn close(&self) -> AppResult<()> {
        self.state.lock().map_err(|_| storage())?.stage = Stage::Closing;
        Ok(())
    }
    fn fail(&self, key: Key) {
        if let Ok(mut state) = self.state.lock() {
            if state.key == Some(key) && state.stage == Stage::Recovering {
                state.stage = Stage::Closed;
            }
        }
    }
    pub(super) async fn drain(&self) -> AppResult<()> {
        let _all = self
            .actual
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| error(Code::Interrupted))?;
        Ok(())
    }
}
impl SummaryReady {
    pub(crate) fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let state = self.control.state.lock().map_err(|_| storage())?;
        if self.key.core != self.control.id
            || state.key != Some(self.key)
            || state.stage != Stage::Ready
        {
            return Err(error(Code::Interrupted));
        }
        operation()
    }
    pub(crate) fn require_epoch(&self, epoch: u64) -> AppResult<()> {
        self.run(|| {
            if self.key.epoch == epoch {
                Ok(())
            } else {
                Err(error(Code::Interrupted))
            }
        })
    }
}
impl SummaryRecoveryFence {
    pub(super) fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let state = self.control.state.lock().map_err(|_| storage())?;
        if self.key.core != self.control.id
            || state.key != Some(self.key)
            || state.stage != Stage::Recovering
        {
            return Err(error(Code::Interrupted));
        }
        operation()
    }
    fn publish(&self) -> AppResult<SummaryReady> {
        let mut state = self.control.state.lock().map_err(|_| storage())?;
        if state.key != Some(self.key) || state.stage != Stage::Recovering {
            return Err(error(Code::Interrupted));
        }
        state.stage = Stage::Ready;
        Ok(SummaryReady {
            control: self.control.clone(),
            key: self.key,
        })
    }
}
struct RecoveryOwner {
    _owner: OperationLease,
    _actual: tokio::sync::OwnedSemaphorePermit,
}
pub(super) async fn reconcile(
    core: Arc<SummaryCore>,
    owner: OperationLease,
    epoch: u64,
    reap: JobReapPort,
) -> AppResult<SummaryReady> {
    let permit = core
        .recovery
        .actual
        .clone()
        .try_acquire_owned()
        .map_err(|_| error(Code::Busy))?;
    let fence = core.recovery.begin(epoch)?;
    let observed_fence = fence.clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    let owner: OperationLease = Arc::new(RecoveryOwner {
        _owner: owner,
        _actual: permit,
    });
    let (sent, observed) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result =
            operations::recover_local(core, owner.clone(), fence.clone(), Arc::new(reap), deadline)
                .await
                .and_then(|()| {
                    operations::deadline_check(deadline)?;
                    fence.publish()
                });
        if result.is_err() {
            fence.control.fail(fence.key);
        }
        let _owner = owner;
        let _ = sent.send(result);
    });
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), observed).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            observed_fence.control.fail(observed_fence.key);
            Err(error(Code::Interrupted))
        }
        Err(_) => {
            observed_fence.control.fail(observed_fence.key);
            Err(error(Code::Timeout))
        }
    }
}
