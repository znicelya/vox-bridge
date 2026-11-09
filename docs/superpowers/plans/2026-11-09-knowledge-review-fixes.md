# Knowledge Task 5C Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Continue the already-authorized work in the existing implementation worktree; this is a repair cycle for the accepted Task 5C contract.

**Goal:** Resolve I1–I5 and M1 from the independent Task 5C review, preserving locally recoverable deletion and healthy search results.

**Architecture:** SQLite proves document and job ownership; the dedicated SrvDB owner proves actual persistence. Validate a complete deletion work set before any mutable engine operation. Generation replacement must account for every admitted source deletion and retain its explicit completion proof until real file cleanup and work acknowledgment finish.

**Tech Stack:** Rust, Tauri 2, SQLite, pinned SrvDB, Tokio, existing two-worker Rayon owner pool; real local-index fixtures with synthetic embeddings.

**Spec:** [Independent review](../reviews/2026-11-09-task5c.md), [accepted implementation plan](2026-11-04-workbench-intelligence.md), and the accepted Task 5C architecture, deletion-recovery and compact-work rulings in the [recovery archive](../handoffs/2026-11-07-intelligence/README.md).

## Global Constraints

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

## Task 1: Whole-document search eligibility and title isolation — I2/I5

**Files:** `knowledge/service/state.rs`, `knowledge/search.rs`, `knowledge/reconcile.rs`, `knowledge/index_tests/review_fixes.rs` under `src-tauri/src`.

**Interfaces:** `ready_coverage(tx, document, version, identity, generation) -> AppResult<bool>` and `mark_document_repair_tx(tx, document, version, reason) -> AppResult<()>`. Hydration, final search recheck and a bounded authoritative Ready-document audit consume the same coverage predicate.

- [x] Add the mixed one-chunk healthy/two-chunk broken fixture and remove only the broken document's last mapping. Include corruption during final search recheck and ASCII/multibyte titles over 32,768 bytes.
- [x] Observe actual RED: four failed tests, including two hits instead of one, Ready instead of repair-required, and whole-query ResourceLimit.
- [x] Apply complete current-chunk coverage before hydration and final recheck; audit authoritative Ready documents in pages; repair invalid titles through the current inverse owner.
- [x] Observe GREEN: four tests passed; the subsequent job-proof group also retained these four passes.

```rust
assert_eq!(result.hits.len(), 1);
assert_eq!(result.hits[0].text, "48V");
assert_eq!(result.unavailable_documents, 1);
assert_eq!(document_status(&h, broken), "repair_required");
```

## Task 2: Bounded durable job association — I3

**Files:** `knowledge/service/job_proof.rs`, `service.rs`, `service/{delete,reindex,recovery}.rs`, `index/publication.rs`, `reconcile.rs`, `index_tests/review_jobs.rs`.

**Interfaces:** strict `reindex(tx, job_id)` / `delete(tx, job_id)` produce a `DocumentJobProof`; `require(tx, document_fence)` rechecks kind, object, exact exclusion and bounded payload hash. Local recovery variants isolate invalid retained envelopes while propagating operational storage failures.

- [x] Add live wrong-document Reindex/Delete, stale terminal repair, pre-Ready object drift and ten startup-envelope variants, including the valid owner.
- [x] Observe actual RED: all five new tests failed; malformed startup JSON aborted reconcile and other invalid ownership variants preserved Indexing.
- [x] Validate strict domain input and embedding-profile kind before local index mutations or embedding. Preserve full document and job proof through Ready/Delete/Recovery publication. Replace SQL JSON extraction with bounded decoding. Upsert terminal repair reasons only when the same version's Indexing transition actually occurs.
- [x] Observe GREEN: all nine search/job regression tests passed, without Rust compiler warnings.

```rust
assert_eq!(result.status, JobStatus::Failed);
assert_eq!(result.error, Some(IntelligenceErrorCode::Configuration));
assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
```

