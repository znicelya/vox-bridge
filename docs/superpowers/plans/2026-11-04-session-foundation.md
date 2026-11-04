# Session Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust session state machine, typed event contract, encrypted settings, SQLite persistence, and export foundation used by every later Vox Bridge feature.

**Architecture:** Tauri commands create and control sessions in Rust. A bounded Tokio event bus publishes typed session events to the frontend and a SQLite writer persists only final transcript events. Secrets use a randomly generated data-encryption key stored in the OS keychain and AES-GCM encrypted values in the application data directory.

**Tech Stack:** Rust 1.99, Tauri 2, Tokio, serde, rusqlite with bundled SQLite, keyring, aes-gcm, uuid, tracing, React/TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- Support Windows 10 22H2, macOS 13 Ventura, and Ubuntu 22.04 LTS.
- Keep session records local; do not upload to a Vox Bridge server.
- Do not persist raw or translated audio by default.
- Store provider/API secrets encrypted locally; never expose them to React or ordinary logs.
- Keep live queues bounded and represent temporary versus final transcript events explicitly.
- Use sentence-case UI copy and typed IDs/timestamps; preserve provider log IDs only as diagnostic metadata.

### Task 1: Rust foundation and module boundaries

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/app/mod.rs`
- Create: `src-tauri/src/domain/mod.rs`
- Create: `src-tauri/src/events/mod.rs`
- Test: `src-tauri/src/app/tests.rs`

**Interfaces:**
- Produces `AppState`, `AppError`, `AppResult<T>`, and module registration used by every later plan.
- `AppState::new(data_dir: PathBuf) -> AppResult<Self>` owns the data directory and event bus in this task; Tasks 3 and 4 extend it with secret and SQLite stores.

- [ ] **Step 1: Add dependencies and test-only helpers.**

Add the following dependencies with versions compatible with Rust 1.99: `tokio` with `rt-multi-thread`, `macros`, `sync`, and `time`; `async-trait`; `serde` with `derive`; `serde_json`; `thiserror`; `uuid` with `v4` and `serde`; `rusqlite` with `bundled`; `keyring`; `aes-gcm`; `base64`; `tracing`; `tracing-subscriber`; and `tempfile` as a dev dependency.

- [ ] **Step 2: Define the application error boundary.**

```rust
pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("session not found: {0}")]
    SessionNotFound(uuid::Uuid),
    #[error("internal error: {0}")]
    Internal(String),
}
```

- [ ] **Step 3: Construct `AppState` and wire it into Tauri.**

Create `AppState` with `Arc<LocalStore>`, `Arc<SecretStore>`, and `EventBus`; register it with `tauri::Builder::manage`; keep the existing opener plugin and replace the template `greet` command only after later commands are registered.

- [ ] **Step 4: Add the first state construction test.**

```rust
#[test]
fn app_state_initializes_the_event_bus_and_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(state.data_dir(), dir.path());
    assert!(state.event_bus().is_healthy());
}
```

- [ ] **Step 5: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml app_state_initializes_the_event_bus_and_data_directory`

Expected: PASS.

```bash
git add src-tauri/Cargo.toml src-tauri/src/error.rs src-tauri/src/app src-tauri/src/domain src-tauri/src/events src-tauri/src/lib.rs
git commit -m "feat: establish session foundation modules"
```

### Task 2: Typed session and event contracts

**Files:**
- Create: `src-tauri/src/domain/session.rs`
- Create: `src-tauri/src/domain/audio.rs`
- Create: `src-tauri/src/domain/translation.rs`
- Create: `src-tauri/src/events/bus.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/events/mod.rs`
- Test: `src-tauri/src/domain/session_tests.rs`

**Interfaces:**
- `SessionConfig`, `SessionMode`, `SessionDirection`, `LanguagePair`, `AudioSource`, `ProviderId`.
- `AudioFrame { source, sequence, captured_at, samples }` with 16 kHz/16-bit/mono invariants.
- `SessionEvent::{Status, InterimTranscript, FinalTranscript, TranslationAudio, Error, Latency}`.
- `EventBus::subscribe(session_id) -> broadcast::Receiver<SessionEvent>` and `EventBus::publish(session_id, event)`.

- [ ] **Step 1: Write serialization and invariant tests.**

