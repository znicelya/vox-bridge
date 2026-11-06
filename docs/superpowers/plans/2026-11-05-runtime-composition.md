# Translation Runtime Composition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Connect audio, providers, persistence and desktop commands into usable live/call lifecycles with independent directions, durable subtitles and bounded cleanup.

**Architecture:** Full-direction provider preflight and fresh backend inventory produce an immutable launch snapshot before opening resources. One runtime actor per direction owns provider/audio work; the orchestrator alone commits session lifecycle. Injectable ports isolate supervised Pulse-compatible processes on Linux and CPAL handles retained on their owner thread on Windows/macOS.

**Tech Stack:** Existing Tauri 2, Tokio, CPAL 0.15, rusqlite, serde and provider adapters; Linux pactl/parec/pacat from pulseaudio-utils, also usable through pipewire-pulse. Add Tokio's process feature, not another DSP stack or cloud service.

**Spec:** [Approved workbench design](../specs/2026-11-04-live-translation-workbench-design.md).

## Global Constraints

- 首发最低版本：Windows 10 22H2、macOS 13 Ventura、Ubuntu 22.04 LTS。Linux 同时覆盖 PipeWire 和 PulseAudio 常见配置。
- Live writes translated speech to M1. Calls have independent A→B/M1 and B→A/physical-output paths. Facing another person in the room is outside first-release scope.
- 设备名称固定为逻辑端点 VoxBridge Input（M1，虚拟麦克风）和 VoxBridge Output（M2，虚拟扬声器）. The application opens M1's render side and M2's capture side.
- 通话软件选择 M1 作为麦克风、M2 作为扬声器；应用不静默修改系统默认输入/输出设备。
- Explicit language pair; a stream has one direction and reverse streams swap languages.
- Input is 16 kHz PCM16 mono, target 80 ms/1,280 samples. Internal playback is 24 kHz PCM16 mono.
- Preserve source/translation kind, direction, sequence, original provider event type, supplied times and native/local correlation. Do not invent sentence pairing or provider-internal timing.
- Final text, session context, errors and measured latency are durable. Interim text stays in memory. Audio and credentials never enter frontend events, history, exports or ordinary logs.
- Queues, packet sizes, task counts, startup, shutdown and retries are bounded. Capture overflow drops oldest pending capture; discontinuity clears stale capture/carry/playback.
- Device recovery requires explicit resume. Native handles remain on their owner thread; never add unsafe Send/Sync.
- Default tests use SQLite, injected keys, fixture providers and synthetic audio/process ports; no native keyring, hardware, driver utilities, network or paid API.
- Migrations preserve existing sessions. Real OS routing, package signing, permissions and live provider acceptance remain external gates.

## Prerequisites and scope rulings

Execute after provider phase Task 5 (reviewed code 294a553, status docs adf686b). Consume these existing methods:

~~~rust
ProviderFactory::new(Arc<SecretStore>) -> ProviderFactory
ProviderFactory::capabilities(&self, &ProviderConfig) -> ProviderResult<ProviderCapabilities>
ProviderFactory::prepare(&self, &ProviderConfig, Uuid, &SessionConfig, PcmSpec, PcmSpec)
    -> ProviderResult<PreparedProviderSession>
PreparedProviderSession::configs(&self) -> &[ProviderSessionConfig]
PreparedProviderSession::connect(self)
    -> impl Future<Output = ProviderResult<Vec<DirectedProviderStream>>>
DirectedProviderStream { config: ProviderSessionConfig, stream: ProviderStream }
~~~

Streams returned by connect are not ready until SessionStarted. Preserve full-direction validation before connecting. Task 4 adds a narrow single-direction opening method for explicit resume; existing methods remain unchanged.

This is integration between approved audio/provider work and the workbench. UI, LLM summaries, SrvDB and Tavily stay in the separate phase 4 plan. Do not re-open product brainstorming or request approval for these already-authorized backend routes.

Decisions which executors must retain:

1. Persist typed final events, not fabricated source/translation pairs. Legacy metadata remains absent and identifiable.
2. Linux CPAL/ALSA cannot be assumed to enumerate the installed Pulse null sinks. Implement explicit parec/pacat I/O instead of permanently blocking approved Linux routes or falling back to defaults.
3. An inventory entry and driver Ready do not prove a route is open. Verify exact device association at open; reject ambiguous CPAL names.
4. Playback completion means CPAL callback consumption or successful pacat EOF/server drain. Neither is an acoustic DAC timestamp; unavailable latency remains unknown.
5. FixtureProvider defaults to 16 kHz output. Add an explicit validated 24 kHz fixture profile while preserving its current default/tests.
6. A stuck native owner cannot be killed safely. Suppress callbacks, retain/quarantine its lease, report NeedsRestart/Failed and deny replacement. Never claim Completed or accumulate replacement threads.
7. Provider adapters own automatic reconnect/retry windows. Runtime observes Gap/Started/Failed and does not add another retry loop.

## File and dependency map

Source paths below are under src-tauri/src; migrations are under src-tauri/migrations.

| Task | Depends on | Owned units | Deliverable |
|---|---|---|---|
| 1 | Provider phase | domain/transcript, session/transcript_sink, storage/migration 0002, events/export | Directed text, diagnostics and legacy compatibility |
| 2 | Existing audio | audio/packetizer, frame, queue, router | Bounded input continuity and independent routes |
| 3 | 2 | audio/runtime; platform/inventory, native_io, pulse_io, process_io; capture/playback lifecycle | Real injectable ports and explicit Linux I/O |
| 4 | 1–3 | domain/launch; session/runtime, direction, test_support; narrow factory/fixture additions | Independent translation runtime |
| 5 | 4 | session/orchestrator, storage context/lifecycle | Durable state transitions and concurrent stop |
| 6 | 5 | config, app, commands/subscriptions, commands, lib; acceptance docs | Configured desktop commands |

Tasks 1 and 2 can be researched independently but module edits integrate sequentially. Runtime/lifecycle/app integration executes in order. Task 6's configuration storage/worker foundation may execute as subtask 6A alongside Task 4: it owns only config/{mod,models,secret_store,secret_store_tests,worker,worker_tests}.rs and produces the unchanged ConfigStore/SecretStore/ConfigIo/ConfigTransaction APIs specified below. It has no runtime/lifecycle dependency; current ConfigStore constructor callers are config tests. Subtask 6B retains app ownership, command DTOs and credential-change policy/compensation, subscriptions, provider capability serialization, runtime display-name use and documentation after Task 5. Review 6A independently before its consumers use it, and review the completed Task 6 integration after 6B. Other concurrent workers must not touch 6A's config files. Test modules live beside the corresponding unit. Each task ends with its own test/review/commit gate.

The bounded subscription registry/forwarder may likewise execute independently as subtask 6C, owning commands/subscriptions.rs and its focused tests plus only a module declaration in commands.rs. It consumes an already-resolved broadcast receiver and FrontendEmitter, so it needs no unfinished orchestrator API. Subtask 6B still owns resolving active/historical terminal receivers, frontend wrappers, deletion/window-shutdown wiring and their integration tests. Review 6C before those consumers. Keep Task4 resource fixes in session/audio files, with no concurrent commands.rs edits.

### Task 1: Durable transcript boundary and audio-free events

**Files:** Create domain/transcript.rs, session/transcript_sink.rs, session/transcript_sink_tests.rs and migrations/0002_runtime.sql. Modify domain/mod.rs, session/mod.rs, events/{mod,bus}.rs, storage/{mod,models,sqlite}.rs, export/{mod,json,markdown,plain_text,export_tests}.rs and storage tests.

**Interfaces produced:** In domain/transcript.rs, derive Clone, Debug, PartialEq, Eq, Serialize and Deserialize, with snake_case enum serialization:

~~~rust
pub enum TranscriptKind { Source, Translation }
pub struct TranscriptDetails {
    pub kind: TranscriptKind,
    pub provider: ProviderId,
    pub provider_config_id: Uuid,
    pub stream_epoch: u64,
    pub provider_event_type: Option<String>,
    pub provider_timestamp_ms: Option<u64>,
    pub sentence_id: Option<String>,
    pub local_correlation_id: Option<u64>,
    pub log_id: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}
pub struct TranscriptUpdate {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: SessionDirection,
    pub sequence: u64,
    pub occurred_at_ms: i64,
    pub language: String,
    pub text: String,
    pub is_final: bool,
    pub details: Option<TranscriptDetails>,
}
pub struct TranscriptPage {
    pub rows: Vec<TranscriptUpdate>,
    pub next_cursor: Option<i64>,
}
pub enum RuntimeFailureCode {
    Provider, Storage, DeviceUnavailable, AudioBackendUnavailable, StartupTimeout,
    FinishTimeout, PlaybackTimeout, CleanupTimeout, Interrupted, InvalidEvent,
}
pub enum GapCause { ProviderReconnect, CaptureOverflow, OutputOverflow, DeviceLoss, ManualResume }
pub enum PlaybackBoundary { CallbackConsumed, PulseServerDrained, Discarded }
pub enum DiagnosticPayload {
    Gap { cause: GapCause, dropped_frames: u64 },
    TranscriptOverflow { dropped_interims: u64 },
    Failure { code: RuntimeFailureCode, provider_code: Option<i32>, log_id: Option<String> },
    Latency {
        capture_to_send_ms: Option<u64>, response_to_enqueue_ms: Option<u64>,
        enqueue_to_callback_ms: Option<u64>, provider_processing_ms: Option<u64>,
    },
    Playback { boundary: PlaybackBoundary, accepted_samples: u64, discarded_samples: u64 },
}
pub struct RuntimeDiagnostic {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: Option<SessionDirection>,
    pub stream_epoch: Option<u64>,
    pub occurred_at_ms: i64,
    pub payload: DiagnosticPayload,
}
pub struct DiagnosticPage {
    pub rows: Vec<RuntimeDiagnostic>,
    pub next_cursor: Option<i64>,
}
impl TranscriptSink {
    pub fn new(store: Arc<LocalStore>, bus: EventBus, config: ProviderSessionConfig,
        provider: ProviderId) -> AppResult<Self>;
    pub fn accept(&mut self, event: &ProviderEvent) -> AppResult<Option<TranscriptUpdate>>;
    pub fn advance_epoch(&mut self) -> AppResult<u64>;
    pub fn epoch(&self) -> u64;
    pub fn clear_interims(&mut self);
    pub fn take_dropped_interims(&mut self) -> u64;
}
impl LocalStore {
    pub fn append_transcript(&self, update: &TranscriptUpdate) -> AppResult<()>;
    pub fn transcript_page(&self, session_id: Uuid, after: Option<i64>,
        limit: u32) -> AppResult<TranscriptPage>;
    pub fn next_transcript_sequence(&self, session_id: Uuid,
        direction: SessionDirection) -> AppResult<u64>;
    pub fn append_diagnostic(&self, diagnostic: &RuntimeDiagnostic) -> AppResult<()>;
    pub fn diagnostic_page(&self, session_id: Uuid, after: Option<i64>,
        limit: u32) -> AppResult<DiagnosticPage>;
}
~~~

