# Vox Bridge

Vox Bridge is a Tauri desktop translation runtime with a React/TypeScript frontend. Live mode sends translated speech to **VoxBridge Input (M1)**. Call mode runs independent A→B/M1 and B→A/physical-output directions, capturing incoming call audio from **VoxBridge Output (M2)**. The app does not change system defaults. Workbench/settings/history UI and intelligence features remain phase 4.

```sh
pnpm install
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Default Rust tests use injected audio/process ports, fixture providers, SQLite and memory key backends. They do not contact a paid service, open hardware or access a native keyring. The backend constructor creates local storage, acquires an exclusive `app.lock` before SQLite migration/recovery, and marks interrupted sessions failed. A second process using the directory fails before touching the database. Native audio inventory is requested explicitly; capture and network start only on an explicit session launch.

## Runtime commands and wire values

Tauri top-level arguments use its default camelCase: `configId`, `sessionId`, `subscriberId`, `subscriptionId`. Nested DTO fields use their Rust serde names, generally snake_case. `SaveProviderRequest` is passed under `request`; `SessionLaunch` under `launch`. [Checked-in JSON fixtures](tests/fixtures/runtime-command-wire.json) pin the mixed conventions.

| Commands | Arguments | Result |
| --- | --- | --- |
| `list_audio_endpoints`, `audio_driver_status` | none | inventory / driver status |
| `list_provider_configs` | none | `ProviderConfigView[]` |
| `save_provider_config` | `request: {config, credential}` | saved public config + `credential_configured` |
| `delete_provider_config`, `provider_capabilities` | `configId` | empty / safe capabilities |
| `validate_session_launch`, `create_session` | `launch` | context/formats / Starting session |
| `stop_session` | `sessionId` | durable terminal session after cleanup |
| `resume_session_direction` | `sessionId`, `direction` | empty |
| `get_session`, `delete_session` | `sessionId` | optional session / empty |
| `list_sessions` | optional `limit`, `cursor` | sessions, default 50, maximum 200 |
| `get_session_transcript`, `get_session_diagnostics` | `sessionId`, optional `after`, `limit` | `{rows, next_cursor}` |
| `subscribe_session` | `sessionId`, `subscriberId` | `{id, event_name}` ticket |
| `unsubscribe_session` | `subscriptionId` | empty, idempotent |
| `export_session` | `sessionId`, `format`, `destination` | exported path |

`credential` is tagged by `action`: `{"action":"keep"}`, `{"action":"clear"}`, or `{"action":"replace","api_key":"…"}`. Replace input is 1–4096 visible ASCII bytes without whitespace. Only the official Volcengine endpoint is accepted for Volc profiles. Credentials are encrypted in a separate file, with a native-keyring-protected data key; references are backend-derived from that profile's UUID. Keep preserves the persisted same-profile reference; unreferenced cleanup orphans are not configured. Public views require both a normalized reference and an encrypted entry. Credential inputs have no Debug/Serialize implementation and never enter events, history, exports or ordinary logs.

Replace saves an encrypted before-image, writes the new key, then saves config; a failed config write attempts one restoration. Clear saves the unreferenced config before key deletion; delete removes config before key cleanup. Cleanup/compensation failures return explicit partial-failure errors. These are ordered cross-file operations, not crash-atomic transactions. File/keyring work runs through a process-wide two-slot worker with a 10-second caller budget. Entered native work retains locks and ownership after caller cancellation/timeout until it returns.

A launch contains saved `provider_config_id`, validated session mode/direction/languages, and explicit device IDs from the latest inventory. Live devices are `{mode:"live", input_id, m1_output_id}`; call devices are `{mode:"call", a_input_id, m2_input_id, m1_output_id, physical_output_id}`. The app opens M1's render side and M2's capture side. Validation opens no capture/provider stream; create repeats fresh preflight. One active/reserved session is admitted. Its profile cannot be deleted until cleanup, and config edits apply to future sessions. Device recovery needs explicit `resume_session_direction`; resume retains the same profile/languages/devices and may reload that profile's credential.

Existing audio serialization is preserved: `DriverStatus`, `AudioDirection`, `EndpointRole`, `AudioBackendKind` use PascalCase strings or externally tagged payloads such as `{"NeedsApproval":"…"}`. Capabilities report `native_chunk_duration` as serde Duration: `{"secs":0,"nanos":80000000}` for 80 ms. A future frontend bridge should normalize this object; the backend does not silently change its representation. Runtime outcomes use `{"status":"completed"}` / `{"status":"failed","codes":[…]}`.

## Recoverable events and shutdown

Subscriptions are keyed by `(subscriberId, sessionId)`, limited to 16, with 1–64-character opaque subscriber tokens. Each replacement gets a new UUID/event name and cancels/joins its previous forwarder. Each emit is bounded to one second. Event `type` tags include `status`, `transcript`, `diagnostic`, `audio_state`, and `resync_needed`. Lag emits `resync_needed` and continues. Terminal history, including after restart or replay eviction, produces a fresh durable terminal Status ticket and closes.

After receiving a ticket, register its listener, then fetch `get_session` and both history pages to recover any event emitted before listener registration. Page limits are 1–200 (default 100); `after` is a nonnegative exclusive integer row cursor. Continue from `next_cursor` until null. Session-list cursors use the final row's `started_at:id`. De-duplicate final transcript rows by ID when merging pages/events; interim text is memory-only and may be cleared after resync. Source/translation kind, direction, sequence, supplied provider times and correlation remain distinct; do not invent sentence pairing.

Session deletion joins subscription cleanup. Window close/app exit prevent native destruction/exit until the owned close operation stops the authoritative runtime and observes the registry barrier, even if the mutation queue was full. Closing cancels pending creation. Old work is rejected by its lifecycle generation; admission stays closed after success. A host creating a later window must explicitly call `AppState::open_window()` after successful cleanup; this is not a frontend command. Each close caller observes at most 30 seconds, including admission gates. A timed-out caller leaves the single cleanup operation and native transaction ownership retained. A failed close remains closed and emits `voxbridge://shutdown-error`; retry closing to observe retained cleanup or retry a settled failure. Subscriptions do not own audio and cannot keep it running.

