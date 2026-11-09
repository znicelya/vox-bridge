# Task5C complete repair — scoped independent re-review

Activation condition: Root dispatches only after the new explicit source/Cargo
handback and every required final-round1 gate has completed with exit0.
The first frozen full gate failed and is retained as original evidence.

Worktree: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation
Fix BASE: b72556091fc533dafb0a336907bf077fec377615.
The reviewed original source is d802613/tree76677da1; b725560 adds the
unchanged recovery archive. This is fix round1 for I1–I5/M1, not a new whole
branch review. Original independent reviewer has no fix source authorship.

## Concrete inputs

Read the binding repair plan:
docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md
and full accepted deletion ruling:
docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md.
Original report/findings: docs/superpowers/reviews/2026-11-09-task5c.md.

The final implementation report is this workspace's
task-4-completion-report.md plus final-gate-repair-report.md. Root's
final-round1/source-freeze.json binds every owned
source path/hash, input hashes, the immutable Git tree and complete package
hash. Read final-round1/complete-repair-review.diff completely in bounded passes, including
untracked source files represented by that immutable tree. Do not substitute
a HEAD-only or last-commit diff. final-verification.json binds actual complete
final logs; inspect those results and the author report's concrete focused
RED/GREEN logs. Earlier Tasks1–3/5 regression evidence is archived under
docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/ with raw SHA256
and actual test-result rows. An intermediate filename containing green can
still have a failed run; actual log results control. Counts overlap.

## Scope and process

Re-review all original findings in order and check the complete repair diff
for new breakage. Scope includes Root's earlier Tasks1–3/5 fixes and the
remaining Task4 author, all new knowledge modules/fixtures, and accepted
publication/cohort/file/retirement/terminal/GC contracts. Do not prejudge
severity or suppress an issue because a plan chose it. Code unchanged by the
repair belongs under explicit out-of-scope observations; identify any real
cross-task dependency rather than silently discarding it.

The audited actual-reap dependency is also in scope: intelligence/jobs.rs,
jobs/reap.rs, jobs/child_operations.rs and necessary focused jobs fixtures.
Inspect claim registration/rollback before Running visibility, continuation
pre-reservation then permanent factory closure, actual handler/child/process/
slot completion, bounded live-frame pruning, inert/fresh/prior-joined authority,
terminal_tx lock composition and the independently anchored consumed receipt.
These changes repair the measured terminal-but-unjoined GC path, preserve
admit_job/status/worker/queue/deadlines, and are not a new whole-branch review.

The post-freeze repair of the three measured full-gate failures is also in
scope. Read its separate brief/report and new exact logs: retain the original
no-certificate Storage/RepairRequired symlink behavior and assess the fixture
claim/wait corrections against real concurrency and complete cohort settlement.
The audited fixture scope may now include app/documents/tests.rs and
knowledge/index_tests/binding.rs; do not accept ignored tests, serial-only
success, weaker durable-state assertions or altered production limits.
The old40-path tree3afb692d and its745pass/3fail full log remain archived under
tracked handoff evidence/task-4-completion/failed-full-gate. Only the new frozen
tree and actual final-round1 results authorize current completion claims.

Read-only review: no Rust/Cargo/source/index/database/staging/commit/branch,
paid/network/native/keyring/audio/driver/global/provider actions. Write only
docs/superpowers/reviews/2026-11-09-task5c-repair.md. Never spawn agents.
Root has already run required final gates on the frozen source; do not repeat
suites. If reading raises a specific concern not answered by any existing
run, describe the exact narrow behavior/test needed to Root. Do not open a
SrvDB or production database for review.

Confirm complete fixed-source/package/input/log evidence attribution and
check private typed actual-receipt authority, bounded immutable proof caching
inside a single transaction, complete source-bound cohorts, original work /
managed path evidence, independently checked mutable states, actual reaped
terminal ownership and atomic complete-cohort GC. These are the concrete
accepted repair boundaries, not a verdict that the implementation meets them.