These are required signatures, not comments suggesting an API. Imports use existing crate domain/provider/storage types. TranscriptSink is a real struct storing the collaborators, directed config, epoch, sequence allocator and bounded active identities.

Replace InterimTranscript/FinalTranscript with SessionEvent::Transcript { update: TranscriptUpdate }. Replace TranslationAudio with SessionEvent::Diagnostic { diagnostic: RuntimeDiagnostic }; retain existing status/audio-state events and add ResyncNeeded { session_id: Uuid, missed_events: u64 }. Update session_id() and every producer/test/match site. No payload reachable from SessionEvent may contain AudioFrame, Vec<i16>, secrets or raw provider error strings.

- [ ] **1. Write a failing directed-identity/storage test.** In transcript_sink_tests.rs:

~~~rust
#[test]
fn reverse_source_keeps_its_role_and_interim_identity() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let id = Uuid::new_v4();
    store.create_session(&StoredSession {
        id, started_at: 1, mode: "call".into(), direction: "bidirectional".into(),
        source_language: "zh".into(), target_language: "en".into(),
    }).unwrap();
    let bus = EventBus::new();
    bus.open_session(id);
    let mut rx = bus.subscribe(id);
    let config = ProviderSessionConfig {
        session_id: id, direction: SessionDirection::TargetToSource,
        mode: SessionMode::Call, languages: LanguagePair::new("en", "zh").unwrap(),
        target_audio: PcmSpec::pcm16_mono(24_000), provider_config_id: Uuid::new_v4(),
    };
    let mut sink = TranscriptSink::new(store.clone(), bus, config,
        ProviderId::new("fixture").unwrap()).unwrap();
    let metadata = ProviderEventMetadata {
        session_id: id, direction: SessionDirection::TargetToSource,
        provider_event_type: Some("651".into()), timestamp: Some(Duration::from_millis(10)),
        sentence_id: Some("native-7".into()), local_correlation_id: None, log_id: None,
    };
    let text = TimedTranslation {
        translation: Translation { language: "en".into(), text: "pump".into() },
        start: Some(Duration::from_millis(10)), end: Some(Duration::from_millis(80)),
    };
    let interim = sink.accept(&ProviderEvent::SourceInterim {
        metadata: metadata.clone(), text: text.clone(),
    }).unwrap().unwrap();
    assert_eq!(store.count_segments(id).unwrap(), 0);
    let final_row = sink.accept(&ProviderEvent::SourceFinal { metadata, text })
        .unwrap().unwrap();
    assert_eq!(interim.id, final_row.id);
    assert_eq!(interim.sequence, final_row.sequence);
    assert_eq!(store.transcript_page(id, None, 50).unwrap().rows, vec![final_row.clone()]);
    let details = final_row.details.as_ref().unwrap();
    assert_eq!(details.kind, TranscriptKind::Source);
    assert_eq!(details.sentence_id.as_deref(), Some("native-7"));
    assert_eq!(details.start_ms, Some(10));
    assert!(matches!(rx.try_recv().unwrap(), SessionEvent::Transcript { update }
        if !update.is_final));
    assert!(matches!(rx.try_recv().unwrap(), SessionEvent::Transcript { update }
        if update == final_row));
    let data = store.get_export_data(id).unwrap().unwrap();
    assert_eq!(crate::export::segment_role(&data, &data.segments[0]), "source");
}
~~~

Add cases for translation-before-source, missing correlation remaining unpaired, new epoch not reusing IDs, final persistence with no subscriber, and Audio returning None with no row. Serialize the new events and assert their intended fields. A SQLite BEFORE INSERT trigger raising ABORT must make accept(final) fail without publishing a durable-final event.

- [ ] **2. Establish the failing baseline.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib transcript_sink
~~~

Expected: missing new types/methods, then behavioral failures until implemented.

- [ ] **3. Implement additive migration and bounded reads.** Leave 0001 unchanged. initialize enables foreign keys, runs 0001 when needed, then applies 0002 and PRAGMA user_version=2 in one transaction. Version 0 includes existing 0001 databases; an unknown newer version fails without writes. Start 0002 with:

~~~sql
ALTER TABLE translation_segments ADD COLUMN details_json TEXT;
ALTER TABLE translation_segments ADD COLUMN occurred_at_ms INTEGER;
CREATE TABLE session_runtime_context (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    context_json TEXT NOT NULL
);
CREATE TABLE session_diagnostics (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    occurred_at_ms INTEGER NOT NULL,
    payload_json TEXT NOT NULL
);
CREATE INDEX session_diagnostics_session ON session_diagnostics(session_id);
~~~

Legacy details remain NULL. Add optional details/occurred_at_ms to StoredTranscriptSegment; retain append_final_segment as an explicitly legacy writer. New append_transcript requires final=true, details present, a single direction and checked i64 conversions. All touched INSERT/SELECT statements use explicit columns. Pages use session-filtered rowid > cursor, ORDER BY rowid, limit 1–200; fetch limit+1 to derive next_cursor. Cross-direction insertion order, not per-direction sequence, determines page cursors. Exports may read a whole session for a file; frontend uses pages.

Build a temporary database directly with include_str!("../../migrations/0001_session_foundation.sql"), insert one legacy final row, reopen through LocalStore twice, and verify unchanged ID/text, details=None and version=2. Test migration rollback leaves both schema version and data consistent.

- [ ] **4. Implement the sink, diagnostics and export mapping.** A sink survives reconnect/resume. Use (epoch, kind, native sentence ID), otherwise (epoch, kind, local correlation ID), otherwise one current interim identity per kind. A final reuses that interim ID/sequence and closes it; source and translation never share an invented association. When a provider first supplies correlation after an unassociated interim, prefer an exact key; otherwise promote only the sole same-kind unassociated active identity, preserving ID/sequence. Competing same-kind identities prevent inference. A correlated interim updates its key only after validation/duplicate suppression; a final mutates identity only after storage succeeds. Cover local/native promotion, ambiguity, duplicates and failed-write retry. Bound active and recently finalized identities to 64 each. Late duplicate identified finals are suppressed within that window; unknown/unassociated finals are preserved. Sequence monotonically increases across epochs; advance_epoch clears interim state without resetting it. accept maps subtitle variants only; the runtime calls advance_epoch once per gap, so accept(Gap) must not increment independently. Validate session/direction; carry actual provider and TimedTranslation times with checked conversions.

Use this durable-before-publish order:

~~~rust
if update.is_final {
    self.store.append_transcript(&update)?;
}
let _ = self.bus.publish(update.session_id,
    SessionEvent::Transcript { update: update.clone() });
~~~

Reuse provider text/metadata bounds; interim identity overflow drops the oldest interim and increments a saturating counter returned-and-reset by take_dropped_interims. Task 1 owns typed diagnostic persistence and bounded paging, with storage-failure tests. Task 4 owns persistence-before-publication of runtime diagnostics, one-second coalescing of latency/drop/TranscriptOverflow counters per direction/category, flushing at stop, and one row per distinct error transition. Storage failure returns an error here; Task 4 pauses that direction and reports a local Storage code without recursively persisting failures. This keeps runtime timing/state out of TranscriptSink. Add counter reset/overflow tests; no text is copied into diagnostics.

Change segment_role(data, segment) to use details.kind. Only legacy rows use a fallback based on that row's direction and the session language pair; ambiguous direction/identical languages yields "unknown". JSON includes explicit metadata/legacy origin and diagnostics/context when present. ExportData adds diagnostics: Vec<RuntimeDiagnostic> and context: Option<serde_json::Value>, read from the context table without depending on the later SessionContext type; Task 5 adds the typed session_context accessor over the same stored JSON. No audio enters any export.

- [ ] **5. Verify and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib transcript_sink
cargo test --manifest-path src-tauri/Cargo.toml --lib storage
cargo test --manifest-path src-tauri/Cargo.toml --lib export
git diff --check
~~~

Commit only Task 1 paths: feat: persist directed source and translation events.

### Task 2: Bounded packetization and independent routes

**Files:** Create audio/{packetizer,packetizer_tests}.rs. Modify audio/{mod,frame,queue,router}.rs and their existing test modules.

**Consumes:** Existing AudioFrame, PcmSpec, AudioSource, AudioGraph and AudioRouter.

**Produces:**

~~~rust
pub const INPUT_PACKET_SAMPLES: usize = 1280;
pub const MAX_CAPTURE_FRAME_SAMPLES: usize = 16_000;
impl AudioFrame {
    pub fn at(source: AudioSource, sequence: u64, captured_at: SystemTime,
        spec: PcmSpec, samples: Vec<i16>) -> AppResult<Self>;
}
pub struct AudioPacketizer;
impl AudioPacketizer {
    pub fn new(source: AudioSource) -> Self;
    pub fn push(&mut self, frame: AudioFrame) -> AppResult<Vec<AudioFrame>>;
    pub fn flush(&mut self) -> AppResult<Option<AudioFrame>>;
    pub fn reset(&mut self);
}
impl BoundedAudioQueue {
    pub fn push_sync(&self, frame: AudioFrame) -> Result<(), AudioFrame>;
    pub async fn recv(&self) -> Option<AudioFrame>;
    pub fn close(&self);
    pub fn clear(&self) -> usize;
}
impl<H: EndpointHealthCheck> AudioRouter<H> {
    pub fn pause_direction(&self, direction: Direction, reason: String) -> AppResult<()>;
    pub fn resume_direction(&self, direction: Direction) -> Result<(), AudioRouterStatus>;
    pub fn direction_status(&self, direction: Direction) -> AppResult<AudioRouterStatus>;
    pub fn replace_direction_output(&self, direction: Direction,
        output: mpsc::Sender<AudioFrame>) -> AppResult<()>;
}
~~~

AudioPacketizer's private fields are source, next packet sequence, carry (<1,280 samples), carry first-sample time and input continuity. AudioFrame::new delegates to at(now). Keep queue push/pop/capacity/dropped_frames APIs. Use a short std Mutex, Notify and closed flag so a non-real-time conversion worker and async consumer share one bounded queue; CPAL callback never locks it.

- [ ] **1. Write the packetizer/queue regression tests.**

