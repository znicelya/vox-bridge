use crate::error::{AppError, AppResult};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

const KEY_SERVICE: &str = "com.voxbridge.secrets";
const KEY_ACCOUNT: &str = "data-encryption-key";

pub(crate) trait KeyBackend: Send + Sync {
    fn load_or_create(&self, lock_path: &Path) -> AppResult<Vec<u8>>;
}
struct OsKeyring;
impl KeyBackend for OsKeyring {
    fn load_or_create(&self, lock_path: &Path) -> AppResult<Vec<u8>> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(config_error)?;
        lock.lock_exclusive().map_err(config_error)?;
        let entry = keyring::Entry::new(KEY_SERVICE, KEY_ACCOUNT).map_err(config_error)?;
        match entry.get_password() {
            Ok(value) => STANDARD.decode(value).map_err(config_error),
            Err(keyring::Error::NoEntry) => {
                let mut key = vec![0; 32];
                OsRng.fill_bytes(&mut key);
                entry
                    .set_password(&STANDARD.encode(&key))
                    .map_err(config_error)?;
                Ok(key)
            }
            Err(error) => Err(config_error(error)),
        }
    }
}
#[cfg(test)]
struct MemoryKeyring(std::sync::Mutex<Option<Vec<u8>>>);
#[cfg(test)]
impl KeyBackend for MemoryKeyring {
    fn load_or_create(&self, _lock_path: &Path) -> AppResult<Vec<u8>> {
        let mut stored = self
            .0
            .lock()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        Ok(stored
            .get_or_insert_with(|| {
                let mut key = vec![0; 32];
                OsRng.fill_bytes(&mut key);
                key
            })
            .clone())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct EncryptedValue {
    nonce: String,
    ciphertext: String,
}
pub(crate) struct EncryptedSecretSnapshot(Option<EncryptedValue>);

pub struct SecretStore {
    data_dir: PathBuf,
    path: PathBuf,
    transaction_lock_path: PathBuf,
    dek_lock_path: PathBuf,
    backend: Arc<dyn KeyBackend>,
}
impl SecretStore {
    pub fn new(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        Self::with_backend(data_dir, Arc::new(OsKeyring))
    }
    fn with_backend(data_dir: impl AsRef<Path>, backend: Arc<dyn KeyBackend>) -> AppResult<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        Ok(Self {
            path: data_dir.join("secrets.json"),
            transaction_lock_path: data_dir.join("secrets.lock"),
            dek_lock_path: data_dir.join("secrets.dek.lock"),
            data_dir,
            backend,
        })
    }
    #[cfg(test)]
    pub fn for_tests(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        Self::with_backend(
            data_dir,
            Arc::new(MemoryKeyring(std::sync::Mutex::new(None))),
        )
    }
    #[cfg(test)]
    pub(crate) fn test_backend() -> Arc<dyn KeyBackend> {
        Arc::new(MemoryKeyring(std::sync::Mutex::new(None)))
    }
    #[cfg(test)]
    pub(crate) fn with_test_backend(
        data_dir: impl AsRef<Path>,
        backend: Arc<dyn KeyBackend>,
    ) -> AppResult<Self> {
        Self::with_backend(data_dir, backend)
    }
    #[cfg(test)]
    pub(super) fn backend_key_for_tests(&self) -> AppResult<Vec<u8>> {
        self.backend.load_or_create(&self.dek_lock_path)
    }
    pub fn config_path(&self) -> &Path {
        &self.path
    }
    pub fn set(&self, service: &str, key: &str, value: &[u8]) -> AppResult<()> {
        let _lock = self.acquire_transaction_lock()?;
        self.set_locked(service, key, value)
    }
    pub fn get(&self, service: &str, key: &str) -> AppResult<Option<Vec<u8>>> {
        let _lock = self.acquire_transaction_lock()?;
        self.get_locked(service, key)
    }
    pub fn contains(&self, service: &str, key: &str) -> AppResult<bool> {
        let _lock = self.acquire_transaction_lock()?;
        self.contains_locked(service, key)
    }
    pub fn delete(&self, service: &str, key: &str) -> AppResult<()> {
        let _lock = self.acquire_transaction_lock()?;
        self.delete_locked(service, key)
    }

    pub(super) fn acquire_transaction_lock(&self) -> AppResult<fs::File> {
        fs::create_dir_all(&self.data_dir).map_err(config_error)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.transaction_lock_path)
            .map_err(config_error)?;
        fs2::FileExt::lock_exclusive(&lock).map_err(config_error)?;
        Ok(lock)
    }

