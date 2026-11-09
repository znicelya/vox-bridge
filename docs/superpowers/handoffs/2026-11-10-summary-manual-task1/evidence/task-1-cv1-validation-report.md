# CV-1 actual 200,000-row owned recovery measurement

**Actual result: `Timeout` after 10.000312414 seconds; `availability_established=false`.** One completed recovery measurement ran against a coherent history of exactly 200,000 Native summaries and 200,000 matching terminal jobs. Local readiness was `Interrupted` both at return and after actual drain. The provider collaborator recorded zero calls. Actual cleanup joined, returned all direct/recovery slots, released the real process lock, and completed real scheduler shutdown.

The single focused Cargo test exited 0: **1 passed / 0 failed / 0 ignored / 837 filtered out**. Its passing assertion validates the observed timeout/readiness/resource-release result. It does not establish successful recovery availability at the history ceiling. Root retains the CV-1 capacity/latency disposition and subsequent integration decision.

This is the separate validation report required by `task-1-cv1-validation-brief.md`, authored by the original Task1 implementer. The reviewed production source and original author/reviewer reports remain unchanged. All artifacts below are preserved under `task-1-cv1-validation/`, adjacent to this report.

| Binding identity | Value |
| --- | --- |
| Accepted BASE and unchanged HEAD | `73760d609a050eaddcd8002cec572fbc73c65164` |
| Reviewed frozen tree copied with read-only `git archive` | `a907fe5b9eb006cba8533ab57831c18cda485b7c` |
| Independent Spec PASS / Quality Approved / zero-findings report SHA256 | `19b7a677e5a9826121502e60f9447ec7c8df28c7bbf3b9c11454e424506b74fc` |
| Original run079 source manifest SHA256 | `565c56b5908332628c3a96aef6836ddbb92041394d35ce4746ffa988bed43c4e` |
| Original `task-1-report.md`, still 77,047 bytes, SHA256 | `342ee2c8adb81bea0f9feaa19a3e1d47dc906a1326e6ad5d2e3b921d27098937` |
| Protected original worktree/index `.gitignore` SHA256 | `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a` |

The complete frozen repository copy contains 720 tracked files and 12,468,626 bytes. Before adding the fixture, all 261 run079 Rust/migration paths matched their recorded hashes; copied Rust, migrations, Cargo/build/protobuf inputs and test paths matched the reviewed worktree. The copied frozen `.gitignore` is the sole complete-tree comparison difference from the worktree's separately protected, pre-existing staged `.gitignore`; it is not a build-input difference.

Only the COPY was changed: its `src-tauri/src/summary/recovery.rs` received the appended `#[cfg(test)]` child-module wiring, and `src-tauri/src/summary/recovery/cv1_validation.rs` was added. Every original production function byte remains intact, including the real shared ten-second deadline and actual owner/reap authority. Before Cargo, the exact delta was one wiring change plus one fixture file. After Cargo, all 721 existing copy inputs were unchanged; Tauri added its four generated schema JSON files under the COPY's `src-tauri/gen/schemas/`. The copy, generated schemas, exact fixture diff, synthetic database and actual test executable are retained.

The fixture created one real completed synthetic session, then seeded historical SQL rows in one prepared-statement transaction. All Native/job pairs have canonical disjoint UUID identities, versions 1–200,000, durable counter 200,000, kind `summary`, exact summary object/session association, exclusion `summary:{session_id}`, NULL permanent key, and matching `interrupted` status/error. The full admitted LLM profile is validated by the original production validator and matches each Native profile ID/model. Summary/job milliseconds and summary seconds agree, and summary/job creation follows session completion. Each row represents interruption before model execution: `structured_json='null'`, zero raw-response bytes, and no input snapshot, response or coverage claims.

For each job, the witness hashes the exact serialized UTF-8 payload string bound into that job's SQL insert. The post-measurement immutable read-only SQLite audit independently checked all 200,000 persisted tuples, full strict payload/profile/input associations, canonical identities, and exact raw SHA256 witnesses. All payloads are 365 bytes, totaling 73,000,000 bytes. Their version-ordered raw-byte sequence SHA256 matches the seed record: `175db8004666551d203d013b88466b83506149e593cf554958a58f3ebe45a7b0`. There are zero active jobs, deletion fences, Legacy rows, automatic-delivery pending rows, raw responses, coverage rows or foreign-key violations. The database hash remained unchanged by that read-only audit.

