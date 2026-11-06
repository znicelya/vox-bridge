//! Application composition. Construction owns local storage, never devices/services.

use crate::{
    audio::{
        PcmSpec,
        platform::{
            DriverStatus,
            driver::{
                DriverController, DriverOperation, DriverOperations, UnavailableDriverOperations,
            },
            inventory::{AudioInventory, PlatformAudioBackend},
        },
        runtime::AudioBackend,
    },
    commands::subscriptions::SubscriptionRegistry,
    config::{ConfigIo, ConfigStore, SecretStore},
    domain::{
        launch::{SessionContext, SessionLaunch},
        session::SessionDirection,
        transcript::{DiagnosticPage, TranscriptPage},
    },
    error::{AppError, AppResult},
    events::EventBus,
    providers::{ProviderCapabilities, factory::ProviderFactory},
    session::{SessionOrchestrator, SessionRecord, TranslationRuntime},
    storage::LocalStore,
};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Semaphore, watch};
use uuid::Uuid;

pub use crate::commands::subscriptions::{FrontendEmitter, SubscriptionTicket};
pub use crate::config;
pub use profiles::{CredentialChange, ProviderConfigView, SaveProviderRequest};
mod lifecycle;
mod profiles;

#[derive(serde::Serialize)]
pub struct LaunchValidation {
    pub context: SessionContext,
    pub input_audio: PcmSpec,
    pub output_audio: PcmSpec,
}

