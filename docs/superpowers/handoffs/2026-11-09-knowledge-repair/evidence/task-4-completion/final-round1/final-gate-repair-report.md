# Final-gate repair and frozen handback

The three reported failures have traced repairs and focused GREEN evidence. The final related runs recorded **106 knowledge tests passed / 0 failed / exit 0** and **5 document tests passed / 0 failed / exit 0**, using default test concurrency. Source and focused Cargo work are frozen and handed back to Root. No author source edit, Cargo/test process or exec session remains pending.

This continues the existing Task5C repair under [final-gate-repair-brief.md](final-gate-repair-brief.md). Root owns the next fresh source freeze, default-parallel all-targets gate, broad formatting, production/desktop checks, independent review and integration. Those gates remain pending; this report does not establish independent approval or full product acceptance.

Worktree: `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Branch: `codex/vox-bridge-implementation`.
HEAD/BASE remains `b72556091fc533dafb0a336907bf077fec377615`.
The original frozen tree remains `3afb692d013bbc8e4830b130f504d18fa8f63e33` and describes the pre-repair source.

## Original failures and investigation

The preserved [final-all-targets.log](final-all-targets.log) records **745 passed / 3 failed / 0 ignored / exit 101**, with default concurrency and no compiler warnings. It is unchanged and remains failure evidence.

### Ordinary document cleanup

Fixture: `documents_completion_cleanup_error_retains_intent_and_marks_repair_without_following_symlink`.

The hypothesis recorded before the fix was that `guard_managed_cleanup` resolved the managed directory before establishing whether any completion certificate existed. Its error conversion would short-circuit ordinary import cleanup and prevent that path's durable repair marking.

The unchanged fixture reproduced RED. Added diagnostic assertion text then captured the exact state: `Err(Intelligence(IndexRepairRequired))`, managed row `delete_pending`, and document `Failed/Some(Cancelled)`. Tracing showed `ManagedDirectory::resolve` mapped the copying-layer `Storage` error to `IndexRepairRequired` and returned before `cleanup_candidate` could record `RepairRequired/Storage`.

The sole production change moves directory resolution after a nonempty `journal::scan_operations` result. Ordinary cleanup without a certificate now reaches its existing cleanup and durable repair path. Certificate-bearing cleanup still resolves and validates the directory, metadata set, header, descriptor paths and filesystem state. The exact original assertions remain: public `Storage`, retained `delete_pending`, document `RepairRequired/Storage`, and unchanged foreign bytes after symlink rejection. Only diagnostic assertion text changed in this fixture. Both related document runs recorded all five tests passing.

### Conflicting first-index dimensions

Fixture: `knowledge_binding_concurrent_first_index_conflicting_actual_dimensions_has_one_winner`.

The hypothesis recorded before the fix was that two schedulers sharing one unpartitioned store could claim both competing jobs into one serial Core. The fixture's expectation that both independent embedding barriers would enter was therefore unsupported by job admission alone.

Default-parallel diagnostic RED confirmed both jobs were `Running`, with first embedding calls **0** and second embedding calls **1**. The second scheduler emitted `Running` for both job IDs. Both jobs had genuinely been claimed, but the second waited for that Core's serial lock instead of entering the other fake transport. The fake `Axes` implementation does not enforce profile `timeout_ms`; the production HTTP transport applies that timeout, so the fixture's 1000-ms profile was not this failure's cause.

The fixture now starts the first scheduler, admits the first job and awaits its actual embedding entry. It then admits a spare job and observes `Running`, occupying the first scheduler's second genuine worker while that worker waits on the first Core's lock. Only then does it start the second scheduler and admit the competing job. It awaits the second actual embedding entry, cancels the spare before releasing either embedding barrier, and observes the second job through its actual scheduler. The entry observation retains a finite 60-second test bound.

The fixture verifies one embedding call in each Core, the spare's `Cancelled` status, exactly one successful competing job and exactly `(active, ready) == (1, 1)`. There is no production scheduler change, affinity API, worker-limit change or serial test override. Its controlled focused run passed, followed by the default-parallel related knowledge run.

### Complete 202-member Direct Delete cohort

Fixture: `knowledge_review_direct_delete_certifies_202_exact_terminal_peers_and_retains_history`.

The hypothesis recorded before the observer change was that the shared harness's 15-second yield-only SQLite polling loop added observation churn while complete bounded journal work continued. Actual terminal/domain completion and read counts were measured before changing the harness.

The first unchanged default-parallel knowledge subset passed this cohort while reproducing the binding failure. A second default-parallel diagnostic subset reproduced both failures: the cohort observer expired at **15.230730499 seconds after 3,520 SQL reads**; acquiring the actual serial owner then showed completion at **16.039421677 seconds**, with the durable job `Succeeded` and no error. The later catch-unwind unwrap was consequential. No isolated run of the original cohort was performed, and no such RED result is claimed.

The shared harness now subscribes before its first authoritative job read, awaits scheduler notifications instead of yield-only polling, and rereads durable status after notifications or `Lagged`. It performs a final durable resync if the stream closes or the finite **60-second test observation bound** expires. Notifications only wake a read; they never substitute for durable status. The helper can also observe the correct scheduler and await `Running`, supporting the binding fixture above.

The cohort additionally asserts that the actual scheduler `reap_port().prove(tx, job.id)` returns a proof after terminal observation. That proves the completed handler and every issued child scope, rather than relying solely on a status observation. All original assertions remain: complete **202 operations in two batches**, exact shared publication/input binding, work acknowledgments and consumption for every member, retained Failed/Cancelled/Interrupted history on the active source, later source retirement and certificate GC, exact survivor f32 bytes, unchanged embedding count and survivor search.

The repair reduces observation reads; it does not eliminate complete journal/cleanup costs. Post-fix isolated terminal observation took **14.482726553 seconds with 2 SQL reads**, and the whole fixture took **56.62 seconds**. In the final default-parallel knowledge run, terminal observation took **16.073449931 seconds with 2 SQL reads**. Setup, certificate checks and subsequent rebuild remain part of the whole-fixture runtime; they were not separately profiled. Production deadlines and capacities are unchanged.

## Exact source delta

The following five files differ from the original frozen Git blobs. Only the first path is production code.

| Path | Repair |
| --- | --- |
| `src-tauri/src/knowledge/index/delete_completion/files.rs` | Resolve the managed directory only after discovering certificate-bearing operations. |
| `src-tauri/src/app/documents/tests.rs` | Add exact outcome/pending/document diagnostic assertion text; preserve all expectations. |
| `src-tauri/src/knowledge/index_tests.rs` | Replace yield-only status polling with bounded notification-driven durable observation; support the actual scheduler and `Running`. |
| `src-tauri/src/knowledge/index_tests/binding.rs` | Arrange genuine worker occupancy before second-scheduler admission; verify both Core entries and cancelled spare. |
| `src-tauri/src/knowledge/index_tests/review_replacement/cohorts.rs` | Retain timing/actual-serial diagnostics and add the actual reap-proof assertion. |

Exactly three of the original 40 frozen sources differ: `files.rs`, the shared harness and `cohorts.rs`. The other **37** still match the original hashes. The documents and binding fixtures are the two additional test paths explicitly authorized by the repair brief. No test was added or ignored; the library suite remains **748 tests**. No manifest, lockfile, frontend, schema, public API, scheduler status writer, production deadline or capacity changed in this repair.

Current SHA256 fingerprints at frozen handback:

| Path | SHA256 |
| --- | --- |
| `src-tauri/src/knowledge/index/delete_completion/files.rs` | `68e39a6020dfeb58882c574dce4063ceb9143c0a09cc27320b257a816f069869` |
| `src-tauri/src/app/documents/tests.rs` | `9acc775d9f827b8ddc05de873e5c9b6080c7d48be92d1354607e1abbd71f5f86` |
| `src-tauri/src/knowledge/index_tests.rs` | `623b9747b6d8bff22b6155773dcd73ea70e9dee5cce45f131067a82dbe093c36` |
| `src-tauri/src/knowledge/index_tests/binding.rs` | `197815cbb90080cc19fd3cd7b80e2a01dedff66550b89ae35c083b63473b9111` |
| `src-tauri/src/knowledge/index_tests/review_replacement/cohorts.rs` | `fa82632d1eb5759f661bcc9829e231dc724ab64c1be3a9d5bac92567c7a11e32` |

## Actual commands and RED/GREEN evidence

Every new Cargo run used `login:false`, the saved local toolchain/ALSA environment from `final-verification.json`, and `--offline --locked`. Cargo invocations were serialized; Rust test concurrency stayed at its default. `RUST_TEST_THREADS` was verified unset, and no command used `--test-threads=1`. Tests used synthetic credentials and local fixtures.

The exact command prefix below was followed by the arguments in the table. Full expanded command, output and actual exit are retained in each linked log.

```bash
env CARGO_HOME=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/cargo-writable-20261106 \
 CARGO_TARGET_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/cargo-target-intelligence-20261107 \
 TMPDIR=/dev/shm/voxbridge-intelligence-20261109.q7xaof9u \
 PKG_CONFIG_PATH=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig \
 PKG_CONFIG_SYSROOT_DIR=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot \
 LIBRARY_PATH=/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu \
 cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib
