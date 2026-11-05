# Translation Runtime Composition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** Draft for implementation self-review; verify consumed factory signatures after provider Task 5 lands.

**Goal:** Connect the implemented audio and provider modules into real live/call session lifecycles with durable source/translation events and usable backend commands.

**Architecture:** ProviderFactory performs preflight before resources are opened. A session runtime owns one independent worker per direction, each connecting native capture, bounded frame packing, a provider stream, an audio route, playback and a transcript sink. SessionOrchestrator owns startup/stop and persistence; React receives text and status only.

**Tech Stack:** Existing Tauri 2, Tokio, cpal, rusqlite, serde, provider contract and native platform drivers. No new audio driver or cloud service.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- First-release modes are live and call. Live writes translated audio to M1; call has independent A→B/M1 and B→A/physical-output paths.
- Each provider connection has one direction. Reverse streams swap the selected source and target languages.
- Capture/provider input is 16 kHz PCM16 mono in approximately 80 ms chunks. Playback is 24 kHz PCM16 mono.
- Raw audio and credentials never enter frontend events, session records or ordinary logs. Only final source/translation text is persisted.
- A direction failure cannot stop the other direction. Device recovery requires explicit user resume; missing drivers or permissions never count as ready.
- Queues, startup, shutdown and retries are bounded. Stale audio is discarded across discontinuities rather than replayed.
- Normal tests use fixture providers and injected audio devices. Real provider calls and OS audio/driver acceptance retain their explicit external gates.
- Existing saved sessions must remain readable and exportable after migration. No schema reset or deletion of user data.

## Prerequisite and scope rulings

Execute after provider-adapters Task 5. Its `ProviderFactory::prepare(saved, session_id, session, input_audio, output_audio)` returns `PreparedProviderSession`; `configs()` exposes the validated directed configurations and async `connect()` returns `Vec<DirectedProviderStream>` (`config`, `stream`). A returned stream is not ready until its SessionStarted event.

The earlier foundation intentionally uses NoopSessionStartup and incomplete event metadata. The provider plan does not implement a capture-to-playback runtime. This integration is required before the workbench may expose a functioning Start action.

Store source and translation as explicitly typed final event rows, linked by provider/native or local correlation when available. Do not infer a row's role from its language or fabricate a sentence association. This preserves asynchronous source/translation arrival and absent IDs; the workbench can display paired rows when a reliable correlation exists. Legacy rows retain a marked legacy origin and a direction-aware role fallback.

## File structure

- `domain/transcript.rs`: serializable text metadata shared by events, storage and commands.
- `session/transcript_sink.rs`: per-stream interim/final identity, provider metadata and durable final writes.
- `storage` and `migrations/0002_runtime.sql`: additive versioned migration, session context and final-event metadata.
- `audio/packetizer.rs`: bounded 80 ms framing of variable capture buffers, preserving origin/time.
- `audio/runtime.rs`: injectable audio ports and a native owner thread for cpal handles.
- `audio/router.rs`: direction-scoped pause/resume in addition to existing whole-graph controls.
- `session/runtime.rs` and `session/direction.rs`: lifecycle owner and independent provider/audio workers.
- `session/orchestrator.rs`: lifecycle persistence and startup/stop coordination.
- `app`, `commands.rs`, `config`: real composition and backend configuration/device commands.

### Task 1: Durable transcript boundary and audio-free UI events

**Files:** Create `src-tauri/src/domain/transcript.rs`, `src-tauri/src/session/transcript_sink.rs`, `src-tauri/src/session/transcript_sink_tests.rs`, `src-tauri/migrations/0002_runtime.sql`. Modify `domain/mod.rs`, `session/mod.rs`, `events/mod.rs`, `storage/{mod,models,sqlite}.rs`, `export/{mod,json,markdown,plain_text}.rs` and covering tests.

