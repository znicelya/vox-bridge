//! Application state and service ownership.

use std::path::{Path, PathBuf};

use crate::{error::AppResult, events::EventBus};

#[derive(Debug)]
pub struct AppState {
    data_dir: PathBuf,
    event_bus: EventBus,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Self> {
        Ok(Self {
            data_dir,
            event_bus: EventBus::new(),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

#[cfg(test)]
mod tests;
