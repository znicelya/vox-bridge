# OO1 focused source-copy investigation

**Measured conclusion: the required two-worker executor has a valid availability counterexample.** In run-002, both actual scheduler workers reached the real SQLite acquisition while an independent OS thread held SQLite. The 25 ms heartbeat made no progress, the primed 100 ms timer did not complete, capture remained accepting, and ordinary stop/App close first polled only after the external release about 750 ms later. Both cases had zero queued jobs. The fixture validated its actual entry, external release and cleanup evidence before failing the intended availability assertion.

The supported mechanism is the original [synchronous claim transaction](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs.rs:775) calling [SQLite's synchronous mutex acquisition](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/storage/sqlite.rs:149) on the async worker before stopped/closed/empty-queue checks. Once both worker threads wait there, other queued async work and timer polling cannot run on that executor. Required ownership stayed retained; durable completion and genuine joins succeeded after release.

**Scope and identities.** This investigation follows [the OO1 brief](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/oo1-investigation-brief.md:1). The original implementation worktree remains `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`, HEAD `85eda3a8bca3ddabd72edad0763b4d5776166f9a`. The controlled `source/` copy was made by git archive from exact immutable tree `d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`, selecting src-tauri, tests and package.json. The archive was 3,983,360 bytes with SHA256 `17d2dc9ef9c55e6b2b7c30ef34bdb812bec1c950deb43902a85219695fad1198`; its 346 original files are individually recorded in `source-copy-baseline.json`.

The accepted complete Task6 review remains immutable at SHA256 `f1aea3cfe4e36741df4828936cadccc8cce9f7f3fd9a1fdd3a9c4c2727a1977b`. This deliverable is the focused OO1 measurement and ownership handback requested by Root.

**Fixture construction.** The fixture uses the actual AppState, LocalStore, actual two-worker JobScheduler, a two-worker Tokio runtime, and the existing injected offline provider/audio collaborators. The actual running session and published intelligence generation are established through normal App methods. A real SQL count confirms zero queued jobs before each hold.

Copy-only cfg(test) instrumentation is instance-scoped. The original claim transaction has a non-waiting callback immediately before acquisition and a callback inside the actual acquired transaction, before the original control/queue checks. Both worker-thread IDs must appear before release, and their actual transaction-body entries must occur after release. No SQLite implementation, admission capability, job proof, native parent, completion source or actual join is replaced by the probe.

Run-002 also uses a cfg(test) rendezvous **before** claim entry: the two real workers first receive original scheduler tokens and park on a watch. The independent OS holder opens that rendezvous with SQLite already held, making both genuine continuations eligible to reach the original synchronous claim. The original acquisition itself contains no added await or yield. This is a controlled arrangement of simultaneous claim entry, rather than an estimate of its production scheduling frequency. Each case records two distinct actual task IDs and the two worker threads `ThreadId(3)` / `ThreadId(4)`.

The SQL holder uses `std::time::Instant` and an independent OS-thread wait. Its release never depends on a Tokio timer or an action/close result. The 750 ms interval is the deliberate held-SQL observation window. There are no arbitrary startup sleeps or worker-count changes. Capture probing uses the existing backend's actual queue forwarding, which performs one synchronous queue push, without starting another executor.

**Runs and immutable raw output.** Exactly the same one focused library fixture was run twice. Each run executes separate ordinary-stop and App-close cases; no full suite, CV-1, paid, keyring or physical native/audio acceptance was invoked.

| Run | Entry validity and outcome | Rust result | Cargo exit | Libtest / total time |
| --- | --- | --- | ---: | --- |
| run-001 | Only one observed claim per case; timer already completed while waiting for the second entry. Fixture validity guard failed. Retained as insufficient entry evidence. | 0 passed; 1 failed; 0 ignored; 893 filtered | 101 | 5.74 s / 143.941583 s |
| run-002 | Both real claim entries established in both cases; all release/cleanup checks passed; intended availability assertion failed. Valid RED. | 0 passed; 1 failed; 0 ignored; 893 filtered | 101 | 1.70 s / 58.204346 s |

Both full logs were read. Neither compilation emitted a warning. Both Cargo waits completed with no harness timeout, forced group cleanup, or remaining process-group member. The raw commands, outputs and run results are preserved without rewriting.

| Evidence identity | SHA256 |
| --- | --- |
| run-001 complete log, 10,468 bytes | `99187aab4ed16895a30322ef7a44376f05de6b96fd84099d07424e8c201afaca` |
| run-001 command/environment | `d5eca869d049e4bbf4a2b882cb427e47ebe93453ae965ddbf28ba614e9b34c9d` |
| run-001 result | `b47db0f439d1c4ad71f6e241ca6e01b2be2bb0794b4150b472230bd6b2f086dd` |
| fixture-001 diff, 23,109 bytes / 580 lines | `dcfee5b539b19de92ded485bab88d816e3a33f13232e7b610adbf9ba292abe3c` |
| run-002 complete log, 11,372 bytes | `9942d87bc50bb2dcb11d5132c1bc03e12c8f5b06b8ae4980126c481786e04fc3` |
| run-002 command/environment | `4d2b3e07597c8a51771d85f5bfffb26060de260593f68876523a226eb74753cf` |
| run-002 result | `b75ad528fadb1055f3aaa290d42c27ee85397c5dcd973c227a6e188ae0050aba` |
| fixture-002 diff, 26,443 bytes / 655 lines | `f3f56cb45fa4c6ee90bfdfbde07649a0ffac8cfaa9f65d35bdd499d35a55e026` |

The exact argv and verbatim inherited environment overrides are in [run-001-command.json](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/run-001-command.json:1) and [run-002-command.json](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/run-002-command.json:1). They use the environment from the original final verification record: existing task-local CARGO_HOME/CARGO_TARGET_DIR/TMPDIR and ALSA overrides. Task-owned TMPDIR probes succeeded and were removed. Cargo used `test --offline --locked --lib`, the copy's manifest, exact fixture `app::intelligence::tests::oo1::actual_scheduler_keeps_timer_and_capture_stop_pollable_while_sqlite_claims_wait`, and `-- --exact --nocapture`.

**Decisive run-002 monotonic timeline.** Values below are milliseconds relative to that case's actual SQLite hold entry. Complete microsecond events and literal observations are in [run-002-observations.json](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/run-002-observations.json:1); derived checks are in `oo1-observation-analysis.json`.

| Observed event | Ordinary stop | App close |
| --- | ---: | ---: |
| First / second actual claim acquisition entry | 0.061 / 0.066 | 0.038 / 0.082 |
| Independent OS-thread release requested | 750.217 | 750.310 |
| Holder's actual transaction returned | 750.274 | 750.361 |
| First actual claim transaction body entered | 750.322 | 750.409 |
| Requested action first polled | 750.631 | 750.769 |
| Primed 100 ms timer completion polled | 750.961 | 751.384 |
| Terminal Completed / stopped capture / reopen fence observed with original native parent still held | 783.549 | 781.451 |
| Original native parent release sent | 783.557 | 781.457 |
| Actual App close and scheduler joins returned | 784.253 | 782.039 |
| All fixture tasks joined | 784.304 | 782.085 |

The heartbeat count remained **1 → 1** between confirmed dual entry and release in both cases. The timer was incomplete at both entry confirmation and release. Measured from its initial poll, the nominal 100 ms timer completed after **751.095 ms** and **751.472 ms**, approximately 651 ms late. Ordinary-stop request-to-first-poll was **750.536 ms**; App-close request-to-first-poll was **750.661 ms**.

The capture queue accepted the probe immediately before external release, so stop had not closed capture at that observation. After release, the actual stop/close path closed capture and durable SQL status was observed as `completed`. Gate closure is bounded by these actual before/after observations; an exact internal stop_capture timestamp was not instrumented. In the App-close case, the actual App admission fence was still open immediately before release, and the close future's first poll occurred afterward. The 30 ms close observer timeout begins when close first polls; it is distinct from the separately primed 100 ms timer and the earlier action request.

**Actual ownership and cleanup.** Each case retains the original registered native parent through the existing `delay_actual_join` collaborator. The positive native-finished notification is accepted only as `Ok(Ok(()))`, after its original close/playback workers have joined while the parent remains held. The real close observer then times out, and both `open_window` and `reopen_window` reject while that parent is still retained. Releasing its original oneshot permits actual parent completion; the subsequent real App close returns success and the actual JobScheduler shutdown and summary-attempt drain return success. Only then are the heartbeat and timer joined and the App dropped.

Thus both cases joined the independent SQL holder, requested action task, original native close/playback workers and retained parent, App close sources, both actual scheduler workers, summary attempts, heartbeat and timer before the final assertion. Successful durable terminal status or a close timeout was never substituted for actual ownership completion. The fixture's final failure happened after these joins. Cargo process groups **1647039** and **1655531** were empty after their completed waits; neither required forced termination.

**Evidence timing clarification.** The first build generated four schema JSON files in the copy, separate from the four Rust test/instrumentation source files. These compiler outputs are archived in `cargo-generated-copy-schemas/` and recorded in fixture-002 metadata. The second runner started with the prepared fixture before the metadata archive was completed; the numeric fixture-002 capture timestamp is after run-002 finished. Its sentence saying “captured during compilation” is inaccurate. The append-only [timing note](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/run-002-metadata-timing-note.md:1) corrects that wording. Fixture-002 and `run-002-source-input-identities.json` are post-run snapshots, rather than a separately persisted pre-launch hash snapshot. The four test-source files remained unchanged from their pre-launch preparation through completion and capture. Raw run records and source/diff artifacts remain preserved.

**Calibration and remaining boundary.** This result establishes executor starvation under the required configured two-worker, simultaneous dual-claim condition, including an empty job queue. It does not measure how often that scheduling condition occurs. The host reported available_parallelism=12; no production-default twelve-worker or hardware run was performed. Locked Tauri 2.12.5's default_runtime uses `TokioRuntime::new`, and locked Tokio is 1.54.1. Availability on larger pools, specific deployment worker counts and physical native/platform behavior remain unmeasured. The pre-entry rendezvous and the insufficient run-001 are explicit limits on frequency/generalization claims.

Root owns the minimal original-source fix scope and subsequent validation. The original seven gate records and accepted CV-1 evidence remain unchanged; no complete Task6 review or ceiling recovery measurement was repeated.

**Protection and handback.** The baseline and final preservation audit record all 422 original public input hashes, original index bytes and entries, HEAD/refs, protected `.gitignore`, accepted review and the 37 assigned frozen input identities. The source-copy artifacts are the exclusive write scope of this investigation; the original implementation and sibling UI worktrees were not edited. No agents, source fix, stage/commit/ref mutation, network/paid/keyring/hardware action or host repair was performed.

The report, analysis and preservation audit complete this investigation. **There are no pending investigator writes or processes. Exclusive focused Cargo and source-copy ownership are returned to Root.** Original-source authority remains with Root.
