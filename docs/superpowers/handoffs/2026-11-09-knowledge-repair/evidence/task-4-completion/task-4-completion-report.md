# Task 4 deletion completion — author handback

Status: **implementation and focused verification complete; author source/Cargo ownership is frozen and returned to Root**. No author Rust edits, Cargo/test processes or pending exec sessions remain. Root may resume its final source manifest/freeze, serialized broad gates, independent repair review, staging and integration. This is an author report, not independent approval or full product acceptance.

Workdir: `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Branch: `codex/vox-bridge-implementation`.
BASE/HEAD remains `b72556091fc533dafb0a336907bf077fec377615`.
Binding inputs are `task-4-completion-brief.md`, this workspace's `progress.md`, `docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md`, `docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md` and the original Task5C I1–I5/M1 review. This report replaces the stale in-progress checkpoint.

## Final behavior

The existing knowledge repair now connects the complete admitted source cohort to fresh Swap, Recovery and Ready publication; Direct Delete certifies every exact duplicate peer for its full target/source fence. Unindexed deletion uses a typed local publication. Before any target row is removed, a source publication rejects a valid differently sourced target peer that its receipt cannot certify. Jobs and work are streamed in bounded pages, including terminal retries and valid zero-work members. Both 202-operation flows complete without changing capacity or deadlines.

The publication consumes the actual private owner-issued `PersistedReceipt`; fresh publication verifies its record count, complete dense staged IDs, full destination mapping coverage and exclusion of deleted targets across every mapping state/version. Already-selected changed survivors roll back. The durable journal uses strict canonical, raw-byte-bounded publication/anchor/header/work/file records. The complete ordered anchor root is streamed; an immutable header replacement or missing/foreign/reordered proof is rejected. Destination seal evolution does not revoke an already committed exclusion.

Original managed final and temporary paths remain independently checkable after metadata/document removal. The proof binds the owner-resolved directory digest and complete native path digest, generated file UUID/extension, document/version and original import-job association. It uses the existing NativePath codec limits: encoded 256 KiB and decoded 32 KiB, including native non-UTF-8 path units. Unknown additional owned/import-job files or path drift reject before cleanup/publication. Actual joined filesystem checks run outside SQLite under the existing operation owner; SQL absence or a mutable cleanup flag cannot hide residual bytes. The generic managed-cleanup consumer uses the same certified-set guard.

Each work page is revalidated and acknowledged by its exact operation/mapping association. Missing mappings require an already-acknowledged peer with identical source, document fence, full count/shared root and original page hash; conflicting mappings reject. Acknowledgments batch at most 200 operations in one SQLite transaction. One replaceable 200-leaf immutable anchor cache belongs to that transaction only; mutable headers/pages/peer state are reread, and the complete publication root is verified first. No whole-cohort Header/Completion collection or cross-transaction cache was added.

Source bytes retire only after complete exclusion and original-file proof plus actual reader/engine closure/removal. SQL cleanup requires exact acknowledged work and an independently bound actual source-removal receipt. The source_removed flag must exactly agree with that receipt, including the reversed false-with-existing-receipt case. Consumption and retirement remain separate: a queued/running or terminal-but-unjoined handler keeps its certificate while independently complete source retirement may proceed.

## Backend-private actual-reap dependency

The authorized extension changes only `intelligence/jobs.rs` and necessary `intelligence/jobs/` files. Its exact interfaces are:

```rust
JobScheduler::reap_port() -> JobReapPort
JobReapPort::prove(&Transaction, Uuid) -> AppResult<Option<JobReapProof>>
```

The port uses separate registry/frame mutexes and never relocks terminal_tx's generation control mutex. Every genuine claim registers UUID, SQLite rowid and bounded raw-payload SHA256 inside SQLite/control before Running is visible. Failed commit removes the exact pending frame and active association. The complete ledger is `issued = safely joined/pruned + retained genuine frames`; no rowid frontier or frame absence alone is authority. A matching retained UUID is checked against original rowid/payload before any absent classification. Fresh, active and exact joined-generation authority are explicit.

Pruning requires a committed claim, permanently closed factory, actual handler-future end and zero actual child scopes. At most 18 live frames are retained, derived from two workers and sixteen child slots. Continuation ownership is reserved before the factory closes. A retained context cannot issue a new child afterward or affect a rearmed generation.

Child tokens release process ownership, then the shared slot, then their per-job completion guard. A deterministic runtime RED exposed that shared-slot drain could wake before the last completion guard decremented its frame count, causing synchronous joined() to abandon generation completion permanently. The repaired shutdown awaits a notified quiescence check after worker/owned-cleanup join and shared-slot drain, without holding SQLite/control/registry/frame locks across await. Notification is registered before reading counts to avoid a lost wake. Uncommitted/open/unfinished frames or ledger mismatch fail closed promptly instead of awaiting impossible handler completion. Only after quiescence does shutdown drop the base process lease, mark the exact generation Joined and publish done. Existing shutdown deadlines are unchanged; prove() stays nonmutating.

Knowledge stores independent `knowledge.delete.v1.consumed.{operation_uuid}` receipts bound to the anchored immutable header and opaque owner-issued reap evidence. Existing knowledge header hashes keep their domain/parser-framed job fingerprint; the consumer separately checks the generic producer's raw SHA256 domain. A mutable consumed flag cannot substitute for the receipt or durable terminal association.

## Settlement and GC

Settled Failed/Cancelled/Interrupted Direct Delete history remains independently anchored retirement authority while its source is active, but does not replay the whole cohort for each already-consumed peer. Candidate selection and its fresh recheck both use `resume_input`, which validates the complete publication root/removal state and the individual header/pages/files/consumption receipt. It skips consumed active-source history and retries consumed publications whose source removal is proved, including local/unindexed publication.

Closed complete-cohort collection validates every member and deletes headers/pages/file pages/receipts/anchors/publication atomically in one SQLite transaction. It cannot leave a partially collected anchor after interruption. A real two-member trigger rollback proves both headers/pages/receipts survive when deletion of the second member aborts GC. After source removal, close/reopen retries collection. The expanded fixture additionally recreates the original certified temporary path: retry rejects and retains the full cohort until those actual bytes are removed, then collects without embeddings. Filesystem checks are retained before destructive cleanup/retirement/GC continuation.

## Final verification and self-review

After the last substantive source/test changes, only the 40 explicit owned Rust paths below were formatted using edition 2024 and `skip_children=true`; the command returned exit 0. The final formatted-source knowledge_ run passed **106 / 106**, exit 0, in 312.25 s; the final scheduler group passed **29 / 29**, exit 0, in 0.89 s. Neither final log has compiler warnings. Filters overlap prior evidence and their counts must not be summed with earlier runs. These are focused lib regressions, not Root's final all-targets/production gates.

The knowledge group includes both 202-member paths, later-page consumer automatic rebuild, two retained same-source deletes, pointer-crash/document-gone continuation, fresh Ready/Swap/Recovery and empty destination, different-source rejection, raw 1024/1025-byte evidence, original-file ownership/metadata loss/real cleanup failure, acknowledged/consumed/source-removal corruption, actual terminal waiter, atomic GC rollback/reopen/residual paths, survivor f32 preservation and zero local recovery embeddings. Existing Tasks1–3/5 regressions remain passing. Scheduler coverage includes every original child-operation/continuation case and genuine pre-entry claim, rollback, absent-frame accounting, older out-of-order claim, live rowid/payload drift, terminal hook under held control, last child clone, entered ConfigIo closure after observer/handler loss, closed factory, pruning/fresh/joined/rearm, drop-order shutdown window and impossible unfinished factory.

Self-review traced publication→journal→file/work cleanup→retirement→terminal consumption→whole-cohort GC and the producer/consumer lock/lifetime boundary. Bounded allocations remain explicit: 200 jobs/anchors/work rows, 100 file descriptors, at most 1,000 work-page hashes and 2,000 file-page hashes per operation under the existing 200,000-record bounds. No engine/worker/queue/child-slot/deadline limit, public API, schema/migration, scheduler status writer, paid/native/audio behavior or Cargo manifest/lockfile was changed by this author repair. No staging or commit was performed.

`task-4-handback-integrity.log` records fresh unchanged HEAD/branch, empty git diff --check, only .gitignore staged, no Cargo manifest/lock diff and no active Cargo process. Both worktree and index .gitignore retain SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.

Owner-known remaining implementation failures: **none**. Root's final all-targets tests, production check/build, whole-project fmt check, independent complete-repair verdict, tracked evidence and integration remain required. Platform/paid-service acceptance remains outside these local synthetic-embedding fixtures.

## Exact final repair source paths

The explicit formatting/source set includes preserved Tasks1–3/5 files where their repaired composition shares knowledge modules. The Task4 protocol/lifetime files and their connected consumers are described above; this list does not claim sole original authorship of inherited repairs. All paths are relative to the worktree.

```text
src-tauri/src/intelligence/jobs.rs
src-tauri/src/intelligence/jobs/child_operations.rs
src-tauri/src/intelligence/jobs/reap.rs
src-tauri/src/intelligence/jobs/tests.rs
src-tauri/src/intelligence/jobs/tests/continuations.rs
src-tauri/src/intelligence/jobs/tests/reap.rs
src-tauri/src/knowledge/copying.rs
src-tauri/src/knowledge/documents/handler.rs
src-tauri/src/knowledge/index.rs
src-tauri/src/knowledge/index/delete_completion.rs
src-tauri/src/knowledge/index/delete_completion/files.rs
src-tauri/src/knowledge/index/delete_completion/journal.rs
src-tauri/src/knowledge/index/delete_completion/publication.rs
src-tauri/src/knowledge/index/delete_completion/retirement.rs
src-tauri/src/knowledge/index/delete_work.rs
src-tauri/src/knowledge/index/owner.rs
src-tauri/src/knowledge/index/publication.rs
src-tauri/src/knowledge/index/repository.rs
src-tauri/src/knowledge/index_tests.rs
src-tauri/src/knowledge/index_tests/publication.rs
src-tauri/src/knowledge/index_tests/review_deletion.rs
src-tauri/src/knowledge/index_tests/review_fixes.rs
src-tauri/src/knowledge/index_tests/review_jobs.rs
src-tauri/src/knowledge/index_tests/review_replacement.rs
src-tauri/src/knowledge/index_tests/review_replacement/authority.rs
src-tauri/src/knowledge/index_tests/review_replacement/cohorts.rs
src-tauri/src/knowledge/index_tests/review_replacement/files.rs
src-tauri/src/knowledge/index_tests/review_replacement/flags.rs
src-tauri/src/knowledge/index_tests/review_replacement/lifetime.rs
src-tauri/src/knowledge/index_tests/review_replacement/settlement.rs
src-tauri/src/knowledge/reconcile.rs
src-tauri/src/knowledge/search.rs
src-tauri/src/knowledge/service.rs
src-tauri/src/knowledge/service/completion.rs
src-tauri/src/knowledge/service/delete.rs
src-tauri/src/knowledge/service/job_proof.rs
src-tauri/src/knowledge/service/recovery.rs
src-tauri/src/knowledge/service/reindex.rs
src-tauri/src/knowledge/service/state.rs
src-tauri/src/knowledge/store.rs
```

## Complete command/result evidence

All logs below are in this report's directory. Their first COMMAND line preserves the exact expanded environment and command, followed by complete output and actual exit. All Cargo commands used login:false, offline/locked lib mode, serialized one-at-a-time execution and `--test-threads=1`. The existing task-local Cargo home/target and ALSA sysroot were preserved. Final invocation pattern:

```bash
env CARGO_HOME="$PWD/.superpowers/toolchain/cargo-writable-20261106" \
 CARGO_TARGET_DIR="$PWD/.superpowers/toolchain/cargo-target-intelligence-20261107" \
 TMPDIR="/dev/shm/voxbridge-intelligence-20261109.q7xaof9u" \
 PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" \
 PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" \
 LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu" \
 cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib FILTER -- --test-threads=1
