mod models;
mod secret_store;

pub use models::{EmbeddingConfig, LlmConfig, ProviderConfig, PublicProviderConfig, TavilyConfig};
pub use secret_store::SecretStore;

use crate::error::{AppError, AppResult};
use std::{
    fs,
    io::Write,
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
        atomic_write(&self.path, &bytes)
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

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Configuration("configuration path has no parent".into()))?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| AppError::Configuration(e.to_string()))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::Configuration(error.to_string()));
    }
    drop(file);
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::Configuration(error.to_string()));
    }
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| AppError::Configuration(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod secret_store_tests;
