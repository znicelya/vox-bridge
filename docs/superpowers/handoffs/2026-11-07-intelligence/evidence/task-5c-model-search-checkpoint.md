# Task 5C model/search checkpoint preparation

2026-11-07, read-only preparation at HEAD `c8f77708ce959f9ab3e454147329fcff3771cc37`. Consumed refreshed task-5-brief.md, progress.md rulings, accepted task-5b-index-owner-checkpoint.md/brief, committed 5A APIs, m4 tables and both completed import/search handoffs. No 5A/5B re-review, source/Cargo/index/native/network work or tests. 5B is being authored; its suggested method names are not existing producer APIs.

## Accepted boundaries

5A provides crate-private `JobContext::operation_owner()`, `ServiceConfigStore::scoped(OperationLease)` and `resolve_snapshot_credentials(ModelProfile)`. Owner issuance requires an active, uncancelled job and open generation; terminal cleanup retains an earlier token. ConfigIo retains scoped ownership inside actual entered blocking work after observer timeout/drop. Keep original resolve_credentials/probe behavior unchanged.

5B owns inert/start-owned IndexOwner, independent cached-engine base lease, bounded typed owner requests/repository receipts, trusted-live/clean distinctions and actual join lifecycle. Future 5C composes seal refresh, mappings, generation pointer and affected Ready state in one fenced SQL transaction. No config/HTTP/arbitrary callback enters its owner queue. Existing config→SQL→generation-control order remains binding.

## Embeddings carrier: precise interface still to settle

The brief's draft `embed(profile, inputs, cancel)` cannot convey actual-completion ownership; no Embeddings implementation currently exists. Proposed mandatory call parameter replaces cancellation-only input:

```rust
struct EmbeddingExecution {
    owner: OperationLease,
    cancel: watch::Receiver<bool>,
}
// Proposed: embed(profile, inputs, execution) -> Result<Vec<Vec<f32>>, Code>
```

Settle trait/type visibility and constructor before code; retain no unowned compatibility overload. Jobs construct it from an already-issued 5A token; direct search uses the actual AppState intelligence lease combined with its two-search permit. Execution/token clones retain entered config/index work and search capacity through real completion. Long-lived transport stores only unscoped config/HTTP; each call scopes config with its owner, performs exact full-profile resolution, moves opaque SecretString into existing JsonRequest, then invokes JsonHttp. Resolver failure becomes Configuration with zero HTTP calls. No secrets enter identities, payloads or diagnostics.

Accepted transport limits: ≤32 inputs/≤128KiB serialized request, ≤4 simultaneous requests globally, ≤8MiB response. Build batches by measured JSON bytes, not text sums. Preserve input indexes; reject duplicate/missing/out-of-range, non-finite/empty/zero vectors and inconsistent dimensions; checked L2 normalization and actual dimension 128–4096. Decide retry policy explicitly using existing HttpRetryPolicy rather than inventing HTTP machinery. Observer cancellation cannot release a retained config callback/accepted index envelope.

## Local binding and readiness

Accepted vector semantics are profile UUID, normalized endpoint, model, actual dimension and normalization version; execution comparison includes every public profile field. For configured dimensions=None, resolve exactly one published/trusted-live binding by semantic profile/endpoint/model/normalization and recorded requestedDimension option. Zero/ambiguous bindings fail IndexRepairRequired locally without HTTP. Some(d) requires matching actual dimension; None→Some(same d) preserves the old space. Never invent a discovery HTTP call, guessed dimension or fingerprint result identity.

Explicit first Reindex may discover actual dimension from its valid embedding response, then publish its binding with the successful mapping/Ready transaction. Concurrent first-index operations must converge or reject conflicting dimensions; settle the binding lookup/publication port and uniqueness fence. A truly empty library returns hits=[]/identity=None locally, even without a profile. A nonempty library with no healthy selected space fails before embedding; mixed healthy/broken returns only healthy hits and accurate unavailable count. Recheck after work: loss of the last healthy document returns repair. No translation or automatic keystroke/subtitle query; provenance IDs validate the exact submitted query without expanding it.

## Operation snapshot and continuations

Proposed immutable ticket records operation/job UUID, full admitted profile, resolved identity/generation, expected prior seal, document version/hash/parser/import association and paged work reference. It retains the existing owner through vector work, SQL publication and later cleanup. Coordinate cancellation/job Running checks with operation/generation/version checks inside publication; only committed publication can yield 5B's acknowledgment. Lost replies/acks require exact repository recovery, never persist-success inference. Adopt actual frozen 5B port signatures later.

