# Task5C repair round 2 — N1, N2 and N3

Read this first: it is the requirements for the next single fix assignment.
The binding repair plan is `docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md`;
the full deletion authority contract is
`docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md`.
The finalized findings are in
`docs/superpowers/reviews/2026-11-09-task5c-repair.md` (SHA256
`1f467d5e6010a3e11f86ae2c0538b6da0883ab91f1f134210268c78242431dd5`).
Read all three N1–N3 findings and their requested narrow regressions verbatim.
I1–I5/M1 are addressed for their original routes; their passing regressions remain binding.
This is fix round 2/5, not a new whole-product review or Task6 activation.

## Source and evidence ownership

Work only in `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`,
branch `codex/vox-bridge-implementation`, HEAD
`b72556091fc533dafb0a336907bf077fec377615`.
The preceding reviewed source is the immutable tree
`da23738987e178f23b1f5bda34c9c8250d5f4f4e`, recorded in
`final-round1/source-freeze.json` beside this brief.
Root freshly verified all 42 sources, 14 inputs, complete package, five gate
logs, finalized review and protected staging before this handoff.
The prior default-concurrency all-targets record is 764 passed, zero failed,
and one intentional paid-service ignore. This is unchanged-source evidence,
not evidence that N1–N3 are already repaired.

You receive narrow Rust implementation and focused Cargo ownership only.
Root owns all other files, immutable evidence, final source freeze, final broad
Cargo gates, independent re-review, staging and integration.
Do not spawn agents, stage, commit, merge, push, open services, use real keys,
alter native devices/drivers, repair host settings, or change dependencies.
Leave the existing staged `.gitignore` bytes and index state untouched; its
SHA256 is `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.

Allowed production paths: `knowledge/index/delete_work.rs`,
`knowledge/index/delete_completion.rs` and its `journal`, `publication`,
`files`, `retirement` modules; `knowledge/service/{completion,delete,recovery}.rs`
and `knowledge/reconcile.rs` under `src-tauri/src` when necessary to connect
the owned cleanup boundary. Allowed tests: existing `knowledge/index_tests`
fixtures and their module registration, with small new round2 modules if useful.
If another producer must change, explain the concrete dependency to Root before
editing it. Do not expand the scheduler, public APIs, migrations or product scope.

## Root reconciliation of the binding contract

N1: Keep the original raw page fingerprint immutable for each operation.
Equivalent bounded typed fence JSON may have different raw whitespace and
must remain accepted. Duplicate acknowledgment therefore needs a separately
anchored semantic association authority (or equivalent exact compatible peer
proof); changing raw bytes after their certification must still reject.
The old phrase "identical original page" refers to canonical complete row
associations for peer compatibility, while each operation still validates its
own original raw evidence. Preserve every document, version, identity,
generation, ordinal, vector, chunk, content hash and full typed fence boundary.

N2: A terminal SQLite hook can record actual-reap consumption, but cannot
authorize filesystem GC from a prior cleaned receipt. GC must follow a current
original-path absence check performed outside SQLite under actual owned work,
then revalidate the complete closed cohort atomically. Startup `jobs.reconcile()`
must preserve the certificate until this owned check. A genuinely recreated
original final or temporary path must be retained and rejected until removed.
Preserve eventual local collection once actual cleanup is complete.

N3: Preflight and publication must account for the complete inverse original
file-owner set: direct document ownership, current import ownership and
historical original Import associations. A historical file whose document link
changes to NULL must fail before destructive source/domain/publication work;
do not synthesize a corrected owner or postpone its discovery until cleanup.
Native admission/version history, generated paths, import rows, healthy neighbors
and all original deletion work remain intact after rejection.

## Execution and verification

Use systematic debugging and TDD. First create and run the exact counterexamples
against the unchanged reviewed production paths, preserving behavioral RED logs
and actual exits. A compilation/setup failure is not behavioral RED. Fix one
understood cause at a time, and preserve its GREEN command/output.

Use the review's real producer paths: same-source genuine duplicate deletes for
Direct and fresh replacement/restart; a genuinely cleaned but Queued member,
fresh scheduler startup and recreated final/temporary path; two genuine native
import versions with only the historical managed-file document link nulled,
through both live deletion and fresh cohort publication. Add a raw-evidence
post-certificate drift assertion for N1 if the new proof format needs it.
Do not forge cleaned flags instead of obtaining real receipts, bypass native
admission, substitute serial-only passes or weaken state/ownership assertions.

Prefix the new regression names with `knowledge_review_round2_` for a single
focused filter. Keep local recovery embedding calls at zero and check healthy
survivor f32 bytes and exact retained/acknowledged mapping and work associations.
Retain the accepted 100-file and 200-record page sizes, cardinality/byte caps,
16-entry owner queue, exactly two joined Rayon workers and two-engine maximum.
No new public DTO or migration is required. Reject unknown/incomplete proof
without deleting retained evidence.

Use the task-local Cargo home, ALSA sysroot and target directory already recorded
in `final-round1/final-verification.json`; create a new task-local TMPDIR if needed.
Never set RUST_TEST_THREADS for final covering runs. Run only focused RED/GREEN
and the relevant default-concurrency knowledge group after the final edits;
Root runs all-targets/check/desktop/fmt only after explicit source handback.
Format only owned files during implementation. Preserve every prior log,
report, brief, freeze and package byte-for-byte.

Write the new report to `round2-repair-report.md` beside this brief; append your
fix work there, keeping the preceding frozen reports immutable. Report exact
owned paths and final SHA256, root cause for each finding, real RED/GREEN command,
exit/result/output/log identity, actual behavior, default-concurrency coverage,
any concern, and an explicit declaration that all edits and Cargo processes
have ended and Root again owns source/Cargo. Return only short status, report
path and one-line verification. No commits in this assignment.
