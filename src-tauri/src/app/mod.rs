//! Application state and service ownership.

use std::path::{Path, PathBuf};

use crate::session::SessionOrchestrator;
use crate::storage::LocalStore;
use crate::{error::AppResult, events::EventBus};
use std::sync::Arc;

pub use crate::config;

#[derive(Debug)]
pub struct AppState {
    data_dir: PathBuf,
    event_bus: EventBus,
    store: Arc<LocalStore>,
    orchestrator: SessionOrchestrator,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Self> {
        let store = Arc::new(LocalStore::open(&data_dir)?);
        let event_bus = EventBus::new();
        Ok(Self {
            data_dir,
            event_bus: event_bus.clone(),
            store: store.clone(),
            orchestrator: SessionOrchestrator::new(Arc::clone(&store), event_bus),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
    pub fn store(&self) -> &LocalStore {
        &self.store
    }
    pub fn orchestrator(&self) -> &SessionOrchestrator {
        &self.orchestrator
    }
}

#[cfg(test)]
mod tests;