**Interfaces:**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind { Source, Translation }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptMetadata {
    pub kind: TranscriptKind,
    pub direction: SessionDirection,
    pub provider: ProviderId,
    pub provider_config_id: Uuid,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub language: String,
    pub provider_event_type: Option<String>,
    pub sentence_id: Option<String>,
    pub local_correlation_id: Option<u64>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptUpdate {
    pub id: Uuid,
    pub session_id: Uuid,
    pub occurred_at: SystemTime,
    pub metadata: TranscriptMetadata,
    pub text: String,
    pub is_final: bool,
}
// TranscriptSink::new(store: Arc<LocalStore>, bus: EventBus,
//                     config: ProviderSessionConfig, provider: ProviderId) -> Self
// TranscriptSink::accept(&mut self, event: &ProviderEvent) -> AppResult<()>
// LocalStore::list_final_transcript(session_id: Uuid) -> AppResult<Vec<TranscriptUpdate>>
```

- [ ] Write failing tests using real in-memory SQLite and EventBus. Feed source interim, source final, translation final and Audio; assert only two final text rows are stored, roles/direction/timing/native metadata survive reload/export, interim replaces the current role's text identity, and serialized frontend events contain neither samples nor credentials. A target-to-source call row must export its role correctly regardless of the base session language pair.
- [ ] Add a migration test that opens a database built with `0001_session_foundation.sql`, inserts a legacy final row, opens it with LocalStore twice and verifies data preserved and migration idempotent. Add transactional failure coverage: if final storage fails, do not emit a successful durable final event.
- [ ] Implement additive migrations with a schema version inside a transaction. Use explicit SQL insert/select columns, preserve existing final rows, and attach new metadata/session context without resetting the database. Expose ordered final rows through LocalStore for the runtime and later history UI.
- [ ] Implement TranscriptSink. Emit interim text only in memory, persist finals before publishing them, allocate monotonically increasing per-direction sequences, and advance stream_epoch on Gap so restarted provider timestamps cannot join old sentences. Keep absent correlations absent; finalize/clear active interim identity on a terminal boundary. Publish gap/error/status metadata without remote payloads.
- [ ] Replace the raw-sample `SessionEvent::TranslationAudio` with a typed playback-status event. Extend transcript events with TranscriptUpdate. Update exports to use explicit kind and include source/translation metadata in JSON; retain legacy readability and never export audio.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib transcript_sink`, `--lib storage`, and `--lib export`. Commit only this task's files as `feat: persist directed source and translation events`.

### Task 2: Bounded audio ports and independent route recovery

**Files:** Create `src-tauri/src/audio/{packetizer,packetizer_tests,runtime,runtime_tests}.rs`. Modify `audio/{mod,frame,capture,router,router_tests}.rs` and narrowly required playback lifecycle code.

**Interfaces:**

```rust
// AudioPacketizer::new(source: AudioSource) -> Self
// AudioPacketizer::push(frame: AudioFrame) -> AppResult<Vec<AudioFrame>>
// AudioPacketizer::flush() -> AppResult<Option<AudioFrame>>
// AudioPacketizer::reset() discards carry and advances sequence continuity.
// AudioRuntime::open(input: AudioEndpoint, output: AudioEndpoint,
//                    source: AudioSource) -> AppResult<AudioPorts>
// AudioPorts exposes bounded captured frames, a playback sender, a health watch,
// stop-capture, bounded drain-playback, stop-all and completion acknowledgement. Handles are Send.
// NativeAudioRuntime implements AudioRuntime; fixture runtime lives in tests.
// AudioRouter::{pause_direction,resume_direction,direction_status} take Direction.
```

- [ ] Write packetizer tests using known increasing samples: fragmented input becomes exact 1,280-sample packets without loss/duplication, the final tail is emitted once, reset discards an incomplete old packet, metadata keeps original capture time/source, wrong PCM is rejected and an oversized source frame cannot cause unbounded allocation. Bound accepted source-frame duration to one second (16,000 samples); production cpal buffers fit below this limit.
- [ ] Write route tests with real bounded channels and mutable injected endpoint health. Disconnect A's output, assert its frame is rejected/paused while B's next frame reaches physical output; restoring health alone cannot resume A; explicit resume succeeds only when that direction's endpoints are ready. Preserve existing whole-graph pause/resume behavior.
- [ ] Implement packetizer and explicit capture origin. Do not identify M2 by matching M1's display name. Preserve capture timestamps when splitting frames; maintain bounded carry below one packet. Expose capture overflow/gap counters to the direction worker. The runtime capture queue retains at most 50 frames per source and discards the oldest queued frame on overflow; do not treat a full queue as permission for an unbounded pending send.
- [ ] Implement native audio ownership on a dedicated thread where cpal streams are created, retained and dropped; never add unsafe Send/Sync for platform handles. Use bounded channels/watch control and completion acknowledgement. Startup failure closes already-created handles; stop capture before final provider drain, then stop playback. Expose queued/consumed sample counters so drain-playback can wait for actual consumption, not merely enqueue; bound playback drain to five seconds and report discarded tail on timeout. Closed consumers terminate conversion workers. Native open is reached only by an explicit session start after preflight.
- [ ] Implement per-direction router state and events. Whole-graph methods retain their prior meaning; route admission checks only the selected direction. No wait holds the routing/state mutex across an async operation. Device loss clears pending output for that direction.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib audio::packetizer`, `--lib audio::runtime`, `--lib audio::router`. Commit as `feat: compose bounded audio ports with independent recovery`.

### Task 3: Session runtime with real readiness and graceful shutdown

**Files:** Create `src-tauri/src/session/{runtime,direction,runtime_tests}.rs`. Modify `session/{mod,orchestrator,orchestrator_tests}.rs`, `domain/session.rs`, and events/storage context interfaces from Task 1.

**Interfaces:**

```rust
// SessionLaunch contains SessionConfig, saved provider configuration UUID,
// selected live/A input ID, M1 output ID, and optional M2 input/physical output IDs.
// IDs are resolved to fresh backend-discovered endpoints; client-ready flags are not trusted.
// TranslationRuntime::start(session_id: Uuid, launch: SessionLaunch)
//     -> AppResult<RunningSession> using ProviderFactory, AudioRuntime and TranscriptSink.
// RunningSession::stop(&self) -> AppResult<()> is idempotent and waits for cleanup.
// RunningSession::resume_direction(&self, direction: SessionDirection) -> AppResult<()>.
// Runtime status is a watch channel retained by the orchestrator.
```

- [ ] Write a fixture live integration test through the actual factory/runtime, injected audio ports, EventBus and SQLite. Assert no input sent before provider handshake, paced frames reach the provider, final source/translation are durable, normalized TTS reaches only M1, and graceful stop delivers the provider's final tail before Completed.
- [ ] Write call tests for reverse languages and independent routing, unilateral provider disconnect/reconnect gap, exhausted retries, one device loss, explicit recovery, output saturation and stop during a pending handshake. Add two concurrent stop callers and a consumer that stops polling events; all must complete within bounded time with no duplicate finish or retained runtime tasks.
- [ ] Implement one independent direction worker for each prepared stream. Use provider events and native health to control input admission; on reconnect/gap discard old input/carry and show continuity loss. On final failure pause only that direction. On explicit resume revalidate the same selected devices and open a fresh provider stream, retaining session identity and incrementing stream epoch.
- [ ] Wire provider Audio only to that direction's AudioRouter and playback port; route text/status through TranscriptSink. Audio metadata/latency/status can reach EventBus; samples cannot. Record capture-to-send and response-to-enqueue timings from available clocks; do not fabricate provider-internal timing or label enqueue as actual playback. Unavailable latency stages remain explicitly unknown.
- [ ] Replace the production Noop startup path with runtime ownership. Persist Starting before startup and Running only after required directions' audio/provider readiness. Roll back partial startup on any failure. Keep no global active-session lock across network/device waits. Replace the one-permit Notify startup wait with replayable completion state so multiple stop callers cannot hang.
- [ ] Stop ordering: seal/stop capture, flush accepted input, request provider finish once, keep reading final text/TTS up to the provider's ten-second deadline, drain playback for up to five seconds, stop output and join owned tasks, persist terminal state, then close EventBus. Force cancellation on timeout; failed cleanup is recorded as failure rather than success. Persist final text even with zero UI subscribers.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib session`. Commit as `feat: run live and bidirectional translation sessions`.

### Task 4: App composition, launch commands and recoverable subscriptions

**Files:** Modify `src-tauri/src/app/{mod,tests}.rs`, `commands.rs`, `lib.rs`, `config/{mod,models,secret_store,secret_store_tests}.rs`. Create `src-tauri/src/commands_tests.rs`; update backend command documentation in README.

**Interfaces:**

```rust
// Backend commands: list_audio_endpoints, audio_driver_status,
// list_provider_configs, save_provider_config, delete_provider_config,
// provider_capabilities, validate_session_launch, create_session,
// stop_session, resume_session_direction, get_session_transcript,
// get_session, list_sessions, delete_session, export_session.
// create_session accepts SessionLaunch and returns SessionRecord in Starting state.
// No raw credential is returned by a save/list/capability/validation command.
```

- [ ] Write command-service tests with a temporary data directory and injected audio/provider/secret dependencies. Save a provider profile and credential, reload it, verify public serialization has only a configured marker/reference, reject unknown/stale endpoint IDs and unready drivers before connecting, and prove an AppState session uses the runtime rather than Noop.
- [ ] Compose ConfigStore/SecretStore, ProviderFactory, platform driver service, NativeAudioRuntime and TranslationRuntime in AppState. Keep constructors free of microphone capture/network calls. Enumerate physical and virtual endpoints with accurate capture/playback direction; M1's playback side and M2's capture side must resolve to real backend IDs. Keep existing platform install/repair/redistribution gates intact.
- [ ] Serialize configuration writes under an application-owned lock and make encrypted secret read/modify/write atomic across concurrent saves. Keep stable secret identities from the provider factory. Clear/save credentials only in the backend; never put a secret inside a URL. Return safe configuration/capability/permission errors close to the corresponding command result.
- [ ] Register the commands. Snapshot/history reads include provider/device context and transcript metadata. Resume is explicitly direction-scoped. Input validation uses current backend-discovered endpoints, saved provider settings and the factory's full-direction preflight.
- [ ] Replace the event forwarder's `while let Ok` exit-on-lag behavior: on broadcast lag publish a typed resync-needed state and continue; provide final-row snapshot recovery. Add unsubscribe/cancellation ownership so closing a UI subscription does not leave an unbounded set of forwarding tasks. EventBus final replay remains bounded.
- [ ] Run complete `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm build`, formatter and diff checks. Use only synthetic audio/provider command tests. Document exact fixture vs external acceptance status; commit as `feat: expose configured translation runtime to desktop commands`.

## Verification environment

Use the recorded worktree-local ALSA environment with `login:false` when system ALSA development metadata is unavailable:

```bash
env PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu" cargo test --manifest-path src-tauri/Cargo.toml --lib session
```

For individual tasks, use the same prefix with that task's filter. Run the complete Cargo target at the phase gate; do not confuse a library-only check with a desktop cross-platform build. Real OS audio/driver acceptance and the ignored Volcengine test stay pending until explicitly exercised on their required environments.
