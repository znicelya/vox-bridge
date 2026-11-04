# Platform Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install and operate Vox Bridge virtual audio endpoints on Windows, macOS, and Linux, then provide bounded capture, resampling, and routing for live sessions.

**Architecture:** `AudioCapture` and `AudioRouter` use a canonical PCM contract and delegate device installation/discovery to OS adapters. Driver packages are versioned resources with explicit install/repair/uninstall commands and health probes. Audio queues are bounded Tokio channels; device loss stops output until the user explicitly resumes.

**Tech Stack:** Rust 1.99, Tauri 2, cpal, rubato, tokio, serde, tracing; signed Windows driver package, macOS system audio extension package, PipeWire/PulseAudio setup scripts.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- Windows 10 22H2, macOS 13 Ventura, Ubuntu 22.04 LTS; Linux must cover PipeWire and PulseAudio.
- First release must install/manage real M1 and M2 endpoints on all three platforms.
- M1 is `VoxBridge Input` and M2 is `VoxBridge Output`; do not change system defaults silently.
- Capture canonical format is 16 kHz, 16-bit, mono PCM; playback bus is 24 kHz, 16-bit, mono PCM.
- Queues have explicit limits; audio device disconnects never fall through to an arbitrary device.
- Administrator/system-extension approval is allowed and must be visible in the UI.

### Task 1: Canonical audio types and resampling

**Files:**
- Create: `src-tauri/src/audio/mod.rs`
- Create: `src-tauri/src/audio/frame.rs`
- Create: `src-tauri/src/audio/resample.rs`
- Test: `src-tauri/src/audio/resample_tests.rs`
- Modify: `src-tauri/src/domain/audio.rs`

**Interfaces:**
- `PcmSpec { sample_rate: u32, channels: u16, sample_format: SampleFormat }`.
- `AudioFrame::new(source, sequence, spec, samples) -> AppResult<AudioFrame>`.
- `Resampler::convert(input: &[i16], from: PcmSpec, to: PcmSpec) -> AppResult<Vec<i16>>`.

- [ ] **Step 1: Write resampling tests.**

```rust
#[test]
fn converts_24khz_output_to_a_48khz_device_without_changing_duration() {
    let input = vec![0i16; 24_000];
    let output = Resampler::convert(&input, PcmSpec::pcm16_mono(24_000), PcmSpec::pcm16_mono(48_000)).unwrap();
    assert_eq!(output.len(), 48_000);
}
```

- [ ] **Step 2: Implement format validation and rubato conversion.**

Reject non-mono or unsupported sample formats; use a fixed resampler configuration with no allocation in the callback thread; return duration metadata with every converted frame.