~~~rust
#[test]
fn fragments_preserve_samples_and_time_then_flush_once() {
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(7);
    let mut p = AudioPacketizer::new(AudioSource::System);
    let mut packets = Vec::new();
    for (sequence, (offset, count)) in [(0, 700), (700, 1800)].into_iter().enumerate() {
        packets.extend(p.push(AudioFrame::at(
            AudioSource::System, sequence as u64,
            at + Duration::from_micros(offset as u64 * 1_000_000 / 16_000),
            PcmSpec::pcm16_mono(16_000),
            (offset..offset + count).map(|n| n as i16).collect(),
        ).unwrap()).unwrap());
    }
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].samples().len(), 1280);
    packets.push(p.flush().unwrap().unwrap());
    assert!(p.flush().unwrap().is_none());
    assert_eq!(packets[1].captured_at(), at + Duration::from_millis(80));
    let all: Vec<i16> = packets.iter().flat_map(|p| p.samples().iter().copied()).collect();
    assert_eq!(all, (0..2500).map(|n| n as i16).collect::<Vec<_>>());
}
#[tokio::test]
async fn overflow_keeps_recent_frames_and_close_is_replayable() {
    let q = BoundedAudioQueue::new(2);
    for sequence in 0..3 {
        q.push_sync(AudioFrame::new(AudioSource::Microphone, sequence,
            PcmSpec::pcm16_mono(16_000), vec![1; 1280]).unwrap()).unwrap();
    }
    assert_eq!(q.dropped_frames(), 1);
    assert_eq!(q.recv().await.unwrap().sequence(), 1);
    assert_eq!(q.recv().await.unwrap().sequence(), 2);
    q.close();
    assert!(q.recv().await.is_none());
    assert!(q.recv().await.is_none());
}
~~~

Also test reset discards old carry, source/spec/oversize rejection before allocating output, sequences remain monotonic, and concurrent waiting receivers all wake on close. Router tests use existing mutable health fixtures: disconnect A output, reject/pause A but deliver B; restore health without resume keeps A paused; explicit resume validates only A. Whole-graph pause/resume retains all-route semantics. Resume also rejects closed selected output channels. Test that replacing a paused direction's closed sender with a fresh open sender does not resume it implicitly, explicit resume uses only the new sender, and its peer continues. Unknown/running routes and closed replacements reject.

- [ ] **2. Run failing filters.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::packetizer
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::queue
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::router
~~~

- [ ] **3. Implement primitives and per-route state.** push_sync removes one oldest frame when full and rejects after close; close preserves accepted frames for graceful drain and wakes all waiters. Register/enable Notify before checking emptiness in recv, avoiding a lost wakeup. clear never reopens a closed queue. Packet splitting advances timestamps by sample offset, checked sequence, bounded carry; flush emits one nonempty <=1,280-sample tail. Forward input-sequence gaps discard carry and start at the newer frame timestamp. Duplicate/backward input sequences reject atomically; explicit reset permits a new capture sequence while retaining monotonic output packet sequence. Test these discontinuity/reset cases.

Use fixed route slots indexed by Direction and a separate whole-graph pause; SessionDirection need not gain Hash. status() remains Running while any graph member route runs and the whole graph is not paused; when all routes are paused it reports Paused (the sole route in live mode). A per-direction change emits its directed event; publish aggregate audio-state only when that aggregate changes. Whole-graph pause overrides all route admission until whole-graph resume; resume_direction does not silently clear that override. Admission checks the selected route and uses try_send. Add AudioRouterEvent::DirectionStatusChanged { direction: Direction, status: AudioRouterStatus }. Full output remains a counted drop; closed output pauses only its route. Replace a paused direction output only under the short route/state gate, without changing peer state or resuming. Preserve constructors; mutable sender slots must not hold guards over async/OS waits. Resume checks output closure as well as endpoint health. No async wait holds route/state locks. Router cannot clear receiver-owned audio; Task 3 exposes that control and Task 4 invokes it on loss.

- [ ] **4. Verify and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib audio
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
~~~

Commit Task 2 paths: feat: preserve bounded audio continuity per direction.

### Task 3: Real audio ports and exact platform mappings

**Files:** Create audio/{runtime,runtime_tests}.rs and audio/platform/{inventory,native_io,pulse_io,process_io}.rs with focused tests. Modify audio/{mod,capture,playback}.rs, audio/platform/mod.rs, domain/transcript.rs (additive audio-free accounting types), error.rs (typed audio failure classification), Cargo.toml/Cargo.lock and tests/platform-audio/README.md.

**Consumes:** Task 2 queue/frames, existing AudioDriverManager/status and capture/playback DSP. Extend existing lifecycle code rather than duplicating resampling/callback code.

**Produces:** In inventory.rs, derive Clone, Debug, PartialEq, Eq, Serialize and Deserialize:

~~~rust
pub enum EndpointRole { Input, PhysicalOutput, M1Render, M2Capture }
pub enum AudioBackendKind { Cpal, Pulse }
pub struct RuntimeEndpoint {
    pub endpoint: AudioEndpoint,
    pub role: EndpointRole,
    pub backend: AudioBackendKind,
}
pub struct AudioInventory {
    pub generation: Uuid,
    pub driver: DriverStatus,
    pub endpoints: Vec<RuntimeEndpoint>,
}
~~~

An opaque endpoint ID resolves to a private exact backend locator. Frontend display names, ready flags and capabilities are never trusted. Audio/runtime.rs defines:

~~~rust
pub struct AudioOpenRequest {
    pub inventory_generation: Uuid,
    pub input_id: String,
    pub output_id: String,
    pub source: AudioSource,
}
pub enum AudioHealth { Ready, Lost, NeedsRestart }
pub struct PlaybackReport {
    pub boundary: PlaybackBoundary,
    pub statistics: PlaybackStatistics,
}
#[async_trait]
pub trait AudioControl: Send + Sync {
    fn suppress_now(&self);
    fn statistics(&self) -> AudioStatistics;
    async fn stop_capture(&self) -> AppResult<()>;
    async fn discard_pending(&self) -> AppResult<()>;
    async fn drain_playback(&self) -> AppResult<PlaybackReport>;
    async fn close(&self) -> AppResult<()>;
}
pub struct AudioPorts {
    pub capture: BoundedAudioQueue,
    pub playback: mpsc::Sender<AudioFrame>,
    pub health: watch::Receiver<AudioHealth>,
    pub control: Arc<dyn AudioControl>,
}
#[async_trait]
pub trait AudioBackend: Send + Sync {
    async fn inventory(&self) -> AppResult<AudioInventory>;
    async fn open(&self, request: AudioOpenRequest) -> AppResult<AudioPorts>;
}
impl PlatformAudioBackend {
    pub fn new() -> Self;
}
~~~

AudioBackend/AudioControl failure paths preserve a typed RuntimeFailureCode through AppError (add a narrow audio variant with a safe local message). Missing utilities map AudioBackendUnavailable, lost devices DeviceUnavailable, and open/drain/close deadlines StartupTimeout/PlaybackTimeout/CleanupTimeout. Runtime must not parse message substrings. Invalid launch/frame validation can remain InvalidRequest. Cover error categories in injected tests; no raw process/OS payload is surfaced.

PlatformAudioBackend implements AudioBackend and privately owns current inventory/locators, endpoint leases and quarantined owners. new performs no OS call. AudioHealth/PlaybackReport are Clone/Debug/PartialEq/Eq; ports are Send channel handles, never CPAL handles. Claim both endpoint leases atomically; at most two pairs for the desktop's one active session. Reject stale/unknown IDs and duplicate leases. Keep generation stable across unchanged sorted endpoint/locator/readiness sets; advance it on actual changes, not a read-only UI refresh. Test unchanged refresh versus genuinely stale opens. Inventory refresh alone never resumes lost audio.

**Outside-owner suppression and measured counters:** Define these audio-free values in domain/transcript.rs with Clone/Debug/PartialEq/Eq/serde; AudioControl::statistics reads them synchronously without an owner lock or acknowledgement. Keep the existing DiagnosticPayload::Playback representation readable and add PlaybackAccounting { boundary: Option<PlaybackBoundary>, statistics: PlaybackStatistics } and CaptureAccounting { statistics: CaptureStatistics }. New runtime diagnostics use the explicit accounting variants; no SQLite migration is needed.

~~~rust
pub struct CaptureStatistics {
    pub dropped_native_blocks: u64,
    pub dropped_normalized_frames: u64,
    pub finalized: bool,
}
pub struct NativePlaybackStatistics {
    pub sample_rate_hz: u32,
    pub generated_mono_samples: u64,
    pub callback_consumed_mono_samples: u64,
    pub discarded_mono_samples: u64,
}
pub struct PlaybackStatistics {
    pub input_sample_rate_hz: u32,
    pub accepted_for_processing_samples: u64,
    pub discarded_before_conversion_samples: u64,
    pub native: Option<NativePlaybackStatistics>,
    pub finalized: bool,
    pub tail_disposition_known: bool,
}
pub struct AudioStatistics {
    pub capture: CaptureStatistics,
    pub playback: PlaybackStatistics,
}
// Internal, in audio/runtime.rs; allocated before opening any owner/resources.
#[derive(Clone)]
pub(crate) struct AudioShared {
    pub gates: Arc<AudioGates>,
    pub counters: Arc<AudioCounters>,
    pub changed: Arc<tokio::sync::Notify>,
}
pub(crate) struct AudioGates {
    pub capture_suppressed: AtomicBool,
    pub playback_suppressed: AtomicBool,
    pub abort_pending: AtomicBool,
}
~~~

AudioCounters contains fixed atomics for the above counters/rate/finalization flags, no native handles or PCM. Counters saturate. A finalization Release flag is published only after all relevant writers/callbacks acknowledge; snapshots load with Acquire. Live counters can be slightly skewed, so never infer a conservation equation. Define input_sample_rate_hz=24_000. accepted_for_processing_samples counts validated frames dequeued by the backend, not raw mpsc/router enqueue. discarded_before_conversion_samples counts exact accepted input samples discarded before conversion. Native counters are actual negotiated-rate mono time samples before channel duplication; never divide native drops into an invented exact24k loss. Pulse native=None; pipe writes are not callback consumption. Tail disposition remains unknown after an unacknowledged owner or killed Pulse process with a possible server tail. Failure diagnostics use boundary=None, finalized=false where still unacknowledged, never a fabricated successful drain.

Capture drops count native callback blocks separately from normalized AudioFrame evictions; neither implies1,280 samples. Queue dropped_frames supplies normalized evictions. A raw capture drop must clear obsolete conversion carry/resampler state before admitting newer blocks: carry a bounded discontinuity generation through callback blocks, or equivalent ordered marker, and test the actual conversion path. Runtime checks loss counters when admitting capture as well as during periodic diagnostics; coalescing must not delay continuity handling.