## Binding Global Constraints (verbatim)


- 首发最低版本：Windows 10 22H2、macOS 13 Ventura、Ubuntu 22.04 LTS。Linux 同时覆盖 PipeWire 和 PulseAudio 常见配置。
- Live sends translated speech to M1; calls independently route A→B to M1 and B→A to physical output. Facing another person in the room is outside first-release scope.
- 设备名称固定为逻辑端点 `VoxBridge Input`（M1，虚拟麦克风）和 `VoxBridge Output`（M2，虚拟扬声器）。应用不静默修改系统默认输入/输出设备。
- Preserve source/translation role, direction, ID, sequence, epoch and native/local correlation. Do not fabricate sentence pairs, timestamps, provider latency or playback completion.
- Use the approved Taste 2.1.5 `framed-sheet`: 228px sidebar, 48px top bar, white sheet inset 8px at right/bottom, 1px border, no resting shadow. IBM Plex Sans for interface text; IBM Plex Mono for session/device IDs and latency numbers. Colour share stays below 3%, on translated text, data/status and one primary action.
- shadcn/ui primitives and Lucide icons; accessible names, visible focus, labelled fields, required Sheet/Dialog titles. Tool panels move into a Sheet on narrow windows; subtitles remain primary.
- Final transcript, diagnostics, context and intelligence results are local. Audio and service keys never enter frontend output DTOs, ordinary logs, history exports or fixtures committed with real credentials.
- Key entry is write-only: the form temporarily holds the user's typed secret, submits it once, then clears it; saved keys are never read back into React. No key is put in localStorage, URLs, query keys, analytics or generic request logging.
- LLM/Embedding endpoints and models are configurable; SrvDB is pinned to the approved revision. Tavily is the first research provider; suggesting a candidate never performs a network request.
- Every network call, parser, queue, response body, page, worker count and cancellation wait is bounded. No paid endpoints, native keyring, audio hardware or privileged driver actions in default tests.
- Windows/macOS signing/packages, Linux utility behaviour, minimum-OS audio routes and actual paid-service acceptance remain external gates. Browser fixtures are not evidence of real sound or service quality.
- Keep the owner queue at 16, exactly two joined Rayon workers, at most two engines, 200-record pages, 200,000 stored records and the accepted 12 GiB watermark. Retain ownership until actual work and readers end. Never open a dirty source with SrvDB or add an embedding call to local recovery.
- Preserve the pre-existing staged `.gitignore` and its SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`. Root owns source freeze, serialized Cargo, final review and integration.


## Original findings under verification (verbatim)

**I1 — Important: generation replacement strands other pending deletes and retires their retained proof.**

The common admission boundary is [src-tauri/src/knowledge/service/delete.rs:31](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:31). In one transaction, admission captures the active source generation, stages that document's published vector work (lines 84–134), then sets its document/chunks to Deleting (lines 136–143). Admission does not take `Core.serial`; the DeleteHandler takes it only when execution begins (line 167). A queued or Running handler waiting for that mutex therefore already has a durable DeleteInput/work set bound to the old source.

The ordinary rebuild path is [src-tauri/src/knowledge/reconcile.rs:281](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:281). It selects only `Record::Healthy` survivors (lines 301–314); classification requires the owning document to be Ready at [src-tauri/src/knowledge/service/state.rs:349](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:349). A delete admitted before its source page is classified is omitted. `PublicationPlan::Swap` commits the new pointer at [src-tauri/src/knowledge/index/publication.rs:503](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/publication.rs:503); its completeness check covers Ready survivors and does not consume the pending delete (lines 577–586 and 699–726). Rebuild then refreshes/acknowledges the source, closes it, removes its physical generation, and removes its mapping/seal rows (reconcile lines 365–372). This is also a production consumer path: [src-tauri/src/knowledge/service/reindex.rs:27](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/reindex.rs:27) invokes rebuild before changing the reindex target to Indexing.

The retirement authorization at [src-tauri/src/knowledge/index/repository.rs:46](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/repository.rs:46) checks only that the target is now active and differs from the source. [src-tauri/src/knowledge/index/owner.rs:597](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:597) checks clean engine phases and actual reader closure, then removes the directory at line 644. Neither checks another retained delete/work association. `remove_generation_rows` first deletes mappings in committed pages, then attempts seal removal ([src-tauri/src/knowledge/reconcile.rs:506](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:506)). The work FK at [src-tauri/migrations/0004_knowledge.sql:121](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/migrations/0004_knowledge.sql:121) can reject that last SQL deletion, but cannot restore bytes/mappings already removed. Later retired cleanup explicitly deletes *all* work for the old generation before removing its seal ([src-tauri/src/knowledge/reconcile.rs:465](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:465)), without distinguishing a still-bound pending delete.

Witnessed recovery has the same problem through a distinct path. [src-tauri/src/knowledge/service/recovery.rs:74](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/recovery.rs:74) copies eligible survivors, excluding Deleting documents, but its Recovery plan completes only `input.document`. At lines 145–152 it acknowledges the destination, closes the reader, retires the old dirty source, acknowledges only `input.operation_id`, then removes generation rows. [src-tauri/src/knowledge/index/owner.rs:647](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:647) authorizes retirement through the exact dirty fence and a pointer that is no longer the source; [src-tauri/src/knowledge/index/recovery.rs:67](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/recovery.rs:67) does not fence another pending delete. With two retained deletes on the same source, the first recovered destination excludes both, but only one document row/work operation is completed.

The second delete cannot use its original continuation after either replacement. Clean execution requires the admitted old generation's clean seal/open ([src-tauri/src/knowledge/service/delete.rs:188](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:188)). Dirty recovery evidence requires that old generation to remain active ([src-tauri/src/knowledge/index/recovery.rs:64](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/recovery.rs:64)). After a Recovery pointer swap that condition fails even if bytes were retained; the fallback clean delete also fails on the dirty source. After physical retirement neither path can reconstruct the old proof. [src-tauri/src/knowledge/service/recovery.rs:45](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/recovery.rs:45) preserves the remaining Deleting row on failure, leaving it stranded; its identity-repair fallback can also quarantine otherwise-valid Ready survivors.

This violates retained deletion work/proof and normal local crash recovery, not the reader-close ordering, which is correctly implemented. See [.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-delete-recovery-ruling.md:3](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-delete-recovery-ruling.md:3), lines 58–74, and the independent-work retention contract at [src-tauri/migrations/0004_knowledge.sql:102](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/migrations/0004_knowledge.sql:102).

Required repair: account for every admitted delete that refers to a source across survivor selection, pointer publication, retirement and cleanup. Keep a provable continuation for each, or safely complete/carry its deletion proof as part of replacement. Never physically retire source proof or erase another operation's work merely because the active pointer moved. A SQL RESTRICT failure after byte removal is insufficient. Root owns the constrained implementation choice; retain the existing one-owner/two-worker/two-engine/200-row/zero-local-embedding limits.

Required deterministic regressions, owned by Root/fix author:

- Ordinary Swap: start a consumer-driven automatic rebuild/reindex, pause at `BeforeLocalCopy` on the first source page, and admit a delete for an indexed document whose vectors are on a later page beyond ID 199. Its handler must remain queued or Running while waiting for serial. Release the rebuild, then finish/reopen reconciliation. Verify the delete completes locally, healthy survivor bits/searchability remain, and no source proof/work is discarded early. A delete admitted after its *already-selected* current-page survivor was frozen correctly makes Swap roll back; that case alone does not exercise this defect.
- Dirty Recovery: retain two interrupted same-source indexed deletes, reopen, and run local recovery. Verify both deletions finish, survivor vector bits remain, both work sets are acknowledged only after valid proof, cleanup is idempotent, and embeddings remain at zero. Include a pointer-publication/retirement interruption.

Existing tombstone-trigger fixtures wait for deletion to Succeed before reconcile ([src-tauri/src/knowledge/index_tests/publication.rs:218](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/publication.rs:218) and line 267). Existing recovery fixtures cover one deletion target per source ([src-tauri/src/knowledge/index_tests/recovery.rs:34](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/recovery.rs:34), 86, 149, 182). Their successful results do not cover these pending-owner interactions.

**I2 — Important: incomplete Ready document coverage is counted unavailable but its remaining chunks can still be returned.**

[src-tauri/src/knowledge/service/state.rs:113](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:113) defines a healthy document by *every* current chunk having a matching published active-generation mapping. However, [src-tauri/src/knowledge/reconcile.rs:180](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:180) audits only existing mappings; a missing mapping cannot appear in that enumeration and cannot repair its authoritative owning document. `classify` at state lines 318–401 checks only the candidate's own mapping/chunk and current Ready state. [src-tauri/src/knowledge/search.rs:293](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/search.rs:293) hydrates only that candidate, and the final recheck at lines 171–211 likewise checks only the hit's own chunk. Neither applies the whole-document coverage predicate used by health.

Concrete counterexample: index one healthy one-chunk document and a second two-chunk document, then remove the published mapping for one chunk of the second document in SQLite. The remaining mapped chunk still classifies Healthy. Local reconcile tombstones the now-unmapped vector as an orphan, but never repairs the second document. One obsolete record among three is below the accepted 50% rebuild trigger, so no completeness-checking Swap intervenes. Health reports one healthy/one unavailable, while search can still return the surviving chunk of the unavailable document alongside the healthy result. This contradicts document-wide Ready/search eligibility and corruption exclusion.

Required repair: audit authoritative current Ready chunks/coverage in bounded pages, attribute missing mappings through their SQLite document association, and recheck UUID/version/Ready inside repair transactions. Apply complete-document eligibility consistently to hydration and final recheck. Preserve Parsing/Parsed/Indexing/Deleting states and the healthy neighbor.

Required regression, Root/fix author: the mixed one-chunk healthy/two-chunk broken fixture above, before and after local reconcile, with stored count three and deterministic scores. Every broken-document hit must be excluded, its current association must receive the correct repair/unavailable state, the healthy hit must remain, and reconcile must perform zero embeddings. Existing missing-metadata/foreign-metadata fixtures and the late-*new*-survivor-chunk Swap rollback at [src-tauri/src/knowledge/index_tests/publication.rs:300](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/publication.rs:300) exercise different cases.

**I3 — Important: runtime, startup and terminal logic do not prove the durable job's document association.**

The briefing explicitly requires bounded matching job association before trusting unfinished Reindex evidence ([.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-review-brief.md:131](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-review-brief.md:131); see its late-gap paragraph). Startup preservation at [src-tauri/src/knowledge/reconcile.rs:16](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:16) trusts kind, `object_id`, unfinished status and a raw JSON `input.document_version`; it neither bounded-decodes the envelope/IndexInput nor compares `input.document_id` to the durable object. A retained Reindex for object A with input B and the same version therefore preserves A's Indexing row as though it had a valid owner. Raw `json_extract` also evaluates unbounded/unvalidated retained payloads.

The live ReindexHandler decodes the envelope and IndexInput at [src-tauri/src/knowledge/service.rs:247](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service.rs:247), then `Core::reindex` mutates the input document at [src-tauri/src/knowledge/service/reindex.rs:36](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/reindex.rs:36). It never compares that UUID to the durable job object/exclusion. The named scheduler producer check confirms this is not supplied implicitly: JobContext has only job ID/cancellation/fence/child ownership ([src-tauri/src/intelligence/jobs.rs:24](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs.rs:24)), and commit/publication fences check only Running/cancellation/generation state (lines 36–89). Generic `decode_payload` validates envelope/profiles, not domain association ([src-tauri/src/intelligence/store.rs:211](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/store.rs:211)). Thus a bounded, otherwise-valid retained job for A can reindex B under A's exclusion and can leave A Indexing after that job Succeeds.

DeleteHandler has the equivalent missing external association at [src-tauri/src/knowledge/service/delete.rs:154](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:154). Its new checks correctly compare DeleteInput's internal ID/version/fence and operation ID to the context ID, but not its document ID to `job.object_id`. If two parsed documents A/B have admitted deletes and their handlers wait on serial, a retained job A payload consistently rewritten to B's full DeleteInput, retaining operation ID A, passes those checks. With no index/work branch, it removes B under job A while A remains Deleting. The current inconsistent-input fixture at [src-tauri/src/knowledge/index_tests/recovery.rs:343](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/recovery.rs:343) tests disagreement *inside* DeleteInput, not this durable-job mismatch.

Terminal handling is also partial: [src-tauri/src/knowledge/service.rs:270](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service.rs:270) trusts object/version JSON for its Indexing repair, while its subsequent repair-reason upsert (lines 282–290) has no version fence and can overwrite the repair reason of a newer current association merely because an old job names the same object.

Required repair: verify bounded envelope/domain input and matching durable job kind/object/exclusion/current association before state changes or metered work; preserve that proof at publication and terminal/startup decisions. Retain the existing operation-ID and internal DocumentFence checks. Fence every repair-reason write to the same current association; do not infer ownership from a matching version number alone.

Required regressions, Root/fix author: valid unfinished Reindex ownership still preserves Indexing; wrong input document with matching version, missing/oversize/malformed payload and invalid kind/profile association cannot supply ownership proof; a runtime Reindex for A/input B fails before embedding or foreign writes; two consistent DeleteInputs bound to the wrong durable job cannot remove B; stale terminal jobs cannot overwrite a newer repair association. The positive owner fixture at [src-tauri/src/knowledge/index_tests/search.rs:742](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/search.rs:742) has no negative association variants.

**I4 — Important: delete work IDs mutate the engine before their association is validated, and the later validation is incomplete.**

At [src-tauri/src/knowledge/service/delete.rs:207](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:207), each retained work page is read as only ordinal/vector ID, then sent directly to the engine (lines 225–233). [src-tauri/src/knowledge/index/owner.rs:431](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:431) validates batch/count bounds but does not prove which document owns those IDs. The Delete publication at [src-tauri/src/knowledge/index/publication.rs:436](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/publication.rs:436) runs only *after* actual tombstoning/persist and checks identity/generation/document ID, not the work vector's association to the retained published target mapping/chunk/version/hash. The staged full document evidence is not examined there.

Change one admitted target work vector ID to another Ready document's valid in-range vector ID in the same generation, leaving its work identity/generation/document fields unchanged. The foreign vector is tombstoned and persisted; the publication's foreign-row query is false, so the intended target row is deleted. `ack_delete` then removes the foreign mapping by identity/generation/vector alone ([src-tauri/src/knowledge/service/delete.rs:267](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:267)). The healthy foreign document loses its searchable vector/mapping. If the work's identity/document is malformed instead, publication rejection occurs too late to preserve the source from the already-applied mutation.

This is a malformed retained deletion-proof gap, separate from I3's payload/job association and I1's retirement. The canonical u64 CHECK at migration4 lines 110–115 prevents invalid encodings; it cannot establish vector ownership.

Required repair: validate each operation's complete retained delete work against authoritative prior published mappings and the target fence before scheduling any mutable vector delete, in bounded pages, then preserve/recheck the association through publication and acknowledgment. Historical target versions must remain supported; do not require deleted/replaced historical chunk rows to exist when their durable published mappings are the intended cleanup evidence. Reject missing/malformed/foreign proof without tombstoning a different current document or removing the target/work/source.

Required regression, Root/fix author: hold serial, admit an indexed target delete, change its work ID to a healthy neighbor's valid vector, and release the real handler. Verify safe failure before source mutation, intact foreign vector/mapping/searchability, and retained target Deleting/work. Also exercise a malformed work association on a later page, so an earlier valid page cannot mutate the source before whole-operation validation, and repeat local resume without embeddings.

**I5 — Important: an authoritative title above the byte limit aborts mixed search instead of isolating its owner.**

The accepted search policy says invalid bounded source fields use local repair, excluding their document ([.superpowers/sdd/2026-11-04-workbench-intelligence/task-5-search-budget-handoff.md:14](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-04-workbench-intelligence/task-5-search-budget-handoff.md:14) and [.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-architecture-reconciliation.md:591](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-architecture-reconciliation.md:591)). `classify` does not inspect the document title ([src-tauri/src/knowledge/service/state.rs:318](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:318)). A current published candidate with otherwise-valid body/location/hash therefore passes classification. [src-tauri/src/knowledge/search.rs:326](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/search.rs:326) then propagates `state::text(...,32768)?`, whose over-byte-limit result is ResourceLimit (state lines 12–22). That aborts the entire result, leaves the bad owner Ready, and hides a healthy neighbor. The borrowed read prevents an oversized copy, but does not implement the required corruption-isolation policy.

Required repair: bound/validate the title through the active inverse owner before copying, repair only its matching current Ready association, skip all hits from that document, and retain healthy results. Keep valid whole-hit byte-budget overflow separate from invalid-source repair.

Required regression, Root/fix author: add title variants to the mixed-corruption fixture at [src-tauri/src/knowledge/index_tests/search.rs:597](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/search.rs:597), which currently covers only location/body/identity. Include a title over 32,768 UTF-8 bytes, including a multibyte case whose scalar count alone would fit. Expect the healthy neighbor, one unavailable repaired document, exact title/body fidelity for retained hits, and no repair re-embedding.

**M1 — Minor: the required over-byte-limit compact work-evidence regression is absent.**

The explicit required coverage at [.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-work-evidence-ruling.md:46](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-04-workbench-intelligence/task-5c-work-evidence-ruling.md:46) includes over-byte-limit evidence rejecting before Ready. The new migration CHECKs are present and the borrowed decoder enforces 1,024 bytes ([src-tauri/migrations/0005_knowledge_index.sql:15](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/migrations/0005_knowledge_index.sql:15) and [src-tauri/src/knowledge/index/publication.rs:649](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/publication.rs:649)). The multi-page fixture covers `{}` and NULL at ordinal 240 ([src-tauri/src/knowledge/index_tests/publication.rs:71](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/publication.rs:71)), and the migration5 reopen fixture tests binding uniqueness/legacy absence ([src-tauri/src/knowledge/document_tests.rs:6](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/document_tests.rs:6)), but neither tests the new evidence byte boundary. The full owned fixture review and constrained searches found no such case. This is a missing mandated regression, not a claim that the visible CHECK is incorrect.

Required completion, Root/fix author: add an actual 1,024/1,025-byte evidence boundary and multibyte raw-byte rejection through the relevant SQLite/publication path, proving no Ready/seal/pointer partial publication. Keep the existing NULL/malformed/page-240 rollback cases. New location-digest/previous-ID constraints should also retain their canonical-boundary assertions when this fixture is extended.


## Report contract

Begin with per-finding ADDRESSED / NOT ADDRESSED and exact file:line behavior
evidence for I1–I5 and M1. Record new Critical/Important/Minor breakage with
concrete consequences and requested repair; out-of-scope observations are
separate. Include any Cannot verify items with a concrete owner/check.
Provide both Spec PASS/FAIL and Quality APPROVED/NEEDS_FIXES plus counts.
Final short return: finding verdicts, new breakage summary, report path,
Spec/Quality verdict. An attempted fix or passing tests alone is not approval.
