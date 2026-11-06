# Platform audio acceptance

The Cargo example and Tauri command are development diagnostics. Neither calls a translation provider. A successful synthetic run is **not** a Windows, macOS, Linux, driver, microphone, speaker, or translated-call acceptance result.

## Synthetic check (no devices)

From the repository root, with Rust and the normal Tauri/CPAL development dependencies installed:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo run --manifest-path src-tauri/Cargo.toml --example run-loopback -- --synthetic --frames 50
```

The example is registered in `src-tauri/Cargo.toml`. `tauri.conf.json` watches this directory during development. A 50-frame run represents one second: JSON must report `mode: "synthetic"`, capture `{frames: 50, samples: 16000}`, playback `{frames: 50, samples: 24000}`, and `dropped_frames: 0`. Exit 0 means those checks completed. Invalid arguments or validation failures exit 1. A frame is 20 ms; requests are limited to 1–250 frames.

The deterministic integer waveform has a 16-sample period at 16 kHz (1 kHz), PCM16 mono, approximately -20 dBFS peak. Synthetic capture feeds the real bounded audio queue; one continuous resampler converts to the 24 kHz playback bus; the real live graph/router sends to a bounded synthetic playback receiver. Sequence gaps, duplicates, rate mismatches, missing frames, changed queued PCM, and final frame/sample count mismatches fail. This does not exercise CPAL callbacks, device latency, acoustic content, or providers.

In a Tauri **debug** frontend, the same check is available through the registered command:

```ts
import { invoke } from "@tauri-apps/api/core";
const report = await invoke("dev_audio_loopback", { frames: 50 });
console.log(report);
```

`frames` may be omitted (50). The shared module, command, and invoke-handler entry are all excluded when `debug_assertions` is false. The example in a normal release build exits 2 without opening devices. No configuration enables this command in a normal release build.

## Runtime ports (injected default checks)

The runtime/backend tests use injected non-Send owners, in-memory process pipes, and replayable child-exit signals. They do not enumerate or open hardware, run `pactl`/`parec`/`pacat`, access a keyring, or contact a provider:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib audio::platform
cargo test --manifest-path src-tauri/Cargo.toml --lib audio
```

These checks cover exact private locator and inventory-generation validation, atomic pair leases and quarantine, non-Send owner-thread confinement, bounded cleanup and truthful drain, outside-owner suppression/statistics, native capture discontinuities, Pulse argv and stream association, process timeout/size bounds, EOF and odd-byte handling, and legacy/new diagnostic serialization. Passing them is implementation evidence only; it is not platform acceptance.

The following Task 3 gates remain manual on both PulseAudio and pipewire-pulse and must be recorded separately for each server: actual utility versions; bounded JSON compatibility for sources, sinks, modules, source-outputs, and sink-inputs; the exact `VoxBridge_M1` and `VoxBridge_Output` sink set; `VoxBridge_Input` remapped from `VoxBridge_M1.monitor`; `VoxBridge_Output.monitor` linked to the output sink; unique `media.name` association to the requested indices; `parec` odd/clean EOF behavior; `pacat` exit-zero drain after stdin EOF; cancellation of a blocked write; and loss without automatic resume. Do not change system defaults during these checks. Until evidence is attached to the OS-specific file, mark the corresponding result **PENDING**, not PASS.

## Physical diagnostic (explicit opt-in)

Run only on the selected acceptance machine after connecting the intended loopback route. Start with low monitoring volume. This command opens **both named devices**, emits a finite 1 kHz tone, and captures PCM. It does not change OS defaults, install drivers, configure routes, retry, or resume a disconnected stream.

```sh
cargo run --manifest-path src-tauri/Cargo.toml --example run-loopback -- --list-devices
cargo run --manifest-path src-tauri/Cargo.toml --example run-loopback -- --physical --input "EXACT INPUT CPAL NAME" --output "EXACT OUTPUT CPAL NAME" --seconds 1 --timeout-seconds 5
```

Replace both names with the exact names returned for the desired directions. There are no default selections. Ambiguous names and `default`/`sysdefault` aliases are rejected; if the platform does not expose unique selectable endpoints, record that blocker. CPAL names are not portable OS endpoint IDs: record both in the platform evidence. The primitive currently resolves devices by name, so stable-ID hot-plug behavior still requires the manual check.

`--seconds` is required and must be 1–5. `--timeout-seconds` is required, at least `seconds + 2`, and at most 15. The executable has a wall-clock watchdog including discovery/start/cleanup: a stalled backend exits 124 and the OS closes its streams. Normally the tone finishes, capture runs for one further second, and both streams close. No argument starts hardware implicitly; no arguments show help.

JSON `mode: "physical_observation"` reports actual contiguous capture frame/sample counts, playback **enqueued** frame/sample counts, device names and drop counters. Capture callback sizes vary, so its frame count need not equal playback's 50 frames per second. Empty capture, sequence gaps, disconnects, failed sends and detected drops fail. Playback-enqueued counts do not prove rendered samples. Captured silence can have perfectly valid sequence counts. Independently record the target endpoint in the call application/recorder, inspect the spectrum for 1 kHz (±20 Hz), and listen to the spoken phrase before marking the endpoint path PASS. This diagnostic deliberately does not claim acoustic or translation acceptance.

