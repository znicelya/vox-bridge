# Task4 deletion completion continuation

Read this first: it is the remaining implementation requirements for the
already-authorized Task5C repair, not a new feature/design cycle.

Workdir: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation
BASE: b72556091fc533dafb0a336907bf077fec377615.

## Binding inputs

1. docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md (all Global
   Constraints; implement remaining Task4 and its covering checks).
2. docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md, full text.
3. docs/superpowers/reviews/2026-11-09-task5c.md, original I1–I5/M1.
4. This plan workspace's progress.md, including continuation ownership.

Tasks1–3 and5 are already implemented with prior focused GREEN. Preserve their
regressions; do not restart the original Task5C. Current Task4 has partial
unvalidated code in knowledge/index/delete_completion.rs and its journal,
publication, files, retirement modules; service/completion.rs; and their
publication/owner/repository/delete/recovery/reconcile consumers. Inspect and
finish this existing implementation. Its last RED logs are in
.superpowers/sdd/2026-11-04-workbench-intelligence/
task-5c-repair-replacement-red.log and
task-5c-repair-replacement-cohorts-red.log. Last cohort run:20 pass/5 fail.
The current six review_replacement fixtures include >200 terminal duplicates,
managed metadata loss with surviving bytes, consumer automatic rebuild/delete
beyond vector199, two dirty deletes, post-pointer restart and unstaged target.

## Scope and ownership

You are the sole remaining source/focused Cargo implementer until frozen
handback. Only Rust knowledge modules and their necessary focused fixtures
are assigned. No frontend, schema/migration, public API, scheduler status
writer, native/paid/driver work, global/host settings or subagents. Root owns
documentation outside your report, final freeze, final broad Cargo, review,
staging/commit/merge/push. Do not stage or commit. Preserve the unrelated
staged .gitignore and its exact hash in the binding plan. If a source change
outside knowledge is required, explain the concrete dependency to Root first.

## Implement and prove

Use TDD and systematic debugging for actual failures. Preserve previous RED
evidence; new behavior needs a meaningful failing fixture before its repair.
Finish all accepted ruling edges, not just the current five failures:

- complete source cohort in fresh Swap/Recovery/Ready and exact Direct Delete
  peers; native persisted receipt/count/dense staged coverage and complete
  target exclusion; no 200-operation truncation;
- strict independently anchored immutable publication/header/pages, canonical
  raw byte limits; independently validate acknowledged/consumed flags;
- actual original final/temp path absence after managed cleanup, even after
  metadata loss; reject unknown additional owned files/path drift;
- exact operation-specific work acknowledgments and proven identical peers;
- source bytes/SQL retirement only after complete certified exclusions and
  actual readers/engine/work cleanup;
- domain-only terminal_tx consumption within the existing scheduler terminal
  transaction; certificate retained until actual joined terminal work;
- settled Failed/Cancelled/Interrupted Direct Delete history excluded from
  future cohorts but independently anchored until source retirement;
- closed complete-cohort atomic journal GC, with source-retirement eligibility
  separate from terminal consumption; no cycle/premature proof loss;
- deterministic reopen/interruption/cleanup-failure, malformed/zero-work,
  different-source/flag-corruption/seal-evolution and survivor f32/no-local-
  embeddings checks required by the ruling. Use real local SQLite/SrvDB.

Existing limits and model/provider contracts are binding; do not lower them.
No unbounded collection or whole cohort decode/Vec. Cohort enumeration,
cleanup descriptors and validation remain bounded by their approved pages.

## Local toolchain and verification

Use login:false and this existing task environment on every Cargo invocation:

env CARGO_HOME="$PWD/.superpowers/toolchain/cargo-writable-20261106" \
 CARGO_TARGET_DIR="$PWD/.superpowers/toolchain/cargo-target-intelligence-20261107" \
 TMPDIR="/dev/shm/voxbridge-intelligence-20261109.q7xaof9u" \
 PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" \
 PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" \
 LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu" \
 cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib knowledge_review_ -- --test-threads=1

Check the TMPDIR still exists; create a fresh task-local /dev/shm directory
and report it if necessary. Never repair mounts or kill/reconfigure editor
workers. No concurrent Cargo is running from Root. Store full command/output/
actual exit evidence in THIS plan workspace; bounded tool output is enough
provided the actual complete log is retained. Run covering targeted checks
and the knowledge filter when substantive fixes require regression scope.
Root runs final all-targets, production check/build and fmt once after your
explicit frozen handback. Do not duplicate those broad gates beforehand.

## Report / handback

Write .superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-completion-report.md.
Include final code behavior, exact modified paths, all actual RED/GREEN
commands/results/log paths, remaining risks/requirements and self-review.
Clearly say whether source/Cargo are frozen, no pending exec sessions/edits,
and Root may resume final gates. Do not claim independent approval or full
product acceptance. Return only status, one-line verification, report path
and concerns. No subagents; independent review is dispatched by Root.

## Audited actual-reap dependency scope extension

Root accepted the actual terminal/reap waiter runtime RED and authorized only
intelligence/jobs.rs plus intelligence/jobs/ production/test files necessary
for the per-job actual-reap dependency. Production additions are jobs/reap.rs
and jobs/child_operations.rs with the existing scheduler composition. No
other intelligence, app, schema, frontend or public command path is granted.
The author remains the only Rust/focused Cargo owner; Root keeps final gates.

Produce the backend-private JobScheduler::reap_port()->JobReapPort and
JobReapPort::prove(&Transaction,Uuid)->AppResult<Option<JobReapProof>>. Register
the claim frame inside the same SQLite/control transaction before Running is
visible. Reserve continuation ownership, permanently close the frame's owner
factories and prove handler actual completion before terminal_tx; actual
child scopes must reach zero only after existing process/slot release order.
Do not re-lock terminal_tx's held control or invert SQLite/control ordering.

Track only bounded live frames derived from workers2/childslots16. Prune
quiescent frames only under explicit closed-factory/joined/zero-child
invariants. Inert/fresh and exact prior-generation join have distinct
authority; missing frame/generation alone is not proof. Generation done stays
after worker/cleanup/ChildOperations drain. Do not stop the current worker to
obtain proof, add a status writer, change admit_job or alter existing limits.

Knowledge consumes the private actual proof with a separate immutable
header-bound consumed receipt; one mutable flag cannot substitute for it.
Pending/unjoined operations retain evidence without blocking independently
proved source retirement. Cover claim-before-entry, real awaiting_reap waiter,
entered child after observer/handler loss, continuation pre-reservation,
closed-factory rejection, rollback/pruning and fresh/joined reopen, alongside
the original child-operation/continuation regressions. Report the exact new
paths/signatures and actual logs; Task6 will reuse this accepted producer.
