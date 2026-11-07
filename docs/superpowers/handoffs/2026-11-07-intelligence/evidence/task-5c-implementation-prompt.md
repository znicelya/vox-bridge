You are the sole Task 5C implementer for the Vox Bridge workbench intelligence plan. You have no need to ask whether to continue; implement the accepted task end to end. Work only in /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation at BASE 1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee. Do not spawn agents, push, merge, or stage/commit. Leave the unowned .gitignore untouched.

Read these first and treat them as requirements:
- .superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-architecture-reconciliation.md (accepted D1-D10 and full design)
- task-5-brief.md, global-constraints.md
- task-5c-model-search-checkpoint.md, task-5c-ownership-decision.md
- task-5-search-budget-handoff.md, task-5-import-continuation-handoff.md
- task-5-seal-format-handoff.md, task-5-job-credential-ownership-handoff.md
- completed task-5b-index-owner-report.md and task-5b-index-owner-review.md
- reviewed 5A/4B checkpoint/report/schema files as referenced by the reconciliation

Implement all of Task 5C, including its required integration seams, migration, tests and knowledge-wire decoders. The accepted controller rulings are binding:
1. Add typed bounded PublicationPlan via owner/repository and a facade close. Seal, mappings, pointer and Ready/chunk/repair changes are one fenced SQLite transaction. Do not publish Ready separately.
2. Add additive migration 5 exactly as section 3 of the checkpoint; no m4 table rebuild/trusted backfill.
3. Implement semantic binding identity and requested_dimension semantics, including configured None local resolution and concurrent first-index winner/loser fencing.
4. Implement EmbeddingExecution/Embeddings, ConfigIo-scoped credentials, 4-request gate, OnceTransient, input/request/response bounds, index/dimension/vector validation and checked L2 normalization.
5. Add the narrow AfterTerminal scheduler seam and selected_embedding_snapshot one-gate capture. Parent success cannot be downgraded; fallback keeps Parsed plus auto_index_deferred.
6. Call reconcile_owned in AppState startup after index readiness and before jobs start; zero paid re-embedding.
7. Fix knowledge-wire raw heading bound to 16 MiB and add conservative 32 MiB aggregate defense; backend remains authoritative.
8. Implement durable paged delete cleanup, local tombstone reconciliation, bit-preserving rebuild/pointer swap and old-generation close/delete ordering.
9. Compose KnowledgeService with store/config/jobs/embeddings/index/data_dir; direct search owns an intelligence lease and a two-outstanding cancel-on-replacement gate.
10. Add cfg(test)-only failure seams at SQLite intent, vector mutation, vector persist and Ready publication using the KnowledgeHarness described in section 9.

Use test-driven development. For each behavioral slice, create meaningful tests first, run and capture RED, then implement minimal production behavior and run GREEN. Cover the full reindex/delete/rebuild/reconcile/search/import-continuation matrix listed in sections 5-9, not only a smoke path. Use real pinned SrvDB in default tests with synthetic deterministic 128-dimensional embeddings. No paid endpoint, real keyring, native hardware, network, arbitrary index open, or third engine. Remove the blanket production dead-code allow only once consumers exist. Preserve queue16, one owner plus exactly two Rayon workers, two-engine cap, 200-record paging and all reviewed 5B join/ownership semantics.

You own only the paths listed in checkpoint section 9 plus tests/logs needed for those paths. Keep changes within Task 5C and required existing producer seams. Do not reimplement Task 4B parser/documents behavior or Tasks 6-10 UI/research/summary/release work.

Cargo environment:
export TMPDIR=/dev/shm/voxbridge-task5b-resume.UF4InG
export CARGO_HOME="$PWD/.superpowers/toolchain/cargo-writable-20261106"
export PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot"
export LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu"
Use --offline --locked. Exclude src-tauri/target and .superpowers/toolchain from broad recursive searches. Do not run multiple Cargo commands concurrently. Use the local package-manager commands available for frontend tests/build without changing dependencies.

Run at minimum the Task5C canonical knowledge and intelligence filters, then the full library, all-targets, desktop binary build, cargo fmt check and both diff checks; run knowledge-wire focused tests and the normal frontend build/format/lint gates applicable to touched files. If a command fails, diagnose from actual output and fix the cause. Do not declare success from partial output.

Write a complete implementation report at .superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-report.md, including exact changed paths, RED/GREEN evidence/log paths, crash/state-machine coverage, commands and counts, unresolved limitations and external gates. Do not invent evidence or commit. Return a short final message: status DONE/DONE_WITH_CONCERNS/BLOCKED, changed-path count, test summary, report path, and concerns.