## Task 3: Complete retained deletion work proof — I4

**Files:** create `knowledge/index/delete_work.rs`; modify `service/delete.rs`, `service/recovery.rs`, `service/state.rs`, `index/publication.rs`; test in `index_tests/review_deletion.rs`.

**Interfaces:** paged validation returns the operation's count and fingerprint; page readers prove canonical fields, full target fence and exact prior published mapping. Historical versions use their durable mappings; only current-version mappings require a current matching chunk.

- [x] Add first-page and page-200 foreign-vector mutations, malformed later association/fence, missing work and historical-chunk removal fixtures.
- [x] Run RED and retain actual failure logs. Correct the foreign-association fixture to use a malformed document ID allowed by SQLite, and separate each negative wrapper. Give the healthy neighbor a distinct vector so approximate-search ties cannot hide it.
- [x] Validate the entire operation before opening a mutable index intent. Revalidate each mutable page and the full proof at publication. Acknowledge mappings only through the full exact document/version/chunk/hash association. Classify pending mapped deletions as retained work so orphan cleanup cannot consume their proof.
- [x] Run GREEN; repeat local recovery with zero embedding calls and intact foreign vectors, mappings and target work. Seven tests passed. A separate RED exposed clean-neighbor quarantine; the next RED exposed orphan/retirement loss. Limit quarantine to dirty active sources and retain Deleting records when calculating automatic rebuild thresholds.

```sql
UPDATE knowledge_index_work_items SET vector_id='0'
WHERE operation_id=?1 AND ordinal=200;
```

```rust
assert_ne!(h.wait(job.id).await.status, JobStatus::Succeeded);
assert_eq!(document_status(&h, target), "deleting");
assert_eq!(result.hits[0].document_id, healthy_neighbor);
```

## Task 4: Every-source-delete replacement and explicit cleanup continuation — I1

**Files:** create `index/delete_completion.rs` and `index/delete_completion/{journal,publication,files,retirement}.rs`; modify `index/{delete_work,repository,owner,publication}.rs`, `service/{delete,recovery}.rs`, `reconcile.rs`; test in `index_tests/review_replacement.rs`. The audited actual-reap dependency also owns `intelligence/jobs.rs`, `intelligence/jobs/{reap,child_operations}.rs` and necessary jobs fixtures; this is the existing sole author's narrow scope extension. Follow the [complete deletion proof ruling](../reviews/2026-11-09-delete-proof-ruling.md).

**Interfaces:** fresh Swap/Recovery publication accounts for every source-bound admitted delete. A temporary strict completion certificate in existing `intelligence_meta` binds the operation, full input, job payload, persisted receipt and paged work fingerprints. Its reference remains until actual managed-file cleanup and individually checked work acknowledgment end. No scheduler-status write or new table/migration is required.

The measured terminal-but-unjoined cleanup path additionally consumes
`JobScheduler::reap_port() -> JobReapPort` and
`JobReapPort::prove(&Transaction,Uuid) -> AppResult<Option<JobReapProof>>`.
The separate actual-claim registry never re-locks terminal_tx's control mutex.
Every claim registers before Running, pre-reserves continuation ownership,
permanently closes its factories and records handler end/actual child drain
before safe frame pruning. Complete issued/joined/retained accounting and
explicit inert/current/joined lifecycle authority replace status/absent-frame
inference; a rowid frontier is auxiliary. Knowledge binds the opaque actual
proof to a separate immutable header-bound consumed receipt, using the raw
payload digest expected by the producer while retaining its original domain
job/header fingerprint unchanged. A mutable consumed flag is insufficient.

