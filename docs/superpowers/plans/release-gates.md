# Vox Bridge release gates

## Summary source acceptance — 2026-11-11

Task6 manual and automatic summaries are independently accepted at `00d54bf` and `d260d59`. The final source tree is `8b82dbdf849918487b59b3eb9e9130c6124ddef5`; the complete review's OO1 HOLD is resolved by the separate scoped Spec PASS / Quality PASS. [Accepted evidence](../handoffs/2026-11-11-summary-automatic-task2/README.md) preserves the original full review and final scoped review.

The unchanged final candidate passed `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets`: 918 passes, zero failures, one intentionally ignored paid provider case. Offline locked library check, desktop binary build and full Rust formatting also passed. Preserved earlier document-worker timeouts remain unattributed, with subsequent unchanged-source focused/default-parallel/full gates passing. The 200000-row CV-1 recovery result remains Timeout with availability=false. Tavily, full tool/history UI and smoke completion remain pending, together with the existing real-platform/signing/paid release gates.

Implementation order: Session Foundation → Platform Audio primitives and acceptance harness → Provider Adapters → Runtime Composition → Workbench and Intelligence. Software implementation can proceed while external acceptance is pending; pending hardware/service evidence still blocks a release claim.

| Gate | Required evidence | Current boundary |
| --- | --- | --- |
| Rust software | `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | Focused/library checks during implementation do not replace the complete phase run. |
| Frontend | `pnpm build`, workbench behavior/smoke tests, Taste app-screen inspection | Workbench/intelligence implementation and smoke checks remain required. |
| Synthetic audio | `cargo run --manifest-path src-tauri/Cargo.toml --example run-loopback -- --synthetic --frames 50` | Exercises queues/resampling/routing; does not prove physical audio or translation. |
| Windows audio | Windows 10 22H2 clean-machine installation, uninstall/repair, M1/M2 route and disconnect/resume evidence | Pending real platform acceptance; see [Windows checklist](../../../tests/platform-audio/windows.md). |
| macOS audio | macOS 13 clean-machine signing/approval/restart and M1/M2 route evidence | Pending real platform acceptance; see [macOS checklist](../../../tests/platform-audio/macos.md). |
| Linux audio | Ubuntu 22.04 LTS on PipeWire and PulseAudio, install/repair/uninstall and both routes | Pending real platform acceptance; see [Linux checklist](../../../tests/platform-audio/linux.md). |
| Driver packages | `pnpm check:audio-packages`, package hashes/licenses and independently verified valid signing/redistribution rights | Windows/macOS artifacts are absent. The automated package check does not itself prove signing. |
| Volcengine | Explicit opt-in ignored integration test with deterministic speech, final source/translation/TTS and successful terminal completion | Fixture tests do not establish live service compatibility. Operator key and an explicit invocation are required. |
| End-to-end sessions | Live input→provider→M1 and simultaneous A→B/M1 + B→A/physical-output, durable text, stop and independent recovery | Requires completed runtime/UI plus real platform/service acceptance. |
| Knowledge/research | Pinned SrvDB revision `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`, document import/search/repair, summary versions, confirmed Tavily requests | Workbench/intelligence phase; preserve no-audio/default-local storage and source citations. |

For physical diagnostics, follow [the acceptance sequence](../../../tests/platform-audio/README.md). Never substitute generated endpoints, synthetic test output or enqueued sample counts for an OS/device PASS.

For live service acceptance, the provider integration README records the ignored test command, speech fixture provenance and cost boundary. Normal test/build commands must not read a real API key or invoke a paid endpoint. Aliyun and Palabra remain explicitly “未联调” for the first release.

Before packaging, record the exact commit, OS/build, artifact hash/signing identity, endpoint IDs, provider test result and dated evidence links. Preserve unresolved gates as pending with their concrete reason; do not report the desktop application as release-ready solely because code compiles.

## Configured runtime integration

The backend now composes saved profiles, real runtime ownership, durable history and bounded subscriptions through AppState and the registered Tauri commands. [Wire fixtures](../../../tests/fixtures/runtime-command-wire.json) pin camelCase command argument envelopes, snake_case nested launch/credential/outcome tags, unchanged PascalCase audio enums and the 80 ms Duration object. Default integration tests cover process-lock-before-recovery, key replacement compensation and cleanup errors, cancelled native deletion ownership, create/delete admission, fixture ports/finals, restart/eviction terminal subscriptions, saturated close admission and pending-create shutdown. Run the normal parallel Cargo gate; serial-only success does not satisfy it.

Window/app shutdown must observe successful owned cleanup before destruction/exit. Failed cleanup keeps admission closed and requests an explicit retry. A later host window explicitly opens a new lifecycle after successful close. Frontends listen to the ticket event then snapshot/page; resync and duplicate final IDs are part of the recovery contract. No raw audio or credential belongs in those records.

The Linux manual gate includes the actual `pactl`, `parec`, and `pacat` versions, bounded discovery JSON, unique `media.name` source-output/sink-input association to selected IDs, odd versus clean capture EOF, successful `pacat` exit after stdin EOF, blocked-write cancellation and explicit resume after loss. Run separately under PulseAudio and pipewire-pulse; all remain pending without dated evidence. Windows/macOS signed installers/native IDs and real minimum-OS device acceptance also remain pending.

Latency diagnostics are application boundary measurements; provider output timestamps are not provider processing time or acoustic end-to-end latency. Playback accounting reports input samples at `input_sample_rate_hz` and native mono samples at their own `sample_rate_hz`; `finalized` and `tail_disposition_known` must remain visible in analysis. Unknown native tails cannot be promoted to played samples. Synthetic success does not establish actual routing or paid AST acceptance. Workbench/settings/history UI, summaries, SrvDB and Tavily remain phase 4.

## Runtime source-review checkpoint

Runtime Composition source review is closed at `92565a9`, including the final independent repair review. Linux graceful playback now drops the owned stdin before waiting for process exit; app close has a 30-second caller observation bound while retaining pending cleanup ownership; native playback-ring overflow triggers only the affected route's same-stream recovery, and live playback accounting is published with an unknown completion boundary.

Verification evidence is tied to its source: the complete phase run at `25c6ff4` passed 400 library, two offline integration and three example tests (one paid test ignored), plus the frontend production build. The final repair at `92565a9` passed the affected audio (93), session (118), app (27) and provider (82) filters, with formatting/diff checks. These overlapping filters are not added as a distinct-test total or represented as another full-suite run. The workbench bridge is undergoing its own recovery review; settings, history, summaries, SrvDB and Tavily remain in progress. All real OS, signing/package and paid-service gates above remain pending.