Delete first persists Deleting plus paged prior-ID cleanup intent; persist tombstones before removing live rows/copies. m4 knowledge_index_work_items and knowledge_generation_mappings survive live-row deletion; keep intent/seal until cleanup acknowledgment. Rebuild retains maintenance ownership, copies ≤200 records preserving bits, publishes every new vector mapping with the pointer swap, then cleans the old generation. No third engine, bare compact or paid recovery replay.

Import auto-index admission follows actual committed Parent Succeeded/exclusion release. Reserve its continuation owner before terminal removal; capture selected Embedding plus profile under one ConfigIo gate, then SQL/fence checks and Manual Reindex admission. Existing settings()+with_profile_snapshot(ids) cannot atomically capture selection; a narrow new gate and AfterTerminal hook remain proposals. Queue32/child-capacity conflicts leave visible Parsed with manual Reindex, without parent failure/unbounded retries. Reject obsolete associations.

## Full-fidelity bounded hydration

Accepted: ≤100 compact candidates; one borrowed SQLite row before copies; title≤32768 UTF-8 bytes, chunk≤2400 scalars/9600 bytes, canonical location≤16MiB, raw heading wire≤16MiB; body8MiB unchanged. Verify active generation/published mapping/current Ready/version/hash, inline location and complete canonical digest. Decode only the bounded row; preserve raw headings/text.

Measure borrowed whole hits with checked count-only serde writer including escaped strings, scores, envelope/commas; admit the eligible score prefix within actual compact KnowledgeResults JSON32MiB. Stop at first eligible overflow, never truncate/fill from lower scores/count omission unavailable. One unfit valid hit yields ResourceLimit. Recheck readiness/envelope; remove only trailing hits if needed and cap final serializer. Frontend aggregate defense cannot assume JS/Rust float-byte equality. This bounds body allocation, not Tauri envelope/RSS.

Later evidence must cover owner loss, exact snapshot/no-HTTP failures, concurrent None binding, atomic receipt/publication/delete/rebuild, stale import continuation, selected partial query, legal raw-space/tab/6000-CJK headings, escaped JSON boundaries, prefix overflow and late readiness loss. No evidence is claimed here.

## Frozen 5B producer reconciliation — 2026-11-07

Read-only alignment against Task5B cached tree
`7312057b8069ac209cb1fa40675b0bce3ee3b5c1`. This does not claim 5B review or
start 5C source.

- `IndexOwner::new(root: PathBuf, store: Arc<LocalStore>)` remains inert;
  `start_owned(OperationLease)` returns the active owner epoch UUID.
- A private durable `WorkTicket` is obtained from `create(Generation, lease)` or
  `open_existing(Generation, lease)`. Its operation lease and queue envelope
  outlive an observer. `enqueue_open` and asynchronous add/scan/delete/replace/
  search/persist operations all use the one bounded owner queue.
- `persist` returns the private `PersistedReceipt`; `publish(ticket, receipt)`
  performs the fenced SQLite publication transaction and acknowledgment is
  returned to the owner with `acknowledge(ticket, ack)`. Future 5C must extend
  the publication plan inside that same transaction for mappings/pointer/Ready;
  it must not publish Ready separately around this port.
- `begin_maintenance` returns a non-cloneable `MaintenanceTicket` instead of a
  bare token. `copy_vectors(&ticket, start, limit)` yields at most 200 owned
  `(source_id, full_precision_vector)` rows and excludes tombstones; borrowed
  mapped slices never escape the owner. Explicit `end_maintenance(ticket)` or
  ticket Drop releases the same-source fence through a cancellation-protected
  fallback.
- Source maintenance starts only after a successful generated receipt, at which
  point the source engine is `Awaiting` publication. Future 5C therefore follows
  open→read/validate→persist→maintenance copy/rebuild ordering. It must not
  attempt source mutation while maintained or assume a bare token exists.
- The owner already enforces queue16, fixed owner plus two Rayon workers, clean-
  only engine LRU, at most two engines including source/staging, replacement
  append IDs, tombstones in capacity and bounded scans/searches.
- AppState now starts/joins the index owner as part of the same host generation
  as jobs. Close drains jobs and child operations first, then acquires all 16
  direct-operation permits and closes the index. A job-close timeout/error
  leaves the index cleanup available for retry; successful reopen uses a fresh
  index epoch.

5C still owns concrete document/work/fence records, actual dimension binding
lookup and publication, import continuation, cleanup intents, mapping batches,
the atomic pointer/Ready transaction, authoritative hydration and public
search/reindex/delete orchestration. The exact frozen 5B interfaces are expected
to compose with those future ports; 5C must not bypass them with a per-method
`std::thread::spawn`, arbitrary owner callback, bare compact call or separate
Ready publication.
