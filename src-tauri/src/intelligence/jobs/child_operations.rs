//! Actual-work accounting, independent of async observers and handler lifetime.
use super::{OperationLease, error};
use crate::{error::AppResult, intelligence::types::IntelligenceErrorCode as Code};
use std::sync::{Arc, Weak};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const CAPACITY: u32 = 16;

#[derive(Clone)]
pub(super) struct ChildOperations {
    slots: Arc<Semaphore>,
    // Contexts/factories must not retain process ownership after generation done.
    process: Weak<dyn Send + Sync>,
}
struct ChildOperation {
    // Fields drop in declaration order: release actual process ownership before
    // making the slot available to the generation's all-permits drain.
    _process: OperationLease,
    _slot: OwnedSemaphorePermit,
}
impl ChildOperations {
    pub(super) fn new(process: &OperationLease) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(CAPACITY as usize)),
            process: Arc::downgrade(process),
        }
    }

    /// Caller holds the generation control guard and has checked job liveness.
    pub(super) fn try_acquire(&self) -> AppResult<OperationLease> {
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(Code::Busy))?;
        // A failed upgrade drops the acquired slot on this return path.
        let process = self
            .process
            .upgrade()
            .ok_or_else(|| error(Code::Interrupted))?;
        Ok(Arc::new(ChildOperation {
            _process: process,
            _slot: slot,
        }))
    }

    /// The generation has stopped admission and joined workers/owned cleanup.
    pub(super) async fn drain(self) {
        let _all = self
            .slots
            .acquire_many_owned(CAPACITY)
            .await
            .expect("private child-operation semaphore is never closed");
    }
}
