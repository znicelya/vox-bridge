# Frozen full-gate failure repair

Read this first. Continue the existing Task5C repair, not Task6 or new audio work.

Worktree: `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Binding requirements: the existing repair plan, accepted deletion ruling, and
`task-4-completion-brief.md` in this workspace. The original handback report,
freeze, review package, controller script and final logs must remain byte-for-byte
unchanged. Write a separate `final-gate-repair-report.md` with exact investigation,
RED/GREEN evidence, changed paths and final explicit source/Cargo handback.

## Actual failure evidence

Root resumed and observed session53848 complete with exit101. The manifest and
every original frozen source/input hash still match. `final-all-targets.log`
records the default parallel full gate:745pass/3fail; no warnings. The failures:

1. `app::documents::tests::documents_completion_cleanup_error_retains_intent_and_marks_repair_without_following_symlink`
   asserts `Storage` at `src-tauri/src/app/documents/tests.rs:364`.
2. `knowledge::index_tests::binding::knowledge_binding_concurrent_first_index_conflicting_actual_dimensions_has_one_winner`
   reaches its bounded entry observation timeout at `binding.rs:148`.
3. `knowledge::index_tests::review_replacement::cohorts::knowledge_review_direct_delete_certifies_202_exact_terminal_peers_and_retains_history`
   reaches `KnowledgeHarness::wait` timeout at `index_tests.rs:168`; the later
   catch-unwind unwrap at `cohorts.rs:96` is consequential, not another failure.

## Ownership and investigation

You are the sole source and focused Cargo owner for this narrow repair. Root will
not run Cargo or edit Rust while you own it. Investigate all three using
systematic-debugging and TDD; never guess that a timeout is harmless. Reproduce
against the frozen source, inspect the actual outcome/error and trace producers,
compare unaffected fixtures, and record one hypothesis per issue before a fix.
Existing original tests provide the observed failure; add only meaningful
regressions needed to verify new behavior.

Test ownership includes the three named files and the existing shared knowledge
test harness. Production ownership is limited to the original40-path repair
scope; request a specific additional production path if evidence requires one.
Do not weaken ownership, symlink rejection, durable cleanup/repair assertions,
atomic single-winner dimensions, complete202-member cohort or retained history.
If the contract intentionally changed an error, retain an exact, justified
assertion on the current public error and durable state, documenting the cause.
Observe actual settled/joined completion rather than using a shorter arbitrary
timer; preserve a finite test bound and diagnose substantial runtime costs.
Do not alter production limits/deadlines, ignore tests, force serial success or
change default Cargo concurrency to conceal a failure.

Use the saved local toolchain/ALSA environment from `final-verification.json`,
`--offline --locked`; synthetic credentials and local fixtures only. Preserve
the live index and protected staged `.gitignore`; no staging, commit, merge,
push, new agents, paid/native services, external data, or host changes.
Save new uniquely named logs alongside the new report. Run affected tests and
related tests once green; Root owns the next frozen default-parallel full gate,
format/production/desktop verification and the one scoped independent review.

Return concise status, actual test results, report path, risks, and an explicit
handback confirming no pending Cargo/process/source edits. An unverified claim
or a test-only expectation change without a traced cause is not completion.