```rust
#[test]
fn session_config_round_trips_without_losing_direction() {
    let config = SessionConfig::call(LanguagePair::new("zh", "en")).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let decoded: SessionConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.direction, SessionDirection::Bidirectional);
}

#[test]
fn audio_frame_rejects_non_16khz_shape() {
    let result = AudioFrame::new(AudioSource::Microphone, 1, 48_000, vec![0; 960]);
    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
}
```

- [ ] **Step 2: Implement the domain enums and constructors.**

Use `#[serde(rename_all = "snake_case")]`; validate ISO-like two-letter language codes, require a nonempty provider ID, and make `SessionConfig::live` single-direction and `SessionConfig::call` bidirectional.

- [ ] **Step 3: Implement the bounded event bus.**

Create one `broadcast::channel(512)` per active session; return a typed `Lagged` error when a subscriber falls behind instead of silently growing memory. Include `event_id: Uuid`, `session_id`, and `occurred_at` on every event.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml domain -- --nocapture`

Expected: PASS with no serialization or invariant failures.

```bash
git add src-tauri/src/domain src-tauri/src/events
git commit -m "feat: add typed session and event contracts"
```

### Task 3: Encrypted local settings

**Files:**
- Create: `src-tauri/src/config/mod.rs`
- Create: `src-tauri/src/config/secret_store.rs`
- Create: `src-tauri/src/config/models.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Test: `src-tauri/src/config/secret_store_tests.rs`

**Interfaces:**
- `SecretStore::set(service: &str, key: &str, value: &[u8]) -> AppResult<()>`.
- `SecretStore::get(service: &str, key: &str) -> AppResult<Option<Vec<u8>>>`.
- `ConfigStore::save_provider_config(config: ProviderConfig) -> AppResult<()>` and `load_provider_config(id: Uuid) -> AppResult<ProviderConfig>`.
- `ProviderConfig` stores endpoint, public fields, and secret references; it never serializes secret bytes to the frontend.

- [ ] **Step 1: Write round-trip and redaction tests.**

```rust
#[test]
fn secret_round_trip_uses_keyring_backed_dek() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::for_tests(dir.path()).unwrap();
    store.set("volcengine", "api_key", b"secret").unwrap();
    assert_eq!(store.get("volcengine", "api_key").unwrap().unwrap(), b"secret");
    assert!(!std::fs::read_to_string(store.config_path()).unwrap().contains("secret"));
}
```

- [ ] **Step 2: Generate and store the data-encryption key.**

Generate 32 random bytes once; save the key in the platform keyring under service `com.voxbridge.secrets` and account `data-encryption-key`; encrypt secret values with AES-256-GCM and a fresh 96-bit nonce per value; store only nonce and ciphertext as Base64 in the local config file.

- [ ] **Step 3: Add typed provider/LLM/Embedding/Tavily config models.**

Persist endpoint, model, resource ID, language capabilities and secret references; expose `PublicProviderConfig` without secret material. Reject empty endpoints, non-HTTPS remote URLs, and missing required fields before writing.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml secret_store -- --nocapture`

Expected: PASS; test output must contain no secret value.

```bash
git add src-tauri/src/config src-tauri/src/app
git commit -m "feat: encrypt local service configuration"
```

### Task 4: SQLite schema and repository

**Files:**
- Create: `src-tauri/migrations/0001_session_foundation.sql`
- Create: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/src/storage/sqlite.rs`
- Create: `src-tauri/src/storage/models.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Test: `src-tauri/src/storage/sqlite_tests.rs`

**Interfaces:**
- `LocalStore::open(data_dir: &Path) -> AppResult<Self>`.
- `LocalStore::create_session(session: &SessionRecord) -> AppResult<()>`.
- `LocalStore::append_final_segment(segment: &TranscriptSegment) -> AppResult<()>`.
- `LocalStore::list_sessions(limit: u32, cursor: Option<String>) -> AppResult<Vec<SessionSummary>>`.
- `LocalStore::delete_session(session_id: Uuid) -> AppResult<()>`.

- [ ] **Step 1: Write repository tests for sessions, final segments, and cascade deletion.**

```rust
#[test]
fn deleting_session_removes_segments_and_summary() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    store.append_final_segment(&fixture_segment(session.id)).unwrap();
    store.delete_session(session.id).unwrap();
    assert!(store.get_session(session.id).unwrap().is_none());
    assert_eq!(store.count_segments(session.id).unwrap(), 0);
}
```

- [ ] **Step 2: Create the schema.**

Create `sessions`, `translation_segments`, `session_summaries`, `research_runs`, `research_sources`, `documents`, and `document_chunks`; enable foreign keys; index `sessions.started_at`, `translation_segments(session_id, sequence)`, and `documents.sha256`.

- [ ] **Step 3: Implement transactions and final-only persistence.**

Reject interim transcript writes in repository methods; insert final segments with unique `(session_id, direction, sequence)`; use transactions for session deletion and summary version creation.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml sqlite -- --nocapture`

