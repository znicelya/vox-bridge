# Translation Provider Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a stable provider contract, a tested Volcengine AST WebSocket/protobuf adapter, and capability-safe Aliyun/Palabra adapters ready for later real credentials.

**Architecture:** The session layer sees only `TranslationProvider`, `ProviderStream`, and normalized `ProviderEvent`. Each adapter owns its WebSocket framing, authentication, native audio timing, and error mapping. A local fixture server tests the contract without network access; one opt-in Volcengine integration test uses real credentials.

**Tech Stack:** Rust 1.99, Tokio, tokio-tungstenite, prost/prost-build, serde_json, futures-util, uuid, tracing, wiremock or a small Tokio fixture server.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- Only Volcengine is a real provider requirement for the first release.
- Volcengine input is 16 kHz, 16-bit, mono raw PCM in approximately 80 ms chunks.
- Volcengine uses protobuf event values 100/102/150/200/351/352/650-655/152/153/250.
- Provider streams are independent for call directions and use bounded queues.
- Never log API keys, raw audio, transcript payloads, or Base64 TTS data.
- Provider capability declarations must disable unsupported language/audio choices before session start.

### Task 1: Provider contract and fixture protocol

**Files:**
- Create: `src-tauri/src/providers/mod.rs`
- Create: `src-tauri/src/providers/types.rs`
- Create: `src-tauri/src/providers/fixture.rs`
- Modify: `src-tauri/src/domain/translation.rs`
- Test: `src-tauri/src/providers/contract_tests.rs`

**Interfaces:**
- `TranslationProvider::id() -> ProviderId`.
- `TranslationProvider::capabilities() -> ProviderCapabilities`.
- `TranslationProvider::connect(config: ProviderSessionConfig) -> ProviderResult<ProviderStream>`.
- `ProviderStream::send_audio(frame: AudioFrame)`, `finish()`, `cancel()`, `next_event()`.
- `ProviderEvent::{SessionStarted, SourceInterim, SourceFinal, TranslationInterim, TranslationFinal, Audio, Muted, Finished, Failed}`.
- `ProviderResult<T> = Result<T, ProviderError>`; `ProviderSessionConfig` contains session ID, mode, language pair, target audio spec, and provider config ID.
- `ProviderCapabilities` contains supported language pairs, input/output `PcmSpec`, native chunk duration, `supports_tts`, `supports_bidirectional`, `ready`, and `unavailable_reason`.

- [ ] **Step 1: Write provider contract tests against a fixture stream.**

```rust
#[tokio::test]
async fn provider_contract_waits_for_started_before_audio() {
    let fixture = FixtureProvider::new(FixtureScript::started_then_final_text());
    let mut stream = fixture.connect(fixture_config()).await.unwrap();
    assert!(stream.next_event().await.unwrap().is_session_started());
    stream.send_audio(fixture_frame(1)).await.unwrap();
    assert!(stream.next_event().await.unwrap().is_final_translation());
}
```

- [ ] **Step 2: Implement capability and event types.**

Include input/output PCM specs, supported language pairs, `supports_tts`, `supports_bidirectional`, native chunk duration, and a `ready`/`unavailable_reason` field. Normalize provider timestamps to `Duration` and attach `provider_event_type`.

- [ ] **Step 3: Implement the bounded fixture provider.**

Use Tokio channels with explicit capacity 64; make fixture scripts emit handshake, interim, final, audio, error, and disconnect events deterministically.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml providers::contract`

Expected: PASS.

```bash
git add src-tauri/src/providers src-tauri/src/domain/translation.rs
git commit -m "feat: define translation provider contract"
```

### Task 2: Volcengine protobuf inputs

**Files:**
- Create: `src-tauri/proto/volc_ast.proto`
- Create: `src-tauri/build.rs`
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/providers/volc/protocol.rs`
- Create: `src-tauri/src/providers/volc/mod.rs`
- Test: `src-tauri/src/providers/volc/protocol_tests.rs`

- [ ] **Step 1: Add the official AST protobuf definitions.**

Extract the protobuf files from the official archive linked in `docs/同声传译/火山引擎/doc.md`; commit only the `.proto` definitions needed for StartSession, TaskRequest, FinishSession, SessionStarted, subtitle, TTS, usage, muted, finished, and failed events. `build.rs` must compile them with `prost-build` and fail the build if a required message is absent.

- [ ] **Step 2: Write event encoding/decoding tests from the documented event values.**

```rust
#[test]
fn volc_start_session_uses_event_100_and_audio_contract() {
    let bytes = encode_start_session(fixture_volc_config()).unwrap();
    let request = decode_request(&bytes).unwrap();
    assert_eq!(request.event, 100);
    assert_eq!(request.request.unwrap().source_audio.unwrap().rate, 16_000);
}
```

- [ ] **Step 3: Implement protobuf helpers.**