- [ ] **Step 3: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml audio::resample`

Expected: PASS, including duration and silence preservation assertions.

```bash
git add src-tauri/src/audio src-tauri/src/domain/audio.rs src-tauri/Cargo.toml
git commit -m "feat: add canonical PCM and resampling pipeline"
```

### Task 2: Driver lifecycle contract and packages

**Files:**
- Create: `src-tauri/src/audio/platform/mod.rs`
- Create: `src-tauri/src/audio/platform/windows.rs`
- Create: `src-tauri/src/audio/platform/macos.rs`
- Create: `src-tauri/src/audio/platform/linux.rs`
- Create: `src-tauri/resources/audio/manifest.json`
- Create: `src-tauri/resources/audio/windows/install.ps1`
- Create: `src-tauri/resources/audio/macos/install.sh`
- Create: `src-tauri/resources/audio/linux/setup-pipewire.sh`
- Create: `src-tauri/resources/audio/linux/setup-pulseaudio.sh`
- Test: `src-tauri/src/audio/platform/tests.rs`

**Interfaces:**
- `AudioDriverManager::status() -> AppResult<DriverStatus>`.
- `install()`, `repair()`, `uninstall()`, and `list_endpoints() -> AppResult<Vec<AudioEndpoint>>`.
- `AudioEndpoint { id, display_name, direction, sample_rates, channels, ready }`.
- `DriverStatus::{NeedsPrivileges, NeedsApproval, NeedsRestart, Failed(String), Ready}`.
- `CommandRunner::run(program: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput>`.

- [ ] **Step 1: Record the driver release and license gate.**

Add the exact signed Windows endpoint package, macOS system extension bundle, and Linux setup scripts to `manifest.json` with SHA-256, version, install command, uninstall command, required privileges, and reboot/approval behavior. The package must have a redistribution license before it is bundled; a failed license check blocks packaging rather than silently downloading an unknown driver.

- [ ] **Step 2: Implement platform status probes.**

Windows probes endpoint names through WASAPI; macOS probes the approved AudioServerPlugIn/system extension; Linux probes PipeWire nodes and PulseAudio sources/sinks. Normalize all results to `AudioEndpoint` and require both M1 and M2 before reporting `Ready`.

- [ ] **Step 3: Implement privileged lifecycle commands.**

Run the platform installer through Tauri's privileged helper mechanism, capture exit code and stderr, validate endpoint names after install, and return `NeedsApproval`, `NeedsRestart`, `Failed`, or `Ready` states. Never change the system default endpoint.

- [ ] **Step 4: Add dry-run tests.**

Inject a `CommandRunner` so tests can assert install arguments, SHA-256 validation, and state mapping without touching host drivers.

- [ ] **Step 5: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml audio::platform`

Expected: PASS with platform command runners fully mocked.

```bash
git add src-tauri/src/audio/platform src-tauri/resources/audio src-tauri/tauri.conf.json
git commit -m "feat: manage platform virtual audio endpoints"
```

### Task 3: Capture and playback primitives

**Files:**
- Create: `src-tauri/src/audio/capture.rs`
- Create: `src-tauri/src/audio/playback.rs`
- Create: `src-tauri/src/audio/queue.rs`
- Test: `src-tauri/src/audio/queue_tests.rs`

**Interfaces:**
- `AudioCapture::start(endpoint: AudioEndpoint, sink: mpsc::Sender<AudioFrame>) -> AppResult<CaptureHandle>`.
- `AudioPlayback::start(endpoint: AudioEndpoint, source: mpsc::Receiver<AudioFrame>) -> AppResult<PlaybackHandle>`.
- `BoundedAudioQueue::new(capacity_frames: usize)` defaults to 50 frames and reports dropped frame counts.

- [ ] **Step 1: Write queue behavior tests.**

```rust
#[tokio::test]
async fn full_audio_queue_drops_oldest_and_reports_backpressure() {
    let queue = BoundedAudioQueue::new(2);
    queue.push(fixture_frame(1)).await.unwrap();
    queue.push(fixture_frame(2)).await.unwrap();
    queue.push(fixture_frame(3)).await.unwrap();
    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(queue.pop().await.unwrap().sequence, 2);
}
```

- [ ] **Step 2: Implement cpal stream callbacks.**

Keep callbacks allocation-free: copy samples into a preallocated frame buffer, send with `try_send`, and emit a backpressure metric when the queue is full. Convert device interleaved samples to canonical mono PCM before publishing.

- [ ] **Step 3: Implement playback underflow and stop semantics.**

Fill underflow with silence and emit `AudioUnderflow`; on endpoint loss stop the stream, emit `DeviceDisconnected`, and require a new explicit `start` command after a fresh health check.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml audio::queue`

Expected: PASS; no test may create an unbounded queue.

```bash
git add src-tauri/src/audio/capture.rs src-tauri/src/audio/playback.rs src-tauri/src/audio/queue.rs
git commit -m "feat: capture and play bounded PCM streams"
```

### Task 4: Mode-specific routing

**Files:**
- Create: `src-tauri/src/audio/router.rs`
- Create: `src-tauri/src/audio/graph.rs`
- Test: `src-tauri/src/audio/router_tests.rs`

**Interfaces:**
- `AudioGraph::for_live(input, m1) -> LiveGraph`.
- `AudioGraph::for_call(a_input, m2_input, m1_output, physical_output) -> CallGraph`.
- `AudioRouter::route(provider_audio, target: RouteTarget) -> AppResult<()>`.
- `Direction::{Live, AtoB, BtoA}` and `RouteTarget::{M1, PhysicalOutput}` are the only route targets in the first release.

- [ ] **Step 1: Write graph topology tests.**

```rust
#[test]
fn call_graph_has_two_independent_translation_directions() {
    let graph = AudioGraph::for_call(fixture("A"), fixture("M2"), fixture("M1"), fixture("speaker"));
    assert_eq!(graph.directions(), vec![Direction::AtoB, Direction::BtoA]);
    assert_eq!(graph.target(Direction::AtoB), RouteTarget::M1);
    assert_eq!(graph.target(Direction::BtoA), RouteTarget::PhysicalOutput);
}
```

- [ ] **Step 2: Implement live and call graphs.**

Bind live input to one provider and M1 output; bind call A and M2 to two independent provider channels; never share a receiver between directions. Attach source/destination IDs to every route error.

- [ ] **Step 3: Add explicit pause/resume commands.**

Pause all routes on driver loss, retain only final transcript events, require a device health check before resume, and emit a user-visible state event.

- [ ] **Step 4: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml audio::router`

Expected: PASS.

```bash
git add src-tauri/src/audio/router.rs src-tauri/src/audio/graph.rs
git commit -m "feat: route live and call audio graphs"
```

### Task 5: Platform acceptance harness

**Files:**
- Create: `tests/platform-audio/windows.md`
- Create: `tests/platform-audio/macos.md`
- Create: `tests/platform-audio/linux.md`
- Create: `tests/platform-audio/run-loopback.rs`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Document exact manual acceptance steps.**

For each OS, install from a clean machine, approve privileges, verify M1/M2 names, set them in a test call application, send a 1 kHz tone and spoken phrase, verify translated audio reaches the expected endpoint, unplug/reconnect the device, and confirm output remains stopped until resume.

- [ ] **Step 2: Add a loopback test command.**

Expose a dev-only command that generates a deterministic 1 kHz PCM tone and validates capture/playback sequence counts without contacting a provider.

- [ ] **Step 3: Run platform checks and commit.**

Run on Windows 10 22H2, macOS 13, Ubuntu 22.04 with both PipeWire and PulseAudio. Record device IDs, installer versions, and pass/fail evidence in the platform files.

```bash
git add tests/platform-audio src-tauri/tauri.conf.json
git commit -m "test: document and exercise cross-platform audio loopback"
```