Expected: PASS with foreign-key and uniqueness checks enabled.

```bash
git add src-tauri/migrations src-tauri/src/storage src-tauri/src/app
git commit -m "feat: persist sessions and transcript segments locally"
```

### Task 5: Session orchestrator and Tauri commands

**Files:**
- Create: `src-tauri/src/session/mod.rs`
- Create: `src-tauri/src/session/orchestrator.rs`
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/session/orchestrator_tests.rs`

**Interfaces:**
- `SessionOrchestrator::start(config: SessionConfig) -> AppResult<SessionHandle>`.
- `SessionHandle::stop(self) -> impl Future<Output = AppResult<SessionRecord>>`.
- Tauri commands: `create_session`, `stop_session`, `list_sessions`, `get_session`, `delete_session`, `subscribe_session`.

- [ ] **Step 1: Write lifecycle tests.**

```rust
#[tokio::test]
async fn stopping_a_live_session_persists_end_time_and_status() {
    let orchestrator = fixture_orchestrator();
    let handle = orchestrator.start(SessionConfig::live(fixture_pair())).await.unwrap();
    let record = handle.stop().await.unwrap();
    assert_eq!(record.status, SessionStatus::Completed);
    assert!(record.ended_at.is_some());
}
```

- [ ] **Step 2: Implement start/stop state transitions.**

Use `Arc<RwLock<HashMap<Uuid, ActiveSession>>>`; allow `Created -> Starting -> Running -> Stopping -> Completed`, and route any provider/audio error to `Paused` or `Failed` with a typed error event. Persist metadata before publishing `Running`.

- [ ] **Step 3: Register Tauri commands and event forwarding.**

Serialize public session snapshots through `serde`; emit `voxbridge://session/{id}` events containing JSON `SessionEvent`; never return `SecretStore` values. Return actionable error strings containing a stable error code.

- [ ] **Step 4: Run the Rust suite and build.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` and `pnpm build`.

Expected: all Rust tests pass and TypeScript/Vite build succeeds.

```bash
git add src-tauri/src/session src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add session lifecycle commands"
```

### Task 6: Export and foundation acceptance

**Files:**
- Create: `src-tauri/src/export/mod.rs`
- Create: `src-tauri/src/export/markdown.rs`
- Create: `src-tauri/src/export/json.rs`
- Create: `src-tauri/src/export/plain_text.rs`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/export/export_tests.rs`

**Interfaces:**
- `export_session(session_id: Uuid, format: ExportFormat, destination: &Path) -> AppResult<PathBuf>`.
- `ExportFormat::{Markdown, Json, PlainText}`.

- [ ] **Step 1: Write deterministic export tests.**

```rust
#[test]
fn markdown_export_contains_direction_and_timestamped_final_segments() {
    let output = render_markdown(fixture_session_with_segments());
    assert!(output.contains("Customer discovery"));
    assert!(output.contains("14:32:08"));
    assert!(!output.contains("interim"));
}
```

- [ ] **Step 2: Implement the three renderers.**

Render only final segments and summary versions; JSON must preserve stable field names; plain text must include `[HH:MM:SS] [direction] source` and `translation` lines. Do not include audio paths or secrets.

- [ ] **Step 3: Register export command and file validation.**

Reject directories, non-writable destinations, and unknown extensions; write to a temporary sibling file and atomically rename after complete output.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml export`

Expected: PASS.

```bash
git add src-tauri/src/export src-tauri/src/commands.rs
git commit -m "feat: export local session records"
```