## Accounting and acceptance limits

Input is 16 kHz PCM16 mono in target 80 ms / 1,280-sample packets; internal output is 24 kHz PCM16 mono. Provider timestamps and application queue timing do not measure acoustic playback completion or end-to-end translation latency. `capture_to_send_ms` measures capture age through provider-queue admission using checked wall-clock `SystemTime`; it can be unknown after the clock moves backward. Its endpoint is queue admission, not socket-write completion or a network measurement. `response_to_enqueue_ms` uses local monotonic `Instant` from receipt of a provider response to local playback enqueue. Unavailable callback/provider timings remain null. Phase 4 frontend labels must explicitly preserve these clock and admission meanings.

Playback `accepted_for_processing_samples` and `discarded_before_conversion_samples` use `input_sample_rate_hz` (24,000 Hz). Native generated/consumed/discarded mono samples use their separately reported `sample_rate_hz`; do not subtract across rates. `finalized` means the counters are a terminal observation; `tail_disposition_known: false` means a native tail is unknown and cannot be counted as played. Capture loss distinguishes dropped native blocks from normalized frames and has its own `finalized` flag. Callback consumption and Pulse server drain are different boundaries; neither proves acoustic output.

Synthetic tests are implementation evidence. Signed Windows 10 22H2/macOS 13 packages, actual native IDs, Ubuntu 22.04 PulseAudio/pipewire-pulse utilities and stream drain, physical M1/M2 loopback, and real Volc quality/latency remain pending. See [release gates](docs/superpowers/plans/release-gates.md) and [platform acceptance](tests/platform-audio/README.md). Automatic summaries, SrvDB and Tavily remain phase 4.