The fixture constructs real LocalStore, ConfigStore, test SecretStore, ConfigIo with its actual test runtime, ServiceConfigStore, JobScheduler and SummaryService. It registers the real SummaryHandler before real `jobs.reconcile()`. The scheduler is genuinely fresh, with no worker generation started and no active/unjoined work. First/last samples obtain the real anchored Native proof and `JobReapPort::prove` evidence with `basis=fresh`, rowids 1/200,000, and registry `0f4be36a-f2d0-4119-88ab-e850ca9ab470`. The timed recovery receives that actual scheduler/reap port, rather than a status/bool substitute. Its OperationLease owns an exclusively locked File and an actual owned direct semaphore permit from capacity 16. The LLM collaborator increments its call counter if entered and returns an error; it recorded zero entries. No model, network, native keyring, audio or driver operation was performed.

| Actual observation | Result |
| --- | --- |
| Seeding, excluded from timed recovery | 26.348201866 s |
| Preflight, excluded from timed recovery | 1.027068163 s |
| Timed scope | One actual `reconcile_local_owned` call through `require_epoch(1)` readiness observation |
| Unchanged production deadline / outer observation bound | 10 s / 15 s |
| Actual returned code and elapsed | `Timeout`, 10,000,312,414 ns = 10.000312414 s |
| Readiness at return / after actual drain | `Interrupted` / `Interrupted`; closed both times |
| Provider calls | 0 |
| Sequential return-side observations | Weak strong count 0; direct slots 15; actual recovery slots 0; release record absent; subsequent process-lock probe succeeded |
| Owner resource-release event | 10,000,321,763 ns = 10.000321763 s; File closed, direct permit released, direct 15→16, actual 0 before and after that release |
| Actual drain and real scheduler shutdown | Joined within the 15 s cleanup bound; `Ok` / `Ok` |
| After cleanup | Owner gone; direct slots 16; actual recovery slots 1; ConfigIo slots 2; process lock reacquired and released |
| Complete cleanup stage, excluded from timed recovery | 741,177 ns |
| Successful near-limit availability established | **false** |

The return-side observations were sequential and interleaved with lease destruction. In particular, this measurement does **not** claim a strongly live owner at the returned Timeout. The physical file/direct release event occurred while the actual recovery permit was still held; actual drain then collected completion and observed its release. The recorded Weak/direct/actual observations and release timestamp are preserved exactly, without converting this natural deadline completion into a blocked-worker ownership scenario.

The one measurement used the accepted environment and the standard unoptimized Cargo test profile. Rustc is `1.99.0 (b940084d7 2026-09-28)`, host `x86_64-unknown-linux-gnu`. The complete Cargo invocation lasted 109.547452144 s including compilation and fixture setup; that is separate from the 10.000312414 s recovery interval. This is one synthetic interrupted-history observation with lightweight payloads. It supplies no successful full-scan completion time, release-profile latency estimate or general throughput guarantee. The timeout safety result and closed readiness are concrete; near-ceiling successful availability remains unestablished for this run.

The actual command ran once from `task-1-cv1-validation/source/src-tauri`, with default libtest concurrency and all six accepted overrides unchanged:

```text
CARGO_HOME=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/cargo-writable-20261106
CARGO_TARGET_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/cargo-target-intelligence-20261107
TMPDIR=/dev/shm/voxbridge-intelligence-20261109.q7xaof9u
PKG_CONFIG_PATH=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig
PKG_CONFIG_SYSROOT_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot
LIBRARY_PATH=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu
VOXBRIDGE_CV1_DATA_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-1-cv1-validation/data
VOXBRIDGE_CV1_ARTIFACT_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-1-cv1-validation

cargo test --offline --locked --manifest-path /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-1-cv1-validation/source/src-tauri/Cargo.toml --lib summary::recovery::cv1_validation::cv1_recovery_near_200000_native_history_one_measurement -- --exact --nocapture
```

The first six variables are the accepted Cargo overrides; the last two only locate preserved fixture data/output. The exact argv, cwd, inherited build environment, compiler identity and start time are in [command.json](task-1-cv1-validation/command.json). Cargo ran from 2026-11-09T19:29:36.802144+00:00 through 19:31:26.360697+00:00. The complete [cargo-test.log](task-1-cv1-validation/cargo-test.log) is 5,543 bytes, contains exactly one measurement begin/return/JSON record, and emits no warning. No broad suite, previously passing focused suite or second benchmark ran. Validation uses a separate runner and never writes the original ledger/runner/source snapshots.

