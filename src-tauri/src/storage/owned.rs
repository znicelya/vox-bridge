//! One retained blocking operation for an existing autonomous storage owner.
use crate::error::{AppError, AppResult};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinHandle,
};

#[derive(Default)]
pub(crate) struct SqlSlot {
    task: Mutex<Option<JoinHandle<()>>>,
    failed: Arc<AtomicBool>,
}

impl SqlSlot {
    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub(crate) async fn run<T, G>(
        &self,
        guard: G,
        operation: impl FnOnce() -> AppResult<T> + Send + 'static,
    ) -> AppResult<T>
    where
        T: Send + 'static,
        G: Send + 'static,
    {
        let mut task = self.task.lock().await;
        // A lost observer leaves its real handle here. Only drain may consume
        // that outstanding operation; a replacement cannot detach it.
        if task.is_some() || self.failed.load(Ordering::Acquire) {
            return Err(failure());
        }
        let (send, receive) = oneshot::channel();
        let failed = self.failed.clone();
        *task = Some(tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let result = operation();
            if send.send(result).is_err() {
                failed.store(true, Ordering::Release);
            }
        }));
        let joined = task.as_mut().expect("issued storage handle").await;
        task.take();
        if joined.is_err() {
            self.failed.store(true, Ordering::Release);
            return Err(failure());
        }
        receive.await.map_err(|_| failure())?
    }

    pub(crate) async fn drain(&self) -> AppResult<()> {
        let mut task = self.task.lock().await;
        if let Some(issued) = task.as_mut() {
            if issued.await.is_err() {
                self.failed.store(true, Ordering::Release);
            }
            task.take();
        }
        if self.failed.load(Ordering::Acquire) {
            Err(failure())
        } else {
            Ok(())
        }
    }
}

fn failure() -> AppError {
    AppError::Internal("owned storage operation did not complete normally".into())
}

#[cfg(test)]
mod tests;
