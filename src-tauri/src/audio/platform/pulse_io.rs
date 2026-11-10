//! Explicit Pulse device I/O; process liveness alone never establishes route readiness.
use super::{
    AudioDirection, AudioEndpoint, DriverStatus,
    inventory::{AudioBackendKind, EndpointRole, InventoryRows, Lease, Locator, RuntimeEndpoint},
    process_io::{self, ProcessControl, ProcessSpawner, ProcessSpec},
};
use crate::{
    audio::{AudioFrame, PcmSpec, queue::BoundedAudioQueue, runtime::*},
    domain::transcript::{AudioStatistics, PlaybackBoundary, RuntimeFailureCode},
    error::AppResult,
};
use async_trait::async_trait;
use serde_json::Value;
use std::{
    collections::HashSet,
    ffi::OsString,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, mpsc, watch},
};
use uuid::Uuid;
#[cfg(test)]
pub(crate) mod entry_test_support;
#[derive(Clone)]
struct Device {
    index: u32,
    name: String,
    description: String,
    rate: u32,
    channels: u16,
    monitor: Option<u32>,
}
fn json(bytes: &[u8]) -> AppResult<Value> {
    serde_json::from_slice(bytes).map_err(|_| audio_error("audio utility returned invalid JSON"))
}
fn number(v: &Value) -> Option<u32> {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| v.as_str().and_then(|n| n.parse().ok()))
}
fn devices(value: Value) -> AppResult<Vec<Device>> {
    let rows = value
        .as_array()
        .filter(|r| r.len() <= 512)
        .ok_or_else(|| audio_error("invalid audio device inventory"))?;
    let mut names = HashSet::new();
    let mut indices = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let index = row
            .get("index")
            .and_then(number)
            .ok_or_else(|| audio_error("audio device index missing"))?;
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty() && n.len() <= 256 && !n.contains('\0'))
            .ok_or_else(|| audio_error("invalid audio device name"))?;
        if !names.insert(name.to_owned()) || !indices.insert(index) {
            return Err(audio_error("ambiguous Pulse device"));
        }
        let spec = row
            .get("sample_specification")
            .and_then(Value::as_str)
            .ok_or_else(|| audio_error("audio sample specification missing"))?;
        let mut parts = spec.split_whitespace();
        let _format = parts.next();
        let channels = parts
            .next()
            .and_then(|s| s.strip_suffix("ch"))
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|c| (1..=32).contains(c))
            .ok_or_else(|| audio_error("unsupported Pulse channels"))?;
        let rate = parts
            .next()
            .and_then(|s| s.strip_suffix("Hz"))
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|r| (8000..=384000).contains(r))
            .ok_or_else(|| audio_error("unsupported Pulse rate"))?;
        let description = row
            .get("description")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 512 && !s.contains('\0'))
            .unwrap_or(name);
        out.push(Device {
            index,
            name: name.into(),
            description: description.into(),
            rate,
            channels,
            monitor: row
                .get("monitor_of_sink")
                .and_then(number)
                .filter(|n| *n != u32::MAX),
        });
    }
    Ok(out)
}
fn modules_ready(value: &Value) -> bool {
    let Some(rows) = value.as_array().filter(|v| v.len() <= 512) else {
        return false;
    };
    let (mut m1, mut remap, mut m2, mut owned) = (0, 0, 0, 0);
    for row in rows {
        let name = row.get("name").and_then(Value::as_str).unwrap_or("");
        let args = row
            .get("argument")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>();
        if name == "module-null-sink" && args.contains(&"sink_name=VoxBridge_M1") {
            m1 += 1;
            owned += 1;
        }
        if name == "module-null-sink" && args.contains(&"sink_name=VoxBridge_Output") {
            m2 += 1;
            owned += 1;
        }
        if name == "module-remap-source" && args.contains(&"source_name=VoxBridge_Input") {
            owned += 1;
            if args.contains(&"master=VoxBridge_M1.monitor") {
                remap += 1;
            }
        }
    }
    m1 == 1 && m2 == 1 && remap == 1 && owned == 3
}
pub(crate) async fn inventory(
    spawner: &dyn ProcessSpawner,
    parent: &crate::audio::join::AudioParent,
) -> AppResult<(DriverStatus, InventoryRows)> {
    let group = parent.helpers().begin()?;
    let a = group.leaf()?;
    let b = group.leaf()?;
    let c = group.leaf()?;
    let (sources, sinks, modules) = tokio::join!(
        process_io::output_audio_owned(spawner, &["--format=json", "list", "sources"], a),
        process_io::output_audio_owned(spawner, &["--format=json", "list", "sinks"], b),
        process_io::output_audio_owned(spawner, &["--format=json", "list", "modules"], c)
    );
    group.join().await?;
    let sources = devices(json(&sources?)?)?;
    let sinks = devices(json(&sinks?)?)?;
    let modules = json(&modules?)?;
    let m2 = sinks.iter().find(|d| d.name == "VoxBridge_Output");
    let ready = modules_ready(&modules)
        && sinks.iter().any(|d| d.name == "VoxBridge_M1")
        && sources.iter().any(|d| d.name == "VoxBridge_Input")
        && m2.is_some_and(|m2| {
            sources
                .iter()
                .any(|d| d.name == "VoxBridge_Output.monitor" && d.monitor == Some(m2.index))
        });
    let mut rows = Vec::new();
    for (direction, values) in [
        (AudioDirection::Input, sources),
        (AudioDirection::Output, sinks),
    ] {
        for d in values {
            if matches!(d.name.as_str(), "VoxBridge_Input" | "VoxBridge_M1.monitor")
                || (direction == AudioDirection::Output && d.name == "VoxBridge_Output")
            {
                continue;
            }
            let role = match (direction.clone(), d.name.as_str()) {
                (AudioDirection::Output, "VoxBridge_M1") => EndpointRole::M1Render,
                (AudioDirection::Input, "VoxBridge_Output.monitor") => EndpointRole::M2Capture,
                (AudioDirection::Input, _) => EndpointRole::Input,
                _ => EndpointRole::PhysicalOutput,
            };
            let virtual_role = matches!(role, EndpointRole::M1Render | EndpointRole::M2Capture);
            let endpoint = AudioEndpoint {
                id: format!(
                    "pulse:{}:{}",
                    if direction == AudioDirection::Input {
                        "source"
                    } else {
                        "sink"
                    },
                    d.name
                ),
                display_name: d.description,
                direction: direction.clone(),
                sample_rates: vec![d.rate],
                channels: d.channels,
                ready: !virtual_role || ready,
            };
            rows.push((
                RuntimeEndpoint {
                    endpoint,
                    role,
                    backend: AudioBackendKind::Pulse,
                },
                Locator::Pulse {
                    name: d.name,
                    index: d.index,
                    direction: direction.clone(),
                },
            ));
        }
    }
    Ok((
        if ready {
            DriverStatus::Ready
        } else {
            DriverStatus::Failed(
                "Pulse virtual routing is incomplete; repair VoxBridge devices".into(),
            )
        },
        rows,
    ))
}
fn stream_spec(record: bool, device: &str, name: &str) -> ProcessSpec {
    let args = [
        if record { "--record" } else { "--playback" },
        "--raw",
        "--format=s16le",
        if record {
            "--rate=16000"
        } else {
            "--rate=24000"
        },
        "--channels=1",
        "--latency-msec=80",
    ];
    ProcessSpec {
        program: if record { "parec" } else { "pacat" }.into(),
        args: args
            .into_iter()
            .map(OsString::from)
            .chain(
                [
                    format!("--device={device}"),
                    "--client-name=VoxBridge".into(),
                    format!("--stream-name={name}"),
                ]
                .into_iter()
                .map(OsString::from),
            )
            .collect(),
        pipe_stdin: !record,
        pipe_stdout: record,
    }
}
async fn associated_owned(
    spawner: &dyn ProcessSpawner,
    record: bool,
    name: &str,
    index: u32,
    execution: crate::audio::join::AudioLeafExecution,
) -> AppResult<bool> {
    let bytes = process_io::output_audio_owned(
        spawner,
        &[
            "--format=json",
            "list",
            if record {
                "source-outputs"
            } else {
                "sink-inputs"
            },
        ],
        execution,
    )
    .await?;
    let value = json(&bytes)?;
    let rows = value
        .as_array()
        .filter(|v| v.len() <= 512)
        .ok_or_else(|| audio_error("invalid Pulse stream inventory"))?;
    let matching = rows
        .iter()
        .filter(|r| {
            r.get("properties")
                .and_then(|v| v.get("media.name"))
                .and_then(Value::as_str)
                == Some(name)
        })
        .collect::<Vec<_>>();
    Ok(matching.len() == 1
        && matching[0]
            .get(if record { "source" } else { "sink" })
            .and_then(number)
            == Some(index))
}
async fn association_pair(
    spawner: &dyn ProcessSpawner,
    helpers: &crate::audio::join::AudioLeafGroup,
    capture: Option<(&str, u32)>,
    playback: Option<(&str, u32)>,
) -> AppResult<(bool, bool)> {
    let group = helpers.begin()?;
    let a = capture.map(|_| group.leaf()).transpose()?;
    let b = playback.map(|_| group.leaf()).transpose()?;
    let (a, b) = tokio::join!(
        async {
            match (capture, a) {
                (Some((name, index)), Some(execution)) => {
                    associated_owned(spawner, true, name, index, execution).await
                }
                _ => Ok(true),
            }
        },
        async {
            match (playback, b) {
                (Some((name, index)), Some(execution)) => {
                    associated_owned(spawner, false, name, index, execution).await
                }
                _ => Ok(true),
            }
        },
    );
    group.join().await?;
    Ok((a?, b?))
}
#[cfg(test)]
async fn associated(
    spawner: &dyn ProcessSpawner,
    record: bool,
    name: &str,
    index: u32,
) -> AppResult<bool> {
    let registry = crate::audio::join::AudioRegistry::new();
    let parent = registry.stream()?;
    let result = association_pair(
        spawner,
        &parent.helpers(),
        record.then_some((name, index)),
        (!record).then_some((name, index)),
    )
    .await;
    parent.finish().await?;
    result.map(|(a, b)| a && b)
}
#[derive(Clone)]
struct WorkerEnd {
    acknowledged: bool,
    result: Result<Option<PlaybackReport>, AudioFailure>,
}
async fn worker_end(rx: &watch::Receiver<Option<WorkerEnd>>) -> WorkerEnd {
    let mut rx = rx.clone();
    loop {
        if let Some(value) = rx.borrow_and_update().clone() {
            return value;
        }
        if rx.changed().await.is_err() {
            return WorkerEnd {
                acknowledged: false,
                result: Err(AudioFailure::new(
                    RuntimeFailureCode::CleanupTimeout,
                    "audio worker acknowledgement closed",
                )),
            };
        }
    }
}
struct Inner {
    shared: AudioShared,
    capture: BoundedAudioQueue,
    seal: Arc<AtomicBool>,
    health: watch::Sender<AudioHealth>,
    capture_done: watch::Receiver<Option<WorkerEnd>>,
    playback_done: watch::Receiver<Option<WorkerEnd>>,
    monitor_done: watch::Receiver<bool>,
    monitor_stop: watch::Sender<bool>,
    lease: Arc<Lease>,
    deadlines: Deadlines,
}
impl Inner {
    async fn close(&self) -> AppResult<()> {
        self.shared.suppress();
        self.capture.clear();
        self.monitor_stop.send_replace(true);
        let (capture, playback) = tokio::join!(
            worker_end(&self.capture_done),
            worker_end(&self.playback_done)
        );
        let mut monitor = self.monitor_done.clone();
        while !*monitor.borrow_and_update() {
            monitor
                .changed()
                .await
                .map_err(|_| audio_error("audio monitor ended without acknowledgement"))?;
        }
        if !capture.acknowledged || !playback.acknowledged {
            self.health.send_replace(AudioHealth::NeedsRestart);
            self.lease.quarantine();
            return Err(audio_error("Pulse cleanup could not be acknowledged"));
        }
        self.lease.release();
        Ok(())
    }
}
type OperationResult = Result<Option<PlaybackReport>, AudioFailure>;
struct Control {
    actual: crate::audio::join::AudioOwnerJoin,
    inner: Arc<Inner>,
    operations: [Mutex<Option<watch::Receiver<Option<OperationResult>>>>; 4],
}
impl Control {
    async fn request(&self, index: usize) -> AppResult<Option<PlaybackReport>> {
        let mut slot = self.operations[index].lock().await;
        let mut result = if let Some(rx) = &*slot {
            rx.clone()
        } else {
            let (tx, rx) = watch::channel(None);
            let inner = self.inner.clone();
            tokio::spawn(async move {
                let duration = if index == 2 {
                    inner.deadlines.drain
                } else {
                    inner.deadlines.close
                };
                let value = tokio::time::timeout(duration, async {
                    match index {
                        0 => {
                            inner.shared.stop_capture();
                            let end = worker_end(&inner.capture_done).await;
                            if end.acknowledged {
                                end.result
                            } else {
                                Err(AudioFailure::new(
                                    RuntimeFailureCode::CleanupTimeout,
                                    "Pulse capture cleanup failed",
                                ))
                            }
                        }
                        1 | 3 => inner
                            .close()
                            .await
                            .map(|_| None)
                            .map_err(AudioFailure::from_error),
                        _ => {
                            inner.seal.store(true, Ordering::Release);
                            inner.shared.changed.notify_waiters();
                            worker_end(&inner.playback_done).await.result
                        }
                    }
                })
                .await
                .unwrap_or_else(|_| {
                    inner.shared.suppress();
                    inner.monitor_stop.send_replace(true);
                    inner.health.send_replace(AudioHealth::NeedsRestart);
                    inner.lease.quarantine();
                    Err(AudioFailure::new(
                        if index == 2 {
                            RuntimeFailureCode::PlaybackTimeout
                        } else {
                            RuntimeFailureCode::CleanupTimeout
                        },
                        "Pulse audio acknowledgement timed out",
                    ))
                });
                tx.send_replace(Some(value));
            });
            *slot = Some(rx.clone());
            rx
        };
        drop(slot);
        loop {
            if let Some(value) = result.borrow_and_update().clone() {
                return value.map_err(Into::into);
            }
            result
                .changed()
                .await
                .map_err(|_| audio_error("Pulse control closed"))?;
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        self.inner.shared.suppress();
        self.inner.monitor_stop.send_replace(true);
        let inner = self.inner.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if tokio::time::timeout(inner.deadlines.close, inner.close())
                    .await
                    .is_err()
                {
                    inner.lease.quarantine();
                    inner.health.send_replace(AudioHealth::NeedsRestart);
                }
            });
        }
    }
}
#[async_trait]
impl AudioControl for Control {
    fn actual_join_port(&self) -> AppResult<crate::audio::join::AudioOwnerJoin> {
        Ok(self.actual.clone())
    }
    fn suppress_now(&self) {
        self.inner.shared.suppress();
    }
    fn statistics(&self) -> AudioStatistics {
        self.inner.shared.statistics()
    }
    async fn stop_capture(&self) -> AppResult<()> {
        self.inner.shared.stop_capture();
        self.request(0).await.map(|_| ())
    }
    async fn discard_pending(&self) -> AppResult<()> {
        self.inner.shared.suppress();
        self.inner.capture.clear();
        self.request(3).await.map(|_| ())
    }
    async fn drain_playback(&self) -> AppResult<PlaybackReport> {
        self.request(2)
            .await?
            .ok_or_else(|| audio_error("missing Pulse drain report"))
    }
    async fn close(&self) -> AppResult<()> {
        self.inner.shared.suppress();
        self.request(3).await.map(|_| ())
    }
}
pub(crate) async fn open(
    request: AudioOpenRequest,
    input: String,
    source: u32,
    output: String,
    sink: u32,
    spawner: Arc<dyn ProcessSpawner>,
    lease: Arc<Lease>,
    parent: crate::audio::join::AudioParent,
    #[cfg(test)] entry: Option<Arc<entry_test_support::EntryControl>>,
) -> AppResult<AudioPorts> {
    open_timed(
        request,
        input,
        source,
        output,
        sink,
        spawner,
        lease,
        Deadlines::default(),
        parent,
        #[cfg(test)]
        entry,
    )
    .await
}
#[derive(Clone, Copy)]
struct Deadlines {
    open: Duration,
    close: Duration,
    drain: Duration,
}
impl Default for Deadlines {
    fn default() -> Self {
        Self {
            open: Duration::from_secs(3),
            close: Duration::from_secs(2),
            drain: Duration::from_secs(5),
        }
    }
}
async fn open_timed(
    request: AudioOpenRequest,
    input: String,
    source: u32,
    output: String,
    sink: u32,
    spawner: Arc<dyn ProcessSpawner>,
    lease: Arc<Lease>,
    deadlines: Deadlines,
    parent: crate::audio::join::AudioParent,
    #[cfg(test)] entry: Option<Arc<entry_test_support::EntryControl>>,
) -> AppResult<AudioPorts> {
    let (source_owner, actual) = crate::audio::join::AudioOwnerSource::new();
    let (publish, receive) = tokio::sync::oneshot::channel();
    let entered = parent.clone();
    let task = tokio::spawn(async move {
        match open_inner(
            request,
            input,
            source,
            output,
            sink,
            spawner,
            lease,
            deadlines,
            entered,
            actual,
            #[cfg(test)]
            entry,
        )
        .await
        {
            Ok(Opened {
                ports,
                capture,
                playback,
                monitor,
                streams,
            }) => {
                // A cancelled observer drops the returned ports and signals
                // teardown; the original aggregate still owns all joins.
                let _ = publish.send(Ok(ports));
                let _ = tokio::join!(capture, playback, monitor);
                let _ = streams.join().await;
            }
            Err(error) => {
                let _ = publish.send(Err(error));
            }
        }
    });
    source_owner.retain_task(task, parent);
    receive
        .await
        .map_err(|_| audio_error("Pulse stream owner ended"))?
}
struct Opened {
    ports: AudioPorts,
    capture: tokio::task::JoinHandle<()>,
    playback: tokio::task::JoinHandle<()>,
    monitor: tokio::task::JoinHandle<()>,
    streams: crate::audio::join::AudioGroupScope,
}
async fn open_inner(
    request: AudioOpenRequest,
    input: String,
    source: u32,
    output: String,
    sink: u32,
    spawner: Arc<dyn ProcessSpawner>,
    lease: Arc<Lease>,
    deadlines: Deadlines,
    parent: crate::audio::join::AudioParent,
    actual: crate::audio::join::AudioOwnerJoin,
    #[cfg(test)] entry: Option<Arc<entry_test_support::EntryControl>>,
) -> AppResult<Opened> {
    #[cfg(test)]
    if let Some(entry) = &entry {
        entry.enter(entry_test_support::Boundary::OwnerReady).await;
    }
    let streams = async {
        parent.require_ready()?;
        #[cfg(test)]
        if let Some(entry) = &entry {
            entry.enter(entry_test_support::Boundary::StreamGroup).await;
        }
        parent.streams()?.begin()
    }
    .await;
    let streams = match streams {
        Ok(streams) => streams,
        Err(error) => {
            // No native leaf or process can enter before these checks finish.
            // The registered parent still joins its real empty groups below.
            lease.release();
            return Err(error);
        }
    };
    let helpers = parent.helpers();
    let capture = BoundedAudioQueue::default();
    let shared = AudioShared::new(capture.clone());
    let capture_name = format!("{}-capture", Uuid::new_v4());
    let playback_name = format!("{}-playback", Uuid::new_v4());
    let mut capture_process = None;
    let mut playback_process = None;
    let prepared=tokio::time::timeout(deadlines.open,async{
        capture_process=Some(spawner.spawn_audio_owned(stream_spec(true,&input,&capture_name), streams.leaf()?).await?.into_parts().0);
        playback_process=Some(spawner.spawn_audio_owned(stream_spec(false,&output,&playback_name), streams.leaf()?).await?.into_parts().0);
        if capture_process.as_ref().unwrap().stdout.is_none() || playback_process.as_ref().unwrap().stdin.is_none() {
            return Err(audio_error("Pulse audio process is missing its pipe"));
        }
        loop{
            let a=capture_process.as_ref().unwrap().control.clone();let b=playback_process.as_ref().unwrap().control.clone();
            let valid=tokio::select!{_ = a.wait()=>return Err(audio_error("Pulse capture exited before route readiness")),_ = b.wait()=>return Err(audio_error("Pulse playback exited before route readiness")),value=async{let(a,b)=association_pair(spawner.as_ref(),&helpers,Some((&capture_name,source)),Some((&playback_name,sink))).await?;Ok::<bool,crate::error::AppError>(a&&b)}=>value?};
            if valid{return Ok(());}tokio::time::sleep(Duration::from_millis(40)).await;
        }
    }).await.map_err(|_| crate::error::AppError::from(AudioFailure::new(RuntimeFailureCode::StartupTimeout, "Pulse stream association timed out"))).and_then(|r|r);
    if let Err(error) = prepared {
        let a = capture_process.as_ref().map(|p| p.control.clone());
        let b = playback_process.as_ref().map(|p| p.control.clone());
        let clean = tokio::time::timeout(deadlines.close, async move {
            let a = async {
                if let Some(p) = a {
                    p.terminate().await.map(|_| ())
                } else {
                    Ok(())
                }
            };
            let b = async {
                if let Some(p) = b {
                    p.terminate().await.map(|_| ())
                } else {
                    Ok(())
                }
            };
            let (a, b) = tokio::join!(a, b);
            a.and(b)
        })
        .await;
        if matches!(clean, Ok(Ok(()))) {
            lease.release();
            return Err(error);
        }
        lease.quarantine();
        return Err(AudioFailure::new(
            RuntimeFailureCode::CleanupTimeout,
            "Pulse failed-open cleanup was not acknowledged",
        )
        .into());
    }
    let mut capture_process = capture_process.unwrap();
    let mut playback_process = playback_process.unwrap();
    let seal = Arc::new(AtomicBool::new(false));
    let eof = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel(50);
    let (health, health_rx) = watch::channel(AudioHealth::Ready);
    let (capture_finished, capture_done) = watch::channel(None);
    let (playback_finished, playback_done) = watch::channel(None);
    let (monitor_stop, monitor_cancel) = watch::channel(false);
    let (monitor_finished, monitor_done) = watch::channel(false);
    let source_control = capture_process.control.clone();
    let sink_control = playback_process.control.clone();
    let reader = capture_process
        .stdout
        .take()
        .ok_or_else(|| audio_error("Pulse capture has no output pipe"))?;
    let writer = playback_process
        .stdin
        .take()
        .ok_or_else(|| audio_error("Pulse playback has no input pipe"))?;
    let s = shared.clone();
    let q = capture.clone();
    let h = health.clone();
    let capture_task = tokio::spawn(async move {
        capture_worker(
            reader,
            capture_process.control,
            q,
            request.source,
            s,
            h,
            capture_finished,
            deadlines.close,
        )
        .await;
    });
    let s = shared.clone();
    let h = health.clone();
    let sealed = seal.clone();
    let writer_eof = eof.clone();
    let playback_task = tokio::spawn(async move {
        playback_worker(
            writer,
            playback_process.control,
            rx,
            sealed,
            s,
            h,
            playback_finished,
            writer_eof,
            deadlines,
        )
        .await;
    });
    let s = shared.clone();
    let h = health.clone();
    let monitor_task = tokio::spawn(async move {
        monitor(
            spawner,
            source_control,
            sink_control,
            capture_name,
            playback_name,
            source,
            sink,
            s,
            eof,
            h,
            monitor_cancel,
            helpers,
        )
        .await;
        monitor_finished.send_replace(true);
    });
    let inner = Arc::new(Inner {
        shared,
        capture: capture.clone(),
        seal,
        health,
        capture_done,
        playback_done,
        monitor_done,
        monitor_stop,
        lease,
        deadlines,
    });
    Ok(Opened {
        ports: AudioPorts {
            capture,
            playback: tx,
            health: health_rx,
            control: Arc::new(Control {
                actual,
                inner,
                operations: std::array::from_fn(|_| Mutex::new(None)),
            }),
        },
        capture: capture_task,
        playback: playback_task,
        monitor: monitor_task,
        streams,
    })
}
async fn capture_worker(
    mut reader: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>>,
    control: Arc<dyn ProcessControl>,
    queue: BoundedAudioQueue,
    source: crate::domain::audio::AudioSource,
    shared: AudioShared,
    health: watch::Sender<AudioHealth>,
    done: watch::Sender<Option<WorkerEnd>>,
    cleanup: Duration,
) {
    let mut bytes = [0u8; 2561];
    let mut carry = 0;
    let mut sequence = 0u64;
    let mut samples = 0u64;
    let at = SystemTime::now();
    let result=async{loop{
        let changed=shared.changed.notified();tokio::pin!(changed);changed.as_mut().enable();
        if shared.gates.capture_suppressed.load(Ordering::Acquire){return Ok(None);}
        let count=tokio::select!{_=&mut changed=>continue,result=reader.read(&mut bytes[carry..carry+2560])=>result.map_err(|_|"Pulse capture read failed")?};
        if count==0{return Err(if carry!=0{"Pulse capture ended with an odd PCM byte"}else{"Pulse capture ended unexpectedly"});}
        let total=carry+count;let whole=total/2*2;if whole==0{carry=total;continue;}
        let pcm=bytes[..whole].chunks_exact(2).map(|b|i16::from_le_bytes([b[0],b[1]])).collect::<Vec<_>>();
        let time=at.checked_add(sample_offset(samples,16000)).ok_or("Pulse capture clock exhausted")?;
        samples=samples.checked_add(pcm.len() as u64).ok_or("Pulse capture clock exhausted")?;
        let frame=AudioFrame::at(source,sequence,time,PcmSpec::pcm16_mono(16000),pcm).map_err(|_|"Pulse capture frame invalid")?;sequence=sequence.checked_add(1).ok_or("Pulse capture sequence exhausted")?;
        if shared.gates.abort_pending.load(Ordering::Acquire){return Ok(None);}if queue.push_sync(frame).is_err(){return Ok(None);}
        carry=total-whole;if carry==1{bytes[0]=bytes[whole];}
    }}.await;
    if result.is_err() {
        shared.suppress();
        mark_lost(&health);
    }
    drop(reader);
    let acknowledged = matches!(
        tokio::time::timeout(cleanup, control.terminate()).await,
        Ok(Ok(_))
    );
    queue.close();
    shared
        .counters
        .capture_finalized
        .store(acknowledged, Ordering::Release);
    if !acknowledged {
        health.send_replace(AudioHealth::NeedsRestart);
    }
    if shared.gates.abort_pending.load(Ordering::Acquire) {
        queue.clear();
    }
    done.send_replace(Some(WorkerEnd {
        acknowledged,
        result: result.map_err(AudioFailure::device),
    }));
}
async fn playback_worker(
    writer: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    control: Arc<dyn ProcessControl>,
    mut input: mpsc::Receiver<AudioFrame>,
    seal: Arc<AtomicBool>,
    shared: AudioShared,
    health: watch::Sender<AudioHealth>,
    done: watch::Sender<Option<WorkerEnd>>,
    eof: Arc<AtomicBool>,
    deadlines: Deadlines,
) {
    let mut writer = Some(writer);
    let mut pipe_touched = false;
    let result: Result<(), AudioFailure> = async {
        loop {
            let changed = shared.changed.notified();
            tokio::pin!(changed); changed.as_mut().enable();
            if seal.load(Ordering::Acquire) { input.close(); }
            if shared.gates.abort_pending.load(Ordering::Acquire) { return Err(AudioFailure::new(RuntimeFailureCode::Interrupted,"Pulse playback aborted")); }
            let frame = tokio::select! { _ = &mut changed => continue, value = input.recv() => value };
            let Some(frame) = frame else { break; };
            validate_playback(&frame).map_err(AudioFailure::from_error)?;
            add(&shared.counters.accepted, frame.samples().len() as u64);
            for (i, chunk) in frame.samples().chunks(1920).enumerate() {
                if shared.gates.abort_pending.load(Ordering::Acquire) {
                    add(&shared.counters.pre_conversion_discarded, (frame.samples().len() - i * 1920) as u64);
                    return Err(AudioFailure::new(RuntimeFailureCode::Interrupted,"Pulse playback aborted"));
                }
                let bytes = chunk.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<_>>();
                pipe_touched = true;
                let write = tokio::select! {
                    biased;
                    _ = shared.aborted() => Err(AudioFailure::new(RuntimeFailureCode::Interrupted,"Pulse playback aborted")),
                    result = writer.as_mut().unwrap().write_all(&bytes) => result.map_err(|_| AudioFailure::device("Pulse playback write failed")),
                };
                if let Err(error) = write {
                    // A cancelled write can be partial. Count only untouched later input chunks.
                    add(&shared.counters.pre_conversion_discarded, (frame.samples().len() - i * 1920 - chunk.len()) as u64);
                    return Err(error);
                }
            }
        }
        tokio::select! {
            biased;
            _ = shared.aborted() => return Err(AudioFailure::new(RuntimeFailureCode::Interrupted,"Pulse playback aborted")),
            result = writer.as_mut().unwrap().shutdown() => result.map_err(|_| AudioFailure::device("Pulse playback EOF failed"))?,
        };
        // ChildStdin::shutdown is a no-op on Unix; dropping ownership closes the pipe.
        drop(writer.take());
        eof.store(true, Ordering::Release);
        // EOF is not a drain acknowledgement; wait remains cancellable by close/loss.
        let exit = tokio::select! {
            biased;
            _ = shared.aborted() => return Err(AudioFailure::new(RuntimeFailureCode::Interrupted,"Pulse playback aborted")),
            exit = tokio::time::timeout(deadlines.drain, control.wait()) => exit
                .map_err(|_| AudioFailure::new(RuntimeFailureCode::PlaybackTimeout,"Pulse server drain timed out"))?
                .map_err(AudioFailure::from_error)?,
        };
        if exit.code != Some(0) { return Err(AudioFailure::device("Pulse server drain failed")); }
        Ok(())
    }.await;
    drop(writer);
    let success = result.is_ok();
    let acknowledged = success
        || matches!(
            tokio::time::timeout(deadlines.close, control.terminate()).await,
            Ok(Ok(_))
        );
    if !success {
        shared.suppress();
        if acknowledged {
            mark_lost(&health);
        } else {
            health.send_replace(AudioHealth::NeedsRestart);
        }
        input.close();
        // try_recv is bounded even if a sender still holds a reserved permit.
        while let Ok(frame) = input.try_recv() {
            if validate_playback(&frame).is_ok() {
                add(&shared.counters.accepted, frame.samples().len() as u64);
                add(
                    &shared.counters.pre_conversion_discarded,
                    frame.samples().len() as u64,
                );
            }
        }
    }
    shared.counters.tail_known.store(
        success || (!pipe_touched && acknowledged),
        Ordering::Release,
    );
    shared
        .counters
        .playback_finalized
        .store(acknowledged, Ordering::Release);
    let result = result.map(|_| {
        Some(PlaybackReport {
            boundary: PlaybackBoundary::PulseServerDrained,
            statistics: shared.statistics().playback,
        })
    });
    done.send_replace(Some(WorkerEnd {
        acknowledged,
        result,
    }));
}
async fn monitor(
    spawner: Arc<dyn ProcessSpawner>,
    source: Arc<dyn ProcessControl>,
    sink: Arc<dyn ProcessControl>,
    capture_name: String,
    playback_name: String,
    source_index: u32,
    sink_index: u32,
    shared: AudioShared,
    eof: Arc<AtomicBool>,
    health: watch::Sender<AudioHealth>,
    mut cancel: watch::Receiver<bool>,
    helpers: crate::audio::join::AudioLeafGroup,
) {
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if *cancel.borrow_and_update() || shared.gates.abort_pending.load(Ordering::Acquire) {
            return;
        }
        let capture_active = !shared.gates.capture_suppressed.load(Ordering::Acquire);
        let output_active = !eof.load(Ordering::Acquire);
        let failed = tokio::select! {
            _=cancel.changed()=>return,
            _=shared.aborted()=>return,
            _=source.wait(),if capture_active=>!shared.gates.capture_suppressed.load(Ordering::Acquire),
            _=sink.wait(),if output_active=>!eof.load(Ordering::Acquire),
            _=tick.tick()=>{
                let check=async{match association_pair(spawner.as_ref(),&helpers,capture_active.then_some((capture_name.as_str(),source_index)),output_active.then_some((playback_name.as_str(),sink_index))).await { Ok((a,b)) => (!shared.gates.capture_suppressed.load(Ordering::Acquire) && !a) || (!eof.load(Ordering::Acquire) && !b), Err(_) => true }};
                tokio::select!{_ = cancel.changed()=>return,_ = shared.aborted()=>return,failed=check=>failed}
            }
        };
        if failed {
            shared.suppress();
            mark_lost(&health);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    mod owned;
    use super::super::process_io::{ProcessPipes, test_support::TestControl};
    use super::*;
    use crate::domain::audio::AudioSource;
    use std::{
        collections::HashMap,
        sync::{
            Mutex as StdMutex,
            atomic::{AtomicU8, Ordering as AtomicOrdering},
        },
    };
    const CLEANUP_NORMAL: u8 = 0;
    const CLEANUP_BLOCKED: u8 = 1;
    const CLEANUP_ERROR: u8 = 2;
    struct FixtureControl {
        inner: Arc<TestControl>,
        cleanup_mode: Arc<AtomicU8>,
        cleanup_release: watch::Receiver<bool>,
    }
    #[async_trait]
    impl ProcessControl for FixtureControl {
        async fn wait(&self) -> AppResult<super::super::process_io::ProcessExit> {
            self.inner.wait().await
        }
        async fn terminate(&self) -> AppResult<super::super::process_io::ProcessExit> {
            match self.cleanup_mode.load(AtomicOrdering::Acquire) {
                CLEANUP_BLOCKED => {
                    let mut release = self.cleanup_release.clone();
                    while !*release.borrow_and_update() {
                        release.changed().await.unwrap();
                    }
                    self.inner.terminate().await
                }
                CLEANUP_ERROR => Err(audio_error("injected process termination failure")),
                _ => self.inner.terminate().await,
            }
        }
    }
    // Tokio ChildStdin acknowledges shutdown without closing its descriptor.
    // Model that production contract: only dropping this writer delivers EOF.
    struct ChildStdinLike(tokio::io::DuplexStream);
    impl tokio::io::AsyncWrite for ChildStdinLike {
        fn poll_write(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            bytes: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::pin::Pin::new(&mut self.0).poll_write(cx, bytes)
        }
        fn poll_flush(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::pin::Pin::new(&mut self.0).poll_flush(cx)
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }
    struct Fixture {
        rows: StdMutex<HashMap<String, Value>>,
        specs: StdMutex<Vec<ProcessSpec>>,
        spawn_audio_calls: std::sync::atomic::AtomicUsize,
        children: StdMutex<Vec<(String, Arc<TestControl>)>>,
        capture: StdMutex<Option<tokio::io::DuplexStream>>,
        playback: StdMutex<Option<tokio::io::DuplexStream>>,
        omit_stdin: AtomicBool,
        fail_playback: AtomicBool,
        cleanup_mode: Arc<AtomicU8>,
        cleanup_release: watch::Sender<bool>,
        held_helper: StdMutex<Option<String>>,
        held_children: StdMutex<Vec<Arc<TestControl>>>,
    }
    fn device(index: u32, name: &str, monitor: Option<u32>) -> Value {
        serde_json::json!({"index":index,"name":name,"description":name,"sample_specification":"s16le 2ch 48000Hz","monitor_of_sink":monitor})
    }
    impl Fixture {
        fn new() -> Arc<Self> {
            let (cleanup_release, _) = watch::channel(false);
            let rows = HashMap::from([
                (
                    "sources".into(),
                    serde_json::json!([
                        device(10, "mic", None),
                        device(11, "VoxBridge_M1.monitor", Some(30)),
                        device(12, "VoxBridge_Input", None),
                        device(20, "VoxBridge_Output.monitor", Some(40))
                    ]),
                ),
                (
                    "sinks".into(),
                    serde_json::json!([
                        device(30, "VoxBridge_M1", None),
                        device(40, "VoxBridge_Output", None),
                        device(50, "speaker", None)
                    ]),
                ),
                (
                    "modules".into(),
                    serde_json::json!([
                        {"index":1,"name":"module-null-sink","argument":"sink_name=VoxBridge_M1"},
                        {"index":2,"name":"module-null-sink","argument":"sink_name=VoxBridge_Output"},
                        {"index":3,"name":"module-remap-source","argument":"source_name=VoxBridge_Input master=VoxBridge_M1.monitor"}
                    ]),
                ),
            ]);
            Arc::new(Self {
                rows: StdMutex::new(rows),
                specs: StdMutex::default(),
                spawn_audio_calls: std::sync::atomic::AtomicUsize::new(0),
                children: StdMutex::default(),
                capture: StdMutex::default(),
                playback: StdMutex::default(),
                omit_stdin: AtomicBool::new(false),
                fail_playback: AtomicBool::new(false),
                cleanup_mode: Arc::new(AtomicU8::new(CLEANUP_NORMAL)),
                cleanup_release,
                held_helper: StdMutex::default(),
                held_children: StdMutex::default(),
            })
        }
        fn stream_child(&self, program: &str) -> Arc<TestControl> {
            self.children
                .lock()
                .unwrap()
                .iter()
                .find(|(name, _)| name == program)
                .unwrap()
                .1
                .clone()
        }
        fn stream_rows(&self, record: bool) -> Value {
            let program = if record { "parec" } else { "pacat" };
            let specs = self.specs.lock().unwrap();
            let spec = specs
                .iter()
                .rev()
                .find(|s| s.program.to_str() == Some(program))
                .unwrap();
            let arg = |prefix: &str| {
                spec.args
                    .iter()
                    .find_map(|a| a.to_str().and_then(|s| s.strip_prefix(prefix)))
                    .unwrap()
            };
            let name = arg("--stream-name=");
            let device = arg("--device=");
            let index = match device {
                "mic" => 10,
                "VoxBridge_Output.monitor" => 20,
                "VoxBridge_M1" => 30,
                "speaker" => 50,
                _ => 999,
            };
            if record {
                serde_json::json!([{"source":index,"properties":{"media.name":name}}])
            } else {
                serde_json::json!([{"sink":index,"properties":{"media.name":name}}])
            }
        }
    }
    #[async_trait]
    impl ProcessSpawner for Fixture {
        async fn spawn_audio_owned(
            &self,
            spec: ProcessSpec,
            execution: crate::audio::join::AudioLeafExecution,
        ) -> AppResult<process_io::AudioOwnedProcess> {
            self.spawn_audio_calls.fetch_add(1, AtomicOrdering::SeqCst);
            if spec.program.to_str() == Some("pacat") && self.fail_playback.load(Ordering::Acquire)
            {
                return Err(audio_error("injected missing playback utility"));
            }
            let (source, actual) = execution.enter()?;
            let pipes = self.spawn(spec).await?;
            // Fixture wait reads the controlled positive exit independently
            // of its termination-error observer. It is an actual producer.
            let exit = pipes.control.clone();
            let process = process_io::AudioOwnedProcess::attach(pipes, actual, &source);
            source.run_owned(move |source| async move {
                let code = loop {
                    match exit.wait().await {
                        Ok(exit) => break exit.code,
                        Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                    }
                };
                drop(exit);
                source.exited(code).await;
            });
            Ok(process)
        }
        async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes> {
            let program = spec.program.to_str().unwrap().to_owned();
            self.specs.lock().unwrap().push(spec.clone());
            if program == "pacat" && self.fail_playback.load(Ordering::Acquire) {
                return Err(audio_error("injected missing playback utility"));
            }
            let control = TestControl::new();
            self.children
                .lock()
                .unwrap()
                .push((program.clone(), control.clone()));
            if program == "pactl" {
                let key = spec.args.last().unwrap().to_str().unwrap();
                let explicit = self.rows.lock().unwrap().get(key).cloned();
                let value = explicit.unwrap_or_else(|| self.stream_rows(key == "source-outputs"));
                let held = self.held_helper.lock().unwrap().as_deref() == Some(key);
                if held {
                    self.held_children.lock().unwrap().push(control.clone());
                } else {
                    control.exit(0);
                }
                let observer: Arc<dyn ProcessControl> = Arc::new(FixtureControl {
                    inner: control.clone(),
                    cleanup_mode: self.cleanup_mode.clone(),
                    cleanup_release: self.cleanup_release.subscribe(),
                });
                return Ok(ProcessPipes {
                    stdin: None,
                    stdout: Some(Box::pin(std::io::Cursor::new(
                        serde_json::to_vec(&value).unwrap(),
                    ))),
                    control: observer,
                });
            }
            let process_control: Arc<dyn ProcessControl> = Arc::new(FixtureControl {
                inner: control.clone(),
                cleanup_mode: self.cleanup_mode.clone(),
                cleanup_release: self.cleanup_release.subscribe(),
            });
            let (pipe, peer) = tokio::io::duplex(8192);
            if program == "parec" {
                *self.capture.lock().unwrap() = Some(peer);
                Ok(ProcessPipes {
                    stdin: None,
                    stdout: Some(Box::pin(pipe)),
                    control: process_control,
                })
            } else {
                *self.playback.lock().unwrap() = Some(peer);
                Ok(ProcessPipes {
                    stdin: if self.omit_stdin.load(Ordering::Acquire) {
                        None
                    } else {
                        Some(Box::pin(ChildStdinLike(pipe)))
                    },
                    stdout: None,
                    control: process_control,
                })
            }
        }
    }
    async fn ports(
        fixture: Arc<Fixture>,
        input: &str,
        output: &str,
    ) -> (
        super::super::inventory::PlatformAudioBackend,
        AudioOpenRequest,
        AppResult<AudioPorts>,
    ) {
        let backend = super::super::inventory::PlatformAudioBackend::with_spawner(fixture);
        let inventory = backend.inventory().await.unwrap();
        let request = AudioOpenRequest {
            inventory_generation: inventory.generation,
            input_id: format!("pulse:source:{input}"),
            output_id: format!("pulse:sink:{output}"),
            source: AudioSource::System,
        };
        let result = backend.open(request.clone()).await;
        (backend, request, result)
    }
    async fn failed_open_with_cleanup(
        fixture: Arc<Fixture>,
        close: Duration,
    ) -> (
        super::super::inventory::PlatformAudioBackend,
        AudioOpenRequest,
        AppResult<AudioPorts>,
    ) {
        fixture.omit_stdin.store(true, Ordering::Release);
        let backend = super::super::inventory::PlatformAudioBackend::with_spawner(fixture.clone());
        let inventory = backend.inventory().await.unwrap();
        let request = AudioOpenRequest {
            inventory_generation: inventory.generation,
            input_id: "pulse:source:mic".into(),
            output_id: "pulse:sink:VoxBridge_M1".into(),
            source: AudioSource::Microphone,
        };
        let (_, _, lease) = backend.claim(&request).unwrap();
        let result = open_timed(
            request.clone(),
            "mic".into(),
            10,
            "VoxBridge_M1".into(),
            30,
            fixture,
            lease,
            Deadlines {
                open: Duration::from_millis(100),
                close,
                drain: Duration::from_millis(100),
            },
            super::super::inventory::actual_test_support::stream(&backend),
            None,
        )
        .await;
        (backend, request, result)
    }
    #[tokio::test]
    async fn exact_virtual_roles_and_process_argv_are_verified_before_ready() {
        let fixture = Fixture::new();
        let (_, _, result) =
            ports(fixture.clone(), "VoxBridge_Output.monitor", "VoxBridge_M1").await;
        let ports = result.unwrap();
        assert_eq!(*ports.health.borrow(), AudioHealth::Ready);
        let specs = fixture.specs.lock().unwrap().clone();
        for (record, program, device, rate) in [
            (true, "parec", "VoxBridge_Output.monitor", 16000),
            (false, "pacat", "VoxBridge_M1", 24000),
        ] {
            let spec = specs
                .iter()
                .find(|s| s.program.to_str() == Some(program))
                .unwrap();
            let args = spec
                .args
                .iter()
                .map(|s| s.to_str().unwrap())
                .collect::<Vec<_>>();
            assert!(args.contains(&format!("--device={device}").as_str()));
            assert!(args.contains(&format!("--rate={rate}").as_str()));
            assert!(args.contains(&"--format=s16le"));
            assert!(args.contains(&"--latency-msec=80"));
            assert_eq!(spec.pipe_stdout, record);
            assert_eq!(spec.pipe_stdin, !record);
        }
        assert!(
            specs
                .iter()
                .any(|s| s.args.last().is_some_and(|s| s == "source-outputs"))
        );
        assert!(
            specs
                .iter()
                .any(|s| s.args.last().is_some_and(|s| s == "sink-inputs"))
        );
        ports.control.close().await.unwrap();
        assert!(fixture.stream_child("parec").exited());
        assert!(fixture.stream_child("pacat").exited());
    }
    #[tokio::test]
    async fn missing_or_wrong_remap_master_cannot_make_virtual_inventory_ready() {
        for modules in [
            serde_json::json!([]),
            serde_json::json!([
                {"name":"module-null-sink","argument":"sink_name=VoxBridge_M1"},
                {"name":"module-null-sink","argument":"sink_name=VoxBridge_Output"},
                {"name":"module-remap-source","argument":"source_name=VoxBridge_Input master=other.monitor"}
            ]),
        ] {
            let fixture = Fixture::new();
            fixture
                .rows
                .lock()
                .unwrap()
                .insert("modules".into(), modules);
            let registry = crate::audio::join::AudioRegistry::new();
            let parent = registry.inventory().await.unwrap();
            let (driver, rows) = inventory(fixture.as_ref(), &parent).await.unwrap();
            parent.finish().await.unwrap();
            assert_ne!(driver, DriverStatus::Ready);
            assert!(
                rows.iter()
                    .filter(|(r, _)| matches!(
                        r.role,
                        EndpointRole::M1Render | EndpointRole::M2Capture
                    ))
                    .all(|(r, _)| !r.endpoint.ready)
            );
        }
    }
    #[tokio::test]
    async fn wrong_stream_device_association_is_rejected() {
        let fixture = Fixture::new();
        fixture.rows.lock().unwrap().insert(
            "source-outputs".into(),
            serde_json::json!([{"source":999,"properties":{"media.name":"fixture"}}]),
        );
        assert!(
            !associated(fixture.as_ref(), true, "fixture", 20)
                .await
                .unwrap()
        );
    }
    #[tokio::test]
    async fn partial_start_failure_reaps_capture_and_releases_pair() {
        let fixture = Fixture::new();
        fixture.fail_playback.store(true, Ordering::Release);
        let (backend, req, result) = ports(fixture.clone(), "mic", "VoxBridge_M1").await;
        assert!(result.is_err());
        assert_eq!(
            fixture.stream_child("parec").kills.load(Ordering::Acquire),
            1
        );
        fixture.fail_playback.store(false, Ordering::Release);
        backend
            .open(req)
            .await
            .unwrap()
            .control
            .close()
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn missing_pipe_reaps_both_children_and_releases_pair() {
        let fixture = Fixture::new();
        fixture.omit_stdin.store(true, Ordering::Release);
        let (backend, req, result) = ports(fixture.clone(), "mic", "VoxBridge_M1").await;
        assert!(matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::DeviceUnavailable,
                ..
            })
        ));
        assert!(fixture.stream_child("parec").exited());
        assert!(fixture.stream_child("pacat").exited());
        fixture.omit_stdin.store(false, Ordering::Release);
        backend
            .open(req)
            .await
            .unwrap()
            .control
            .close()
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn failed_open_cleanup_timeout_is_typed_and_quarantines_pair() {
        let fixture = Fixture::new();
        fixture
            .cleanup_mode
            .store(CLEANUP_BLOCKED, AtomicOrdering::Release);
        let (backend, request, result) =
            failed_open_with_cleanup(fixture.clone(), Duration::from_millis(30)).await;
        let replacement_denied = backend.claim(&request).is_err();
        // Release even on a regression so no injected termination remains blocked.
        fixture.cleanup_release.send_replace(true);

        assert!(matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::CleanupTimeout,
                ..
            })
        ));
        assert!(replacement_denied);
    }
    #[tokio::test]
    async fn failed_open_terminate_error_is_typed_and_quarantines_pair() {
        let fixture = Fixture::new();
        fixture
            .cleanup_mode
            .store(CLEANUP_ERROR, AtomicOrdering::Release);
        let (backend, request, result) =
            failed_open_with_cleanup(fixture.clone(), Duration::from_millis(100)).await;
        let replacement_denied = backend.claim(&request).is_err();
        fixture.cleanup_release.send_replace(true);

        assert!(matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::CleanupTimeout,
                ..
            })
        ));
        assert!(replacement_denied);
    }
    #[tokio::test]
    async fn abort_after_eof_cancels_server_wait_promptly() {
        let control = TestControl::new();
        let (writer, mut reader) = tokio::io::duplex(32);
        let (tx, rx) = mpsc::channel(2);
        drop(tx);
        let shared = AudioShared::new(BoundedAudioQueue::default());
        let (health, _) = watch::channel(AudioHealth::Ready);
        let (end, done) = watch::channel(None);
        let task = tokio::spawn(playback_worker(
            Box::pin(writer),
            control.clone(),
            rx,
            Arc::new(AtomicBool::new(true)),
            shared.clone(),
            health,
            end,
            Arc::new(AtomicBool::new(false)),
            Deadlines::default(),
        ));
        assert_eq!(reader.read(&mut [0u8; 1]).await.unwrap(), 0);
        shared.suppress();
        let result = tokio::time::timeout(Duration::from_millis(100), worker_end(&done)).await;
        control.exit(0);
        task.await.unwrap();
        assert!(
            result.is_ok(),
            "abort must interrupt waiting for server drain after EOF"
        );
    }
    #[tokio::test]
    async fn final_fix_real_stdin_eof_precedes_exit_zero_drain_acknowledgement() {
        let fixture = Fixture::new();
        let (_, _, ports) = ports(fixture.clone(), "mic", "VoxBridge_M1").await;
        let ports = ports.unwrap();
        ports.control.stop_capture().await.unwrap();
        assert!(fixture.stream_child("parec").exited());
        assert!(!fixture.stream_child("pacat").exited());
        assert_eq!(*ports.health.borrow(), AudioHealth::Ready);
        ports
            .playback
            .send(
                AudioFrame::new(
                    AudioSource::System,
                    0,
                    PcmSpec::pcm16_mono(24000),
                    vec![17; 1280],
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let mut reader = fixture.playback.lock().unwrap().take().unwrap();
        let control = ports.control.clone();
        let mut drain = tokio::spawn(async move { control.drain_playback().await });
        let mut written = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), reader.read_to_end(&mut written))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(written.len(), 2560);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut drain)
                .await
                .is_err()
        );
        fixture.stream_child("pacat").exit(0);
        let report = drain.await.unwrap().unwrap();
        assert_eq!(report.boundary, PlaybackBoundary::PulseServerDrained);
        assert_eq!(report.statistics.accepted_for_processing_samples, 1280);
        assert!(report.statistics.native.is_none());
        assert!(report.statistics.finalized && report.statistics.tail_disposition_known);
        ports.control.close().await.unwrap();
    }
    #[tokio::test]
    async fn blocked_write_is_cancelled_and_only_untouched_input_is_counted_as_discarded() {
        let control = TestControl::new();
        let (writer, mut reader) = tokio::io::duplex(1);
        let (tx, rx) = mpsc::channel(2);
        for (n, length) in [(0, 24000), (1, 1280)] {
            tx.try_send(
                AudioFrame::new(
                    AudioSource::Microphone,
                    n,
                    PcmSpec::pcm16_mono(24000),
                    vec![7; length],
                )
                .unwrap(),
            )
            .unwrap();
        }
        let shared = AudioShared::new(BoundedAudioQueue::default());
        let (health, _) = watch::channel(AudioHealth::Ready);
        let (end, done) = watch::channel(None);
        tokio::spawn(playback_worker(
            Box::pin(writer),
            control.clone(),
            rx,
            Arc::new(AtomicBool::new(false)),
            shared.clone(),
            health,
            end,
            Arc::new(AtomicBool::new(false)),
            Deadlines::default(),
        ));
        reader.read_exact(&mut [0u8; 1]).await.unwrap();
        shared.suppress();
        let end = tokio::time::timeout(Duration::from_millis(100), worker_end(&done))
            .await
            .unwrap();
        assert!(end.acknowledged && end.result.is_err());
        let stats = shared.statistics().playback;
        assert_eq!(stats.accepted_for_processing_samples, 25280);
        assert_eq!(stats.discarded_before_conversion_samples, 23360);
        assert!(!stats.tail_disposition_known);
        assert!(stats.finalized);
        assert!(stats.native.is_none());
        assert_eq!(control.kills.load(Ordering::Acquire), 1);
    }
    struct ChunkReader {
        chunks: std::collections::VecDeque<Vec<u8>>,
        eof: bool,
    }
    impl tokio::io::AsyncRead for ChunkReader {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            out: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if let Some(chunk) = self.chunks.pop_front() {
                out.put_slice(&chunk);
                std::task::Poll::Ready(Ok(()))
            } else if self.eof {
                std::task::Poll::Ready(Ok(()))
            } else {
                std::task::Poll::Pending
            }
        }
    }
    #[tokio::test]
    async fn capture_carries_odd_reads_and_preserves_explicit_source() {
        let reader = ChunkReader {
            chunks: [vec![0x34], vec![0x12, 0x78, 0x56]].into(),
            eof: false,
        };
        let control = TestControl::new();
        let queue = BoundedAudioQueue::default();
        let shared = AudioShared::new(queue.clone());
        let (health, _) = watch::channel(AudioHealth::Ready);
        let (end, done) = watch::channel(None);
        tokio::spawn(capture_worker(
            Box::pin(reader),
            control,
            queue.clone(),
            AudioSource::System,
            shared.clone(),
            health,
            end,
            Duration::from_millis(100),
        ));
        let frame = tokio::time::timeout(Duration::from_secs(1), queue.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.samples(), [0x1234, 0x5678]);
        assert_eq!(frame.source(), AudioSource::System);
        shared.stop_capture();
        assert!(worker_end(&done).await.result.is_ok());
        assert!(shared.statistics().capture.finalized);
    }
    #[tokio::test]
    async fn odd_byte_eof_fails_and_reaps_capture() {
        let reader = ChunkReader {
            chunks: [vec![1]].into(),
            eof: true,
        };
        let control = TestControl::new();
        let queue = BoundedAudioQueue::default();
        let shared = AudioShared::new(queue.clone());
        let (health, state) = watch::channel(AudioHealth::Ready);
        let (end, done) = watch::channel(None);
        capture_worker(
            Box::pin(reader),
            control.clone(),
            queue,
            AudioSource::System,
            shared,
            health,
            end,
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(
            worker_end(&done).await.result.unwrap_err().message,
            "Pulse capture ended with an odd PCM byte"
        );
        assert_eq!(*state.borrow(), AudioHealth::Lost);
        assert_eq!(control.kills.load(Ordering::Acquire), 1);
    }
    #[tokio::test]
    async fn association_timeout_rolls_back_both_processes_with_typed_startup_failure() {
        let fixture = Fixture::new();
        fixture
            .rows
            .lock()
            .unwrap()
            .insert("sink-inputs".into(), serde_json::json!([]));
        let backend = super::super::inventory::PlatformAudioBackend::with_spawner(fixture.clone());
        let inventory = backend.inventory().await.unwrap();
        let request = AudioOpenRequest {
            inventory_generation: inventory.generation,
            input_id: "pulse:source:mic".into(),
            output_id: "pulse:sink:VoxBridge_M1".into(),
            source: AudioSource::Microphone,
        };
        let (_, _, lease) = backend.claim(&request).unwrap();
        let result = open_timed(
            request.clone(),
            "mic".into(),
            10,
            "VoxBridge_M1".into(),
            30,
            fixture.clone(),
            lease,
            Deadlines {
                open: Duration::from_millis(60),
                close: Duration::from_millis(100),
                drain: Duration::from_millis(100),
            },
            super::super::inventory::actual_test_support::stream(&backend),
            None,
        )
        .await;
        assert!(matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::StartupTimeout,
                ..
            })
        ));
        for name in ["parec", "pacat"] {
            assert_eq!(fixture.stream_child(name).kills.load(Ordering::Acquire), 1);
        }
        let (_, _, lease) = backend.claim(&request).unwrap();
        lease.release();
    }
    #[tokio::test]
    async fn nonzero_server_exit_does_not_fabricate_a_drain_boundary() {
        let control = TestControl::new();
        control.exit(1);
        let (tx, rx) = mpsc::channel(1);
        drop(tx);
        let shared = AudioShared::new(BoundedAudioQueue::default());
        let (health, _) = watch::channel(AudioHealth::Ready);
        let (end, done) = watch::channel(None);
        playback_worker(
            Box::pin(tokio::io::sink()),
            control,
            rx,
            Arc::new(AtomicBool::new(true)),
            shared,
            health,
            end,
            Arc::new(AtomicBool::new(false)),
            Deadlines::default(),
        )
        .await;
        assert!(worker_end(&done).await.result.is_err());
    }
}
