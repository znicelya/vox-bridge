# Vox Bridge release gates

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
