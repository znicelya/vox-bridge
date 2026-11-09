# Manual Task1 — final controller gate repair, not a review round

Same Task1 author resumes. BASE remains 73760d609a050eaddcd8002cec572fbc73c65164.
The original report, handback acceptance and frozen61-path source remain
preserved. No independent task reviewer has yet been dispatched; this is a
failed final integration gate repair, not a completed SDD review fix round.

Read the existing task-1-brief.md/global-constraints.md and your report.
Root's completed all-targets run is in final-manual-task1/final-all-targets.log
and final-verification.json. The frozen tree is
5ddddf75628520799365aafbe18f0612c79e42ee. Log SHA256:
01afb1a318a6e2bc47905ec38a582425af5b0104916e3c9ee30b0ddffd441f32.
Actual result:835 passed,2 failed, exit101,98.73s test time. Subsequent
production-check/build/fmt gates were not reached. Original log and freeze
must not be edited, moved or overwritten.

## Concrete failures to resolve

1. src-tauri/src/knowledge/document_tests.rs:13 —
   knowledge_migration5_binding_fences_and_reopen_preserve_untrusted_legacy_rows
   expects current completed upgrade user_version5, observes6.
2. Same file:175 —
   documents_migration_preserves_legacy_nullable_metadata_and_chunks_across_reopen
   expects current completed upgrade user_version5, observes6.
   The task already requires updating current completed-upgrade literals while
   preserving rollback/legacy data assertions. Change only those two current
   schema expectations; do not change historical migrations or weaken the
   unrelated legacy/binding/dirty-source behavior. You may add this exact
   test path to your source inventory.
3. Production lib (not lib-test) emits three dead-code warnings:
   export/mod.rs:178 safe_summary_json is used only in new tests;
   summary/snapshot.rs:53 FinalSnapshot.normalized_bytes is read only in tests;
   summary/store.rs:72 create_summary_job is currently used only in tests,
   while the actual application uses create_summary_job_in_tx. Resolve each
   according to the binding service/Task2 interface requirement: remove or
   cfg(test)-scope genuinely test-only seams, or document a narrowly retained
   required production port with a scoped annotation. Do not add a blanket
   allow(dead_code), fake consumer or second transaction merely to silence
   the warning. Preserve the complete planner/snapshot/actual admission
   behavior and exact exported command shape.

Use systematic-debugging/TDD evidence. The genuine all-targets failures above
are the completed RED; do not fabricate earlier source witnesses. Keep fresh
focused fix logs/source manifests in task-1-evidence and append the fix report
to task-1-report.md. Cover the two amended migration tests, affected summary/
export cases as warranted by changed production seams, a real production
lib check, and final fmt. Do not repeat the broad all-targets suite here;
Root owns its fresh re-freeze/re-run after your explicit handback.

Root transfers sole source/focused-Cargo ownership back to you after sending
this brief. Root will neither edit Rust nor run concurrent Cargo. The failed
freeze and original handback JSON remain immutable; the report may append
new fix results and an updated complete path→SHA256 inventory. Return a
fresh explicit no-pending-edits/commands/processes source/Cargo handback.
No new agents, stage/commit/refs, cleanup, global changes, paid/native/keyring
actions or Task2 implementation. All six existing environment overrides apply.