suppress_now is bounded/infallible: set both suppression gates and abort_pending with Release, then notify workers. It never awaits/joins/calls OS or locks owner state. stop_capture sets only capture_suppressed before requesting owner cleanup and drains accepted capture; an emergency abort sets abort_pending and discards stale data. Playback callbacks observe their gate with Acquire and fill silence before ring pops; workers observe abort/gates before publishing, and Pulse writes select cancellation. Gates are one-way until fresh ports. An in-flight OS buffer cannot be recalled; this is next-observation suppression, not an acoustic stop guarantee. Capture worker exit must not depend exclusively on Stream::drop disconnect: bounded receive plus shared stop allows it to finish even if driver drop blocks.

Count actual ring drops: after initial Full, increment only for a successful producer discard-pop and/or a rejected replacement push. A callback can consume between these operations; Full alone is not a drop. Count explicit clear pops, not a racy len snapshot. Add a synthetic interleaving test, a24k/48k unit-serialization test, a blocked-owner test proving statistics/suppression still respond with silent callback output, graceful-capture-stop preserving playback, and legacy/new diagnostic round trips. Release injected blocked-owner latches at test end.

The existing AudioDriverManager trait has no Send/Sync bound. Construct/call concrete managers inside their owner boundary and pass plain status data back; do not place a non-Send manager inside AudioBackend or add an unsafe marker. Linux runtime inventory reuses the driver's naming/readiness policy through the bounded process boundary, not its synchronous unbounded SystemCommandRunner.

- [ ] **1. Add a non-Send owner boundary and failing drain tests.** In native_io.rs:

~~~rust
pub(crate) trait NativeOwnerFactory: Send + Sync {
    fn open_on_owner_thread(&self, request: &AudioOpenRequest,
        capture: BoundedAudioQueue, playback: mpsc::Receiver<AudioFrame>, shared: AudioShared)
        -> AppResult<Box<dyn NativeOwner>>;
}
pub trait NativeOwner {
    fn stop_capture(&mut self) -> AppResult<()>;
    fn discard_pending(&mut self) -> AppResult<()>;
    fn seal_playback(&mut self) -> AppResult<()>;
    fn playback_drained(&mut self) -> AppResult<Option<PlaybackReport>>;
    fn close(&mut self) -> AppResult<()>;
}
#[cfg(test)]
pub(crate) async fn open_with_factory(request: AudioOpenRequest,
    factory: Arc<dyn NativeOwnerFactory>) -> AppResult<AudioPorts>;
~~~

Create/call/drop Box<dyn NativeOwner> inside one std::thread; do not add Send to this trait. The real owner holds CaptureHandle/PlaybackHandle. Test owner holds Rc<Cell<u32>> to make accidental migration a compile error and records thread IDs at creation/calls/drop. Define test-only OwnerProbe::new() -> (Arc<dyn NativeOwnerFactory>, OwnerProbe), with thread_ids() -> Vec<ThreadId>, set_conversion_pending(bool), set_ring_samples(usize), release_conversion(), close_count() -> usize. Probe controls counters in the same drain state machine used by production.

~~~rust
#[tokio::test]
async fn drain_waits_for_inflight_conversion_and_callback_consumption() {
    let (factory, probe) = OwnerProbe::new();
    let ports = native_io::open_with_factory(AudioOpenRequest {
        inventory_generation: Uuid::nil(), input_id: "input".into(),
        output_id: "m1".into(), source: AudioSource::Microphone,
    }, factory).await.unwrap();
    probe.set_conversion_pending(true);
    probe.set_ring_samples(0);
    let control = ports.control.clone();
    let mut drain = Box::pin(control.drain_playback());
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut drain).await.is_err());
    probe.set_ring_samples(1920);
    probe.release_conversion();
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut drain).await.is_err());
    probe.set_ring_samples(0);
    assert_eq!(tokio::time::timeout(Duration::from_secs(1), drain).await
        .unwrap().unwrap().boundary, PlaybackBoundary::CallbackConsumed);
    let (a, b) = tokio::join!(ports.control.close(), ports.control.close());
    a.unwrap(); b.unwrap();
    assert_eq!(probe.close_count(), 1);
    let ids = probe.thread_ids();
    assert!(ids.iter().all(|id| *id == ids[0]));
}
~~~

Add rollback of an input when output opening fails, full-output stop, conversion exiting when its consumer closes, loss discarding queued/ring audio, and close-ack timeout causing NeedsRestart plus denied replacement lease. Inject short test deadlines, not multi-second test sleeps.

- [ ] **2. Run failing audio runtime/platform tests.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::platform
~~~

- [ ] **3. Implement native ownership and truthful drain.** Send controls through an 8-slot control channel, with a coalesced high-priority stop flag and replayable watch acknowledgement; concurrent close shares one result. Controls bypass audio queues. Open acknowledgement is bounded to 3 s, close acknowledgement to 2 s.

Retain callback pool8×8192 and emit normalized capture from the conversion worker into a 50-frame drop-oldest queue. Split each resampler output into <=1,280-sample frames before queue admission, preserving source/sample-cursor time and monotonic frame sequence; a low native rate can upsample one callback beyond Task 2's16,000-sample frame limit. Apply the same split to the graceful resampler tail and test a low-rate synthetic callback without relaxing packetizer bounds. Use AudioOpenRequest.source, not a display-name guess. Capture callbacks perform no heap growth, blocking lock or async operation.

Playback validates 24k mono PCM16 and <=24,000 samples before buffering. Its mpsc ingress has 50 slots; conversion writes chunks <=1,920 input samples into the existing ring capped to five seconds of negotiated device samples. Track ingress sealed, conversion worker done, accepted/dropped/consumed counts and ring occupancy. drain_playback closes ingress, finishes accepted conversion and succeeds only when worker_done AND ring_empty; an empty ring while conversion is running does not mean complete. Bound drain to5 s. Timeout first suppresses output, attempts bounded discard, and returns failure with statistics available even if cleanup cannot acknowledge. Unknown tail remains explicit. discard_pending suppresses further output, clears ingress/ring and leaves the port unusable until close/reopen, preventing a stale frame from winning a clear race.

Never block a Tokio executor joining an unresponsive driver. Suppress callbacks before cleanup; retain one quarantined owner/lease per failed route on timeout, return NeedsRestart and reject replacements. A native thread's unacknowledged exit must not be reported as success.

- [ ] **4. Implement CPAL and Linux inventory with a concrete process boundary.** CPAL enumerates actual input/output devices and supported configs on its owner thread. IDs include backend/direction/exact name where CPAL lacks a stable native ID. Reject duplicate names instead of first-match. Virtual roles must map to actual render/capture endpoints exposed by the package; absent signed Windows/macOS packages keep existing unavailable/approval gates.

Linux uses process_io.rs, not the unbounded SystemCommandRunner::output path:

~~~rust
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub pipe_stdin: bool,
    pub pipe_stdout: bool,
}
pub struct ProcessExit { pub code: Option<i32> }
#[async_trait]
pub trait ProcessControl: Send + Sync {
    async fn wait(&self) -> AppResult<ProcessExit>;
    async fn terminate(&self) -> AppResult<ProcessExit>;
}
pub struct ProcessPipes {
    pub stdin: Option<Pin<Box<dyn AsyncWrite + Send>>>,
    pub stdout: Option<Pin<Box<dyn AsyncRead + Send>>>,
    pub control: Arc<dyn ProcessControl>,
}
#[async_trait]
pub trait ProcessSpawner: Send + Sync {
    async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes>;
}
pub struct SystemProcessSpawner;
~~~

Use tokio::process::Command with individual args, kill_on_drop, idempotent owned wait/kill, stdout pipes only as requested and stderr discarded or drained into a fixed4 KiB ring never logged verbatim. No shell/interpolation/unbounded collection. Missing tools produce AudioBackendUnavailable with local guidance to install pulseaudio-utils and enable PulseAudio or pipewire-pulse. Short pactl operations use this same boundary, cap stdout to1 MiB, endpoints to512 and time to2 s; timeout/oversize/malformed JSON kills and reaps.

One process-owner actor retains Child and selects exit against termination control, publishing a replayable exit watch. ProcessControl::wait must not hold a Child mutex across child.wait() and block terminate behind that same mutex. Bound owner/control channels and reap once.

Read bounded pactl --format=json list sources, list sinks and list modules. Verify the exact M1/M2 null-sink set and VoxBridge_Input remap master=VoxBridge_M1.monitor before Ready; name presence alone is insufficient. These independent bounded reads may run together. Test missing/wrong remap using injected process results. Resolve the existing Linux driver names:

| Application role | Exact Pulse device | Direction |
|---|---|---|
| M1 render | VoxBridge_M1 | sink/output |
| M2 capture | VoxBridge_Output.monitor | source/input |
| Call app's logical M1 | VoxBridge_Input | source, not app render |
| Call app's logical M2 | VoxBridge_Output | sink, not app capture |

Verify monitor_of_sink links M2 capture to VoxBridge_Output. Physical outputs exclude the virtual sinks; selectable inputs include physical sources and clearly labeled monitors. IDs are pulse:source:<name> or pulse:sink:<name>, with private exact names; reject NUL/overlong names and never interpret them as shell text.

Capture argv is parec --record --raw --format=s16le --rate=16000 --channels=1 --latency-msec=80 --device=<source> --client-name=VoxBridge --stream-name=<unique>. Output uses pacat --playback with the same options except rate=24000 and the selected sink. Each --device argument is one OsString. Unique stream names are generated UUID/direction/role, not user text.

Before Ready, bounded pactl JSON list source-outputs/list sink-inputs must show each unique media.name attached to the exact source/sink index within3 s. Spawn/liveness alone is not readiness. Monitor child state and association at most once per second; loss immediately stops writes on that route. Never change system defaults.

Capture reads <=2,560 bytes plus one carried odd byte, decodes little-endian i16 and timestamps via a sample cursor anchored to local/monotonic time. Odd-byte EOF fails locally. Playback writes bounded chunks with cancellation selected alongside write_all. Graceful stop finishes accepted writes, closes pacat stdin and requires exit0 within5 s; pacat EOF drains the Pulse server. Label this PulseServerDrained, with detailed playback latency None. Timeout/nonzero exit terminates and reaps owned children within2 s, reporting discarded tail; device loss terminates rather than drains stale speech.

Test ProcessSpawner with tokio::io::duplex and watch-based child exits: exact M1/M2 argv, correct/wrong stream association, missing tool, oversized JSON, odd PCM reads, blocked stdin cancellation, EOF preceding successful exit, timeout kill/reap once and no retained child after rollback. Default tests do not launch pactl/parec/pacat. Actual utility/version/EOF behavior on PulseAudio and pipewire-pulse stays an acceptance gate.

- [ ] **5. Verify and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib audio
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
~~~

Commit Task 3 paths: feat: open exact native and Pulse routes with bounded cleanup.

