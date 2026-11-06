//! Application state and service ownership.

use std::path::{Path, PathBuf};

use crate::audio::platform::inventory::PlatformAudioBackend;
use crate::config::{ConfigIo, ConfigStore, SecretStore};
use crate::providers::factory::ProviderFactory;
use crate::session::{SessionOrchestrator, TranslationRuntime};
use crate::storage::LocalStore;
use crate::{error::AppResult, events::EventBus};
use std::sync::Arc;

pub use crate::config;

pub struct AppState {
    data_dir: PathBuf,
    event_bus: EventBus,
    store: Arc<LocalStore>,
    orchestrator: SessionOrchestrator,
    config_io: ConfigIo,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Self> {
        let store = Arc::new(LocalStore::open(&data_dir)?);
        let event_bus = EventBus::new();
        let secrets = Arc::new(SecretStore::new(&data_dir)?);
        let config_io = ConfigIo::new(ConfigStore::new(&data_dir, secrets.clone())?);
        let runtime = Arc::new(TranslationRuntime::new(
            Arc::new(ProviderFactory::new(secrets)),
            Arc::new(PlatformAudioBackend::new()),
            store.clone(),
            event_bus.clone(),
        ));
        Ok(Self {
            data_dir,
            event_bus: event_bus.clone(),
            store: store.clone(),
            orchestrator: SessionOrchestrator::with_runtime(Arc::clone(&store), event_bus, runtime),
            config_io,
        })
    }

    pub(crate) fn config_io(&self) -> &ConfigIo {
        &self.config_io
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