#[derive(Clone)]
pub struct AppState {
    // Clones retained by owned work retain process ownership through cleanup.
    _process_lock: Arc<File>,
    event_bus: EventBus,
    store: Arc<LocalStore>,
    orchestrator: SessionOrchestrator,
    config_io: ConfigIo,
    audio: Arc<dyn AudioBackend>,
    driver: Arc<dyn DriverOperations>,
    factory: Arc<ProviderFactory>,
    runtime: Arc<TranslationRuntime>,
    admission: Arc<Mutex<()>>,
    driver_active: watch::Sender<bool>,
    driver_observation_timeout: std::time::Duration,
    subscriptions: SubscriptionRegistry,
    subscription_admission: Arc<Semaphore>,
    subscription_mutation: Arc<Mutex<()>>,
    close_state: Arc<std::sync::Mutex<lifecycle::CloseState>>,
    close_epoch: watch::Sender<u64>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Self> {
        // Lock before constructing config/runtime collaborators or opening SQLite.
        let lock = Self::lock_data_dir(&data_dir)?;
        let secrets = Arc::new(SecretStore::new(&data_dir)?);
        let factory = Arc::new(ProviderFactory::new(secrets.clone()));
        let audio = Arc::new(PlatformAudioBackend::new());
        let driver = Arc::new(DriverController::new(audio.clone())?);
        Self::compose(data_dir, lock, secrets, audio, factory, driver)
    }
    pub fn with_dependencies(
        data_dir: PathBuf,
        secrets: Arc<SecretStore>,
        audio: Arc<dyn AudioBackend>,
        factory: Arc<ProviderFactory>,
    ) -> AppResult<Self> {
        Self::with_driver_dependencies(
            data_dir,
            secrets,
            audio,
            factory,
            Arc::new(UnavailableDriverOperations),
        )
    }
    pub fn with_driver_dependencies(
        data_dir: PathBuf,
        secrets: Arc<SecretStore>,
        audio: Arc<dyn AudioBackend>,
        factory: Arc<ProviderFactory>,
        driver: Arc<dyn DriverOperations>,
    ) -> AppResult<Self> {
        let lock = Self::lock_data_dir(&data_dir)?;
        Self::compose(data_dir, lock, secrets, audio, factory, driver)
    }
    pub async fn manage_audio_driver(&self, operation: DriverOperation) -> AppResult<DriverStatus> {
        use crate::audio::platform::driver::{busy_error, safe_result};
        let epoch = self.admitted_epoch()?;
        // No pending mutex waiters or queued mutations: one accepted operation.
        let admission = self
            .admission
            .clone()
            .try_lock_owned()
            .map_err(|_| busy_error())?;
        self.check_epoch(epoch)?;
        if self.orchestrator.has_active_operation() {
            return Err(busy_error());
        }
        self.driver_active.send_replace(true);
        let owner = self.clone();
        let work = tokio::spawn(async move {
            let _admission = admission;
            let result = match owner.check_epoch(epoch) {
                Ok(()) => safe_result(owner.driver.perform(operation).await),
                Err(error) => Err(error),
            };
            owner.driver_active.send_replace(false);
            result
        });
        // The task retains AppState/process ownership and admission after a caller
        // disconnects or times out. close_all observes that same admission barrier.
        tokio::time::timeout(self.driver_observation_timeout, work)
            .await
            .map_err(|_| {
                AppError::Internal(
                    "driver maintenance observation timed out; completion remains pending".into(),
                )
            })?
            .map_err(|_| AppError::Internal("driver maintenance owner stopped".into()))?
    }
    fn lock_data_dir(data_dir: &Path) -> AppResult<File> {
        std::fs::create_dir_all(data_dir)
            .map_err(|_| AppError::Configuration("cannot create data directory".into()))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data_dir.join("app.lock"))
            .map_err(|_| AppError::Configuration("cannot open data directory lock".into()))?;
        lock.try_lock_exclusive()
            .map_err(|_| AppError::Configuration("data directory already in use".into()))?;
        Ok(lock)
    }
    fn compose(
        data_dir: PathBuf,
        lock: File,
        secrets: Arc<SecretStore>,
        audio: Arc<dyn AudioBackend>,
        factory: Arc<ProviderFactory>,
        driver: Arc<dyn DriverOperations>,
    ) -> AppResult<Self> {
        let store = Arc::new(LocalStore::open(&data_dir)?);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        store.interrupt_incomplete_sessions(now)?;
        let event_bus = EventBus::new();
        let config_io = ConfigIo::new(ConfigStore::new(&data_dir, secrets)?);
        let runtime = Arc::new(TranslationRuntime::new(
            factory.clone(),
            audio.clone(),
            store.clone(),
            event_bus.clone(),
        ));
        let orchestrator =
            SessionOrchestrator::with_runtime(store.clone(), event_bus.clone(), runtime.clone());
        let (close_epoch, _) = watch::channel(0);
        Ok(Self {
            _process_lock: Arc::new(lock),
            event_bus,
            store,
            orchestrator,
            config_io,
            audio,
            driver,
            factory,
            runtime,
            admission: Arc::new(Mutex::new(())),
            driver_active: watch::channel(false).0,
            driver_observation_timeout: std::time::Duration::from_secs(20),
            subscriptions: SubscriptionRegistry::new(),
            subscription_admission: Arc::new(Semaphore::new(16)),
            subscription_mutation: Arc::new(Mutex::new(())),
            close_state: Arc::default(),
            close_epoch,
        })
    }
    pub fn data_dir(&self) -> &Path {
        self.config_io.data_dir()
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
    pub async fn list_audio_endpoints(&self) -> AppResult<AudioInventory> {
        self.audio.inventory().await
    }
    pub async fn audio_driver_status(&self) -> AppResult<DriverStatus> {
        Ok(self.audio.inventory().await?.driver)
    }
    pub async fn provider_capabilities(&self, config_id: Uuid) -> AppResult<ProviderCapabilities> {
        let owner = self.clone();
        let saved = self
            .config_io
            .run(move |tx| {
                let _owner = owner;
                tx.load_provider_config(config_id)
            })
            .await?;
        self.factory
            .capabilities(&saved)
            .map_err(|_| AppError::Configuration("provider configuration is unavailable".into()))
    }
    pub async fn validate_session_launch(
        &self,
        launch: SessionLaunch,
    ) -> AppResult<LaunchValidation> {
        let id = launch.provider_config_id;
        let saved = self.load_launch_provider_config(id).await?;
        let prepared = self
            .runtime
            .prepare(Uuid::new_v4(), &launch, &saved)
            .await?;
        Ok(LaunchValidation {
            context: prepared.context().clone(),
            input_audio: PcmSpec::pcm16_mono(16_000),
            output_audio: PcmSpec::pcm16_mono(24_000),
        })
    }
    pub async fn create_session(&self, launch: SessionLaunch) -> AppResult<SessionRecord> {
        let mut closed = self.close_epoch.subscribe();
        let mut driver = self.driver_active.subscribe();
        if *driver.borrow_and_update() {
            return Err(crate::audio::platform::driver::busy_error());
        }
        let epoch = self.admitted_epoch()?;
        tokio::select! {
            biased;
            _ = closed.changed() => Err(lifecycle::closing_error()),
            _ = driver.changed() => Err(crate::audio::platform::driver::busy_error()),
            result = async {
                let _guard = self.admission.lock().await;
                self.check_epoch(epoch)?;
                let id = launch.provider_config_id;
                let saved = self.load_launch_provider_config(id).await?;
                self.check_epoch(epoch)?;
                self.orchestrator.start(launch, saved).await
            } => result,
        }
    }
    pub async fn stop_session(&self, session_id: Uuid) -> AppResult<SessionRecord> {
        self.orchestrator.stop_session(session_id).await
    }
    pub async fn resume_session_direction(
        &self,
        session_id: Uuid,
        direction: SessionDirection,
    ) -> AppResult<()> {
        self.admitted_epoch()?;
        self.orchestrator
            .resume_direction(session_id, direction)
            .await
    }
    pub async fn get_session(&self, session_id: Uuid) -> AppResult<Option<SessionRecord>> {
        self.orchestrator.get(session_id).await
    }
    pub async fn list_sessions(
        &self,
        limit: Option<u32>,
        cursor: Option<String>,
    ) -> AppResult<Vec<SessionRecord>> {
        self.orchestrator
            .list(limit.unwrap_or(50).min(200), cursor)
            .await
    }
    pub fn get_session_transcript(
        &self,
        session_id: Uuid,
        after: Option<i64>,
        limit: Option<u32>,
    ) -> AppResult<TranscriptPage> {
        self.require_session(session_id)?;
        self.store
            .transcript_page(session_id, after, limit.unwrap_or(100))
    }
    pub fn get_session_diagnostics(
        &self,
        session_id: Uuid,
        after: Option<i64>,
        limit: Option<u32>,
    ) -> AppResult<DiagnosticPage> {
        self.require_session(session_id)?;
        self.store
            .diagnostic_page(session_id, after, limit.unwrap_or(100))
    }
    fn require_session(&self, id: Uuid) -> AppResult<()> {
        self.store
            .get_session(id)?
            .ok_or(AppError::SessionNotFound(id))
            .map(|_| ())
    }
    pub async fn export_session(
        &self,
        session_id: Uuid,
        format: crate::export::ExportFormat,
        destination: PathBuf,
    ) -> AppResult<PathBuf> {
        let owner = self.clone();
        tokio::task::spawn_blocking(move || {
            crate::export::export_session(&owner.store, session_id, format, &destination)
        })
        .await
        .map_err(|_| AppError::Internal("export worker stopped".into()))?
    }
}

#[cfg(test)]
mod profiles_tests;
#[cfg(test)]
mod subscription_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod driver_test_support {
    use super::*;
    pub(crate) fn short_observation(app: &mut AppState) {
        app.driver_observation_timeout = std::time::Duration::from_millis(30);
        app.close_state.lock().unwrap().observation_timeout =
            Some(std::time::Duration::from_millis(30));
    }
}