    pub(super) fn set_locked(&self, service: &str, key: &str, value: &[u8]) -> AppResult<()> {
        let mut values = self.read_values_locked()?;
        let cipher = Aes256Gcm::new_from_slice(&self.backend.load_or_create(&self.dek_lock_path)?)
            .map_err(config_error)?;
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), value)
            .map_err(config_error)?;
        values.insert(
            entry_key(service, key),
            EncryptedValue {
                nonce: STANDARD.encode(nonce),
                ciphertext: STANDARD.encode(ciphertext),
            },
        );
        super::atomic_write(
            &self.path,
            &serde_json::to_vec_pretty(&values).map_err(config_error)?,
        )
    }
    pub(super) fn get_locked(&self, service: &str, key: &str) -> AppResult<Option<Vec<u8>>> {
        let values = self.read_values_locked()?;
        let Some(value) = values.get(&entry_key(service, key)) else {
            return Ok(None);
        };
        let nonce = STANDARD.decode(&value.nonce).map_err(config_error)?;
        if nonce.len() != 12 {
            return Err(AppError::Configuration(
                "encrypted value nonce must be 12 bytes".into(),
            ));
        }
        let ciphertext = STANDARD.decode(&value.ciphertext).map_err(config_error)?;
        let cipher = Aes256Gcm::new_from_slice(&self.backend.load_or_create(&self.dek_lock_path)?)
            .map_err(config_error)?;
        cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map(Some)
            .map_err(config_error)
    }
    pub(super) fn contains_locked(&self, service: &str, key: &str) -> AppResult<bool> {
        Ok(self
            .read_values_locked()?
            .contains_key(&entry_key(service, key)))
    }

    pub(super) fn delete_locked(&self, service: &str, key: &str) -> AppResult<()> {
        let mut values = self.read_values_locked()?;
        values.remove(&entry_key(service, key));
        self.write_values_locked(&values)
    }

    pub(super) fn snapshot_locked(
        &self,
        service: &str,
        key: &str,
    ) -> AppResult<EncryptedSecretSnapshot> {
        Ok(EncryptedSecretSnapshot(
            self.read_values_locked()?
                .get(&entry_key(service, key))
                .cloned(),
        ))
    }

    pub(super) fn restore_locked(
        &self,
        service: &str,
        key: &str,
        prior: EncryptedSecretSnapshot,
    ) -> AppResult<()> {
        let mut values = self.read_values_locked()?;
        let entry = entry_key(service, key);
        match prior.0 {
            Some(value) => {
                values.insert(entry, value);
            }
            None => {
                values.remove(&entry);
            }
        }
        self.write_values_locked(&values)
    }

    fn read_values_locked(&self) -> AppResult<HashMap<String, EncryptedValue>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(config_error),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(config_error(e)),
        }
    }

    fn write_values_locked(&self, values: &HashMap<String, EncryptedValue>) -> AppResult<()> {
        super::atomic_write(
            &self.path,
            &serde_json::to_vec_pretty(values).map_err(config_error)?,
        )
    }
}
fn entry_key(service: &str, key: &str) -> String {
    format!("{service}/{key}")
}
fn config_error(error: impl std::fmt::Display) -> AppError {
    AppError::Configuration(error.to_string())
}
