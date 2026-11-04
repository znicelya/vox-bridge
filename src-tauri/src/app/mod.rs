//! Application state and service ownership.

use std::path::{Path, PathBuf};

use crate::storage::LocalStore;
use crate::{error::AppResult, events::EventBus};

pub use crate::config;

#[derive(Debug)]
pub struct AppState {
    data_dir: PathBuf,
    event_bus: EventBus,
    store: LocalStore,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Self> {
        let store = LocalStore::open(&data_dir)?;
        Ok(Self {
            data_dir,
            event_bus: EventBus::new(),
            store,
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
}

#[cfg(test)]
mod tests;
