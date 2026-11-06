use super::{ConfigStore, EncryptedSecretSnapshot, ProviderConfig};
use crate::error::{AppError, AppResult};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

const CONFIG_OPERATION_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct ConfigIo {
    store: Arc<Mutex<ConfigStore>>,
    permits: Arc<Semaphore>,
    data_dir: PathBuf,
    deadline: Duration,
    #[cfg(test)]
    faults: Arc<Mutex<std::collections::HashSet<&'static str>>>,
}

pub(crate) struct ConfigTransaction<'a> {
    store: &'a mut ConfigStore,
    _config_lock: fs::File,
    secret_lock: Option<fs::File>,
    #[cfg(test)]
    faults: Arc<Mutex<std::collections::HashSet<&'static str>>>,
}

enum EntryState {
    Queued,
    Entered,
    Cancelled,
}

struct CancelBeforeEntry {
    state: Arc<Mutex<EntryState>>,
}

impl Drop for CancelBeforeEntry {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock()
            && matches!(*state, EntryState::Queued)
        {
            *state = EntryState::Cancelled;
        }
    }
}

impl ConfigIo {
    pub(crate) fn new(store: ConfigStore) -> Self {
        Self::with_runtime(store, process_config_gate(), CONFIG_OPERATION_DEADLINE)
    }

    fn with_runtime(store: ConfigStore, permits: Arc<Semaphore>, deadline: Duration) -> Self {
        let data_dir = store.data_dir().to_path_buf();
        Self {
            store: Arc::new(Mutex::new(store)),
            permits,
            data_dir,
            deadline,
            #[cfg(test)]
            faults: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_runtime_for_tests(
        store: ConfigStore,
        permits: Arc<Semaphore>,
        deadline: Duration,
    ) -> Self {
        Self::with_runtime(store, permits, deadline)
    }

    /// Integration fault seam: one method fails before mutating; all other
    /// transaction methods still execute real locking/encryption/persistence.
    #[cfg(test)]
    pub(crate) fn fail_next_for_tests(&self, operation: &'static str) {
        self.faults.lock().unwrap().insert(operation);
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub(crate) async fn run<T, F>(&self, operation: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(&mut ConfigTransaction<'a>) -> AppResult<T> + Send + 'static,
    {
        let state = Arc::new(Mutex::new(EntryState::Queued));
        let cancel = CancelBeforeEntry {
            state: Arc::clone(&state),
        };
        let permits = Arc::clone(&self.permits);
        let store = Arc::clone(&self.store);
        #[cfg(test)]
        let faults = self.faults.clone();
        let work = async move {
            let permit = permits
                .acquire_owned()
                .await
                .map_err(|_| AppError::Internal("configuration worker gate closed".into()))?;
            let task = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let mut store = store
                    .lock()
                    .map_err(|error| AppError::Internal(error.to_string()))?;
                let config_lock = store.acquire_config_lock()?;
                {
                    let mut entry = state
                        .lock()
                        .map_err(|error| AppError::Internal(error.to_string()))?;
                    match *entry {
                        EntryState::Cancelled => {
                            return Err(AppError::Configuration(
                                "configuration operation cancelled before entry".into(),
                            ));
                        }
                        EntryState::Queued => *entry = EntryState::Entered,
                        EntryState::Entered => {
                            return Err(AppError::Internal(
                                "configuration operation entered twice".into(),
                            ));
                        }
                    }
                }
                let mut transaction = ConfigTransaction {
                    store: &mut store,
                    _config_lock: config_lock,
                    secret_lock: None,
                    #[cfg(test)]
                    faults,
                };
                operation(&mut transaction)
            });
            task.await.map_err(|error| {
                AppError::Internal(format!("configuration worker failed: {error}"))
            })?
        };

        let result = tokio::time::timeout(self.deadline, work)
            .await
            .map_err(|_| AppError::Configuration("configuration operation timed out".into()))?;
        drop(cancel);
        result
    }
}

impl ConfigTransaction<'_> {
    #[cfg(test)]
    fn check_fault(&self, operation: &'static str) -> AppResult<()> {
        if self.faults.lock().unwrap().remove(operation) {
            return Err(AppError::Configuration(
                "injected configuration IO failure".into(),
            ));
        }
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for the approved service-config transaction consumer"
        )
    )]
    pub(crate) fn data_dir(&self) -> &Path {
        self.store.data_dir()
    }

    pub(crate) fn load_provider_config(&mut self, id: Uuid) -> AppResult<ProviderConfig> {
        self.store.load_provider_config_locked(id)
    }

    pub(crate) fn list_provider_configs(&mut self) -> AppResult<Vec<ProviderConfig>> {
        self.store.list_provider_configs_locked()
    }

    pub(crate) fn save_provider_config(&mut self, config: ProviderConfig) -> AppResult<()> {
        #[cfg(test)]
        self.check_fault("save_config")?;
        self.store.save_provider_config_locked(config)
    }

    pub(crate) fn delete_provider_config(&mut self, id: Uuid) -> AppResult<()> {
        self.store.delete_provider_config_locked(id)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for the approved intelligence credential transaction consumer"
        )
    )]
    pub(crate) fn secret_get(&mut self, service: &str, key: &str) -> AppResult<Option<Vec<u8>>> {
        self.ensure_secret_lock()?;
        self.store.secret_store().get_locked(service, key)
    }

    pub(crate) fn secret_set(&mut self, service: &str, key: &str, value: &[u8]) -> AppResult<()> {
        self.ensure_secret_lock()?;
        self.store.secret_store().set_locked(service, key, value)
    }

    pub(crate) fn secret_contains(&mut self, service: &str, key: &str) -> AppResult<bool> {
        self.ensure_secret_lock()?;
        self.store.secret_store().contains_locked(service, key)
    }

    pub(crate) fn secret_delete(&mut self, service: &str, key: &str) -> AppResult<()> {
        self.ensure_secret_lock()?;
        #[cfg(test)]
        self.check_fault("delete_secret")?;
        self.store.secret_store().delete_locked(service, key)
    }

    pub(crate) fn snapshot_secret(
        &mut self,
        service: &str,
        key: &str,
    ) -> AppResult<EncryptedSecretSnapshot> {
        self.ensure_secret_lock()?;
        self.store.secret_store().snapshot_locked(service, key)
    }

    pub(crate) fn restore_secret(
        &mut self,
        service: &str,
        key: &str,
        prior: EncryptedSecretSnapshot,
    ) -> AppResult<()> {
        self.ensure_secret_lock()?;
        #[cfg(test)]
        self.check_fault("restore_secret")?;
        self.store
            .secret_store()
            .restore_locked(service, key, prior)
    }

    fn ensure_secret_lock(&mut self) -> AppResult<()> {
        if self.secret_lock.is_none() {
            self.secret_lock = Some(self.store.secret_store().acquire_transaction_lock()?);
        }
        Ok(())
    }
}

fn process_config_gate() -> Arc<Semaphore> {
    static GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(GATE.get_or_init(|| Arc::new(Semaphore::new(2))))
}
