# Task6 dispatch preparation — not yet active

Task5C is still implementing. Do not start Task6 source or Cargo from this
draft. Activate only after Task5C is independently reviewed/committed; record
its actual HEAD as Task6 BASE. Worktree:
`/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.

## Authoritative requirements and producer handoffs

Read `task-6-source-plan.md` first: it is the complete current Task6 contract.
Then read `global-constraints.md`, `task-6-current-ownership-note.md`,
`task-6-snapshot-handoff.md` and the reviewed Task5C report/review (not yet
available). All files are siblings in this plan's SDD directory. The canonical
plan and accepted ledger rulings bind summary migration6 and research
migration7; historical references to migration5 summaries are superseded.
Do not reread the entire plan or reimplement reviewed Tasks1–5.

Implement Task6 only: owned compatible LLM transport, atomic admission with
durable version counters/permanent automatic markers, strict summary storage,
fixed final-only snapshot, exact split-part coverage, bounded hierarchy,
automatic scheduling after durable terminal commit, AppState/commands and
allowlisted complete structured exports. New native summaries use the strict
schema; existing summary rows and export compatibility remain readable.

## Required pre-implementation architecture reconciliation

Before source changes, inspect the actual reviewed producer seams and write a
focused checkpoint with exact ports/ownership/transaction/lifecycle decisions:

1. Owned LLM execution carries a pre-issued actual-completion owner and watch
   receiver into ephemeral scoped config. Reuse the reviewed Task5C ownership
   pattern and full ModelProfile snapshot credential resolver. Cancellation
   alone does not own an entered ConfigIo/HTTP operation. Do not add plaintext
   credentials to requests, public DTOs, diagnostics or fixtures.
2. Show exactly how the successful Completed/Failed lifecycle commit notifies
   automatic summary admission while the audio stop result remains independent
   of any config/SQLite admission or LLM await. The session event bus closes
   normally. Continuation queue/tasks, process/permit ownership, shutdown/rearm
   and late callbacks must be bounded and actually joined, with no scheduler
   ownership cycle or captured AppState Arc.
3. Specify one config selected-LLM/profile snapshot gate followed by
   SQLite/generation-fence admission. The one write transaction uses the
   reviewed admit_job helper and atomically owns the prospective summary UUID,
   permanent marker, durable checked-u32 counter and Queued/Failed row.
   Permanent Existing returns without counter/version allocation. Automatic
   local failures and startup uncertainty never replay a paid call.
4. Specify the private borrowed-ValueRef fixed snapshot on the existing SQLite
   owner, its deterministic bounded encoding/hash, rowid/session query plan,
   metadata/legacy role retention and read-transaction release before network.
   Input16MiB/100000 rows never becomes a successful prefix. Requests charge
   actual rendered system+user UTF-8 plus1024 overhead against context-output.
5. Specify immutable complete part manifest and actual supplied/retained map
   coverage, including middle-part omission detection, Unicode reconstruction,
  64 total calls/8 levels, singleton carry and strict reduction progress.
   Persist raw/hash+map coverage before parse; each response2MiB and total32MiB.
6. Reconcile native and legacy public/export projections. Concrete existing
   legacy producers use status `complete` and JSON `{title,text}` or `{text}`
   (`storage/sqlite_tests.rs` and `export/export_tests.rs`); foundation stores
   arbitrary TEXT status and signed versions/timestamps. Preserve original
   storage and readable legacy export; do not erase legacy content by parsing
   only the new schema or silently fabricate a native JobStatus/metadata.
   Flag any DTO contract mismatch before implementation for a ledgered ruling.

This is implementation reconciliation of an approved feature, not new product
brainstorming/reapproval. Propose a concrete seam for any contract conflict;
the controller rules and records the decision rather than stopping the plan.

## Execution and report contract (to activate after checkpoint)

One source/Cargo implementer, no subagents, no stage/commit/branch/merge/push or
publication. Root owns ledger, exact source freeze, independent task review and
commit. Preserve unowned `.gitignore` SHA256
`1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a` and main's
pre-existing untracked Cargo.lock. No paid/network/native-keyring/audio/driver
actions; deterministic LLM/HTTP and real SQLite/runtime fixtures only.

Use the accepted per-command local Cargo/ALSA environment and an isolated
task-local target consistently; do not kill/reconfigure the editor or modify
global config/mounts/permissions. The controller will confirm TMPDIR/target at
activation. Cargo offline/locked, one owner. Final gates: focused summary/LLM/
session/export tests, full all-targets, desktop build, fmt/diff and existing
frontend integration gates as relevant. Broaden/repeat only for new changes,
failures or unresolved concerns. Save genuine behavioral RED and GREEN logs;
compilation-only diagnostics are not runtime RED.

Write `task-6-report.md` with exact owned paths, checkpoint decisions, each
contract's substantive test/evidence mapping, final commands/exits/counts,
self-review, concerns and external gates. Completion is the entire Task6
contract followed by independent Spec PASS/Quality APPROVED, not an isolated
transport/planner slice. Tasks7–10 stay queued until then.