- [ ] Add consumer-driven automatic reindex/rebuild paused at the first `BeforeLocalCopy`; admit a real delete beyond vector ID 199 while its handler waits for serial.
- [ ] Add two admitted same-source deletes interrupted before publication, including a pointer-publication interruption, empty replacement, malformed/zero-work retention and document-gone/work-retained cleanup.
- [ ] Run RED and retain actual source/work failure evidence.
- [ ] Fence every pending input/work association in the pointer transaction. Require actual fresh persisted record count, complete staged dense-ID coverage, and target absence across destination mapping versions/states. A changed already-selected survivor rolls back. Record explicit committed completion for each target; retain every source and work/certificate reference until cleanup ends.
- [ ] Resume document-gone cleanup only from its exact certificate and matching job/work. Validate each original 200-row page fingerprint before acknowledgment. Reject missing/malformed proof without source retirement.
- [ ] Guard physical removal and all paged SQL cleanup before the first destructive action. Completed, acknowledged historical payloads stop pinning; unresolved work pins independently of document/job status.
- [ ] Prove real pre-entry/awaiting-reap/entered-child ownership, claim rollback,
  missing-frame accounting failure, out-of-order eligible claims, exact
  rowid/payload drift, continuation-before-factory-close and fresh/joined
  reopen. The producer/consumer use the existing worker2/child16 capacities;
  actual process/slot release precedes completion. Fully validated consumed
  history may skip redundant resume work; retirement/GC still checks original
  actual files and complete certificates.
- [ ] Run GREEN and repeat reopen/reconcile. Check survivor f32 bytes, both acknowledged work sets, idempotent cleanup and zero local embeddings.

```rust
assert_eq!(h.wait(reindex.id).await.status, JobStatus::Succeeded);
assert_eq!(h.wait(delete.id).await.status, JobStatus::Succeeded);
assert!(!exists(&h, deleted));
assert_eq!(&new_vectors[24..24 + 128 * 4], survivor_bits.as_slice());
```

The independent technical review accepted jointly anchored immutable records under the stated localized-corruption model. Its remaining edges are included in the ruling: complete streamed cohorts, actual cleanup paths, independently checked mutable flags, settled failed history and atomic journal GC. Fresh destination receipt/count/coverage is positive exclusion evidence; a SQL-only absence test against an arbitrary already-sealed generation is insufficient.

## Task 5: Raw-byte work evidence boundaries — M1

**Files:** `knowledge/index_tests/publication.rs` and focused migration/work-evidence fixtures.

- [x] Exercise actual 1,024-byte acceptance and 1,025-byte SQLite rejection, including a multibyte value whose scalar count fits.
- [x] Inject oversized retained evidence in the isolated fixture with CHECK enforcement restored before publication; require rollback of all pages, Ready, seal and pointer. Three tests passed; these cover already-correct production bounds rather than inventing a production defect.
- [x] Preserve existing NULL/malformed/page-240 regressions and canonical previous-ID/location digest boundaries.

```rust
assert_eq!(evidence.as_bytes().len(), 1024);
assert!(oversized_write.is_err());
assert_eq!(published_mapping_count, 0);
assert_ne!(document_status(&h, document), "ready");
```

## Task 6: Final verification, independent repair review and integration

- [ ] Run appropriate focused GREEN groups after each substantive fix; inspect each actual exit and failure assertion.
- [ ] Freeze the final owned sources and hashes. Keep unrelated staging intact.
- [ ] Run offline/locked Rust all-targets once on that source, production check/build and Rustfmt. Existing unchanged frontend evidence remains attributable to its prior frozen source; rerun only if frontend/build inputs change or a new concern requires it.
- [ ] Request an independent review of the complete repair diff, original I1–I5/M1 findings, new rulings and actual logs. Resolve Important findings before proceeding.
- [ ] Archive the verdict, implementation/failure evidence and current Task 6 checkpoint in tracked docs. Integrate only the reviewed task paths under the existing user authorization.

```bash
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --lib
cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --bin vox-bridge
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Every Cargo invocation uses the existing task-local Cargo home, ALSA sysroot and target override from the Task 5C environment. Source/Cargo are Root-owned and serialized. Tasks 6–10 remain outside this repair cycle until its independent gate passes.
