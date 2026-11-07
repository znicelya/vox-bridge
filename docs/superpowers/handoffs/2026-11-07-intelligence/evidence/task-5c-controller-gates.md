# Task5C controller acceptance map — 2026-11-07

This is a review/verification map, not a new product design or a completion
claim. Binding requirements: active brief, accepted architecture D1–D10,
source-plan excerpt, global constraints and dirty-delete recovery ruling.
BASE `1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee`. Author explicitly froze and
handed back source/Cargo on2026-11-07; Root is now the sole owner for final
verification, freeze, independent review and commit. Exact52 task paths are
staged at cached tree `56f69dc5ff0557615fed6d8f13a6e3a0377594db`, manifest
SHA256 `7941d11a1a0174a074fab7aef935992189f06dda0a7027075c7ee3b5f756a9c1`.
The protected .gitignore remains unstaged. See task-5c-source-freeze.json.
Latest re-freeze after the two legacy schema fixture assertions:53 paths,
frozen tree `76677da1a712b77a10f17fd59b1bfcc2c31553a2`, manifest SHA256
`172d891e12bb5dcd0997d27957439696f6ec827f61cd8c5b6c34fc7171f4222c`.
Final required gates passed; actual commands/exits/log hashes are in
task-5c-final-verification.json. Independent review is not yet dispatched.

## Evidence to require before source freeze

| Contract | Required evidence / review focus |
|---|---|
| Binding and migration5 | Additive version5, reopen/idempotence, NULL/Some partial uniqueness, legacy repair status, exact canonical semantic key and requested/actual dimension fencing; Some/None probe transition, first-index race, saved-profile/endpoint distinction. |
| Owned embedding | Full-profile snapshot equality before credential/HTTP; actual-operation ownership; shared4-request gate; actual serialized128KiB/32-input batching;8MiB responses; index reorder/duplicate/missing/out-of-range;128–4096/configured dimensions; finite nonzero L2 normalization; OnceTransient delegated without extra replay. |
| Atomic publication | Typed compact operation/fence references; <=200-row durable staging/borrowed validation pages; seal/mappings/pointer/Ready/repair state in one rollback-safe transaction; real >200-chunk publication, stale version/hash/parser/pointer/binding rejection. No in-memory document-wide ID plan. |
| Reindex | Real index Ready/search/consecutive-search/clean-reopen; all four injected commit boundaries; explicit retry after interrupted persist; replacement ID0→new-ID and old-version exclusion; cancellation/duplicate retry; no implicit paid repair. |
| Delete | Durable Deleting/work/mappings before vector mutation; persisted deletion before row/file cleanup; deletion/no embeddings across reopen; work and inverse mappings survive document/job cascade until explicit cleanup ack. |
| Dirty delete recovery | Prior Seal/Intent/exact whole vector witness checked before and after copy under exclusive source lock; ordinary read-only File only, no dirty SrvDB/VectorStorage/metadata/graph open; owned reader and <=200 bit-preserving pages; fresh survivor/deletion/pointer publication; two-doc survivor bits/search, last-doc empty destination, tamper preservation, pre/post pointer crashes, repeat reopen, zero embeddings. |
| Reconcile/rebuild | Active published inverse mapping determines actual corruption owner; unmapped/obsolete records never alter Parsing/Parsed/Indexing/Deleting; first0/last(count-1)/tombstone scan; orphan cleanup; missing/malformed/vector/dimension/hash/location failures; bit-preserving dense-ID swap; source refresh publish/ack, actual close, then durable retired cleanup. Capacity/tombstone and metadata watermark triggers stay local. |
| Search | Empty-library local success with identity=None; unhealthy/no-binding/late-last-Ready loss -> repair with no known-bad paid call; exact partial and multi-row submitted query, foreign-session rejection, bounded selection provenance; optional-metadata candidates preserve healthy hits and repair inverse owner, including foreign-document metadata. |
| Search bytes | Bound borrowed title/chunk/location before copies; actual escaped serde envelope <=32MiB; eligible score prefix stops at first overflow; single unfit hit -> ResourceLimit; no truncation/fill or unavailable inflation; final readiness/count/envelope recheck trims trailing whole hits only; resolved identity=Some(lowercase64hex). |
| Direct-search lifetime | Two actual outstanding slots, predecessor cancellation, watch into HTTP/owner envelopes, observer-loss capacity retention, task registry and operation owners joined across close; no early close/rearm/unlock while entered work survives. |
| Import continuation | Only committed Succeeded Import triggers immutable notice; child reservation while parent active; one config settings/profile snapshot gate then SQLite/fence; current Parsed/version/hash/parser and parent-success recheck; missing profile/queue-full/exclusion/cancel/deletion/stale notice fallback preserves parse with deferred repair; no startup paid replay, no handler/scheduler Arc cycle. |
| AppState/commands | Register Reindex/Delete/AfterTerminal once before scheduler start; index readiness then owned local reconcile then jobs; knowledge shutdown before index closure with direct-operation barrier/timeout ownership retained; search/reindex/delete command registry and DTO/error mapping tested at command boundary. |
| Wire defense | Three DTO strict decoders with per-field/candidate/identity/provenance bounds; heading16MiB/body8MiB preserved; conservative32MiB frontend aggregate defense without assuming JS/Rust float serialization equality. |
| Producer constraints | One actual index owner, exactly2 joined Rayon workers, queue16, <=2 engines including rebuild, <=200-record pages, one bounded reader lease,200000 records including tombstones, preflight file caps unchanged. Remove blanket allow(dead_code) after consumers land; record any deliberate non-consumed port narrowly. |

Focused groups overlap; do not sum their counts. Compilation-only API errors
are not behavioral RED. Log the actual intended runtime assertion failure and
GREEN command/result for each behavioral slice. Independent reviewer must
check the frozen source and tests rather than rely on the author's report.

## Final commands and integration gates

One Cargo owner, isolated target and the accepted local Cargo/ALSA/TMPDIR
environment. Run offline/locked knowledge and intelligence tests, then full
all-targets and desktop binary build on the final source. Run cargo fmt check,
git diff check, focused knowledge-wire tests and the normal full frontend
tests/build/format gates. Record actual exits, counts and any warnings; preserve
the intentional paid-service ignore. Broaden/repeat only for new edits,
failures or unresolved concerns.

Freeze the exact owned paths and hashes, excluding the existing `.gitignore`
SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.
The controller's canonical-plan migration6/7 renumbering is task-owned. No
Cargo.toml/lock change is expected. Main checkout's pre-existing untracked
src-tauri/Cargo.lock stays untouched. Independent Spec PASS and Quality
APPROVED, with no unassigned load-bearing finding, precede the task commit.

Tasks6–10 remain queued. Native/paid/minimum-OS/audio/driver/signing/package
gates are external and must never be inferred from these fixtures. No merge,
push or publication is authorized.