### Task 4: Independent live/call translation runtime

**Files:** Create domain/launch.rs and session/{runtime,direction,runtime_tests,test_support}.rs. Modify domain/mod.rs, session/mod.rs and providers/{factory,factory_tests,fixture,contract_tests}.rs; narrowly extend providers/volc/{client,client_tests}.rs for bounded credential work only. Keep cloud protocols unchanged.

**Consumes:** ProviderFactory, Task1 sink/diagnostics, Task2 packetizer/router and Task3 backend/ports.

**Produces:** domain/launch.rs defines Clone/Debug/Eq/serde values:

~~~rust
pub enum DeviceSelection {
    Live { input_id: String, m1_output_id: String },
    Call { a_input_id: String, m2_input_id: String,
        m1_output_id: String, physical_output_id: String },
}
pub struct SessionLaunch {
    pub config: SessionConfig,
    pub provider_config_id: Uuid,
    pub devices: DeviceSelection,
}
pub struct SessionContext {
    pub launch: SessionLaunch,
    pub provider: ProviderId,
    pub provider_name: String,
    pub endpoints: Vec<RuntimeEndpoint>,
}
pub enum DirectionPhase { Starting, Running, Reconnecting, Paused, Stopped }
pub struct DirectionSnapshot {
    pub direction: SessionDirection,
    pub phase: DirectionPhase,
    pub stream_epoch: u64,
    pub failure: Option<RuntimeFailureCode>,
}
pub enum RuntimePhase { Starting, Running, Paused, Stopping, Stopped }
pub enum RuntimeOutcome { Completed, Failed { codes: Vec<RuntimeFailureCode> } }
pub struct RuntimeSnapshot {
    pub phase: RuntimePhase,
    pub directions: Vec<DirectionSnapshot>,
    pub outcome: Option<RuntimeOutcome>,
}
impl TranslationRuntime {
    pub fn new(factory: Arc<ProviderFactory>, audio: Arc<dyn AudioBackend>,
        store: Arc<LocalStore>, bus: EventBus) -> Self;
    pub async fn prepare(&self, session_id: Uuid, launch: &SessionLaunch,
        saved: &ProviderConfig) -> AppResult<PreparedRuntime>;
    pub fn start(&self, prepared: PreparedRuntime) -> RunningSession;
}
impl PreparedRuntime {
    pub fn context(&self) -> &SessionContext;
}
impl RunningSession {
    pub fn status(&self) -> watch::Receiver<RuntimeSnapshot>;
    pub async fn stop(&self) -> RuntimeOutcome;
    pub async fn resume_direction(&self, direction: SessionDirection) -> AppResult<()>;
}
// PreparedProviderSession also derives Clone: its provider is Arc and configs are immutable.
impl PreparedProviderSession {
    pub async fn connect_direction(self, direction: SessionDirection)
        -> ProviderResult<DirectedProviderStream>;
}
~~~

Pin launch/runtime JSON: DeviceSelection uses #[serde(tag="mode",rename_all="snake_case")], RuntimeOutcome uses #[serde(tag="status",rename_all="snake_case")], and DirectionPhase/RuntimePhase use snake_case strings. Add exact JSON and round-trip tests. Either single live SessionDirection maps to Router Direction::Live; retain factory language swapping for TargetToSource. Call maps SourceToTarget to AtoB and TargetToSource to BtoA.

TranslationRuntime holds Arc collaborators. PreparedRuntime holds session/context, validated directed configs, provider preparation and exact audio requests. RunningSession is Clone, holding shared stop/control/status/task completion channels. Do not expose provider streams or PCM via public snapshots. start runs inside Tokio and immediately owns its supervisor; only the orchestrator writes aggregate lifecycle or closes EventBus.

- [ ] **1. Add explicit24k fixture support and a failing runtime integration test.** FixtureScript stores output_audio and adds:

~~~rust
impl FixtureScript {
    pub fn with_output_audio(output: PcmSpec,
        steps: impl IntoIterator<Item = FixtureStep>) -> ProviderResult<Self>;
}
~~~

Accept only16k or24k monoPCM16, with emitted audio <=80 ms at the selected rate. Existing new(steps) delegates to16k; FixtureProvider::new stays unchanged and capabilities use the script profile. Input stays16k/1,280 samples; retain64 instructions and64 KiB event bounds. Factory with_test_provider becomes #[cfg(test)] pub(crate) only. Fixture Gap must require Started, set started=false, discard pending input and allow the next Started; sends return NotStarted until then. Keep duplicate Started without Gap invalid. Test the actual fixture state machine, including NotStarted before queued Gap consumption, rather than introducing a replacement mock protocol.

Define cfg(test) session/test_support.rs with RuntimeHarness::live(script) and RuntimeHarness::call(forward_script,reverse_script) -> AppResult<RuntimeHarness>. Its fields are runtime: Arc<TranslationRuntime>, store: Arc<LocalStore>, bus: EventBus, audio: Arc<FixtureAudioBackend>, provider: Arc<ObservedFixtureProvider>, launch: SessionLaunch, saved: ProviderConfig, id: Uuid and an owned TempDir. Constructors use SecretStore::for_tests, create a StoredSession, open EventBus and inject the real factory.

FixtureAudioBackend implements AudioBackend with fixed IDs (live-input, a-input, m2-capture, m1-render, physical-output), bounded queues and Task3 drain semantics. Define send_capture(&self,input_id:&str,frame:AudioFrame) -> AppResult<()> and next_playback(&self,output_id:&str) -> Option<AudioFrame> as async methods; lose(&self,id:&str), restore(&self,id:&str), open_ports(&self)->usize are synchronous. next_playback consumption acknowledges fixture playback, so a drain waits for actual test consumption.

ObservedFixtureProvider chooses each direction's script, delegates connect to FixtureProvider::new(script), and wraps the real bounded ProviderStream only to count observed send/finish. Define connect_counts(&self)->[usize;2], finish_counts(&self)->[usize;2], sent_sequences(&self,direction:SessionDirection)->Vec<u64>. Do not write another mock transport. Export test_support as #[cfg(test)] pub(crate); place the three helpers below there so runtime, orchestrator and command tests can import them. Add these real helper bodies and test:

~~~rust
pub(crate) fn metadata() -> ProviderEventMetadata {
    ProviderEventMetadata {
        session_id: Uuid::nil(), direction: SessionDirection::SourceToTarget,
        provider_event_type: None, timestamp: None, sentence_id: None,
        local_correlation_id: None, log_id: None,
    }
}
pub(crate) fn final_text(language: &str, text: &str) -> TimedTranslation {
    TimedTranslation {
        translation: Translation { language: language.into(), text: text.into() },
        start: Some(Duration::ZERO), end: Some(Duration::from_millis(80)),
    }
}
pub(crate) async fn wait_phase(run: &RunningSession, phase: RuntimePhase) {
    let mut status = run.status();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if status.borrow_and_update().phase == phase { break; }
            status.changed().await.unwrap();
        }
    }).await.unwrap();
}
#[tokio::test]
async fn provider_tail_is_saved_and_played_before_stop_completes() {
    let script = FixtureScript::with_output_audio(PcmSpec::pcm16_mono(24_000), [
        FixtureStep::Emit(ProviderEvent::SessionStarted { metadata: metadata() }),
        FixtureStep::ReceiveAudio { count: 1 },
        FixtureStep::WaitForFinish,
        FixtureStep::Emit(ProviderEvent::SourceFinal {
            metadata: metadata(), text: final_text("zh", "泵"),
        }),
        FixtureStep::Emit(ProviderEvent::TranslationFinal {
            metadata: metadata(), text: final_text("en", "pump"),
        }),
        FixtureStep::Emit(ProviderEvent::Audio {
            metadata: metadata(), frame: AudioFrame::new(AudioSource::Microphone, 0,
                PcmSpec::pcm16_mono(24_000), vec![7; 1920]).unwrap(),
        }),
    ]).unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let prepared = h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap();
    let run = h.runtime.start(prepared);
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio.send_capture("live-input", AudioFrame::new(AudioSource::Microphone, 0,
        PcmSpec::pcm16_mono(16_000), vec![1; 1280]).unwrap()).await.unwrap();
    let run2 = run.clone();
    let stopped = tokio::spawn(async move { run2.stop().await });
    let played = tokio::time::timeout(Duration::from_secs(1),
        h.audio.next_playback("m1-render")).await.unwrap().unwrap();
    assert_eq!(played.samples(), &[7; 1920]);
    assert_eq!(stopped.await.unwrap(), RuntimeOutcome::Completed);
    assert_eq!(h.store.transcript_page(h.id, None, 50).unwrap().rows.len(), 2);
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    assert_eq!(h.audio.open_ports(), 0);
}
~~~

Add a delayed SessionStarted case asserting sent_sequences empty before handshake, then only new post-handshake capture sent. Provider rejecting an early send does not satisfy that test.

- [ ] **2. Run failing runtime/provider filters.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib session::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib providers
~~~

- [ ] **3. Implement preparation/startup.** prepare requires saved.id == launch.provider_config_id, valid mode/device shape, distinct IDs, exact roles/directions, DriverStatus::Ready and factory.prepare(16k,24k). Live accepts any selectable Input/M2Capture endpoint with input direction and requires M1Render output; call requires its explicit Input/M2Capture/M1Render/PhysicalOutput mapping. Resolve current inventory and construct AudioGraph::for_live/for_call plus immutable context. It opens neither capture nor network. provider_name falls back to provider ID until Task6's config display name.

Clone the fully validated PreparedProviderSession for each initial direction actor and use connect_direction for initial opening as well as explicit resume; full connect remains available unchanged. Each actor owns its connect future and immediately owns the returned stream, so peer failure/cancel never hides acquired streams inside an opaque aggregate connect future. Startup owns one actor per direction and both peers must reach audio Ready plus SessionStarted before aggregate Running. Capture may begin physically during open, but no provider input is admitted; clear pre-handshake queue/carry at initial Started. Bound whole startup to30 s, audio open to3 s and provider connect/handshake to10 s each. Any startup failure/cancel cleans all partial peers, including one already ready. Volc connect returns before network/handshake, so connect success is not readiness and the post-connect Started deadline is independent. Stop/control bypass saturated data.

Bound native credential work before spawning: use one process-wide two-slot Arc<Semaphore> shared across reconstructed VolcengineProvider instances, acquire within the existing connect deadline, and move the OwnedSemaphorePermit into the spawn_blocking closure. Timeout/cancel does not release capacity until the underlying lookup returns. A per-provider or future-owned permit is insufficient. Add an injected isolated gate/blocked-lookup regression; release latches after the test, never use native keyring. Waiting callers time out safely without spawning more jobs; no attempt to kill a native keyring thread.

