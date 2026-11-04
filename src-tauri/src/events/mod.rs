//! Application event delivery boundaries.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// The process-wide event bus owned by [`crate::app::AppState`].
///
/// The concrete session event stream is added by the session foundation's
/// event task. Keeping health and ownership here gives callers a stable
/// boundary while that stream evolves.
#[derive(Clone, Debug)]
pub struct EventBus {
    healthy: Arc<AtomicBool>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
