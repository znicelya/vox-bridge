# Task 5C active implementation brief — 2026-11-07

BASE: `1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee`.
Work only in `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.

Read this first: these are your requirements. Read the following referenced
files for the exact values and contracts, without reading the entire plan:

1. `task-5c-source-plan.md` (the freshly extracted complete Task 5 text).
2. `task-5c-architecture-reconciliation.md` (all sections; D1–D10 were accepted).
3. `global-constraints.md`.
4. `task-5c-model-search-checkpoint.md`, `task-5c-ownership-decision.md`,
   `task-5-import-continuation-handoff.md`, `task-5-search-budget-handoff.md`,
   `task-5-job-credential-ownership-handoff.md`, `task-5-seal-format-handoff.md`.
5. `task-5b-index-owner-report.md` and `task-5b-index-owner-review.md` for the
   producer interfaces and parked observations your consumers must resolve.

These files are siblings in this plan's SDD directory. Task 5A and 5B are
complete and reviewed. Implement only Task 5C and its necessary producer seams;
preserve the existing owner queue, pool, cap, durability and joined shutdown.
The accepted architecture and the D1–D10 entries in `progress.md` resolve
differences between the original plan and the actual committed code.

## Additional bounded-publication clarification

The full-document publication must support the existing document chunk limit
while the typed plan and temporary batches remain bounded. Stage returned
IDs in `knowledge_index_work_items` / `knowledge_generation_mappings` in
batches no larger than the established paging limit. A compact publication
plan references the durable operation and document/generation fences rather
than carrying a process-wide mapping array. Its one SQLite transaction
validates and publishes every planned mapping, pointer and Ready transition
with bounded borrowed pages. This is required for plain reindex as well as
generation rebuild. Unpublished staged rows never qualify for hydration.

A successful read-only search must complete the admitted generation's
persist/publication/ack lifecycle; it must not strand a dirty Open intent.
Test consecutive searches and clean reopen in addition to the prescribed
cancellation and late-readiness tests.

Candidate corruption must be attributed through the active published SQLite
inverse mapping. Metadata alone cannot name a document to repair. Unmapped,
unpublished or obsolete-version candidates are excluded as orphans/stale
candidates; they cannot change a current Parsing, Parsed, Indexing or Deleting
document's state. A repair transition for a current mapped document must
recheck its version and Ready state in the transaction. Cover a mixed healthy
search during replacement/import and a metadata payload naming a different
healthy document. This preserves authoritative hydration and the successful
import continuation's Parsed-state fence.

The committed strict `IndexOwner::search` aborts the complete candidate list
on missing/malformed metadata. Add a narrow compact-candidate port for 5C that
keeps validated ID/finite score plus optional decoded metadata. Retain the
existing strict search behavior as a wrapper for its existing consumers.
Storage, ID-range and non-finite-score errors remain failures; individual
metadata absence/decoding failure can be attributed to the inverse mapping
and excluded without hiding healthy candidates. Keep the same owner queue,
worker/pool/candidate limits and add a real-index mixed-corruption regression.

For delete crashes after vector persist but before domain publication, read
`task-5c-delete-recovery-ruling.md`. The normal witnessed delete-only case
must recover locally through a fresh generation without a dirty SrvDB open
or paid re-embedding; failed witness proof preserves Deleting/work/bytes.

Read `task-5c-work-evidence-ruling.md`: the accepted compact publication needs
three additive nullable migration5 work evidence columns for bounded full
document/location/previous-vector fences. This is the precise D2 extension;
no trusted backfill or in-memory full-document fence array is allowed.

Read `task-5c-test-fixture-ruling.md` for the existing AppState owned-handler
tests that now collide with required Reindex registration. The accepted
pre-first-start cfg(test) helper preserves production register-once and the
original document-kind/exclusion/ownership assertions.

`task-5c-rebuild-trigger-ruling.md` accepts the existing12GiB watermark,
checked200000 capacity pressure and a50% obsolete-record local-copy heuristic
on certified clean bindings. Empty handling and post-copy/pre-embedding
capacity checks must preserve the established owner/provenance/caps.

## Scope and process

Implement all the accepted Task 5C slices: additive migration and types;
owned embedding transport and binding; recoverable reindex; paged deletion,
reconcile and bit-preserving local rebuild; authoritative bounded search;
successful-import continuation; AppState/command integration; knowledge wire
decoders and their aggregate defense. Keep files focused, using the module
structure from architecture section 9 and its natural submodules.

Follow Superpowers TDD and meaningful-test rules. Write each behavioral RED
test, run it and save the expected failure, then implement the minimal GREEN
behavior and record its successful command. Use the real pinned SrvDB with
synthetic vectors in default tests. Tests and injected seams must not change
shipping transaction behavior. Never use paid services, real keyring,
physical audio, privileged drivers, arbitrary untrusted index opens or a
third SrvDB engine. Do not implement Tasks 6–10 or change visual layout.

Do not spawn subagents. Do not stage, commit, create branches, merge, push or
publish. Leave `.gitignore` at SHA-256
`1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.
The controller owns source freeze, independent review and task commit.

## Environment and verification

The old temporary directory in `task-5c-implementation-prompt.md` no longer
exists. Use this environment for every Cargo command from the worktree:

```bash
export TMPDIR=/dev/shm/voxbridge-task5c.PpMKwW
export CARGO_HOME="$PWD/.superpowers/toolchain/cargo-writable-20261106"
export CARGO_TARGET_DIR="$PWD/.superpowers/toolchain/cargo-target-intelligence-20261107"
export PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot"
export LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu"
```

Use `--offline --locked`. Do not modify global configuration, permissions or
mounts. Exclude `src-tauri/target`, `.superpowers/toolchain`, node_modules and
the pinned source checkout from broad recursive searches.

The task-local target override isolates tests/builds from automatic editor
`cargo check --all-targets` runs using the default target. Use this same target
for every focused and final Cargo command; do not kill or reconfigure the
editor. The baseline used the default target before this interference was
diagnosed, so one initial cache warm-up is expected in the new directory.

The controller is currently running the untouched all-target baseline and
owns Cargo until it sends the baseline-ready message. Read requirements and
code while waiting; do not change source or run Cargo before that message.
After it, you become the exclusive Cargo owner. Run focused checks while
iterating and the prescribed final Rust and frontend gates once on the final
source. Share actual failures promptly; never infer success from partial
output. If the complete task needs smaller reviewable pieces, propose the
precise boundary to the controller while continuing independent work.

Controller continuation update: to avoid two complete final runs on the same
source, finish all remaining implementation, behavioral focused GREEN groups
and self-review, then report exact owned paths and hand back frozen source/
Cargo ownership. The controller will execute the final gates once on that
frozen tree and send the actual log/results for the final report. Do not also
run all-targets/desktop/full frontend final gates before this handoff. If such
a gate has already run, report it; repeats require new source, a failure or a
concrete unresolved concern. The final report/DONE contract still requires
the full final gates followed by independent review. Until explicit handoff,
you remain the only source/Cargo owner.

## Report

Write `task-5c-report.md` beside this brief with exact owned paths,
RED/GREEN commands and log paths, substantive crash/ownership/search/rebuild
coverage, final commands and counts, self-review, unresolved concerns and
external gates. Report progress between slices. Return only status
DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED, path count, concise test
summary and report path. Completion requires the entire Task 5C contract;
self-review is followed by the controller's independent task review.