Retain each owned audio-open operation through startup cancellation or peer failure. The3 s audio readiness deadline stops admission/startup; Task3 Pulse opening can then spend up to2 s acknowledging rollback. Do not truncate that rollback with a second outer3 s timeout. During bounded cleanup observe the pending open, suppress/close any late successful ports and forbid late Running; cancellation is Completed only after acknowledged resource cleanup. If the owned operation does not acknowledge within the remaining open plus cleanup allowance, report CleanupTimeout and preserve backend quarantine, without reacquiring its endpoints. Test delayed open, late success after stop and delayed failure rollback with injected ports.

connect_direction consumes a fully validated prepared object, rejects Bidirectional/absent direction, opens only the matching config using the existing10 s timeout. Test that a reverse resume swaps languages and never connects its healthy peer. Full connect remains unchanged.

- [ ] **4. Implement direction state, continuity and resume.** Poll provider events and audio/control independently of pending send; the existing ProviderConnection &self boundary supports concurrent send/receive. Use one next_event consumer, bounded input work and cancel-safe recv/event futures; do not hold an outer provider mutex across receive. Packet sends follow Tokio Instant pacing at packet duration; reset pacing on gaps so backlog cannot burst.

Handle provider admission results explicitly: Ok means provider-queue admission, not socket-write acknowledgement, and must never be replayed. QueueFull drops/counts the rejected packet without cancelling/reconnecting. NotStarted after Running suspends input, drops that packet, resets carry/pacing and pauses/suppresses stale output, then keeps consuming ordered events from the same stream. Keep a private awaiting_provider_boundary flag; do not invent Gap or advance epoch from the send error. The actual queued Gap performs the single epoch transition and shares any already-started local cleanup. Closed outside intentional stop suspends input and still drains queued valid finals through the real terminal event. Invalid format/direction/frame errors fail locally. Add the concrete NotStarted→queued final→Gap→Started and QueueFull regressions.

Route Audio only to that direction's AudioRouter target; require24k bus format. The Volc mapper allows up to 48,000 PCM16 samples per event, while Task 3 admits frames <=24,000 samples. Validate the whole event against the provider bound, then split accepted audio into <=1,920-sample frames before routing, preserving sample order/source and advancing capture timestamps by sample offset with checked monotonic per-direction output sequence. Never truncate a valid event or pass an oversized frame to playback. Unit-test the production splitting path with a 48,000-sample ramp and an oversize invalid event, plus route integration; the fixture provider itself remains capped at 80 ms. Send subtitles to TranscriptSink regardless of subscribers. Existing AudioFrame contains SystemTime, not Instant: capture-to-send uses a checked local wall-clock duration from its preserved captured_at, returning None on backward/invalid clock movement; label its clock basis in backend documentation. send_audio completes at bounded provider-queue admission; label capture_to_send_ms as capture-to-provider-admission in docs/UI, never measured wire/network latency. Response-to-enqueue uses local Instant. Do not invent a monotonic capture timestamp, label enqueue as playback or fabricate provider processing time. Native callback/server-drain observations only populate the stages they actually measure.

On provider Gap pause that router direction, disable admission, clear capture/carry and stale playback, advance epoch once and mark Reconnecting. The adapter already bounds reconnect. Task3 discard_pending permanently disables a port, so after a Gap close that direction's old ports and reopen its same selected route, replace only that paused direction's output sender, then explicitly resume the route at the next SessionStarted; do not call explicit-resume or open a second provider stream for an adapter-owned reconnect. Track provider-ready and audio-ready separately: if Started arrives while close/open is pending, remember it while continuing to consume events, and admit only after both are ready with no intervening failure/stop. Clear pre-readiness capture at that conjunction. If reopening fails, pause the direction and cancel that stream. On provider Failed/EOF, invalid event, storage failure or device loss, pause its router direction, cancel only that direction, stop capture and discard output. Peer continues.

Explicit resume accepts one pending request per direction, rejects nonmembers/Bidirectional/running/stopping, revalidates fresh inventory and full provider capabilities, then calls connect_direction for only that side. After discarding/closing old ports, open fresh ports, replace only that direction's router output sender while paused, require handshake and explicit route resume, advance epoch once with ManualResume and retain the sink's sequence. Restored device presence alone does nothing. Retain session/provider/language/device snapshot; changed selection requires a new session.

Use AudioControl::statistics on capture admission to detect raw-block/normalized-frame loss promptly, and once per second plus after drain/close for diagnostic snapshots. Call suppress_now before forced abort/timeout cleanup; persist the failure and available accounting even if the owner never acknowledges. New playback diagnostics use PlaybackAccounting with boundary=None on uncertain failure, and CaptureAccounting keeps raw blocks separate from normalized frames.

Local capture loss/output overflow uses the same bounded per-direction audio replacement operation to clear stale queued playback: discard_pending permanently disables ports, so merely resuming them is invalid. Reset capture/carry/pacing, close/reopen the retained selection and replace only the paused sender; retain the provider connection and its readiness unless a real provider boundary intervenes. Local loss alone does not advance provider epoch. Ignore old-port health after replacement, reset the loss-counter baseline for new ports, and coalesce losses while replacement is pending into the same operation. Reopen failure pauses that direction until explicit resume. Add exact open/connect-count, no-stale-carry/output and unaffected-peer regressions.

Own bounded per-direction diagnostic accumulators: coalesce latency and repeated gap/drop/TranscriptOverflow counters to at most one row per second per category, using take_dropped_interims after subtitle acceptance, and flush pending values at stop. Persist every distinct failure transition once before publishing its safe diagnostic; a persistence failure pauses the direction and emits a local Storage diagnostic without recursively storing the failure. Add deterministic-clock tests for coalescing, stop flush and storage failure.

Runtime aggregate is Running while at least one started direction is Running/Reconnecting, Paused when all are paused, and Stopping/Stopped during termination; initial startup still requires all directions. Bound outcome codes to one per failure category/direction, never retain an unbounded error list.

- [ ] **5. Implement graceful stop and fault tests.** stop shares a replayable completion operation. Per direction: disable/stop capture (2 s ack), drain accepted queue/packet tail (4 s total, count/discard remainder), finish once, continue provider text/audio through terminal (10 s), seal/drain playback (5 s), close owned ports/tasks (2 s). Directions stop concurrently. A paused/failed direction cancels/discards immediately. cancel() is only an abort request: drain next_event through terminal/EOF within the cleanup deadline (discard audio, retain valid finals if storage works), then drop all stream/helper handles. A cancellation return alone is not acknowledged teardown. A locally generated Cancelled terminal during intentional startup stop is not a new provider failure; retain actual prior failures. Keep the event consumer running during capture pacing/drain, finish wait and playback drain so a saturated event channel cannot deadlock final text/audio. A stop during adapter retry can end with Disconnected rather than Finished and must remain Failed. User cancellation before startup readiness completes successfully only after cleanup; actual failures/timeouts produce Failed. Any lost accepted tail is reported; do not claim Completed after provider-finish/playback/cleanup timeout.

Use the harness for:

| Trigger | Required evidence |
|---|---|
| Call/reverse languages | Two proper configs; A TTS only M1, B only physical; directed source/translation durable |
| Gap then Started | One epoch increment; no old carry/input/output replay; peer unaffected |
| One terminal failure/resume | Peer still handles TTS; exactly one new provider connection |
| Device loss/restore | No output/reopen until explicit resume; peer stays live |
| Full capture/output | Bounded memory, counted drops, responsive stop |
| Stop pending handshake/connect | Every opened resource closes; no later Running |
| Concurrent stop/no UI/lagged UI | Same terminal result, one finish, final persistence and no retained tasks |
| No provider terminal/native close ack | Safe failure code, discarded tail/NeedsRestart; no false completion |

- [ ] **6. Verify and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib session
cargo test --manifest-path src-tauri/Cargo.toml --lib providers
cargo test --manifest-path src-tauri/Cargo.toml --lib audio
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
~~~

Commit Task4 paths: feat: compose independent live and call runtimes.

### Task 5: Durable lifecycle and replayable stop

**Files:** Modify session/{orchestrator,orchestrator_tests,mod,test_support}.rs, storage/{sqlite,models,mod}.rs and app/{mod,tests}.rs; minimally adapt commands.rs create_session to keep the public launch path compiling against the new lifecycle API.

**Consumes:** Task4 launch/context/runtime. Replace production NoopSessionStartup/SessionStartup with actual runtime ownership.

**Produces:**

~~~rust
impl LocalStore {
    pub fn create_runtime_session(&self, session: &StoredSession,
        context: &SessionContext) -> AppResult<()>;
    pub fn session_context(&self, session_id: Uuid) -> AppResult<Option<SessionContext>>;
    pub fn interrupt_incomplete_sessions(&self, ended_at: i64) -> AppResult<Vec<Uuid>>;
}
impl SessionOrchestrator {
    pub fn with_runtime(store: Arc<LocalStore>, event_bus: EventBus,
        runtime: Arc<TranslationRuntime>) -> Self;
    pub async fn start(&self, launch: SessionLaunch,
        saved: ProviderConfig) -> AppResult<SessionRecord>;
    pub async fn resume_direction(&self, id: Uuid,
        direction: SessionDirection) -> AppResult<()>;
    pub(crate) async fn uses_provider_config(&self, config_id: Uuid) -> bool;
}
~~~

Keep get/list/stop_session/delete_session/subscribe names. stop_session is repeatable after terminal state. The existing commands.rs create_session currently calls start(SessionConfig) and then reads SessionHandle.id; adapt it now to the final launch:SessionLaunch argument, load its saved ProviderConfig via the existing ConfigStore API, call start(launch,saved), and return SessionRecord directly. Do not add a temporary frontend-supplied saved-config argument or retain production Noop startup merely for compilation. AppState::new constructs real factory/audio/runtime collaborators without opening native devices/keyring/network. Task 6 centralizes the configured AppState methods, shared config/secret ownership, process lock and bounded transaction worker; avoid duplicating JSON parsing in this compatibility adapter. Extend SessionRecord with context: Option<SessionContext> and runtime: Option<RuntimeSnapshot>; legacy context and inactive live-runtime snapshots are None. Keep SessionHandle only if remaining callers need it; create_session uses Starting records.

- [ ] **1. Add failing concurrent-stop and persistence-failure tests.** Extend RuntimeHarness with orchestrator() -> SessionOrchestrator which calls with_runtime on its Arc collaborators. Its existing fixture row is unrelated to the new ID created by start.

