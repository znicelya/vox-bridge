# Task6 final OO1 repair: yielding SQL with actual ownership

This is the single coherent final-review repair for the confirmed controlled
OO1 availability defect. The full Task6 source review and all historical
gates remain immutable evidence. Do not rerun accepted manual work or CV-1.
The user has authorized continued implementation of the existing design.

Working directory:
`/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Pre-fix immutable tree: `d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`.
HEAD remains `85eda3a8bca3ddabd72edad0763b4d5776166f9a`; automatic code is
already in the working tree, not committed. Do not reset, switch, stage,
commit, merge, push or clean. Preserve the user-staged `.gitignore` exactly.

Root owns integration, final source freeze and the separate scoped review.
One implementer may own original-source edits and serialized focused Cargo
after Root dispatch. The separate UI worktree is frozen under read-only
review and is out of scope. No other Cargo owner is active.

## Required inputs

Read the complete OO1 investigation report, minimum-fix preflight report,
accepted whole-plan review `../task-6-independent-integration-review.md`,
and their precise referenced producers. Read the continuation plan's Global
Constraints and Task2, `../global-constraints.md`, the existing Task2
controller decisions and same-key retry ruling. The approved design and
binding interface amendments are referenced by the plan/briefs. They remain
the authority for product limits, output compatibility and actual joins.

Historical RED is `run-002-cargo.log`, with the genuine fixture in
`oo1-source-copy-fixture-002.diff` and `source/`. The current preflight is
`oo1-minimal-fix-preflight-report.md`. All reports, raw runs, indexes,
source-copy inputs and original frozen blobs must remain unchanged.
Write new evidence only under a new `repair/` child of this directory.

## Root scope ruling

Move synchronous SQL on the named concurrent owners off Tokio worker threads,
with serial bounded submission, strong actual-lifetime ownership and retained
genuine join handles. Reuse the existing blocking pool. This is not a new
executor, a worker-count increase, or a whole-store/command migration.

1. Both original scheduler workers must yield for claim acquisition and their
   nonempty-job notice/terminal/read-publication SQL. Keep the original two
   workers and token channel2; at most one submitted-but-unjoined SQL operation
   per original worker. Claims do not consume the shared sixteen child slots
   or create fictitious reap frames. Generation completion joins actual
   operations, including worker loss, then uses the existing child/reap/lease
   sequence. A committed-but-unentered frame or abnormal join fails closed.
2. Scheduler shutdown immediately closes in-memory admission/watch and active
   cancellation without SQLite. Coalesce queued cancellation and requested
   interruption into one retained generation maintenance owner, with at most
   one actual maintenance SQL operation at a time. Release state/control
   mutexes before acquisition/submission/await. Maintenance may run while
   workers drain, keeps exact generation identity, and joins before retirement.
   Repeated observers cannot accumulate blocked tasks. A timeout while SQL is
   held reports pending/error and retains actual ownership; it does not claim
   durable interruption. Preserve the existing free-store interruption test
   behavior and actual child/process retention.
3. The session's autonomous observer serially yields for phase and terminal/
   error persistence, with one actual SQL operation at a time. Keep the
   original immediate stop intent and post-write stop/publication fences.
   Terminal lifecycle and journal remain one durable transaction; notification
   still follows durable commit, bus close and active-operation release.
4. Each of at most two direction actors serially yields for diagnostic and
   final transcript writes. Do not queue one operation per audio frame or
   diagnostic. Final persistence precedes ID/sequence consumption and event
   publication; diagnostic error/forced-cleanup handling remains real. The
   actor/session resource owners and genuine joins include any issued SQL.
5. The original automatic delivery owner yields for page scans, including
   empty scans, and reachable attempt/error/config-fallback SQL. Reuse its
   one actual child slot; release/join a page operation before entering the
   next child. Keep pages32, rescan200ms, channel32, same-key closing seal,
   receiver-EOF order and actual worker/child/closer joins unchanged.

Every issued blocking operation retains its required actual process/resource
owner until the real operation finishes. Dropped or timed-out observation
does not free capacity, detach an unaccounted handle, create a new owner or
permit rearm. Do not replace actual close receipts with status/empty-queue
checks, change raw job witnesses or lock order, weaken SQL publication gates,
drop final rows, or infer successful native audio/paid service execution.

This scope includes nonempty worker completion so an empty-queue GREEN does
not merely move the defect to ordinary completed jobs. Other synchronous
commands/startup boundaries remain outside this focused repair unless a
concrete covering failure requires a named Root scope decision.

## Test and implementation sequence

Use Superpowers systematic-debugging, TDD and writing-good-tests. Before
production edits send Root a short concrete boundary/handle-ownership plan;
continue preparing the meaningful RED fixture while Root reads it.

1. Adapt the existing genuine OO1 fixture into newly owned test source. Keep
   the real AppState/LocalStore, two actual token-owning scheduler workers,
   two-worker Tokio runtime, real stop and App close, original native parent
   and independent monotonic OS-thread SQL hold/release. Keep probes at the
   actual original claim acquisition and transaction body. Instrument only
   tests; no fake SQLite or replacement completion sources.
2. Strengthen progress across early/middle/late hold windows, or an explicit
   maximum heartbeat gap with scheduling allowance. Separately measure first
   action poll, real capture closure and primed timer. Durable terminal SQL
   may correctly stay pending during the hold. After release join all real
   scheduler/storage/native/App/delivery/fixture tasks before asserting final
   results. Both acquisition threads and original-worker provenance must be
   established; blocking threads need not equal literal original thread IDs.
3. Run this stronger fixture before implementation and confirm the expected
   availability assertion fails. Retain compile/setup/invalid-entry attempts
   separately; those do not count as behavioral RED.
4. Add focused actual-owner tests for stop-before/during claim, queued rows,
   repeated shutdown/observer loss with held SQL, retained-handle draining/
   abnormal worker loss, nonempty terminal writes, and old-generation rearm.
   Preserve the specific existing tests listed in the preflight.
5. Implement the smallest cohesive owner boundaries, then observe covering
   GREEN for the actual integrated fixture and affected scheduler/session/
   transcript/delivery/close ownership filters. Keep defaults/concurrency/
   deadlines. Do not run broad gates repeatedly; Root owns final complete
   gates after explicit source/Cargo handback.
6. Self-review, format only owned source without changing unrelated files,
   and write `repair/oo1-repair-report.md` with exact changed paths, private
   interfaces/limits, fresh source-linked RED/GREEN logs, full commands,
   elapsed/exit/counts, retained failures, source hashes and remaining limits.

Use exactly the existing environment overrides from
`run-002-command.json.environment_overrides_verbatim`. All real keys, audio
devices, keyring, paid endpoints, native drivers and host/global changes are
excluded. No subagents or background test/benchmark/monitor task at handback.
Do not delete evidence or mutate old source-copy fixtures. Return an explicit
immutable source/report/Cargo ownership handback to Root; only Root will
freeze/package/review/commit accepted paths.