```

| Arguments | Actual result | Runtime | Retained log |
| --- | --- | --- | --- |
| `documents_completion_cleanup_error_` | RED: 0 passed / 1 failed / exit 101 | 0.04 s | [original document RED](final-repair-documents-original-red.log) |
| `knowledge::index_tests` | RED: 88 passed / 1 failed / exit 101; binding fails, cohort passes | 86.51 s | [unchanged default-parallel knowledge RED](final-repair-knowledge-original-parallel-red.log) |
| `documents_completion_cleanup_error_` | Diagnostic RED: 0 passed / 1 failed / exit 101; exact wrong outcome/state | 0.05 s | [document trace RED](final-repair-documents-trace-red.log) |
| `app::documents::tests` | GREEN: 5 passed / 0 failed / exit 0 | 0.24 s | [document GREEN](final-repair-documents-green.log) |
| `knowledge::index_tests -- --nocapture` | Diagnostic RED: 87 passed / 2 failed / exit 101; binding and cohort fail | 85.35 s | [default-parallel trace RED](final-repair-knowledge-parallel-trace-red.log) |
| `knowledge_binding_concurrent_` | GREEN: 1 passed / 0 failed / exit 0 | 0.31 s | [controlled binding GREEN](final-repair-binding-controlled-green.log) |
| `knowledge_review_direct_delete_certifies_202_ -- --nocapture` | GREEN: 1 passed / 0 failed / exit 0 | 56.62 s | [isolated cohort GREEN](final-repair-direct-peers-isolated-green.log) |
| `knowledge_ -- --nocapture` | GREEN: **106 passed / 0 failed / exit 0** | 84.86 s | [final default-parallel knowledge GREEN](final-repair-knowledge-default-parallel-green.log) |
| `app::documents::tests` | GREEN: **5 passed / 0 failed / exit 0** | 0.22 s | [final formatted document GREEN](final-repair-documents-formatted-green.log) |

The final focused logs contain no compiler warnings. Formatting was limited to the five owned paths, with child traversal disabled:

```bash
rustfmt --edition 2024 --config skip_children=true src-tauri/src/app/documents/tests.rs src-tauri/src/knowledge/index/delete_completion/files.rs src-tauri/src/knowledge/index_tests.rs src-tauri/src/knowledge/index_tests/binding.rs src-tauri/src/knowledge/index_tests/review_replacement/cohorts.rs
```

Actual exit: **0**, retained in [final-repair-owned-rustfmt.log](final-repair-owned-rustfmt.log). `git diff --check` also recorded exit **0**.

## Preserved evidence and self-review

[final-repair-handback-integrity.log](final-repair-handback-integrity.log) records the completed integrity check: every original input hash matches `source-freeze.json`; the 37 unchanged frozen sources match; the three declared frozen-source differences are exact; original package SHA256 and byte count match; the original failed gate hash matches `final-verification.json`; HEAD/branch are unchanged; only `.gitignore` is staged; and no Cargo process remains.

The original Task4 report, freeze, path inventory, review package, controller and final gate evidence remain byte-for-byte unchanged:

| Preserved artifact | SHA256 |
| --- | --- |
| `task-4-completion-report.md` | `7daf613978ba82375fbe27762135ee18f7350c53e7109cc58fb6e341863bb36a` |
| `source-freeze.json` | `e7826c3bd0ad918a2389144b70553e887b6dac4e125ef08d8a8e60bccb1384f4` |
| `frozen-source-paths.txt` | `179ea00e04fb30d4c8ccbc2249eede2ff21d9285652da21b49ab1cf45012a191` |
| `complete-repair-review.diff` | `ec094d1c6bc813e61d2ead0b6266d1b2bb11fd53be15483e19cb392060129697` |
| `controller-gates.py` | `3ef0cbcabac27a4f595d9e7c20109bef3c08faf53e4a07e325bde3966a91e57e` |
| `final-verification.json` | `041ae766bdcdbcdefcaef28925d33132812fda01b426cb0f79d614f759147194` |
| `final-all-targets.log` | `cbfcc54de9012a05925ad93b1b6251afa960808ac3e2e5b1ec2dc621202c3858` |

The original review package remains **392,723 bytes**. The worktree and index `.gitignore` both retain SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`. The live index and host configuration were untouched. No staging, commit, merge, push, external service or new agent was used.

Self-review compared the five current files directly with their original frozen Git blobs, including the new modules that remain untracked in the live index. The production delta only relocates directory resolution after certificate discovery; it does not remove certificate validators or relax ordinary symlink/repair assertions. The observer keeps durable state authoritative and fails if the expected state is absent at final resync. The dimensions fixture proves both real entries and retains atomic single-winner checks. The complete-cohort fixture retains its existing assertions and now also demands actual reap proof.

Remaining limits are the substantial complete-cohort runtime and the pending Root broad gates and independent review. No new production performance claim is made. The original freeze/package intentionally remain historical evidence and must not be presented as a fresh freeze of these repaired sources.

## Explicit ownership handback

All author implementation, owned-path formatting, focused verification and reporting are complete. **Source and Cargo ownership are now returned to Root**, frozen at the five fingerprints above. There are **no pending source edits, Cargo/test processes or exec sessions**. Root may resume the fresh freeze, default-parallel all-targets gate, broad format/production/desktop verification and the scoped independent review. The author will make no further source or Cargo changes without a new assignment.