## Acceptance sequence and evidence

Use the OS-specific file for installation and device inventory. Each OS/backend needs its own clean-machine run and evidence. Record the commit, UTC time, OS/build, hardware, installer version/SHA-256/signing identity, audio-server version, call/recorder versions, physical and virtual device IDs, CPAL names, stream rates/channels, logs and recordings. Never record credentials or private conversation audio.

1. Record system default input/output before installation. Install the released application and approved driver package/scripts; retain the visible privilege/approval/restart evidence. Record status before and after. Confirm defaults have not changed silently.
2. Verify M1 is the virtual microphone **VoxBridge Input** and M2 is the virtual speaker **VoxBridge Output**. The application's render side of M1 and capture side of M2 can have different backend names/IDs; record the two sides of each loopback pair explicitly. Do not interchange M1 and M2.
3. In a two-machine test call, select **VoxBridge Input** as this machine's microphone and **VoxBridge Output** as its speaker. Disable test-call noise suppression/automatic gain only if needed for tone measurement, record that choice, and restore it afterward. Keep the remote peer on a different device to avoid feedback.
4. For the A→B path, select this machine's physical microphone as A and the M1 backend as the app's output. Independently record the remote peer's received track. Play the diagnostic into the selected M1 render backend and capture its microphone side; verify the 1 kHz tone reaches the remote recording, with no physical-speaker cross-route.
5. For the B→A path, have the remote peer send a 1 kHz tone for one second, then say **“The bridge test is complete. One, two, three.”** Verify call playback enters M2, the app captures M2's loopback side, and the B→A output reaches only the explicitly selected physical output. Record both endpoint paths. A local diagnostic can exercise each chosen pair, but does not establish the whole call route.
6. With the app's real provider/session routing available and explicit test credentials, say the same phrase in A's source language; verify the expected translation at remote M1, then reverse languages and verify the translation at A's physical output. Record requested language pair, final transcript, received recording, provider request/session identifier and endpoint IDs. Do not expect a translator to preserve a sine tone. **Provider translation is a separate manual check; the harness never contacts it.**
7. While sending audio, unplug the selected USB input/output (repeat for each physical device), then reconnect it. Record the user-visible disconnected/paused state and IDs. For at least five seconds after reconnect, verify output remains silent/stopped and no other endpoint takes over. Try explicit resume with the device still unavailable: it must fail and stay paused. Reconnect, select/health-check the intended endpoints, then explicitly resume: only then may output restart. Repeat for loss/recovery of a virtual endpoint using the platform's documented lifecycle controls.
8. Repeat after application restart and, when required by the installer, OS restart. Remove only the test installation using its documented uninstall operation; record cleanup and defaults. Preserve logs/recordings under a dated evidence folder and link them from the platform file.

The current repository has audio primitives and router pause/resume methods, but no completed driver/session/audio UI wiring for the entire sequence. Windows/macOS signed packages and redistribution licenses are absent from the manifest. Until these prerequisites exist, affected installation, translation, lifecycle UI and reconnect steps stay **PENDING** with a specific blocker. Do not substitute mock endpoints, unit-test output, or synthetic JSON for these steps.

## Configured runtime acceptance boundary

The runtime now has AppState/Tauri commands for inventory, saved profiles, fresh preflight/create, stop, explicit directed resume, durable history and recoverable subscription tickets. Its integration tests use real fixture runtime ports, SQLite and EventBus with injected emitters, including cancellation during Starting/preparation and close while subscription admission is saturated. These tests establish ownership and bounded software behavior; the application workbench/driver UI and real platform/service sequence above remain pending.

Use the IDs from `list_audio_endpoints` in `launch.devices`; M1 render and M2 capture IDs are distinct from the call application's virtual microphone/speaker selection. Do not infer IDs from displayed names or change OS defaults. After reconnect, explicit resume must keep the session's original provider/languages/devices. Profile edits apply to future sessions; same-profile credential refresh may be picked up on resume.

Record native rate units and terminal accounting flags in acceptance evidence. Accepted/discarded input samples use 24,000 Hz; native mono generated/consumed/discarded samples use the separately reported native sample rate. Preserve capture `finalized`, playback `finalized`, and `tail_disposition_known`; an unknown native tail is not a successful drain. Application response/enqueue timing and provider output timestamps do not establish acoustic completion or measured end-to-end latency. Independently measure/record actual received speech and elapsed latency when performing the opt-in real-service test.

For shutdown acceptance, close during Starting and with a busy frontend, and confirm capture/provider/native playback cleanup completes before window destruction. A rejected or failed close must remain visible and retryable. For history recovery, register the returned unique event listener, then fetch session/transcript/diagnostic pages (limits 1–200, exclusive `after` cursor), merge final IDs once and clear stale interims on `resync_needed`. The complete command/event contract is in the [project README](../../README.md).
