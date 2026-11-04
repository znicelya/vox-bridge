mod models;
mod secret_store;

pub use models::{EmbeddingConfig, LlmConfig, ProviderConfig, PublicProviderConfig, TavilyConfig};
pub use secret_store::SecretStore;

use crate::error::{AppError, AppResult};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct ConfigStore {
    path: PathBuf,
    secrets: SecretStore,
}

impl ConfigStore {
    pub fn new(data_dir: impl AsRef<Path>, secrets: SecretStore) -> AppResult<Self> {
        let path = data_dir.as_ref().join("provider-configs.json");
        Ok(Self { path, secrets })
    }

    pub fn secret_store(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn save_provider_config(&mut self, config: ProviderConfig) -> AppResult<()> {
        config.validate()?;
        let mut configs: Vec<ProviderConfig> = self.read_configs()?;
        configs.retain(|existing| existing.id() != config.id());
        configs.push(config);
        let bytes = serde_json::to_vec_pretty(&configs)
            .map_err(|e| AppError::Configuration(e.to_string()))?;
        fs::write(&self.path, bytes).map_err(|e| AppError::Configuration(e.to_string()))
    }

    pub fn load_provider_config(&self, id: Uuid) -> AppResult<ProviderConfig> {
        self.read_configs()?
            .into_iter()
            .find(|config| config.id() == id)
            .ok_or_else(|| AppError::Configuration(format!("provider config not found: {id}")))
    }

    fn read_configs(&self) -> AppResult<Vec<ProviderConfig>> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| AppError::Configuration(e.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(AppError::Configuration(error.to_string())),
        }
    }
}

#[cfg(test)]
mod secret_store_tests;