```

For each row, substitute its exact filter for FILTER; the expanded original command is in the named log. Historical logs are preserved, including compiler/setup failures and the interrupted misleadingly named green run; only actual successful results count as GREEN.

| Log | Exact filter | Actual result | Actual exit |
| --- | --- | --- | --- |
| [task-4-cohort-edge-check.log](task-4-cohort-edge-check.log) | `knowledge::index_tests::review_replacement::cohorts` | 3 passed / 1 failed; 20.51 s | 101 |
| [task-4-connected-cohort-check.log](task-4-connected-cohort-check.log) | `knowledge_review_` | 23 passed / 2 failed; 49.11 s | 101 |
| [task-4-connected-knowledge-regression.log](task-4-connected-knowledge-regression.log) | `knowledge_` | 89 passed / 0 failed; 233.48 s | 0 |
| [task-4-direct-peers-batched-green.log](task-4-direct-peers-batched-green.log) | `knowledge_review_direct_delete_certifies_202_` | No completed test result | 130 |
| [task-4-direct-peers-reap-check.log](task-4-direct-peers-reap-check.log) | `knowledge_review_direct_delete_certifies_202_` | 0 passed / 1 failed; 18.04 s | 101 |
| [task-4-direct-peers-timing-red.log](task-4-direct-peers-timing-red.log) | `knowledge_review_direct_delete_certifies_202_` | 0 passed / 1 failed; 21.39 s | 101 |
| [task-4-file-flags-red.log](task-4-file-flags-red.log) | `knowledge::index_tests::review_replacement::flags::` | 0 passed / 2 failed; 0.99 s | 101 |
| [task-4-file-owner-canonical-red.log](task-4-file-owner-canonical-red.log) | `knowledge::index_tests::review_replacement::files` | 2 passed / 4 failed; 2.27 s | 101 |
| [task-4-file-owner-consumers-green.log](task-4-file-owner-consumers-green.log) | `knowledge::index_tests::review_replacement::files` | 6 passed / 0 failed; 3.04 s | 0 |
| [task-4-file-ownership-consumers-red.log](task-4-file-ownership-consumers-red.log) | `knowledge::index_tests::review_replacement::files::` | 1 passed / 4 failed; 1.77 s | 101 |
| [task-4-file-ownership-red.log](task-4-file-ownership-red.log) | `knowledge::index_tests::review_replacement::files::` | 1 passed / 3 failed; 1.74 s | 101 |
| [task-4-file-paths-and-flags-red.log](task-4-file-paths-and-flags-red.log) | `knowledge::index_tests::review_replacement::flags::` | 0 passed / 3 failed; 1.93 s | 101 |
| [task-4-final-knowledge-focused.log](task-4-final-knowledge-focused.log) | `knowledge_` | 106 passed / 0 failed; 312.25 s | 0 |
| [task-4-final-scheduler-focused.log](task-4-final-scheduler-focused.log) | `intelligence::jobs::tests::` | 29 passed / 0 failed; 0.89 s | 0 |
| [task-4-full-path-connected-check.log](task-4-full-path-connected-check.log) | `knowledge::index_tests::review_replacement::files` | 4 passed / 2 failed; 2.91 s | 101 |
| [task-4-initial-red.log](task-4-initial-red.log) | `knowledge_review_` | No completed test result | 101 |
| [task-4-native-path-codec-green.log](task-4-native-path-codec-green.log) | `knowledge_review_native_imported_managed_path_deletes_locally` | 1 passed / 0 failed; 0.38 s | 0 |
| [task-4-native-path-codec-red.log](task-4-native-path-codec-red.log) | `knowledge_review_native_imported_managed_path_deletes_locally` | 0 passed / 1 failed; 0.10 s | 101 |
| [task-4-protocol-green-check.log](task-4-protocol-green-check.log) | `knowledge_review_` | 28 passed / 0 failed; 73.09 s | 0 |
| [task-4-reap-api-compile-red.log](task-4-reap-api-compile-red.log) | `intelligence_reap_` | No completed test result | 101 |
| [task-4-reap-producer-check.log](task-4-reap-producer-check.log) | `intelligence_reap_` | 4 passed / 1 failed; 0.13 s | 101 |
| [task-4-reap-producer-green.log](task-4-reap-producer-green.log) | `intelligence_reap_` | 7 passed / 0 failed; 0.17 s | 0 |
| [task-4-reap-quiescence-green.log](task-4-reap-quiescence-green.log) | `intelligence_reap_` | 9 passed / 0 failed; 0.26 s | 0 |
| [task-4-reap-shutdown-window-red.log](task-4-reap-shutdown-window-red.log) | `intelligence_reap_shutdown_waits_` | 0 passed / 1 failed; 0.02 s | 101 |
| [task-4-replacement-connected-green.log](task-4-replacement-connected-green.log) | `knowledge::index_tests::review_replacement` | 24 passed / 0 failed; 133.06 s | 0 |
| [task-4-replacement-runtime-red.log](task-4-replacement-runtime-red.log) | `knowledge_review_` | 20 passed / 5 failed; 17.20 s | 101 |
| [task-4-retention-authority-green.log](task-4-retention-authority-green.log) | `knowledge_review_retention_authority_` | 2 passed / 0 failed; 2.12 s | 0 |
| [task-4-retention-authority-red.log](task-4-retention-authority-red.log) | `knowledge_review_retention_authority_` | 0 passed / 2 failed; 1.58 s | 101 |
| [task-4-scheduler-focused-green.log](task-4-scheduler-focused-green.log) | `intelligence::jobs::tests::` | 29 passed / 0 failed; 0.89 s | 0 |
| [task-4-settlement-gc-edge-red.log](task-4-settlement-gc-edge-red.log) | `knowledge_review_settlement_` | 4 passed / 1 failed; 5.14 s | 101 |
| [task-4-settlement-gc-retry-green.log](task-4-settlement-gc-retry-green.log) | `knowledge_review_settlement_` | 5 passed / 0 failed; 5.39 s | 0 |
| [task-4-settlement-gc-retry-red.log](task-4-settlement-gc-retry-red.log) | `knowledge_review_settlement_atomic_` | 0 passed / 1 failed; 1.28 s | 101 |
| [task-4-settlement-no-replay-green.log](task-4-settlement-no-replay-green.log) | `knowledge_review_settlement_` | 3 passed / 0 failed; 2.95 s | 0 |
| [task-4-settlement-no-replay-red.log](task-4-settlement-no-replay-red.log) | `knowledge_review_settlement_` | 2 passed / 1 failed; 3.01 s | 101 |
| [task-4-source-removal-authority-red.log](task-4-source-removal-authority-red.log) | `knowledge_review_retention_authority_source_removed_flag` | 0 passed / 1 failed; 1.19 s | 101 |
| [task-4-terminal-reap-waiter-green.log](task-4-terminal-reap-waiter-green.log) | `knowledge_review_terminal_reap_waiter_` | 0 passed / 1 failed; 6.04 s | 101 |
| [task-4-terminal-reap-waiter-red.log](task-4-terminal-reap-waiter-red.log) | `knowledge_review_terminal_reap_waiter_` | 0 passed / 1 failed; 6.23 s | 101 |
| [task-4-terminal-reap-waiter-trace.log](task-4-terminal-reap-waiter-trace.log) | `knowledge_review_terminal_reap_waiter_` | 0 passed / 1 failed; 6.06 s | 101 |
| [task-4-terminal-reap-waiter-verified.log](task-4-terminal-reap-waiter-verified.log) | `knowledge_review_terminal_reap_waiter_` | 1 passed / 0 failed; 6.24 s | 0 |

Evidence interpretation:

- task-4-initial-red.log is missing visibility/wiring compilation, not a new behavioral defect. task-4-reap-api-compile-red.log is seven missing-API compiler errors, not behavioral RED.
- task-4-reap-producer-check.log has four passing tests and one invalid Manual Import initial_failure fixture setup. Normal admission plus controlled terminal SQL corrected that setup; the subsequent seven-case producer run passes.
- task-4-retention-authority-red.log combines one genuine differently sourced target-peer defect with a source_removed setup-helper failure. The separately corrected task-4-source-removal-authority-red.log is the actual flag defect; the two-case authority GREEN verifies both repairs.
- task-4-terminal-reap-waiter-red.log is the original actual terminal-but-unjoined waiter defect. The misleading task-4-terminal-reap-waiter-green.log and trace run still fail because generic raw SHA256 was compared against knowledge's framed hash domain. task-4-terminal-reap-waiter-verified.log is the original real waiter passing after the domain correction.
- task-4-direct-peers-reap-check.log retains the unchanged 15 s handler observation failure. The timing RED measured validation 0.33 s, file phase 5.23 s and acknowledgments ending at 16.69 s. task-4-direct-peers-batched-green.log was intentionally interrupted after discovering multiplicative settled-history replay; exit 130 and no completed result mean it is not GREEN. The no-replay behavioral RED/GREEN and completed 24-case replacement/final 106-case runs supersede it.
- task-4-settlement-gc-edge-red.log fails because reversed source_removed=false with an existing independent receipt was accepted. After exact flag/receipt agreement, task-4-settlement-gc-retry-red.log fails at the expected fully consumed closed-cohort retry assertion. Its five-case GREEN proves the candidate/recheck repair; the final knowledge run also includes the later actual residual-path extension.
- task-4-reap-shutdown-window-red.log fails at the expected slot/completion publication window. The quiescence GREEN then passes nine cases; the complete scheduler runs include the subsequently added unfinished-factory case, all ten reap cases and all original scheduler regressions.

Additional exact non-Cargo evidence is in [task-4-owned-rustfmt.log](task-4-owned-rustfmt.log) and [task-4-handback-integrity.log](task-4-handback-integrity.log). The original .superpowers/sdd/2026-11-04-workbench-intelligence Task5C repair RED logs remain preserved; they were not rerun as unchanged-source gates.

## Explicit handback

Author source edits and focused Cargo are frozen. All focused sessions have completed; no pending source edit, running Cargo/test command or exec session remains. Root now owns the source/Cargo baton and may perform final manifest/freeze, broad verification and independent complete-repair review. This author has not staged, committed, integrated, requested independent approval or claimed product acceptance.
