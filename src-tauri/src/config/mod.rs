mod models;
mod secret_store;
mod worker;

pub use models::{EmbeddingConfig, LlmConfig, ProviderConfig, PublicProviderConfig, TavilyConfig};
pub(crate) use secret_store::EncryptedSecretSnapshot;
pub use secret_store::SecretStore;
pub(crate) use worker::{ConfigIo, ConfigTransaction};

use crate::error::{AppError, AppResult};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

pub struct ConfigStore {
    data_dir: PathBuf,
    path: PathBuf,
    lock_path: PathBuf,
    secrets: Arc<SecretStore>,
}

impl ConfigStore {
    pub fn new(data_dir: impl AsRef<Path>, secrets: Arc<SecretStore>) -> AppResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let path = data_dir.join("provider-configs.json");
        let lock_path = data_dir.join("provider-configs.lock");
        Ok(Self {
            data_dir,
            path,
            lock_path,
            secrets,
        })
    }

    pub fn secret_store(&self) -> &SecretStore {
        &self.secrets
    }

    pub fn save_provider_config(&mut self, config: ProviderConfig) -> AppResult<()> {
        let _lock = self.acquire_config_lock()?;
        self.save_provider_config_locked(config)
    }

    pub fn load_provider_config(&self, id: Uuid) -> AppResult<ProviderConfig> {
        let _lock = self.acquire_config_lock()?;
        self.load_provider_config_locked(id)
    }

    pub fn list_provider_configs(&self) -> AppResult<Vec<ProviderConfig>> {
        let _lock = self.acquire_config_lock()?;
        self.read_configs_locked()
    }

    pub fn delete_provider_config(&mut self, id: Uuid) -> AppResult<()> {
        let _lock = self.acquire_config_lock()?;
        self.delete_provider_config_locked(id)
    }

    pub(super) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub(super) fn acquire_config_lock(&self) -> AppResult<fs::File> {
        fs::create_dir_all(&self.data_dir).map_err(config_error)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(config_error)?;
        fs2::FileExt::lock_exclusive(&lock).map_err(config_error)?;
        Ok(lock)
    }

    pub(super) fn save_provider_config_locked(&mut self, config: ProviderConfig) -> AppResult<()> {
        config.validate()?;
        let mut configs = self.read_configs_locked()?;
        configs.retain(|existing| existing.id() != config.id());
        configs.push(config);
        let bytes = serde_json::to_vec_pretty(&configs)
            .map_err(|e| AppError::Configuration(e.to_string()))?;
        atomic_write(&self.path, &bytes)
    }

    pub(super) fn load_provider_config_locked(&self, id: Uuid) -> AppResult<ProviderConfig> {
        self.read_configs_locked()?
            .into_iter()
            .find(|config| config.id() == id)
            .ok_or_else(|| AppError::Configuration(format!("provider config not found: {id}")))
    }

    pub(super) fn list_provider_configs_locked(&self) -> AppResult<Vec<ProviderConfig>> {
        self.read_configs_locked()
    }

    pub(super) fn delete_provider_config_locked(&mut self, id: Uuid) -> AppResult<()> {
        let mut configs = self.read_configs_locked()?;
        let previous_len = configs.len();
        configs.retain(|config| config.id() != id);
        if configs.len() == previous_len {
            return Err(AppError::Configuration(format!(
                "provider config not found: {id}"
            )));
        }
        let bytes = serde_json::to_vec_pretty(&configs)
            .map_err(|error| AppError::Configuration(error.to_string()))?;
        atomic_write(&self.path, &bytes)
    }

    fn read_configs_locked(&self) -> AppResult<Vec<ProviderConfig>> {
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
    atomic_write_with(path, bytes, |temp, bytes| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })
}

fn atomic_write_with<F>(path: &Path, bytes: &[u8], write: F) -> AppResult<()>
where
    F: FnOnce(&Path, &[u8]) -> std::io::Result<()>,
{
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Configuration("configuration path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(config_error)?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config"),
        Uuid::new_v4()
    ));
    if let Err(error) = write(&temp, bytes) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::Configuration(error.to_string()));
    }
    if let Err(error) = replace_destination(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::Configuration(error.to_string()));
    }
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| AppError::Configuration(e.to_string()))?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_destination(temp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temp, destination)
}

#[cfg(windows)]
fn replace_destination(temp: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn config_error(error: impl std::fmt::Display) -> AppError {
    AppError::Configuration(error.to_string())
}

#[cfg(test)]
mod secret_store_tests;
#[cfg(test)]
mod worker_tests;