~~~rust
#[tokio::test]
async fn concurrent_stop_returns_the_same_durable_terminal_record() {
    let script = FixtureScript::with_output_audio(PcmSpec::pcm16_mono(24_000), [
        FixtureStep::Emit(ProviderEvent::SessionStarted { metadata: metadata() }),
        FixtureStep::WaitForFinish,
    ]).unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let orchestrator = h.orchestrator();
    let started = orchestrator.start(h.launch.clone(), h.saved.clone()).await.unwrap();
    assert_eq!(started.status, SessionStatus::Starting);
    let (a, b) = tokio::join!(
        orchestrator.stop_session(started.id),
        orchestrator.stop_session(started.id),
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(a.status, SessionStatus::Completed);
    assert!(a.ended_at.is_some());
    assert_eq!(orchestrator.stop_session(started.id).await.unwrap(), a);
    assert_eq!(orchestrator.get(started.id).await.unwrap(), Some(a));
    assert_eq!(h.audio.open_ports(), 0);
}
~~~

This includes user stop during Starting: successful cancellation/cleanup gives Completed and no later Running; actual startup/cleanup failure gives Failed. Separately wait until Running and assert one finish per active stream.

Use existing lifecycle SQL failure injection and a new INSERT trigger for Starting. Failed Starting must open nothing; failed Running must clean acquired resources; failed Stopping must still initiate cleanup; failed terminal persistence must not emit success. All waiters resolve to the same bounded error instead of waiting forever.

- [ ] **2. Run failing lifecycle tests.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib session::orchestrator
~~~

- [ ] **3. Implement one lifecycle owner and restart recovery.** create_runtime_session inserts session, Starting lifecycle and context in a transaction. prepare runs before it; opening begins after it. Reserve the singleton Starting slot under the active-state mutex as the first start operation, before runtime.prepare/inventory. Retain its provider_config_id and release the mutex during preparation. A second start returns a local "stop the active session first" error before preparation or any network/hardware activity. Release the reservation on failure or caller cancellation before attachment. After durable Starting/context creation, install the active entry and attach runtime.start without an intervening await. uses_provider_config includes both the reservation and attached runtime. Add a barrier test proving the losing start calls neither inventory/preparation nor provider/audio open.

ActiveSession owns RunningSession and a replayable watch completion:

~~~rust
type SessionCompletion = Option<Result<SessionRecord, String>>;
~~~

The String is a safe local command error, not a remote payload. One lifecycle observer writes state. Copy handles/receivers out of the active map before awaiting; no active-map mutex spans OS/network waits. Persist Running before publishing it. Failed lifecycle writes request runtime.stop and report Storage once. On terminal cleanup, persist terminal state before close_with_final and releasing active ownership. If terminal persistence fails, close with a safe failure and publish the completion error before removing the operation entry. Release the runtime and singleton slot after cleanup even on persistence failure. Every stop caller attached to that operation receives the same result; subsequent get reveals actual persisted state. Do not fabricate a completed record or keep an unbounded permanent error cache. A later stop after removal may use durable state/error semantics; barrier tests distinguish attached concurrent waiters from later calls.

Elect one stop request under the active-state mutex and let the owned lifecycle observer drive it; caller cancellation must not cancel the shared cleanup. Copy handles/completion receivers before awaits. Repeated stop waits on the same watch or reads an already-terminal stored record. Stop during startup cancels immediately instead of waiting on the current single-permit Notify. Direction changes feed the same observer; one failed direction does not pause the whole call while its peer runs. Terminal SessionRecord.runtime is None so repeated stop/get equality is stable.

At app initialization, under a single data-directory process owner, interrupt_incomplete_sessions transactionally marks created/starting/running/stopping/paused sessions Failed with ended_at and an Interrupted diagnostic. Keep final text/context. Never reopen devices/providers automatically. Task6 owns production process-lock wiring: Task5 implements/tests the store recovery operation, but does not call it from an unlocked AppState constructor. Task6 invokes it only after acquiring the data-directory owner lock. Do not interrupt sessions belonging to another live process.

- [ ] **4. Verify and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib session
cargo test --manifest-path src-tauri/Cargo.toml --lib storage
git diff --check
~~~

Commit Task5 paths: feat: coordinate durable lifecycle and idempotent stop.

### Task 6: Configuration, app commands and recoverable subscriptions

**Files:** Modify app/{mod,tests}.rs, commands.rs, lib.rs, config/{mod,models,secret_store,secret_store_tests}.rs, providers/types.rs (safe capability serialization) and session/runtime.rs (saved provider display name). Create commands_tests.rs, commands/subscriptions.rs and config/worker.rs. Update README.md, docs/superpowers/plans/release-gates.md and tests/platform-audio/README.md.

**Consumes:** All earlier APIs. App construction creates local objects/storage only; credential operations access keyring as needed, explicit launch opens capture/network.

**Produces:** ConfigStore owns Arc<SecretStore> and takes it in new(data_dir: impl AsRef<Path>, secrets: Arc<SecretStore>) -> AppResult<Self>. Keep existing load/save, add list_provider_configs(&self)->AppResult<Vec<ProviderConfig>> and delete_provider_config(&mut self,id:Uuid)->AppResult<()>. SecretStore adds delete(&self,service:&str,key:&str)->AppResult<()> and contains(&self,service:&str,key:&str)->AppResult<bool>; contains checks encrypted entry presence without decrypting/keyring access.

ProviderConfig gains display_name with serde default/fallback to provider ID, display_name(&self)->&str, with_display_name(self,name:String)->AppResult<Self> and with_secret_reference(self,reference:Option<String>)->Self. Trim names, require1–80 characters; preserve existing constructor compatibility, include the name in public_view and use it in SessionContext.provider_name.

Define command values:

~~~rust
#[derive(Deserialize)]
#[serde(tag="action",rename_all="snake_case")]
pub enum CredentialChange { Keep, Clear, Replace { api_key: String } }
#[derive(Deserialize)]
pub struct SaveProviderRequest {
    pub config: ProviderConfig,
    pub credential: CredentialChange,
}
#[derive(Serialize)]
pub struct ProviderConfigView {
    pub config: PublicProviderConfig,
    pub credential_configured: bool,
}
#[derive(Serialize)]
pub struct LaunchValidation {
    pub context: SessionContext,
    pub input_audio: PcmSpec,
    pub output_audio: PcmSpec,
}
#[derive(Serialize)]
pub struct SubscriptionTicket { pub id: Uuid, pub event_name: String }
~~~

Credential input gets no Debug/Serialize. ProviderConfigView can retain the existing opaque secret reference; configured is actual encrypted-entry presence for its valid normalized reference, excluding unreferenced cleanup orphans. Derive Serialize for ProviderCapabilities to expose safe capability data.

Task6 also produces ConfigIo/ConfigTransaction in config/worker.rs, re-exported pub(crate) from config. Define EncryptedSecretSnapshot in secret_store.rs around its existing private EncryptedValue representation:

~~~rust
#[derive(Clone)]
pub(crate) struct ConfigIo {
    store: Arc<std::sync::Mutex<ConfigStore>>,
    permits: Arc<tokio::sync::Semaphore>,
    data_dir: PathBuf,
    deadline: std::time::Duration,
}
pub(crate) struct ConfigTransaction<'a> {
    store: &'a mut ConfigStore,
    _config_lock: std::fs::File,
    secret_lock: Option<std::fs::File>,
}
pub(crate) struct EncryptedSecretSnapshot(Option<EncryptedValue>);
impl ConfigIo {
    pub(crate) fn new(store: ConfigStore) -> Self;
    pub(crate) fn data_dir(&self) -> &Path;
    pub(crate) async fn run<T, F>(&self, operation: F) -> AppResult<T>
    where T: Send + 'static,
          F: for<'a> FnOnce(&mut ConfigTransaction<'a>) -> AppResult<T> + Send + 'static;
}
impl ConfigTransaction<'_> {
    pub(crate) fn data_dir(&self) -> &Path;
    pub(crate) fn load_provider_config(&mut self, id: Uuid) -> AppResult<ProviderConfig>;
    pub(crate) fn list_provider_configs(&mut self) -> AppResult<Vec<ProviderConfig>>;
    pub(crate) fn save_provider_config(&mut self, config: ProviderConfig) -> AppResult<()>;
    pub(crate) fn delete_provider_config(&mut self, id: Uuid) -> AppResult<()>;
    pub(crate) fn secret_get(&mut self, service: &str, key: &str) -> AppResult<Option<Vec<u8>>>;
    pub(crate) fn secret_set(&mut self, service: &str, key: &str, value: &[u8]) -> AppResult<()>;
    pub(crate) fn secret_contains(&mut self, service: &str, key: &str) -> AppResult<bool>;
    pub(crate) fn secret_delete(&mut self, service: &str, key: &str) -> AppResult<()>;
    pub(crate) fn snapshot_secret(&mut self, service: &str, key: &str) -> AppResult<EncryptedSecretSnapshot>;
    pub(crate) fn restore_secret(&mut self, service: &str, key: &str, prior: EncryptedSecretSnapshot) -> AppResult<()>;
}
~~~

ConfigIo uses one process-wide two-slot blocking gate, distinct from Volc's connect gate, and a10s caller budget including permit wait. Acquire before spawn_blocking and move the owned permit into the closure; timeouts/cancellation cannot free capacity until native work returns. Skip cancelled jobs that have not entered their transaction; an entered operation completes/compensates under its retained guards. Use injected small deadlines/gates for tests and release blocked latches. Constructor performs no OS operation. ConfigTransaction holds the cross-process config lock through the entire closure and acquires secret/DEK locks in that order; its lock-aware methods must not recursively call public methods that reacquire the same locks. It exposes data_dir, load/list/save/delete_provider_config plus secret_get/set/contains/delete and snapshot_secret/restore_secret (service/key arguments backend-derived; get returns Option<Vec<u8>>, snapshot is opaque encrypted data). The later service-config store uses this same ConfigIo and transaction lock for its own JSON file and credentials, releasing all locks before HTTP. Add serialized-operation, cross-instance RMW, blocked-native capacity retention, cancellation-before-entry and no-plaintext-output tests.

AppState::with_dependencies(data_dir:PathBuf,secrets:Arc<SecretStore>,audio:Arc<dyn AudioBackend>,factory:Arc<ProviderFactory>)->AppResult<Self> supports tests and production composition. new constructs platform dependencies. Retain ConfigIo/shared ConfigStore ownership, the process lock, a narrow async admission gate, LocalStore/EventBus/audio/runtime/orchestrator/subscription registry. If Debug is needed, use an opaque manual implementation for sensitive trait objects.

Commands are thin Tauri wrappers over testable AppState methods of the same names and argument/result shapes. AppState methods below are async except get_session_transcript/get_session_diagnostics, which can remain synchronous local reads:

| Command | Arguments after state | Success type |
|---|---|---|
| list_audio_endpoints | none | AudioInventory |
| audio_driver_status | none | DriverStatus |
| list_provider_configs | none | Vec<ProviderConfigView> |
| save_provider_config | SaveProviderRequest | ProviderConfigView |
| delete_provider_config | config_id:Uuid | () |
| provider_capabilities | config_id:Uuid | ProviderCapabilities |
| validate_session_launch | launch:SessionLaunch | LaunchValidation |
| create_session | launch:SessionLaunch | SessionRecord in Starting |
| stop_session | session_id:Uuid | terminal SessionRecord |
| resume_session_direction | session_id:Uuid,direction:SessionDirection | () |
| get_session_transcript | session_id:Uuid,after:Option<i64>,limit:Option<u32> | TranscriptPage |
| get_session_diagnostics | session_id:Uuid,after:Option<i64>,limit:Option<u32> | DiagnosticPage |
| subscribe_session | app handle,session_id:Uuid,subscriber_id:String | SubscriptionTicket |
| unsubscribe_session | subscription_id:Uuid | () |
| get_session/list_sessions/delete_session/export_session | existing arguments | existing types extended by Task5 |

validate_session_launch loads saved config, calls runtime.prepare with a temporary ID and returns only context/formats. create repeats fresh preflight, never trusts prior validation.

- [ ] **1. Add failing injected app/command tests.**

~~~rust
#[tokio::test]
async fn stale_endpoint_preflight_never_opens_devices_or_provider() {
    let script = FixtureScript::with_output_audio(PcmSpec::pcm16_mono(24_000), [
        FixtureStep::Emit(ProviderEvent::SessionStarted { metadata: metadata() }),
        FixtureStep::WaitForFinish,
    ]).unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let factory = Arc::new(ProviderFactory::new(secrets.clone())
        .with_test_provider(h.provider.clone()));
    let app = AppState::with_dependencies(
        dir.path().to_path_buf(), secrets, h.audio.clone(), factory,
    ).unwrap();
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(), credential: CredentialChange::Keep,
    }).await.unwrap();
    let mut launch = h.launch.clone();
    launch.devices = DeviceSelection::Live {
        input_id: "stale-id".into(), m1_output_id: "m1-render".into(),
    };
    assert!(app.validate_session_launch(launch).await.is_err());
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    assert_eq!(h.audio.open_ports(), 0);
}
~~~

Add save/overwrite/clear/reload synthetic Volc key tests with one shared injected memory KeyBackend, no network. Public JSON/plain config must not contain the synthetic key. Concurrent ConfigStore/SecretStore instances sharing the same directory/backend must preserve both writes without temp collisions. Valid AppState launch must own real fixture runtime ports and persist its finals; second active launch opens nothing. Disabled/unknown provider and wrong endpoint role fail locally.

- [ ] **2. Run failing command/config/app tests.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml --lib commands
cargo test --manifest-path src-tauri/Cargo.toml --lib config
cargo test --manifest-path src-tauri/Cargo.toml --lib app
~~~

- [ ] **3. Implement app ownership and atomic credential storage.** Create the data directory and acquire its nonblocking exclusive fs2 app lock before opening/migrating SQLite, constructing config/runtime collaborators or running startup interruption. Retain its file through shutdown. A failed lock attempt returns before database/config/recovery writes with a local "data directory already in use" error. Test a second owner cannot migrate or interrupt the first owner's database.

Lock each config/secret read-modify-write for its entire transaction across processes. DEK/keyring locking remains separate, ordered config→secret→DEK. Unique UUID temp filenames, create_new, complete write/fsync and atomic replace avoid existing PID-only collision. Reuse reviewed Windows MoveFileExW replacement behavior from export; never delete destination before replacement. Cleanup failed temp files, preserving the prior destination on pre-replace failure.

Normalize backend references: Volc uses volcengine_secret_reference(config.id()) and existing provider service/key helpers; do not accept a reference to another profile. Validate Replace with provider credential format/size rules; Keep preserves, Clear deletes. No request Debug or secret-bearing URL/log. Config and secrets are separate files. Validate the whole request first. Keep saves only normalized config. Replace captures an opaque encrypted before-image (or absence) under the whole transaction, writes the secret, then atomically saves config; if config save fails, attempt one bounded restoration of that encrypted entry (or remove the new entry for prior absence). A deterministic existing-profile key is an overwrite, not an orphan. Clear first saves config with no secret reference, then deletes the encrypted entry. Delete removes config first, then its encrypted entry. Report cleanup/compensation failure explicitly as a safe partial-failure error; never claim cross-file crash atomicity or introduce a journal/reference-format change. configured means a valid normalized reference AND actual encrypted-entry presence, so unreferenced cleanup orphans are not shown as configured. Test existing-key replacement rollback, no-prior-entry rollback, Clear config failure preserving the key, Clear/delete orphan cleanup failure, and compensation failure without false success.

Config updates apply to future sessions. An active session retains provider/language/device/config snapshot; explicit resume can reload credentials for the same profile ID but cannot alter selection. Reject deleting an active/reserved session's provider profile until cleanup. AppState create_session holds the narrow admission gate across saved-config lookup and orchestrator.start; delete_provider_config holds it across uses_provider_config and config removal/secret cleanup. This linearizes create versus delete without an unlocked get/list scan. Keep direction workers and stop outside the gate. A timed-out delete transaction retains the owned admission guard inside its blocking closure until completion; do not release it while native mutation is still running. Test both race orders and caller cancellation; a losing create must not reach runtime preparation or open resources.

Run blocking file/keyring transactions through ConfigIo, not directly on the Tokio event/capture executor; clone safe saved configuration out before provider/audio awaits. This lock serializes configuration only, never both direction workers.

- [ ] **4. Implement owned bounded subscription forwarding.** Registry capacity16, keyed by (subscriber_id,session_id); subscriber_id is a <=64-character opaque token. Repeated key cancels/joins the previous forwarder before starting its replacement, using one bounded owned registry mutation operation so caller cancellation cannot detach replacement cleanup. Each ticket gets a unique event name and generation UUID; a finishing wrapper removes an entry only when its UUID still matches. Copy/remove handles under the map mutex, then cancel/join outside it; never accumulate replacement forwarders beyond capacity. Unsubscribe is idempotent and cancels/joins. Terminal stream, emit failure, deletion or app/window close removes the entry. A subscription never owns/prevents audio stop. Expose close_all for window/app shutdown in lib.rs. For an already-terminal durable session (including restart or EventBus replay eviction), return a fresh ticket backed by its durable terminal Status and then close it; do not make this depend on the in-memory replay cache. Frontend still listens then snapshots/pages to recover any event emitted before listener registration. Add late-old-wrapper, cancelled replacement, close_all and post-restart terminal-ticket tests.

Extract this testable boundary in commands/subscriptions.rs:

~~~rust
#[async_trait]
pub trait FrontendEmitter: Send + Sync {
    async fn emit(&self, event_name: &str, event: &SessionEvent) -> AppResult<()>;
}
pub async fn forward_session_events(
    session_id: Uuid, event_name: String,
    receiver: broadcast::Receiver<SessionEvent>,
    cancel: watch::Receiver<bool>, emitter: Arc<dyn FrontendEmitter>,
) -> AppResult<()>;
~~~

Select cancel alongside receive/emit; bound emit to1 s. RecvError::Lagged emits ResyncNeeded then continues; Closed exits. Use an injected bounded-mpsc emitter to force lag and assert ResyncNeeded, a newer final/status, and full durable recovery via SQLite pages. Do not retain the current while let Ok loop that silently ends on lag. Final rows de-duplicate by ID during snapshot recovery; interim display may clear after resync. Test replacement leaves one task, 17th distinct ticket rejects, unsubscribe repeats safely, and terminal/close releases all tasks.

- [ ] **5. Register commands and update integration docs.** Add all commands to applicable debug/release invoke_handler lists in lib.rs; dev_audio_loopback remains debug-only. Document DTO/events, bounded history cursors, route IDs and explicit resume. Keep Tauri's default camelCase top-level argument keys (configId, sessionId, subscriberId, subscriptionId); DTO fields retain their Rust serde snake_case names. SaveProviderRequest is passed as request, SessionLaunch as launch. Pin JSON fixtures for these envelopes and new launch/outcome/credential tags. Preserve existing DriverStatus/AudioDirection/EndpointRole/AudioBackendKind serialization (PascalCase strings and externally tagged payload variants); do not silently rename stored/public values. ProviderCapabilities.native_chunk_duration uses serde Duration's {secs,nanos} object; pin an80ms fixture and normalize only in the future frontend bridge. Retain UI work for phase4. Update Linux utility/stream-drain gates and preserve signed Windows/macOS/actual-device/live-service gates as pending until tested.

- [ ] **6. Run the phase gate and commit.**

~~~bash
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
~~~

Record test counts, ignored tests and build limits. Synthetic success is not real routing or paid AST acceptance. Commit Task6 paths: feat: expose configured runtime commands and recoverable events.

## Verification environment and completion gate

Where system ALSA development metadata is missing, use the existing worktree-local environment with login:false:

~~~bash
env PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu" cargo test --manifest-path src-tauri/Cargo.toml --lib session
~~~

Retain that prefix with each task's cargo arguments. At the phase gate run full Cargo targets, frontend build, formatter and diff checks; library-only success does not prove desktop/minimum-OS builds. Stage only task-owned files, not unrelated lockfiles or changes in the original checkout. Task commits are local review gates, not publishing/merging.

Final report must distinguish:

- Default fixtures cover factory/preflight, explicit IDs/argv, bounded PCM/queues, independent recovery, durable metadata/diagnostics, migration, stop/drain ordering and commands/subscriptions.
- Actual acceptance still needs Windows/macOS signed packages and native IDs; Linux utilities, association and EOF drain under PulseAudio/pipewire-pulse; physical/M1/M2 loopback on minimum OSes; real Volc credentials/quality/latency. Follow [release-gates.md](release-gates.md) and [platform acceptance](../../../tests/platform-audio/README.md).
- Workbench/settings/history UI, automatic summaries, SrvDB and Tavily remain phase4.

## Plan self-review

- [x] Match consumed factory signatures to the landed implementation and keep fixture24k/single-direction resume additions explicit.
- [x] Check produced/consumed type and method names across all tasks.
- [x] Verify every backend integration requirement maps to a task and external gates stay honest.
- [x] Confirm planned event/storage payloads exclude raw audio/secrets and default tests use injected devices/services/keyring.
- [x] Cover partial startup, stop concurrency, persistence failure, drain races and native timeout without false Completed.
- [x] Scan for unspecified placeholder instructions and repair them before execution.

Controller preflight verified all task-pair interfaces against the approved spec. Resolutions: split provider audio before bounded playback; keep diagnostic aggregation in Task 4; expose a typed overflow counter from Task 1; preserve context JSON until its typed accessor is available. The runtime SDD ledger records the interface table and costs of these decisions.
