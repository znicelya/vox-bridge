# Task 5C implementation handoff report

BASE: `1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee`. Worktree:
`/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Author changes remained unstaged at the original handback. Root staged the
original52 frozen task paths for verification; the scoped storage fixture
correction remains unstaged for Root. Implementation and focused self-review are
ready for the controller's frozen-source broad gates and independent review.
Task completion is not claimed before those gates. No staging, commit, branch,
merge/push, paid endpoint, real keyring, audio hardware, or global configuration
change was performed. The main checkout's existing untracked Cargo.lock was not
touched; no Cargo.toml/lock change is included.

## Shipping behavior and ownership

Migration5 adds semantic binding/requested-dimension columns and partial unique
indexes plus the accepted nullable bounded document/location/previous-vector
work evidence. Legacy rows are not trusted or backfilled. The canonical plan's
migration6/7 correction belongs to the controller.

The transport resolves the full admitted embedding profile against the exact
credential snapshot, retains the actual operation owner through entered config
and HTTP work, uses the shared4-request gate and OnceTransient policy, and checks
measured32-input/128KiB requests and8MiB responses. It restores input order,
rejects malformed/dimension/nonfinite/zero vectors, and normalizes checked L2
values. First valid explicit indexing establishes a local semantic binding;
None→Some(same dimension) reuses that space, while endpoint/model/profile edits
separate it. Concurrent conflicting first-index publications have one winner.

Reindex stages bounded durable mappings/work; its compact publication walks
borrowed pages≤200 and commits every seal/mapping/pointer/Ready/repair transition
in one fenced SQLite transaction. It retains the full document version/SHA/
parser/import fence, canonical location digest, expected pointer and job owner.
Unpublished or interrupted work never hydrates. All four commit boundaries and
explicit retry are covered with the pinned real SrvDB and synthetic vectors.

Delete admits Deleting plus prior mapping/work intent before vector mutation.
Row/file cleanup follows durable vector publication; work and inverse mappings
survive cascades until explicit acknowledgment. DeleteInput rejects mismatched
document IDs/versions, missing identity-generation pairs and job-operation
association; restart checks the retained job/document association before work. Invalid retained
associations still advance the bounded page cursor so later valid deletions resume.

Witnessed dirty-delete recovery uses only ordinary read-only File vectors and an
exclusive source lock, inside the existing owner/two-worker pool. It never opens
dirty SrvDB, VectorStorage, metadata, or graph. Exact prior Seal/Open Intent/
pointer/geometry/full-file length/hash are checked before and after copy,
including preallocation padding. Bounded lstat checks reject unexpected,
nonregular, symlink or over-cap directory entries without opening their data.
A fresh generation publishes all authoritative survivors plus deletion atomically;
only otherwise-valid unsealed-generation repairs are restored. Source reader
close, managed generation removal and paged acknowledgment follow destination
publication. Failed proof retains Deleting/work/source bytes with zero embedding.

Rebuild selects and freezes document/location/previous-vector associations
before each≤200-record vector-copy page and stages those exact values together.
Publication rejects reassociation and any late uncovered current survivor chunk.
Failed target copy/persist/publication completes the exact read-only source
refresh, preserving the prior clean/searchable/reopenable generation. Retirement
occurs only after pointer publication, source refresh acknowledgment and actual
engine close. Automatic local triggers use certified12GiB metadata, checked
200000 capacity pressure with reclaimable records, and50% obsolete/tombstone
pressure. Empty rebuild handling and survivors+intended capacity checks precede
metered embedding; there is no automatic paid repair or bare compact.

Search authorizes candidates through active published inverse mappings. Compact
ports preserve valid ID/finite score plus optional decoded metadata; original
strict scan/search compatibility ports remain strict. Current Ready/version
ownership determines repair, including foreign-document metadata and malformed
location/body/identity fields. Orphans/obsolete rows cannot change Parsing,
Parsed, Indexing or Deleting state. Final rechecks remove earlier hits whose
owning document is later repaired and return repair on last-Ready loss.
Startup reset preserves Indexing with an unfinished matching Reindex job.

Search sends exactly the explicit partial/multi-row submitted query after
foreign-session validation. Borrowed title/body/location bounds precede source
copies. Actual escaped serde bytes admit an eligible score prefix≤32MiB, stop
at the first overflow without filling lower hits or inflating unavailable count,
and trim only trailing whole hits after the final envelope/readiness recheck.
Two outstanding tasks, cancellation on replacement/observer loss, operation
leases and registry joins preserve capacity/process lock across close/rearm.
Read-only search completes persist/publication/ack even on recoverable errors.

Only committed successful Import reserves/executes the awaited AfterTerminal
continuation. One settings/profile ConfigIo gate then SQL/fence rechecks admits
Reindex. Queue capacity, exclusion, no profile and shutdown keep Parsed plus
`auto_index_deferred`; stale version/SHA/parser/import/status/deleted/cancelled-
parent notices make no admission or repair mutation. Parent success is preserved
and startup never replays paid indexing. Collaborators do not retain scheduler
Arc cycles. AppState registers handlers/hooks once, reconciles locally before
jobs start, and joins searches before index close. Real command-boundary tests
cover import/autoreindex, exact selection, foreign-session zero-call rejection,
search/reindex/delete dispatch and safe errors.

Wire decoders enforce DTO/provenance/identity/source bounds, preserve exact query
whitespace/Unicode, accept every finite score, and conservatively defend32MiB
aggregate bytes without assuming JS/Rust float equality. The submission helper
keeps selection IDs only while the query equals the selected text. No visual
layout was changed.

## Focused commands and actual evidence

All log names below are siblings of this report. Cargo commands used the
accepted task-local TMPDIR/CARGO_HOME/ALSA sysroot/CARGO_TARGET_DIR environment,
`--manifest-path src-tauri/Cargo.toml --offline --locked --lib` followed by the
listed filter. Groups overlap; counts must never be summed. Behavioral REDs
preceded their fixes; new coverage of an existing path is labelled behavior.

| Filter or command | RED log / actual failure | Later GREEN or behavior log / actual result |
|---|---|---|
| `knowledge_migration5` | migration-red:1 failed, version4→5 missing | migration-green:1 passed |
| `npx vitest run src/lib/knowledge-wire.test.ts` | wire-red:2 failed/4 passed | wire-green:6 passed |
| embedding transport | embedding-behavior-red:4 failed compiled scaffolding | embedding-green:4 passed; embedding-expanded-green:7 passed, shared4 ownership, response/dimension/index extremes |
| `knowledge::index_tests` initial service | service-red:3 failed | service-green:3 passed |
| delete admission/reopen | delete-red:1 failed | delete-green:5 passed |
| `knowledge_re` | reconcile-red:2 failed/2 passed | reconcile-green:4 passed |
| `knowledge_search_preserves_healthy` | candidates-red:1 failed strict metadata abort | candidates-green:1 passed |
| `knowledge_search_two_slots` | search-ownership-red:1 failed | search-ownership-green:5 passed |
| `knowledge_import_` | import-red:2 failed | import-green:2 passed |
| `knowledge_commands_dispatch` | command-red:1 failed | command-green:1 passed; command-selection-green:1 passed real import/selection/dispatch |
| dirty-delete recovery | recovery-red:2 failed/1 passed; cleanup-red:2 failed/4 passed | repair-recovery-green:22 passed, including6 recovery cases |
| malformed repair isolation/version | repair-isolation-red:2 failed/4 passed; repair-fence-red:1 failed | consumer-expanded-green:27 passed |
| `knowledge_publication` | existing-path behavior | publication-behavior:2 passed with241 chunks and page240 rollback; formatted-publication-focused:2 passed |
| wire query/selection | query-wire-red:1 failed/6 passed; selection-wire-red:1 failed/7 passed | selection-wire-green:8 passed |
| generic AppState handlers | owned-registration-red:1 failed duplicate Reindex registration | app-ownership-green:15 passed with accepted cfg(test) pre-first-start helper |
| maintenance observer loss | maintenance-owner-red:1 failed stranded fence | maintenance-owner-green:1 passed |
| admission observer loss | admission-owner-red:1 failed stranded committed queue | admission-owner-green:1 passed |
| rebuild trigger | rebuild-trigger-red:2 failed/2 passed | rebuild-trigger-green:4 passed, certified threshold/cap/overflow/empty cases |
| AfterTerminal reservation/drain | existing-path behavior | after-terminal-owned-green:2 passed |
| `knowledge_app_` | original app-search-owned-green is actually1 failed/1 passed | app-search-owned-rerun:2 passed; initial pending-close/rearm denial and process-lock retention preserved |
| `knowledge_rebuild_rejects_late` | survivor-coverage-red:1 failed incomplete survivor accepted | survivor-coverage-green:1 passed |
| `knowledge_local_copy_freezes` | copy-fence-red:2 failed same-version SHA accepted after copy | copy-fence-green:2 passed, each SHA/parser/import/location/old mapping mutation |
| `knowledge_rebuild_failed_target` | rebuild-source-red:1 failed dirty source after target failure | rebuild-source-green (`knowledge_rebuild_`):4 passed |
| `knowledge_search_malformed_source_fields` | source-fields-red:1 failed location hid neighbors | source-fields-green:1 passed, location/body/identity variants |
| wire finite score | finite-score-red:1 failed/8 passed unsupported±1 restriction | finite-score-green:9 passed |
| `knowledge_binding_` | existing-path behavior | binding-behavior:2 passed, real two-core conflicting first-index race |
| `knowledge_import_continuation_` | existing-path behavior; compile/fixture attempts below | import-fallback-behavior:2 passed; import-fallback-formatted:2 passed, exit0 |
| recovery directory | recovery-directory-red:1 failed unexpected entry accepted | final-consumer-focused:43 passed |
| inconsistent DeleteInput | delete-payload-red:1 failed foreign fence deleted foreign doc | final-consumer-focused:43 passed |
| unfinished Reindex ownership | startup-reindex-owner-red:1 failed Indexing reset | final-consumer-focused:43 passed |
| `knowledge_delete_resume_advances` | delete-page-progress-red:1 failed first200 invalid rows blocked valid row201 | delete-page-progress-green:1 passed/0 failed/exit0, invalid rows retained and valid row201 removed |
| `knowledge::index_tests` expanded | overlapping focused group after all behavior fixes | final-consumer-focused:43 passed/0 failed/exit0, no warnings,75.07s |

The final43-test consumer group includes both actual byte defenses: distinct
score vectors produce a>31MiB/≤32MiB escaped prefix with no lower-hit filling;
another legal fixture fills the envelope to exactly32MiB, then changes unavailable
count0→10 before final recheck and retains only the leading whole hit and intact
heading. That test took over60s on this environment; it passed. The single-unfit
hit branch remains a defensive ResourceLimit path: with the accepted canonical
location≤16MiB, title≤32KiB and body≤9600-byte bounds a legal single hit cannot
naturally reach32MiB. No invalid oversized fixture is claimed legal coverage.

Production `cargo check --manifest-path src-tauri/Cargo.toml --offline --locked
--lib` completed cleanly in production-warning-rerun.log; a subsequent formatted
check completed cleanly in production-formatted-check.log. These are local
compiler checks, not the controller's desktop/full final gate.

## Preserved failures and corrections

Compile-only attempts are not behavioral RED: original embedding API/timeout
conversion, maintenance test Debug requirement, copy-fence sibling visibility,
and import fallback notice Clone fixture assumption. Import-fallback-fixture-
attempt failed on missing required `knowledge_document_imports.created_at_ms`;
only its test fixture changed before the actual2-pass behavior run.

`recovery-cleanup-green.log` is actually5 passed/1 failed: an injected post-pointer
failure was swallowed, allowing same-call cleanup. Already-committed-domain
cleanup errors now propagate; later repair-recovery-green and final43-pass
consumer groups supersede it. `search-bytes-behavior.log` failed because its
fixture assumed tie-score SrvDB ordering; distinct vectors corrected the fixture
and later actual byte tests passed. Neither fixture failure is a budget RED.

`app-search-owned-green.log` is actually1 passed/1 failed: its last ordinary close
after a successful close/reopen still used the fixture's40ms pending-close limit.
The expected initial close/rearm denial and actual entered-work/process-lock
assertions are retained; the fixture restores2s for later normal close. The
corrected rerun2/2 is the valid evidence. No production close timeout was changed.

Removing the blanket index allow(dead_code) exposed a non-test missing Mutex
import in production-warning-check.log. The shipping search registry now imports
its own Mutex; production-warning-rerun is clean. Strict compatibility facade
scan/search alone retain narrow not(test) dead-code allowances. Test-only rebuild
facade and direct runtime close helper are cfg(test); the unused re-export
allowance and production-used reserved annotations were removed.

The first Prettier attempt failed EROFS at the default npm cache and changed no
source. The controller-authorized task-local npm cache rerun used Prettier3.7.4
and formatted both owned TS files successfully; npm emitted only its unrelated
update notice. The controller's final check will use the observed existing
Prettier3.7.4 cache binary directly. No global npm configuration was changed.

## Self-review, paths and final ownership

Self-review resolved late survivor completeness, pre-copy association freezing,
failed target/source refresh, inverse-owner source field corruption, unfinished
Reindex reset, recovery-directory policy, DeleteInput association, direct-search
observer ownership, maintenance fence loss and admission wake loss with real
behavioral evidence. Durable pages, legacy evidence rejection, exact pointer
fences, source-close-before-removal and operation/worker/cap constraints remain.
Whitespace-only SQL string continuations preserve SQL bytes while making dense
closure bodies readable; focused publication and compiler checks verify them.

Exact task paths are in `task-5c-owned-source-paths.txt`; its53 paths now comprise52
author source/migration/wire/test files and the controller's canonical plan. The hash
manifest/checkpoint is recorded at handback. The protected `.gitignore` remains
SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.
The report/evidence artifacts are in the ignored SDD directory and are not source
commit paths. No source or Cargo ownership is handed back until the explicit
controller message containing the final manifest and pending-command state.

Controller-owned final gates still required on the frozen tree: offline/locked
knowledge/intelligence/all-target tests with intentional paid ignore preserved;
desktop build; focused/full frontend tests/build/format; cargo fmt check and
git diff check; independent Spec PASS/Quality APPROVED. The controller will send
actual log paths/exits/counts for this report. No duplicate full broad run was
performed by the implementation owner. Native/paid/minimum-OS/audio/driver/
signing/package/performance/RSS acceptance remain external gates, as scoped;
Tasks6–10 and visual layout remain outside this task.

## Original source/Cargo handback checkpoint

Timestamp: 2026-11-07T09:49:40.832947+00:00.
The51 author paths and1 controller canonical-plan path are frozen at
`task-5c-owned-source-sha256.txt` (manifest SHA256 `7941d11a1a0174a074fab7aef935992189f06dda0a7027075c7ee3b5f756a9c1`).
The final51-file author source has no pending command, edit or Cargo process.
The implementation owner explicitly hands source/Cargo ownership to Root for
the one broad verification run and independent review; further source edits
require a controller reassignment and invalidate this checkpoint.

Latest focused evidence: invalid-delete-page RED0/1→GREEN1/1; formatted import
fallback/stale notices2/2; expanded consumer43/43 (overlapping earlier groups,
before the final paging addition); formatted publication2/2; final production
check exit0/no warnings; focused48-file rustfmt check exit0 and observed offline
Prettier3.7.4 check exit0 for both owned TS files. Formatting wrapped SQL with
Rust escaped-newline continuations without changing its bytes. The source-path
manifest excludes protected.gitignore, task-local caches and SDD reports/logs.

Controller final gate results and independent-review outcome: awaiting actual
controller evidence; no full-task DONE claim is made by this handback.

## Pre-review migration fixture correction and re-freeze

The controller's accepted progress.md ruling treats the final intelligence
filter99-pass/1-fail result (`task-5c-final-intelligence.log`, exit101) as stale
fixture compatibility with additive migration5, before independent review.
`storage/sqlite_tests.rs` changed only two completed-upgrade assertions4→5:
`legacy_database_migrates_once_preserving_original_rows` and
`intelligence_migration_from_v2_reopens_twice_and_preserves_rows`. All original
row/export/reopen/data-preservation checks and intentional rollback/version
expectations0/2/99 remain unchanged. No shipping migration code changed.

Scoped command, using the accepted isolated environment:
`cargo test --manifest-path src-tauri/Cargo.toml --offline --locked --lib
storage::sqlite_tests` completed exit0,15 passed/0 failed, no warnings,
`task-5c-storage-schema-fixture-green.log`. File rustfmt --check completed
exit0 in `task-5c-storage-schema-fixture-format.log`. The source diff is exactly
two changed expected integer literals; all52 original frozen-path hashes match
the retained round0 manifest. This is a pre-review gate correction, not an
independent-review fix round.

Retained controller gates on unchanged implementation/frontend paths:
knowledge163/163 (exit0, `task-5c-final-knowledge.log`), wire9/9 (exit0,
`task-5c-final-wire.log`), frontend12 files/96 tests (exit0,
`task-5c-final-frontend.log`), frontend format exit0
(`task-5c-final-frontend-format.log`), and frontend build exit0
(`task-5c-final-frontend-build.log`, existing Vite PLUGIN_TIMINGS advisory).
These groups overlap and were not rerun by the scoped author. Root will
resume intelligence/all-targets/desktop/fmt/diff gates and independent review.

Re-freeze timestamp: 2026-11-07T10:02:00.904703+00:00.
Canonical path/hash manifests now contain53 task paths =52 author paths+1
controller plan, excluding the protected.gitignore and report/evidence artifacts.
Manifest SHA256: `172d891e12bb5dcd0997d27957439696f6ec827f61cd8c5b6c34fc7171f4222c`. No pending commands, Cargo processes,
source edits, staging or commits by the scoped author. Source/Cargo ownership
for this correction is explicitly handed back to Root; all further source
changes require controller reassignment and a new hash checkpoint. Full-task
completion remains pending actual final gates and independent-review evidence.

## Controller final verification — review pending

This section supersedes the earlier handback-stage pending gate statements.
Root completed every required final gate at frozen tree
`76677da1a712b77a10f17fd59b1bfcc2c31553a2`, with 53 task paths and canonical manifest SHA256
`172d891e12bb5dcd0997d27957439696f6ec827f61cd8c5b6c34fc7171f4222c`. The report-only author read
`task-5c-final-verification.json` and every actual referenced log, checking
recorded log byte counts and SHA256 hashes before transcribing the results.
No gate was rerun, source reviewed/edited, Cargo command issued, or staging/
commit performed during this report-only follow-up. Source/Cargo stay Root-owned.

All Cargo test/build commands below used the established isolated Task 5C
TMPDIR/CARGO_HOME/ALSA sysroot/CARGO_TARGET_DIR environment and offline/locked
dependencies. The format command used the same worktree/toolchain environment.

| Exact final command | Exit | Actual result | Actual log |
|---|---|---|---|
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib knowledge` | 0 | 163 passed / 0 failed; 77.03s (retained round0) | [task-5c-final-knowledge.log](task-5c-final-knowledge.log) |
| `pnpm exec vitest run src/lib/knowledge-wire.test.ts` | 0 | 1 file / 9 passed (retained round0) | [task-5c-final-wire.log](task-5c-final-wire.log) |
| `pnpm test:run` | 0 | 12 files / 96 passed (retained round0) | [task-5c-final-frontend.log](task-5c-final-frontend.log) |
| `node /home/mint/.cache/pnpm/dlx/5ec39437e8278f141a2042960ccd5aac/mw4tnriu-ft0e/node_modules/.pnpm/prettier@3.7.4/node_modules/prettier/bin/prettier.cjs --check src/lib/knowledge-wire.ts src/lib/knowledge-wire.test.ts` | 0 | Prettier3.7.4 passed both owned TS files (retained round0) | [task-5c-final-frontend-format.log](task-5c-final-frontend-format.log) |
| `pnpm build` | 0 | Production frontend build passed; 25.25s; advisory below (retained round0) | [task-5c-final-frontend-build.log](task-5c-final-frontend-build.log) |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib intelligence` | 0 | 100 passed / 0 failed; 1.72s | [task-5c-final-intelligence-rerun.log](task-5c-final-intelligence-rerun.log) |
| `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets` | 0 | lib 693; binary 5; real document worker 6; Volc offline 2 with 1 paid ignore; loopback unit 3; 0 failed | [task-5c-final-all-targets.log](task-5c-final-all-targets.log) |
| `cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --bin vox-bridge` | 0 | vox-bridge desktop binary built; 1m 14s | [task-5c-final-desktop.log](task-5c-final-desktop.log) |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 0 | cargo fmt --check passed; empty output | [task-5c-final-fmt.log](task-5c-final-fmt.log) |

The all-targets run's five distinct targets were: src/lib.rs 693 passed / 0
failed / 0 ignored; src/main.rs 5 passed / 0 failed / 0 ignored;
`tests/knowledge/document_worker.rs` 6 passed / 0 failed / 0 ignored;
`tests/provider-integration/volcengine.rs` 2 passed / 0 failed / 1 ignored; and
`tests/platform-audio/run-loopback.rs` 3 passed / 0 failed / 0 ignored. The preserved
ignore is `real_volcengine_translates_two_seconds_of_speech`: "paid Volcengine
request; explicit operator key and native keyring required". These offline
fixtures and loopback unit tests do not establish physical-audio, native-keyring
or paid-provider acceptance. All final Rust test/build logs contain no warnings.
Focused knowledge/intelligence/frontend groups overlap broader tests; their
counts are not summed.

The retained round0 knowledge, wire, full frontend, Prettier3.7.4 and frontend
build results apply because those source paths remained byte-for-byte unchanged
after the two storage test assertions 4→5 were corrected. Root retained the
round0 manifests/freeze artifacts and did not repeat those unaffected gates.
Final intelligence100/100 and all-targets validate the re-frozen 53-path Rust
tree. The original `task-5c-final-intelligence.log` remains recorded as exit 101,
99 passed/1 failed due to the stale expected schema 4; its authoritative
successor is `task-5c-final-intelligence-rerun.log`, exit 0/100 passed. This
fixture compatibility correction preserved legacy rows/export/reopen and
rollback 0/2/99 assertions and preceded independent review.

Frontend `pnpm build` exited0. Its existing Vite `PLUGIN_TIMINGS` advisory says
"JavaScript callbacks ran for 5.7s of this 25.2s build (23%)" and identifies
overlapping vite:css/@tailwindcss build callbacks. This timing advisory is
retained explicitly; it is not a failed build or a Rust warning, and provides
no production performance/RSS or native-package acceptance claim.

Root independently recorded all three whitespace checks with exit 0 and empty
output:

- `git diff --check`
- `git diff --cached --check`
- `git diff --check 1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee 76677da1a712b77a10f17fd59b1bfcc2c31553a2`

The controller also verified source/package hashes and the 53-path freeze; the
verification record timestamps source-hash validation at
`2026-11-07T10:19:14.758302+00:00`. Protected.gitignore and the main checkout's
pre-existing Cargo.lock remain outside author changes. There are no pending
controller gate commands.

Current status: **report updated; independent Spec/Quality review pending**.
The required final verification gates passed, but Task 5C is not complete until
independent Spec PASS and Quality APPROVED resolve all load-bearing findings.
Native platforms/minimum-OS/audio/driver/keyring/paid/signing/packages and
performance/RSS remain the previously scoped external gates. No completion,
commit, merge, push or publication is implied by the verification report.