Build StartSession with UUID session ID, `mode: s2s`, source/target language, source raw PCM 16 kHz/16-bit/mono, target PCM 24 kHz/mono, and optional speaker/corpus fields. Build TaskRequest with only audio bytes and FinishSession without audio.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml providers::volc::protocol`

Expected: PASS.

```bash
git add src-tauri/proto src-tauri/build.rs src-tauri/Cargo.toml src-tauri/src/providers/volc/protocol.rs
git commit -m "feat: compile and encode Volcengine AST protobuf"
```

### Task 3: Volcengine WebSocket lifecycle and event mapping

**Files:**
- Create: `src-tauri/src/providers/volc/client.rs`
- Create: `src-tauri/src/providers/volc/events.rs`
- Create: `src-tauri/src/providers/volc/errors.rs`
- Test: `src-tauri/src/providers/volc/client_tests.rs`

**Interfaces:**
- `VolcengineProvider::new(secret_store: Arc<SecretStore>, http: Client) -> Self`.
- `VolcengineProvider::connect(config) -> ProviderResult<ProviderStream>`.
- `map_volc_event(bytes: &[u8]) -> ProviderResult<Vec<ProviderEvent>>`.

- [ ] **Step 1: Write lifecycle tests.**

```rust
#[tokio::test]
async fn client_does_not_send_audio_before_session_started() {
    let server = fixture_server().await;
    let provider = VolcengineProvider::for_endpoint(server.url());
    let mut stream = provider.connect(fixture_volc_config()).await.unwrap();
    assert_eq!(server.received_binary_frames().await.len(), 0);
    assert!(stream.next_event().await.unwrap().is_session_started());
    stream.send_audio(fixture_frame(1)).await.unwrap();
    assert_eq!(server.received_binary_frames().await.len(), 1);
}
```

- [ ] **Step 2: Implement authenticated connection and handshake.**

Read `X-Api-Key` from `SecretStore`; set `X-Api-Resource-Id: volc.service_type.10053`; capture `X-Tt-Logid`; send StartSession only after WebSocket upgrade; gate TaskRequest on event 150.

- [ ] **Step 3: Implement event mapping and TTS pairing.**

Map 651/652 to source interim/final, 654/655 to translation interim/final, 352 to audio with sentence ID, 250 to muted, 152 to finished, and 153 to typed failure. Reject malformed protobuf and oversized audio with bounded errors.

- [ ] **Step 4: Add reconnect and finish behavior.**

Use exponential backoff capped at 5 seconds for three attempts; emit a gap marker on reconnect; do not replay audio after a provider has acknowledged it; send FinishSession once, then wait up to 10 seconds for event 152 before closing.

- [ ] **Step 5: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml providers::volc`

Expected: PASS with fixture server.

```bash
git add src-tauri/src/providers/volc
git commit -m "feat: stream Volcengine AST translations"
```

### Task 4: Aliyun and Palabra capability-safe adapters

**Files:**
- Create: `src-tauri/src/providers/aliyun.rs`
- Create: `src-tauri/src/providers/palabra.rs`
- Create: `src-tauri/src/providers/disabled.rs`
- Test: `src-tauri/src/providers/secondary_provider_tests.rs`

- [ ] **Step 1: Write capability tests.**

```rust
#[test]
fn palabra_declares_24khz_input_and_320ms_chunks() {
    let caps = PalabraProvider::capabilities();
    assert_eq!(caps.input.sample_rate, 24_000);
    assert_eq!(caps.native_chunk_ms, 320);
    assert!(!caps.ready);
}
```

- [ ] **Step 2: Implement disabled adapters with real schemas.**

Store endpoint/auth field definitions and native audio requirements from the checked-in docs; `connect` returns `ProviderUnavailable { reason: "real integration not enabled in first release" }` while `capabilities` remains accurate.

- [ ] **Step 3: Add protocol fixture parsers.**

Implement JSON event parsing and Base64 PCM decoding behind fixture-only code paths so contract regressions are caught before real credentials are added.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml providers::secondary_provider_tests`

Expected: PASS.

```bash
git add src-tauri/src/providers/aliyun.rs src-tauri/src/providers/palabra.rs src-tauri/src/providers/disabled.rs
git commit -m "feat: declare secondary provider capabilities"
```

### Task 5: Provider factory and real integration gate

**Files:**
- Create: `src-tauri/src/providers/factory.rs`
- Create: `tests/provider-integration/volcengine.rs`
- Create: `tests/provider-integration/README.md`
- Modify: `src-tauri/src/session/orchestrator.rs`

- [ ] **Step 1: Implement the provider factory.**

Resolve provider ID to Volcengine, Aliyun, Palabra, or Disabled; validate capability against the requested language pair and audio route before opening a session.

- [ ] **Step 2: Add an opt-in real Volcengine test.**

Read `VOX_VOLC_API_KEY` from the environment; send a 2-second deterministic PCM fixture; assert at least one final source event, one final translation event, and one TTS audio event; redact the key and transcript from test output.

- [ ] **Step 3: Document the credential command and cost boundary.**

Run: `VOX_VOLC_API_KEY=... cargo test --test volcengine -- --ignored --nocapture`. Mark the test ignored by default and require an explicit operator invocation.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` and the fixture suite; run the ignored test only when a real key is available.

```bash
git add src-tauri/src/providers/factory.rs src-tauri/src/session/orchestrator.rs tests/provider-integration
git commit -m "feat: select providers through a capability-safe factory"
```

