//! Two admitted searches retain handles and actual-completion capacity.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch};
struct SearchOwner {
    _parent: OperationLease,
    _permit: OwnedSemaphorePermit,
}
struct Slot {
    cancel: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
pub(super) struct Searches {
    permits: Arc<Semaphore>,
    slots: Mutex<Vec<Slot>>,
    closed: AtomicBool,
}
struct Observer(watch::Sender<bool>);
impl Drop for Observer {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}
impl Searches {
    pub(super) fn new() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(2)),
            slots: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        }
    }
    pub(super) async fn run(
        &self,
        core: Arc<Core>,
        request: KnowledgeSearch,
        parent: OperationLease,
    ) -> AppResult<KnowledgeResults> {
        crate::knowledge::search::validate(&core.store, &request)?;
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(Code::Busy))?;
        let (cancel, receiver) = watch::channel(false);
        let (reply, result) = oneshot::channel();
        {
            let mut slots = self.slots.lock().unwrap();
            slots.retain(|s| !s.task.is_finished());
            if self.closed.load(Ordering::Acquire) || slots.len() >= 2 {
                return Err(error(Code::Busy));
            }
            for slot in slots.iter() {
                slot.cancel.send_replace(true);
            }
            let owner: OperationLease = Arc::new(SearchOwner {
                _parent: parent,
                _permit: permit,
            });
            let task = tokio::spawn(async move {
                let outcome = core.search(request, owner, receiver).await;
                let _ = reply.send(outcome);
            });
            slots.push(Slot {
                cancel: cancel.clone(),
                task,
            });
        }
        let _observer = Observer(cancel);
        result.await.map_err(|_| error(Code::Interrupted))?
    }
    pub(super) async fn shutdown(&self) -> AppResult<()> {
        self.closed.store(true, Ordering::Release);
        let mut slots = std::mem::take(&mut *self.slots.lock().unwrap());
        for slot in &slots {
            slot.cancel.send_replace(true);
        }
        while let Some(mut slot) = slots.pop() {
            match tokio::time::timeout(std::time::Duration::from_secs(10), &mut slot.task).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {}
                Err(_) => {
                    slots.push(slot);
                    self.slots.lock().unwrap().extend(slots);
                    return Err(error(Code::Timeout));
                }
            }
        }
        Ok(())
    }
    pub(super) fn reopen(&self) -> AppResult<()> {
        let mut slots = self.slots.lock().unwrap();
        if slots.iter().any(|s| !s.task.is_finished()) {
            return Err(error(Code::Busy));
        }
        slots.clear();
        self.closed.store(false, Ordering::Release);
        Ok(())
    }
}

impl Drop for Searches {
    fn drop(&mut self) {
        for slot in self.slots.get_mut().unwrap().iter() {
            slot.cancel.send_replace(true);
        }
    }
}
