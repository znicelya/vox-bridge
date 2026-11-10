#[cfg(any(target_os = "windows", target_os = "macos"))]
use super::AudioDriverManager;
use super::{
    AudioDirection, AudioEndpoint, DriverStatus,
    native_io::{self, NativeOwner, NativeOwnerFactory},
    process_io::{ProcessSpawner, SystemProcessSpawner},
    pulse_io,
};
use crate::{
    audio::{
        capture::{AudioCapture, CaptureHandle},
        playback::{AudioPlayback, PlaybackHandle},
        runtime::*,
    },
    domain::transcript::PlaybackBoundary,
    error::AppResult,
};
use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointRole {
    Input,
    PhysicalOutput,
    M1Render,
    M2Capture,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioBackendKind {
    Cpal,
    Pulse,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEndpoint {
    pub endpoint: AudioEndpoint,
    pub role: EndpointRole,
    pub backend: AudioBackendKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInventory {
    pub generation: Uuid,
    pub driver: DriverStatus,
    pub endpoints: Vec<RuntimeEndpoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Locator {
    Cpal {
        endpoint: AudioEndpoint,
    },
    Pulse {
        name: String,
        index: u32,
        direction: AudioDirection,
    },
}
pub(crate) type InventoryRows = Vec<(RuntimeEndpoint, Locator)>;
#[derive(Clone)]
struct Snapshot {
    inventory: AudioInventory,
    locators: BTreeMap<String, Locator>,
}
#[derive(Default)]
struct State {
    snapshot: Option<Snapshot>,
    maintenance: Option<Uuid>,
    leases: HashMap<String, Uuid>,
    quarantined: HashSet<Uuid>,
}
/// Dropping an unacknowledged maintenance lease intentionally keeps admission closed.
pub(crate) struct MaintenanceLease {
    state: Arc<Mutex<State>>,
    id: Uuid,
}
impl MaintenanceLease {
    pub(crate) fn complete(self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.maintenance == Some(self.id) {
            state.maintenance = None;
            state.snapshot = None;
        }
    }
}
pub(crate) struct Lease {
    state: Arc<Mutex<State>>,
    id: Uuid,
    ids: [String; 2],
    released: AtomicBool,
}
impl Lease {
    pub fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.quarantined.contains(&self.id) {
            for id in &self.ids {
                if state.leases.get(id) == Some(&self.id) {
                    state.leases.remove(id);
                }
            }
        }
        self.released.store(true, Ordering::Release);
    }
    pub fn quarantine(&self) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quarantined
            .insert(self.id);
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.released.load(Ordering::Acquire) {
            self.quarantine();
        }
    }
}
pub struct PlatformAudioBackend {
    actual: crate::audio::join::AudioRegistry,
    state: Arc<Mutex<State>>,
    spawner: Arc<dyn ProcessSpawner>,
    use_pulse: bool,
    #[cfg(test)]
    pulse_entry: Arc<pulse_io::entry_test_support::EntryControl>,
}
impl PlatformAudioBackend {
    pub fn new() -> Self {
        Self {
            actual: crate::audio::join::AudioRegistry::new(),
            state: Arc::default(),
            spawner: Arc::new(SystemProcessSpawner),
            use_pulse: cfg!(target_os = "linux"),
            #[cfg(test)]
            pulse_entry: Arc::default(),
        }
    }
    #[cfg(test)]
    pub(crate) fn with_spawner(spawner: Arc<dyn ProcessSpawner>) -> Self {
        Self {
            spawner,
            use_pulse: true,
            ..Self::new()
        }
    }
    pub(crate) fn begin_maintenance(&self) -> AppResult<MaintenanceLease> {
        self.actual.require_ready()?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.maintenance.is_some() || !state.leases.is_empty() || !state.quarantined.is_empty()
        {
            return Err(super::driver::busy_error());
        }
        let id = Uuid::new_v4();
        state.maintenance = Some(id);
        state.snapshot = None;
        Ok(MaintenanceLease {
            state: self.state.clone(),
            id,
        })
    }
    #[cfg(test)]
    fn record(&self, driver: DriverStatus, rows: InventoryRows) -> AppResult<AudioInventory> {
        Self::record_snapshot(&self.state, driver, rows)
    }
    fn record_snapshot(
        state: &Arc<Mutex<State>>,
        driver: DriverStatus,
        rows: InventoryRows,
    ) -> AppResult<AudioInventory> {
        let mut entries = BTreeMap::new();
        let mut locators = BTreeMap::new();
        for (entry, locator) in rows {
            let id = entry.endpoint.id.clone();
            if entries.insert(id.clone(), entry).is_some() {
                return Err(audio_error("ambiguous audio endpoint ID"));
            }
            locators.insert(id, locator);
        }
        let endpoints = entries.into_values().collect::<Vec<_>>();
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if state.maintenance.is_some() {
            return Err(super::driver::busy_error());
        }
        let unchanged = state.snapshot.as_ref().is_some_and(|s| {
            s.inventory.driver == driver
                && s.inventory.endpoints == endpoints
                && s.locators == locators
        });
        if unchanged {
            return Ok(state.snapshot.as_ref().unwrap().inventory.clone());
        }
        let inventory = AudioInventory {
            generation: Uuid::new_v4(),
            driver,
            endpoints,
        };
        state.snapshot = Some(Snapshot {
            inventory: inventory.clone(),
            locators,
        });
        Ok(inventory)
    }
    pub(super) fn claim(
        &self,
        request: &AudioOpenRequest,
    ) -> AppResult<(Locator, Locator, Arc<Lease>)> {
        self.actual.require_ready()?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.maintenance.is_some() {
            return Err(super::driver::busy_error());
        }
        let snapshot = state
            .snapshot
            .as_ref()
            .ok_or_else(|| audio_error("refresh audio inventory before opening"))?;
        if request.inventory_generation != snapshot.inventory.generation
            || snapshot.inventory.driver != DriverStatus::Ready
        {
            return Err(audio_error(
                "audio inventory is stale or driver is not ready",
            ));
        }
        if [&request.input_id, &request.output_id].iter().any(|id| {
            !snapshot
                .inventory
                .endpoints
                .iter()
                .any(|e| &e.endpoint.id == *id && e.endpoint.ready)
        }) {
            return Err(audio_error("selected audio endpoint is unavailable"));
        }
        let input = snapshot
            .locators
            .get(&request.input_id)
            .cloned()
            .ok_or_else(|| audio_error("unknown input endpoint"))?;
        let output = snapshot
            .locators
            .get(&request.output_id)
            .cloned()
            .ok_or_else(|| audio_error("unknown output endpoint"))?;
        let direction = |value: &Locator| match value {
            Locator::Cpal { endpoint } => endpoint.direction.clone(),
            Locator::Pulse { direction, .. } => direction.clone(),
        };
        if direction(&input) != AudioDirection::Input
            || direction(&output) != AudioDirection::Output
            || request.input_id == request.output_id
        {
            return Err(audio_error("invalid audio endpoint directions"));
        }
        if state.leases.len() >= 4
            || state.leases.contains_key(&request.input_id)
            || state.leases.contains_key(&request.output_id)
        {
            return Err(audio_error("audio endpoint is busy or requires restart"));
        }
        let id = Uuid::new_v4();
        let ids = [request.input_id.clone(), request.output_id.clone()];
        for name in &ids {
            state.leases.insert(name.clone(), id);
        }
        Ok((
            input,
            output,
            Arc::new(Lease {
                state: self.state.clone(),
                id,
                ids,
                released: AtomicBool::new(false),
            }),
        ))
    }
}
impl Default for PlatformAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl AudioBackend for PlatformAudioBackend {
    fn actual_epoch(&self) -> AppResult<u64> {
        self.actual.epoch()
    }
    fn begin_close(
        &self,
        key: crate::audio::join::AudioCloseKey,
    ) -> AppResult<crate::audio::join::AudioBackendCloseJoin> {
        self.actual.begin_close(key)
    }
    fn reopen_after_close(&self, ack: &crate::audio::join::AudioBackendCloseAck) -> AppResult<()> {
        self.validate_reopen_after_close(ack)?;
        self.actual.reopen_after_close(ack)
    }
    fn validate_reopen_after_close(
        &self,
        ack: &crate::audio::join::AudioBackendCloseAck,
    ) -> AppResult<()> {
        if !self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .quarantined
            .is_empty()
        {
            return Err(audio_error("native audio still requires restart"));
        }
        self.actual.validate_reopen_after_close(ack)
    }
    async fn inventory(&self) -> AppResult<AudioInventory> {
        let parent = self.actual.inventory().await?;
        let state = self.state.clone();
        let spawner = self.spawner.clone();
        let pulse = self.use_pulse;
        let entered = parent.clone();
        let (source, _actual) = crate::audio::join::AudioOwnerSource::new();
        let (publish, receive) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let result = async {
                entered.require_ready()?;
                let (driver, rows) = if pulse {
                    pulse_io::inventory(spawner.as_ref(), &entered).await?
                } else {
                    let native = entered.clone();
                    tokio::task::spawn_blocking(move || {
                        native.require_ready()?;
                        native_inventory()
                    })
                    .await
                    .map_err(|_| audio_error("native inventory failed"))??
                };
                // Parse and snapshot publication are part of the same actual
                // parent. Closing/epoch change rejects an old late record.
                entered.publish(|| Self::record_snapshot(&state, driver, rows))
            }
            .await;
            let _ = publish.send(result);
        });
        source.retain_task(task, parent);
        tokio::time::timeout(Duration::from_secs(2), receive)
            .await
            .map_err(|_| audio_error("native inventory timed out"))?
            .map_err(|_| audio_error("native inventory owner ended"))?
    }
    async fn open(&self, request: AudioOpenRequest) -> AppResult<AudioPorts> {
        let parent = self.actual.stream()?;
        let (input, output, lease) = match self.claim(&request) {
            Ok(claimed) => claimed,
            Err(error) => {
                parent.finish().await?;
                return Err(error);
            }
        };
        match (input, output) {
            (Locator::Cpal { endpoint: input }, Locator::Cpal { endpoint: output }) => {
                native_io::open_registered(
                    request,
                    Arc::new(CpalFactory { input, output }),
                    Some(lease),
                    native_io::Deadlines::default(),
                    Some(parent),
                )
                .await
            }
            (
                Locator::Pulse {
                    name: input,
                    index: source,
                    ..
                },
                Locator::Pulse {
                    name: output,
                    index: sink,
                    ..
                },
            ) => {
                pulse_io::open(
                    request,
                    input,
                    source,
                    output,
                    sink,
                    self.spawner.clone(),
                    lease,
                    parent,
                    #[cfg(test)]
                    Some(self.pulse_entry.clone()),
                )
                .await
            }
            _ => {
                lease.release();
                parent.finish().await?;
                Err(audio_error("audio routes cannot mix backends"))
            }
        }
    }
}
fn native_inventory() -> AppResult<(DriverStatus, InventoryRows)> {
    #[cfg(target_os = "windows")]
    let driver = super::windows::WindowsAudioDriver::new(super::SystemCommandRunner).status()?;
    #[cfg(target_os = "macos")]
    let driver = super::macos::MacOsAudioDriver::new(super::SystemCommandRunner).status()?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let driver = DriverStatus::Failed("native runtime is not used on this platform".into());
    let host = cpal::default_host();
    let mut rows = Vec::new();
    for (input, devices) in [(true, host.input_devices()), (false, host.output_devices())] {
        let mut names = HashSet::new();
        for device in devices.map_err(|_| audio_error("native device enumeration failed"))? {
            if rows.len() >= 512 {
                return Err(audio_error("too many native devices"));
            }
            let name = device
                .name()
                .map_err(|_| audio_error("native device has no name"))?;
            if name.is_empty()
                || name.len() > 256
                || name.contains('\0')
                || !names.insert(name.clone())
            {
                return Err(audio_error("ambiguous native device name"));
            }
            // These are the call application's sides, never an app input/physical output.
            if (input && name == "VoxBridge Input") || (!input && name == "VoxBridge Output") {
                continue;
            }
            let configs = if input {
                device
                    .supported_input_configs()
                    .map(|c| c.collect::<Vec<_>>())
            } else {
                device
                    .supported_output_configs()
                    .map(|c| c.collect::<Vec<_>>())
            }
            .map_err(|_| audio_error("native supported configurations unavailable"))?;
            let mut rates = Vec::new();
            for config in &configs {
                for rate in [8000, 16000, 24000, 44100, 48000, 96000, 192000] {
                    if config.min_sample_rate().0 <= rate && rate <= config.max_sample_rate().0 {
                        rates.push(rate);
                    }
                }
            }
            let config = if input {
                device.default_input_config()
            } else {
                device.default_output_config()
            }
            .map_err(|_| audio_error("native device configuration unavailable"))?;
            let rate = config.sample_rate().0;
            let channels = config.channels();
            rates.push(rate);
            rates.sort_unstable();
            rates.dedup();
            let ready = (8000..=192000).contains(&rate)
                && (1..=32).contains(&channels)
                && matches!(
                    config.sample_format(),
                    cpal::SampleFormat::I16 | cpal::SampleFormat::F32 | cpal::SampleFormat::U16
                );
            let direction = if input {
                AudioDirection::Input
            } else {
                AudioDirection::Output
            };
            let role = match (input, name.as_str()) {
                (false, "VoxBridge Input") => EndpointRole::M1Render,
                (true, "VoxBridge Output") => EndpointRole::M2Capture,
                (true, _) => EndpointRole::Input,
                (false, _) => EndpointRole::PhysicalOutput,
            };
            let endpoint = AudioEndpoint {
                id: format!("cpal:{}:{name}", if input { "input" } else { "output" }),
                display_name: name.clone(),
                direction: direction.clone(),
                sample_rates: rates.clone(),
                channels,
                ready,
            };
            let locator = AudioEndpoint {
                id: name.clone(),
                display_name: name,
                direction,
                sample_rates: rates.clone(),
                channels,
                ready,
            };
            rows.push((
                RuntimeEndpoint {
                    endpoint,
                    role,
                    backend: AudioBackendKind::Cpal,
                },
                Locator::Cpal { endpoint: locator },
            ));
        }
    }
    Ok((driver, rows))
}
struct CpalFactory {
    input: AudioEndpoint,
    output: AudioEndpoint,
}
struct CpalOwner {
    capture: Option<CaptureHandle>,
    playback: Option<PlaybackHandle>,
    shared: AudioShared,
}
impl NativeOwnerFactory for CpalFactory {
    fn open_on_owner_thread(
        &self,
        request: &AudioOpenRequest,
        capture: crate::audio::queue::BoundedAudioQueue,
        playback: tokio::sync::mpsc::Receiver<crate::audio::AudioFrame>,
        shared: AudioShared,
    ) -> AppResult<Box<dyn NativeOwner>> {
        let input = AudioCapture::start_runtime(
            self.input.clone(),
            capture,
            request.source,
            shared.clone(),
        )?;
        let output =
            match AudioPlayback::start_runtime(self.output.clone(), playback, shared.clone()) {
                Ok(output) => output,
                Err(error) => {
                    shared.suppress();
                    input.stop();
                    return Err(error);
                }
            };
        Ok(Box::new(CpalOwner {
            capture: Some(input),
            playback: Some(output),
            shared,
        }))
    }
}
impl NativeOwner for CpalOwner {
    fn stop_capture(&mut self) -> AppResult<()> {
        if let Some(capture) = self.capture.take() {
            capture.stop();
        }
        Ok(())
    }
    fn discard_pending(&mut self) -> AppResult<()> {
        self.shared.suppress();
        if let Some(playback) = &self.playback {
            playback.discard();
        }
        Ok(())
    }
    fn seal_playback(&mut self) -> AppResult<()> {
        self.playback
            .as_ref()
            .ok_or_else(|| audio_error("playback is closed"))?
            .seal();
        Ok(())
    }
    fn playback_drained(&mut self) -> AppResult<Option<PlaybackReport>> {
        let p = self
            .playback
            .as_ref()
            .ok_or_else(|| audio_error("playback is closed"))?;
        if p.is_disconnected() {
            return Err(audio_error("playback device was lost"));
        }
        if p.drained() {
            self.shared
                .counters
                .playback_finalized
                .store(true, Ordering::Release);
            self.shared
                .counters
                .tail_known
                .store(true, Ordering::Release);
            Ok(Some(PlaybackReport {
                boundary: PlaybackBoundary::CallbackConsumed,
                statistics: self.shared.statistics().playback,
            }))
        } else {
            Ok(None)
        }
    }
    fn close(&mut self) -> AppResult<()> {
        self.shared.suppress();
        self.stop_capture()?;
        if let Some(playback) = self.playback.take() {
            playback.stop();
        }
        Ok(())
    }
    fn lost(&self) -> bool {
        self.capture
            .as_ref()
            .is_some_and(CaptureHandle::is_disconnected)
            || self
                .playback
                .as_ref()
                .is_some_and(PlaybackHandle::is_disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::AudioSource;
    pub(super) fn rows() -> InventoryRows {
        [
            ("microphone", AudioDirection::Input, EndpointRole::Input),
            ("m1", AudioDirection::Output, EndpointRole::M1Render),
            ("m2", AudioDirection::Input, EndpointRole::M2Capture),
            (
                "speaker",
                AudioDirection::Output,
                EndpointRole::PhysicalOutput,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, direction, role))| {
            (
                RuntimeEndpoint {
                    endpoint: AudioEndpoint {
                        id: name.into(),
                        display_name: name.into(),
                        direction: direction.clone(),
                        sample_rates: vec![48000],
                        channels: 2,
                        ready: true,
                    },
                    role,
                    backend: AudioBackendKind::Pulse,
                },
                Locator::Pulse {
                    name: name.into(),
                    index: index as u32,
                    direction,
                },
            )
        })
        .collect()
    }
    pub(super) fn request(
        inventory: &AudioInventory,
        input: &str,
        output: &str,
    ) -> AudioOpenRequest {
        AudioOpenRequest {
            inventory_generation: inventory.generation,
            input_id: input.into(),
            output_id: output.into(),
            source: AudioSource::Microphone,
        }
    }
    #[test]
    fn generation_is_stable_for_sorted_semantic_inventory_and_changes_for_locator_or_readiness() {
        let backend = PlatformAudioBackend::new();
        let first = backend.record(DriverStatus::Ready, rows()).unwrap();
        let mut reverse = rows();
        reverse.reverse();
        assert_eq!(first, backend.record(DriverStatus::Ready, reverse).unwrap());
        let mut changed = rows();
        if let Locator::Pulse { index, .. } = &mut changed[0].1 {
            *index = 50;
        }
        let second = backend
            .record(DriverStatus::Ready, changed.clone())
            .unwrap();
        assert_ne!(first.generation, second.generation);
        changed[0].0.endpoint.ready = false;
        assert_ne!(
            second.generation,
            backend
                .record(DriverStatus::Ready, changed)
                .unwrap()
                .generation
        );
    }
    #[test]
    fn atomic_pair_claim_rejects_stale_unknown_duplicate_and_quarantined_endpoints() {
        let backend = PlatformAudioBackend::new();
        let inventory = backend.record(DriverStatus::Ready, rows()).unwrap();
        let req = request(&inventory, "microphone", "m1");
        let mut stale = req.clone();
        stale.inventory_generation = Uuid::nil();
        assert!(backend.claim(&stale).is_err());
        assert!(
            backend
                .claim(&request(&inventory, "microphone", "unknown"))
                .is_err()
        );
        let (_, _, first) = backend.claim(&req).unwrap();
        assert!(backend.claim(&req).is_err());
        let (_, _, second) = backend
            .claim(&request(&inventory, "m2", "speaker"))
            .unwrap();
        first.release();
        let (_, _, replacement) = backend.claim(&req).unwrap();
        replacement.quarantine();
        replacement.release();
        second.release();
        assert!(backend.claim(&req).is_err());
        // Refresh alone neither releases a quarantined route nor resumes audio.
        let refresh = backend.record(DriverStatus::Ready, rows()).unwrap();
        assert_eq!(refresh.generation, inventory.generation);
        assert!(backend.claim(&req).is_err());
        assert_eq!(backend.state.lock().unwrap().leases.len(), 2);
    }
    #[test]
    fn unready_endpoint_is_rejected_using_private_inventory() {
        let backend = PlatformAudioBackend::new();
        let mut entries = rows();
        entries[0].0.endpoint.ready = false;
        let inventory = backend.record(DriverStatus::Ready, entries).unwrap();
        assert!(
            backend
                .claim(&request(&inventory, "microphone", "m1"))
                .is_err()
        );
    }
    #[tokio::test]
    async fn unacknowledged_native_cleanup_quarantines_pair_even_after_late_exit() {
        let backend = PlatformAudioBackend::new();
        let inventory = backend.record(DriverStatus::Ready, rows()).unwrap();
        let req = request(&inventory, "microphone", "m1");
        let (_, _, lease) = backend.claim(&req).unwrap();
        let release = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(AtomicBool::new(false));
        let ports = native_io::open_owned(
            req.clone(),
            Arc::new(crate::audio::runtime_tests::DropFactory {
                entered: entered.clone(),
                release: release.clone(),
            }),
            Some(lease),
            native_io::Deadlines {
                open: Duration::from_secs(1),
                close: Duration::from_millis(30),
                drain: Duration::from_millis(30),
            },
        )
        .await
        .unwrap();
        let result = ports.control.close().await;
        release.store(true, Ordering::Release);
        assert!(result.is_err());
        assert!(entered.load(Ordering::Acquire));
        assert_eq!(
            *ports.health.borrow(),
            crate::audio::runtime::AudioHealth::NeedsRestart
        );
        assert!(backend.claim(&req).is_err());
        backend.record(DriverStatus::Ready, rows()).unwrap();
        assert!(backend.claim(&req).is_err());
    }
    struct FailedFactory(Arc<std::sync::atomic::AtomicUsize>);
    struct InputGuard(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for InputGuard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }
    impl NativeOwnerFactory for FailedFactory {
        fn open_on_owner_thread(
            &self,
            _: &AudioOpenRequest,
            _: crate::audio::queue::BoundedAudioQueue,
            _: tokio::sync::mpsc::Receiver<crate::audio::AudioFrame>,
            _: AudioShared,
        ) -> AppResult<Box<dyn NativeOwner>> {
            let _input = InputGuard(self.0.clone());
            Err(audio_error("injected output open failure"))
        }
    }
    #[tokio::test]
    async fn opening_failure_rolls_back_input_before_releasing_pair() {
        let backend = PlatformAudioBackend::new();
        let inventory = backend.record(DriverStatus::Ready, rows()).unwrap();
        let req = request(&inventory, "microphone", "m1");
        let (_, _, lease) = backend.claim(&req).unwrap();
        let dropped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        assert!(
            native_io::open_owned(
                req.clone(),
                Arc::new(FailedFactory(dropped.clone())),
                Some(lease),
                native_io::Deadlines::default()
            )
            .await
            .is_err()
        );
        assert_eq!(dropped.load(Ordering::Acquire), 1);
        let (_, _, lease) = backend.claim(&req).unwrap();
        lease.release();
    }
}

#[cfg(test)]
pub(crate) mod driver_test_support {
    use super::*;
    pub(crate) fn seed(backend: &PlatformAudioBackend) -> AudioOpenRequest {
        let inventory = backend.record(DriverStatus::Ready, tests::rows()).unwrap();
        tests::request(&inventory, "microphone", "m1")
    }
    pub(crate) fn claim(
        backend: &PlatformAudioBackend,
        request: &AudioOpenRequest,
    ) -> AppResult<Arc<Lease>> {
        backend.claim(request).map(|(_, _, lease)| lease)
    }
}

#[cfg(test)]
pub(crate) mod actual_test_support {
    pub(crate) fn stream(backend: &super::PlatformAudioBackend) -> crate::audio::join::AudioParent {
        backend.actual.stream().unwrap()
    }
    pub(crate) fn hold_pulse_open(
        backend: &super::PlatformAudioBackend,
        boundary: super::pulse_io::entry_test_support::Boundary,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        backend.pulse_entry.hold(boundary)
    }
    pub(crate) fn pair_health(backend: &super::PlatformAudioBackend) -> (usize, usize) {
        let state = backend.state.lock().unwrap();
        (state.leases.len(), state.quarantined.len())
    }
}