| Preserved artifact | SHA256 |
| --- | --- |
| `original-baseline.json` | `c4fb44f4e5ec5198e8c0fbc6ff752be0e9b1ea1b61a1aadf9ab3e476856e2bc8` |
| `frozen-tree.tar` | `c68cc7d5a6e890c83dd7c21688b107b7cf7d6fbb93342687508e90275f610666` |
| `source-before-fixture.json` | `35e032a19efff1798508e21a08030c5feb5efe11e3cd3f4b2303b224c42cea1e` |
| `source-at-cargo-start.json` | `e50e2fd4986763aa975f4215fe7f951f548a24c1884546f3bdf409677b51bbfb` |
| `source-after-cargo.json` | `4088f18cdac59c07b00c4f36201c8f1ff87a9f566b26d28ebe32ccb78ee22b48` |
| Copy recovery with test-only wiring | `72b44b96fbe69c33a4088fd9847240fa4a332a5bc88e12ca794c787aac7bcaf6` |
| Temporary child fixture | `54d3f240ce79e8dd8c8d8abc01f552c9a823038808be76cc7dfad33291d2f076` |
| `fixture.diff` | `a3b276f59fc6a39c3e33136462caaf1f2605d5b583bd2c93f5e725d24bca5957` |
| `command.json` | `1bfb3a15d51814ad30973ed4bf672fab73f2966487cb65a0b8a2a615cc134edb` |
| `cargo-test.log` | `93ec975c48321a2c82909519d1ac7a7c85ed3184a1405f614a95c15c1c2c05cd` |
| `measurement.json` | `ba4073046e72f9e0747c4f04df13a6f2265d3653227253532350fa3a504869b5` |
| `sqlite-fixture-audit.json` | `9cb14e3a07a1e2952e0bce61428378b3a462829f8ee84a5aec151b7f01519599` |
| `data/vox-bridge.sqlite`, 230,780,928 bytes | `fe6b25571dcbbd2ecf982a4380cbb8c94ea222a14d0a63802df77d543553367a` |
| Actual test executable preserved in `bin/`, 560,710,768 bytes | `adabd2ddc13f04412e952b6b5a80097eb9f526b5ffcfe12bca961a286d9098c1` |
| Original strict `post-validation-audit.json` | `9dd0bd1936bd5eb5947aaae55664df259ff635c8786ed8bd3b0c241138d90eea` |
| Original strict `post-validation-audit.log` | `509e368ba5339152abfddad73465c160d64b31414fa478139e0f09761edb0dfe` |
| `final-preservation-audit.json` and its complete raw log | `b157c253e95d09d4aa955d8ae1425f9740b60af8031641d20f6179f09f6e8574` |

The strict supplemental audit exited **1** because the initial 4,615-file baseline included controller `progress.md`, and Root appended its 448-byte CV-1 progress paragraph during validation: 49,554→50,002 bytes. The original prefix is byte-identical, verified against baseline SHA256 `487d76aaa99c7e211efde7a18f51163fbbe6f23881e25ea146c6cc6588caaa04`; the appended version SHA256 is `744959b33638ac321848f0b3fdabb50557b6c1be796c49e211ba712b9f006b4e`. The original strict failed audit/log are preserved unchanged. Root expressly attributed that sole bookkeeping exception in [controller-progress-authorization.txt](task-1-cv1-validation/controller-progress-authorization.txt). The validation author never modified the original progress file.

The separate final preservation-only audit exited **0** under that exact confirmed exception, without rerunning recovery or the 200,000-row SQL audit. It verified **all other 4,614 original files unchanged**, all copied original-plus-fixture input files unchanged, the original production recovery prefix intact, original input/index/ref identities unchanged, and the failed audit/measurement artifacts still byte-identical. Original HEAD, refs, staged entry map and staged diff match the initial baseline; only the pre-existing `.gitignore` remains staged. The immutable original source/build inputs, author report, prior raw logs/source manifests, frozen gate packages/logs, handbacks, independent review and Root handoff documents are preserved. Mutable accepted Cargo cache/target directories are excluded from the immutable baseline. [artifacts.json](task-1-cv1-validation/artifacts.json) indexes the retained validation files and this report; the per-file source manifests retain the full copied-source identities.

**Validation Cargo handback:** actual drain joined, real scheduler shutdown completed, provider calls are zero, and all owned process/direct/actual resources are released. Cargo PID 965188 and observed child PID 966317 are absent; the final preservation audit found no process with the source-copy cwd. Every validation command session has been collected, including the honestly recorded strict-audit exit 1. There are **no pending validation writes, Cargo/test processes or actual workers**. Sole validation Cargo/copy/report ownership is returned to Root. The preserved copy/database/executable/evidence remain in place for Root's CV-1 disposition, exact integration and later Task2/final-review limitation handoff.
