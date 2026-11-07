# Task 5C architecture reconciliation checkpoint

Status: read-only reconciliation. HEAD `1ea0bdd` (`1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee`)
verified at start. Read in order: task-5-brief.md, task-5c-model-search-checkpoint.md,
task-5c-ownership-decision.md, task-5-import-continuation-handoff.md,
task-5-search-budget-handoff.md, task-5-seal-format-handoff.md,
task-5-job-credential-ownership-handoff.md, global-constraints.md, the completed 5B
report/checkpoint/brief and 5A/4B checkpoints, then committed Rust/frontend/Cargo.
No Cargo run, no mutation, no index open, no network, no agents. This is an
implementation checkpoint; no 5C source exists yet and no success is claimed.

All references are relative to the repository root. `S` below means the pinned
SrvDB checkout `.superpowers/knowledge-source/srvdb/crates/srvdb` at
`1eec46feb2b7e6a6e286fa3361a20cd771dd4e04` (excluded from recursive searches).

## 1. Exact existing interfaces and how they compose

### 1.1 Index owner facade (committed 5B)

`src-tauri/src/knowledge/index/lifecycle.rs`:

- `IndexOwner::new(root: PathBuf, store: Arc<LocalStore>) -> Self`, inert (:180).
- `start_owned(lease: OperationLease) -> AppResult<Uuid>` starts one owner thread,
  queue capacity 16 (`mpsc::sync_channel(16)` :651), returns the owner epoch
  UUID (:638). Readiness timeout 5 s, stop observation 10 s (:36-37).
- `create(generation: seal::Generation, lease: OperationLease) -> AppResult<WorkTicket>`
  (:275) commits a `Create` intent via `IndexRepository::create`.
- `open_existing(generation: seal::Generation, lease: OperationLease) -> AppResult<WorkTicket>`
  (:290) commits an `Open` intent; rejects while a publication transition fence
  exists for the same `(identity, generation)` (:296-312).
- One bounded owner queue carries `Command` variants `Open / Add(Vec<(Vec<f32>, ChunkMetadata)>) /
  Scan{start,limit} / Delete(Vec<u64>) / Replace{id,vector,metadata} /
  Search{vector,limit} / BeginMaintenance(Arc<AtomicBool>) / CopyVectors{token,start,limit} /
  EndMaintenance(Uuid) / Persist / Publish(PersistedReceipt) / Acknowledge(PublicationAck)`
  (:98-119). Each `Envelope` retains `intent`, an already-issued `OperationLease`
  clone, a cancellation flag and a oneshot reply (:140-147). Cancellation is
  checked only before entry; entered synchronous work finishes before reply
  (:977-1047).
- Async operation ports: `enqueue_open` (:365), `add(ticket, rows) -> Vec<u64>`
  (:372), `scan(ticket, start, limit) -> Vec<(u64, Option<ChunkMetadata>)>`
  (:385), `delete(ticket, ids) -> Vec<bool>` (:403), `replace(ticket, id, vector,
  metadata) -> u64` (:412), `search(ticket, vector, limit) ->
  Vec<(u64, f32, ChunkMetadata)>` (:435), `begin_maintenance(ticket) ->
  MaintenanceTicket` (:453), `copy_vectors(&MaintenanceTicket, start, limit) ->
  Vec<(u64, Vec<f32>)>` (:478), `end_maintenance(MaintenanceTicket)` (:503),
  `persist(ticket) -> PersistedReceipt` (:512), `publish(ticket, receipt) ->
  PublicationAck` (:531), `acknowledge(ticket, ack) -> ()` (:555),
  `shutdown() -> ()` (:742).
- `WorkTicket { intent: DurableIntent, lease: OperationLease, cancelled }` (:44-48);
  `MaintenanceTicket` is non-cloneable with cancellation-protected Drop release
  (:53-63, :87-97).

`src-tauri/src/knowledge/index/owner.rs`:

- `Runtime` owns the SrvDB engines, a 2-worker Rayon pool with retained
  JoinHandles (`PoolOwner`, :711-744), at most two engine entries
  (`ensure_mutable`/eviction :123-146), `set_mode(IndexMode::Auto)` on every
  open (:148-156), 200000-record reserve before each mutation (`reserve` :732-739),
  batch cap 200 and finite/nonzero vector validation (:735-742).
- `open` accepts `IntentKind::Create` (fresh directory) or `Open` (sealed
  `preflight::verify`), rejects `Close` (:137-145). `persist` issues an immutable
  `PersistedReceipt` only after SrvDB persist + bounded file inspection + seal
  encoding (:208-241). `publish` delegates the fenced SQLite transaction to
  `IndexRepository::publish` (:243-253). `acknowledge` validates the committed
  ack before flipping `Awaiting -> Clean` (:255-289).
- `begin_maintenance` requires the source entry phase `Awaiting` (:399-410);
  `copy_vectors` requires the same `Awaiting` phase and `count == snapshot_end`
  (:434-480); copied f32 bits are owned before yielding.
- `close_generation(generation) -> PersistedReceipt` and `close_and_publish(key)`
  exist only inside `Runtime` (:506, :588); there is **no facade command**
  exposing them. `Runtime::shutdown` closes all `Clean` entries and discards
  unpublished dirty entries (:602-644).

`src-tauri/src/knowledge/index/repository.rs`:

- `DurableIntent`, `PersistedReceipt` (owner.rs:24-31), `PublicationAck` have
  private fields; `create` (:46), `open` (:59), `close` (:63), `validate` (:80),
  `publish` (:93), `recover_ack` (:103), `validate_ack` (:111).
- `publish` runs `validate_intent` + `publish_seal` in **one** SQLite
  transaction (:93-101); `publish_seal` flips `seal_json/dirty=0/intent=NULL`
  (:146-154). This is the single transaction that 5C must extend with
  mappings/pointer/Ready; it currently changes no document state.

`src-tauri/src/knowledge/index/seal.rs`:

- `Generation { identity: EmbeddingIdentity, requested_dimension: Option<u32>,
  generation: Uuid }` (:23-30); `Generation::key()` = `EmbeddingIdentity::key()`
  and rejects `requested_dimension != dimensions` when present (:32-41).
- `Seal` (version 1, `srvdb_generation_seal`) carries `Policy` (pin
  `1eec46fe...`, redb 4.4.0, header v4, Auto, `chunk_location_ref_v1`,
  schema 2), `descriptor`, `operation_id`, `stored_count`, `metadata_header`,
  literal file seals (:110-175). `Intent` (version 1, `srvdb_generation_intent`)
  carries `operation_id`, `owner_epoch`, `operation` Create/Open/Close,
  `identity`, `descriptor`, `prior_seal_sha256`, `expected_stored_count`
  (:171-238).

`src-tauri/src/knowledge/index/codec.rs`:

- `ChunkMetadata` schema 2 / `chunk_location_ref_v1` with inline nullable page,
  paragraph/start/end and full canonical `TextLocation` SHA-256 (:10-31).
  `ChunkMetadata::new(document_id, document_version, chunk_id, content_hash,
  location)` (:33-57), `encode()` <= 16384 bytes (:59-67),
  `matches_location(&TextLocation)` (:69-75). `is_hash` = 64 lowercase hex
  (:104-110).

`src-tauri/src/knowledge/identity.rs`:

- `EmbeddingIdentity { profile_id: Uuid, base_url: String, model: String,
  dimensions: u32, normalization_version: u32 }` (:14-21);
  `from_profile(&ModelProfile, actual_dimension) -> AppResult<Self>` validates
  Embedding kind, dimension 128..=4096, configured-dimension match (:29-48);
  `key()` hashes canonical serde JSON of the full identity to lowercase 64-hex
  (:50-74); `normalized_base` strips the `/embeddings` suffix (:76-86).
  There is **no `semantic_key()`** yet.

### 1.2 5A ownership and config gates

`src-tauri/src/intelligence/jobs.rs`:

- `JobContext { id, cancel, fence, children }` (:24-30);
  `operation_owner() -> AppResult<OperationLease>` requires stopping=false,
  open closed=false, job not cancelled, active membership, and one of 16 child
  slots; errors are Busy/Cancelled/Interrupted; never consults SQLite (:31-50).
- `JobHandler` trait: `run(context, payload)`, SQL-only `terminal_tx`,
  `cleanup(reaped)`, `cleanup_after_stop()`, `reconcile_startup()` (:72-97).
  There is **no after-terminal collaborator slot**.
- Worker sequence: `handler.run(...)` -> `transition_tx` (commits terminal
  status and calls `handler.terminal_tx`) -> `c.active.remove` ->
  `handler.cleanup(Some(id))` (:523-600). `transition_tx` at :639-660;
  `interrupt` at :661-674. `OperationLease = Arc<dyn Send + Sync>` (:124).
- `JobScheduler::register(kind, handler)` fails on duplicates/after freeze
  (:165-174); `enqueue_with_profiles` (:332-353) is the existing
  config->SQLite job-admission pattern; `admission_fence(kind)` (:310-329).

`src-tauri/src/intelligence/config.rs`:

- `ServiceConfigStore::scoped(owner) -> Self` (:22-29) forwards the lease to
  ConfigIo (`with_operation_owner`, config/worker.rs:87-90).
- `resolve_snapshot_credentials(expected: ModelProfile) -> AppResult<SecretString>`
  exists (:300-320): one ConfigIo gate, full public-profile equality +
  referenced credential, then `secret_get`; mismatch -> Configuration before
  any HTTP. `resolve_credentials(id)` unchanged (:273-291).
- `with_profile_snapshot(ids <= 3, operation)` (:328-350) captures profiles by
  ID inside the config gate and runs the SQLite callback under `fence.run`
  via the caller. `settings()` at :245 reads settings only; there is **no
  single-gate "selected setting + profile snapshot" capture**.

`src-tauri/src/intelligence/config/validation.rs`:

- `endpoint(profile)` (:52-87) normalizes the URL path, appends `/embeddings`
  for Embedding, forbids query/fragment/userinfo, allows http only for
  loopback. Embedding profiles: model <= 200 scalars, base_url <= 2048 bytes,
  no context/output, optional dimensions 128..=4096 (:93-102).

`src-tauri/src/intelligence/http.rs`:

- `SecretString` (private bearer, :8), `HttpRetryPolicy::{Never, OnceTransient}`
  (:36-39), `JsonRequest { endpoint, bearer, body, timeout, max_response_bytes,
  retry_policy }` (:41-48), `JsonHttp::post(request, cancel)` (:50-57),
  `ReqwestJsonHttp` with 120 s hard client timeout (:61-99).

`src-tauri/src/intelligence/probes.rs`:

- The Embedding probe already sends `{"model","input"}` with an 8 MiB response
  limit and `HttpRetryPolicy::OnceTransient` (:97-105), parses
  `data[0].embedding`, enforces 128..=4096 and finite values (:352-364), and
  may persist a discovered dimension into the profile when unused
  (`persist_probe`, config.rs:57-74). This is the transport precedent; 5C must
  not reuse `persist_probe` for index binding publication.

### 1.3 Documents, jobs and storage already in place for 5C

`src-tauri/src/knowledge/documents.rs`:

- `DocumentService::new(store, config, jobs, parser, data_dir)` (:40-56);
  `register_import_handler()` (:86-88); `handler()` builds the ImportHandler
  without config/index/embedding collaborators (:89-97).

`src-tauri/src/knowledge/documents/handler.rs`:

- The Import chunk transaction commits `Parsed` while the job remains Running
  (:49-64); `terminal_tx` is a no-op for Succeeded (:107-112); managed-file
  cleanup rows live in `cleanup_rows` (:169-189). This is exactly the seam the
  continuation handoff describes.

`src-tauri/src/knowledge/documents/admission.rs`:

- Manual admission uses `exclusion_key = "document:{id}"` (:37); replacement
  bumps version and marks prior chunks `index_state='stale'` (:41-42); dedup
  returns the existing job without mutation.

`src-tauri/src/intelligence/store.rs`:

- `MAX_UNFINISHED = 32` (:10); Manual admission requires
  `"document:{object_id}"` for `Import | Reindex | DeleteDocument` (:49-63);
  exclusion collision or full queue -> Busy (:101-117); `JobPayload` caps 3
  profiles and 64 KiB serialized (:248-267); `profile_in_use` scans <= 32
  unfinished payloads (:177-199).

`src-tauri/migrations/0004_knowledge.sql`:

- `documents.status` CHECK already includes `parsing/parsed/indexing/ready/
  repair_required/failed/deleting` (:1-17); `document_chunks` has
  `embedding_identity`, canonical `vector_id TEXT` (0..=u64 max, no leading
  zero), `index_state` (no CHECK) (:19-34); `knowledge_index_generations
  (identity PK, profile_id, dimension, active_generation, state)` (:36-43);
  `knowledge_index_generation_seals` with the 64 KiB/16 KiB clean/dirty
  invariant (:51-79); `knowledge_index_work_items` (no document/job FK,
  RESTRICT to seals, ordinal 0..=199999) (:105-124);
  `knowledge_generation_mappings` (PK identity+generation+chunk_id,
  UNIQUE identity+generation+vector_id, RESTRICT to seals, `published IN(0,1)`)
  (:126-145).

`src-tauri/src/storage/sqlite.rs`:

- Migrations 1-4 registered by `include_str!` and `user_version` (:49-66);
  `LocalStore::transaction` is `TransactionBehavior::Immediate` under one
  connection mutex (:71-79). Adding migration 5 is an append to this list.

`src-tauri/src/app/mod.rs`:

- `AppState` already holds `index_owner`, `documents`, `json_http`,
  `service_config`, `jobs` (:60-82); `index_owner` is constructed at :210-211;
  `register_intelligence_handlers(&documents)` at :219 and :414-418 currently
  registers only the Import handler.

`src-tauri/src/app/intelligence.rs`:

- `start_intelligence` opens the index owner with the process lock, rechecks the
  epoch, then starts jobs (:12-44); `intelligence_lease()` issues one of 16
  process+permit leases (:24-33). There is no knowledge reconcile call in the
  startup path yet.

`src-tauri/src/commands/knowledge.rs`:

- Four commands (`import_document`, `replace_document`, `list_documents`,
  `get_document`), `COMMANDS` const (:44-49), `handle` dispatcher (:51-59).
  lib.rs routes by `COMMANDS.contains` (src-tauri/src/lib.rs:36-44).

`src-tauri/src/intelligence/types.rs`:

- `JobKind::{Import, Reindex, DeleteDocument, Summary, Research,
  ResearchOverview}` (:57-62); `IntelligenceErrorCode` includes
  `IndexRepairRequired`, `ResourceLimit`, `Timeout`, `Cancelled` (:76-95);
  `ModelProfile` public fields (:24-33); `IntelligenceSettings
  .embedding_profile_id` (:47-56).

Frontend:

- `src/features/workbench/useWorkbench.ts:25-29` defines
  `SelectionContext { sessionId, segmentIds, text }`; selection capture keeps
  the exact text and intersecting `data-segment-id`s
  (`src/features/workbench/ConversationStream.tsx:49-77`, per-row search
  button :168-175). Transcript rows live in `translation_segments(id,
  session_id, ...)` (migration 0001:4-5).
- `src/lib/knowledge-wire.ts` has document DTOs only; no KnowledgeSearch/Hit/
  Results decoders. `decodeTextLocation` heading bound is **8 MiB** (:20) while
  the accepted c8f7770 policy is 16 MiB. `decodeIntelligenceErrorCode` and
  `decodeJobKind` include reindex/delete_document
  (`src/lib/intelligence-wire.ts:10`).

### 1.4 How 5C composes these

One `KnowledgeService` (new module, see section 9) holds
`Arc<LocalStore>`, `Arc<ServiceConfigStore>`, `Arc<JobScheduler>`,
`Arc<dyn Embeddings>`, `IndexOwner` (clone), plus `Arc<dyn JsonHttp>` transitively
inside the transport. Command path: `commands/knowledge.rs` -> `AppState`
methods -> `KnowledgeService`. Job path: Reindex/Delete handlers registered on
the scheduler at app/mod.rs:414 before `jobs.start_owned`. All SrvDB work goes
through `IndexOwner` tickets carrying a pre-issued `OperationLease`; the only
SQLite authority change is the one fenced publication transaction in
`IndexRepository::publish` (extended, section 4).

## 2. Plan text vs committed code mismatches (controller rulings)

M1. **Heading wire bound.** Accepted policy is a 16 MiB raw UTF-8 heading bound;
committed frontend is 8 MiB (`src/lib/knowledge-wire.ts:20`). 5C must widen the
decoder (and fixture set) to 16 MiB; body text stays 8 MiB.

M2. **Publication transaction.** `IndexOwner::publish(ticket, receipt)` and
`IndexRepository::publish(receipt)` carry no domain plan; the sealed transaction
touches only the seal row (repository.rs:93-101, :146-154). The brief's
"transactionally store IDs and mark Ready" therefore needs a crate-private
typed `PublicationPlan` threaded through facade -> envelope ->
`Runtime::publish` -> `Repository::publish(receipt, plan)` inside the one
transaction. No separate Ready publication is permitted.

M3. **Facade close.** `Runtime::close_generation`/`close_and_publish`
(owner.rs:506, :588) are unreachable from the facade (Command enum
lifecycle.rs:98-119 has no Close). Post-swap retirement of the old rebuild
generation needs a facade close command + receipt/ack, or reuse of the LRU
close path. Recommended: narrow facade `close(ticket)` command reusing the
existing Runtime close protocol.

M4. **No 5C types/commands exist.** KnowledgeSearch/Hit/Results, `Embeddings`,
`EmbeddingExecution`, KnowledgeService, the three commands and any migration 5
are absent. This is expected; nothing to reconcile against, everything to add.

M5. **No AfterTerminal seam.** The worker has no hook between handler success
and the terminal commit (jobs.rs:575-590). The import-continuation handoff's
"optional AfterTerminal collaborator" requires a narrow scheduler extension
registered before start.

M6. **No one-gate settings+profile capture.** `settings()` then
`with_profile_snapshot` is racy (config.rs:245 vs :328). A single-gate
"selected embedding profile snapshot" capture must be added.

M7. **No semantic binding key.** `EmbeddingIdentity::key()` hashes the full
identity including actual dimension (identity.rs:50-74). Binding lookup by
profile/endpoint/model/normalization needs a separate `semantic_key()` and
persisted `requested_dimension` (migration 5).

M8. **Binding storage.** `knowledge_index_generations` lacks
`semantic_key`/`requested_dimension` columns; `state` has no defined value set
(only `'building'` is written, repository.rs:54). Additive migration 5 fixes
both; SQLite cannot add a CHECK to an existing column without a table rebuild,
so state values are enforced in the typed repository layer.

M9. **Maintenance window.** `begin_maintenance`/`copy_vectors` require the
source entry phase `Awaiting` (owner.rs:399-410, :445-449). Rebuild therefore
must follow open -> persist -> maintenance-copy -> staging publish, and the
source's own refresh receipt must be published+acknowledged before it can be
closed after the pointer swap (details in section 8).

M10. **reconcile_local ownership.** The brief signature
`reconcile_local(&self) -> AppResult<()>` has no lease. KnowledgeService must
not capture AppState; AppState supplies the lease at startup
(`reconcile_owned(OperationLease)` crate-private + public command wrapper), or
the service is constructed with a narrow startup-only lease issuer. Recommended:
AppState calls `knowledge.reconcile_owned(self.intelligence_lease()?)` inside
`start_intelligence` after index readiness and before jobs start.

M11. **Probe dimension persistence.** `persist_probe` can write
`profile.dimensions = Some(d)` when the profile is unused (config.rs:57-74).
This can race a None-binding first index (a user runs a probe while reindexing).
The publication plan must not trust the current saved profile's dimension field;
it must re-resolve the binding under its own SQL fence (section 5).

M12. **Cargo.** All dependencies 5C needs already exist in
`src-tauri/Cargo.toml` (srvdb pin, rayon, sha2, crc32fast, xxhash-rust). No
Cargo.toml/Cargo.lock change is expected unless the plan type needs one.

## 3. Additive SQLite schema / storage boundaries

New file `src-tauri/migrations/0005_knowledge_index.sql`, appended to
`MIGRATIONS` in storage/sqlite.rs:50-55. Additive ALTERs only; no trusted rows
synthesized; every new column starts NULL/empty and the reconcile pass
classifies legacy rows as `repair_required` exactly as migration 4 did.

1. `knowledge_index_generations`:
   - `ALTER TABLE ... ADD COLUMN semantic_key TEXT` (64 lowercase hex);
   - `ALTER TABLE ... ADD COLUMN requested_dimension INTEGER` (NULL or
     128..=4096, enforced in the typed layer);
   - `ALTER TABLE ... ADD COLUMN updated_at_ms INTEGER`;
   - two partial unique indexes forming the binding fence:
     `UNIQUE(semantic_key) WHERE semantic_key IS NOT NULL AND
     requested_dimension IS NULL` and
     `UNIQUE(semantic_key, requested_dimension) WHERE requested_dimension IS
     NOT NULL`. This encodes "exactly one binding per (semantic key,
     requestedDimension option)" without relying on SQLite's NULL uniqueness.
   - Defined `state` values (typed, not a CHECK): `building`, `active`,
     `repair_required`. `active_generation` is written only inside the fenced
     publication transaction.
2. `document_chunks`: no new columns. `index_state` gains defined values
   `parsed | stale | indexing | ready | deleting | repair_required` (typed
   only). Existing rows default `repair_required` from migration 4.
3. Optional bounded indexes: `CREATE INDEX documents_status ON documents(status)`
   and `CREATE INDEX document_chunks_ready ON
   document_chunks(index_state, embedding_identity)` for preflight/hydration;
   both additive.
4. No new cleanup table. The delete cleanup intent is: job payload
   (<= 64 KiB, carries operation UUID + identity + generation reference) +
   paged rows in `knowledge_index_work_items` (vector IDs only; no bulk JSON
   arrays). Work/mapping rows deliberately survive document/job deletion
   (no FK to them) and RESTRICT seal/identity deletion until explicit
   acknowledgment. `knowledge_generation_mappings` doubles as the staging
   store (`published=0` before the swap) and the durable inverse
   mapping vector_id -> chunk/document.
5. `knowledge_repairs` reasons added by 5C (typed constants):
   `index_missing`, `vector_missing`, `stale_hash`, `dimension_mismatch`,
   `unsealed_generation`, `auto_index_deferred`.

Migration strategy: migration 5 runs empty on existing databases (no data
backfill, no seals created). Task 5C owns migration 5 exclusively; Task 6's
research keeps version 6 per the pairing ledger. The migration test must cover:
reopen twice, legacy rows still `repair_required`, both partial unique indexes
hold under insert races, high/u64-max vector IDs unchanged.

Atomic Ready publication (one transaction, fenced): in the extended
`IndexRepository::publish(receipt, plan)`:

- revalidate intent row + `owner_epoch` + `operation_id` + identity/generation
  + prior-seal digest (existing `validate_intent`, repository.rs:128-144);
- `publish_seal` for the receipt's seal (existing, :146-154);
- plan fences re-checked inside the transaction: document UUID/version/
  content_hash/parser association, mapping `published=0 -> 1` for exactly the
  planned ids, `active_generation` pointer old->new, `state='active'`;
- affected documents `status='indexing' -> 'ready'`, `index_identity=identity`,
  `error_code=NULL`; chunk rows `vector_id=...`, `embedding_identity=...`,
  `index_state='ready'`; `knowledge_repairs` rows removed;
- nothing else may change Ready state; on any fence failure the whole
  transaction fails, the intent stays dirty and the live engine stays owned.

## 4. Exact 5B owner operations 5C must call, and narrow 5B port extensions

Operations 5C uses unchanged:

- Reindex/first index: `owner.create(Generation, lease)` (Create) or
  `owner.open_existing(Generation, lease)` when the binding already has an
  active generation; `enqueue_open`; `add(ticket, batch<=200)` per embedded
  batch; `persist(ticket)` -> `PersistedReceipt`; `publish(ticket, receipt,
  plan)` (extended); `acknowledge(ticket, ack)`.
- Delete: `open_existing` (if a binding exists); `delete(ticket, ids<=200)`;
  `persist`; `publish(..., delete_plan)`; `acknowledge`.
- Reconcile scan: `open_existing`; `scan(ticket, start, 200)` over
  `[0, count)` with checked u64 arithmetic (ID 0 valid, count excluded);
  `delete` for orphans; `persist`; `publish(..., reconcile_plan)`; `acknowledge`.
- Rebuild: `open_existing` on the source generation -> `persist` (source now
  Awaiting) -> `begin_maintenance(ticket)` -> `copy_vectors(&mt, start, 200)`
  until the snapshot end -> `end_maintenance`; staging `create` + `enqueue_open`
  (evicts the other idle identity; two engines max) + `add` with recomputed
  `ChunkMetadata` + `persist` + `publish(swap_plan)` + `acknowledge`; then
  publish/ack the source's pending refresh receipt, close the old generation
  (extension below), delete the old directory after ack.
- Search: `open_existing` on the active published generation (create an Open
  intent), `search(ticket, vector, candidates)`; candidates <= 100 so the
  existing `validate_batch(limit)` cap of 200 holds (owner.rs:370-372).
- Observers: every `Pending::observe` has the 10 s stop timeout
  (lifecycle.rs:66-72); entered work retains the envelope lease, so handler
  observer loss never releases the token or the engine.

Narrow 5B port extensions required before 5C handlers (no per-method
`std::thread::spawn`, no callback into the queue):

E1. Typed `PublicationPlan` (crate-private, validated codec) added to
`IndexOwner::publish(ticket, receipt, plan)` and
`IndexRepository::publish(receipt, plan)`; the plan carries only bounded
UUID/hash/id sets (paged <= 200 per transaction), the affected document
fences and the mapping/pointer updates of section 3. The owner `Publish`
command gains the plan; `Runtime::publish` revalidates plan identity/generation/
operation against the receipt before calling the repository.
E2. Facade `close(ticket)` (or `close_generation(Generation, lease)`) exposing
`Runtime::close_generation` + publish/ack of the closed seal, used for
post-swap old-generation retirement and for repair-eviction.
E3. `pub(crate)` re-exports in `knowledge/index.rs` for `WorkTicket`,
`MaintenanceTicket`, `PersistedReceipt`, `PublicationAck`, `DurableIntent`,
`Generation` (currently module-private to lifecycle/owner/repository/seal).
E4. Optional `IndexRepository::publish` plan-free overload retained for the
pure seal refresh/close path (no Ready mutation).

No other 5B change: queue 16, one owner + two Rayon workers, two-engine cap,
clean-only LRU, dirty Busy, 200-record pages, tombstones in capacity, all stay
as committed.

## 5. Embedding execution ownership, batching, retry, bounds, snapshot
validation, dimension binding races

Port (new, crate-private, per task-5c-ownership-decision):

- `pub(crate) struct EmbeddingExecution { owner: OperationLease, cancel:
  watch::Receiver<bool> }` with private fields and a crate-private constructor
  requiring both. No cancellation-only overload exists.
- `#[async_trait] pub(crate) trait Embeddings: Send + Sync { async fn
  embed(&self, profile: &ModelProfile, inputs: &[String], execution:
  EmbeddingExecution) -> AppResult<Vec<Vec<f32>>>; }` (AppResult keeps the
  composition with KnowledgeService single-error; the decision note's
  `IntelligenceErrorCode` return is equally acceptable — pick AppResult and
  map resolver failures to `Configuration`).
- `OpenAiCompatibleEmbeddings::new(config: Arc<ServiceConfigStore>, http:
  Arc<dyn JsonHttp>)` stores unscoped collaborators and one shared
  `Arc<Semaphore>` of 4.

Per call:

1. `config.scoped(execution.owner.clone())` then
   `resolve_snapshot_credentials(profile.clone()).await` under the one
   ConfigIo gate (config.rs:300-320). Equality is the full public ModelProfile;
   the entered ConfigIo callback retains the owner past any observer timeout.
   Resolver failure -> `Configuration` with zero HTTP calls.
2. `JsonRequest { endpoint: endpoint(profile)?, bearer (moved SecretString),
   body: {"model": model, "input": inputs, "encoding_format":"float",
   "dimensions": only if Some}, timeout: profile.timeout_ms,
   max_response_bytes: 8 MiB, retry_policy: HttpRetryPolicy::OnceTransient }`.
   One bounded retry by existing JsonHttp only; no custom replay.
3. Shared 4-permit gate acquired around `http.post(request,
   execution.cancel.clone())`; never while holding SQLite/index locks.
4. Response validation: non-2xx mapping as the probe (probes.rs:334-349);
   parse `data` array; `data[i].index` must cover `0..inputs.len()` exactly
   (reject duplicate/missing/out-of-range); each embedding array non-empty,
   all finite, length equal to the batch dimension; reject zero vectors;
   first valid response fixes the actual dimension, which must be 128..=4096
   and equal to `profile.dimensions` when configured.
5. Checked L2 normalization with finite-norm verification; normalized vectors
   are what `IndexOwner::add` stores and what seals pin.

Batching (reindex/rebuild caller side): accumulate inputs until 32 texts or
the measured serialized request JSON exceeds 128 KiB (measure by serializing
into a counting writer, model and fields included), then flush. No text-sum
heuristics.

Local dimension binding race resolution:

- Binding lookup key = `semantic_key(profile_id, normalized base_url, model,
  normalization_version=1)` + `requested_dimension = profile.dimensions`.
- Lookup uses `knowledge_index_generations` with the migration-5 columns;
  trust only rows whose generation is published clean or owner-certified
  trusted-live, and whose seal decodes to the matching Generation.
- Table (already accepted in task-5-architecture-preflight): requested None
  with exactly one distinct actual dimension -> use it; None with zero or
  multiple -> `IndexRepairRequired` with zero HTTP; Some(d) with matching
  sealed actual d -> use it (a None->Some probe update on the same d stays
  compatible); Some(d) with only another dimension -> `IndexRepairRequired`.
- First-index concurrency: two Reindex jobs may both embed and then race to
  publish. The publication transaction inserts the binding row under the
  partial unique indexes; the loser's plan fails (unique violation) -> its
  job fails with `IndexRepairRequired`, its unsealed staging generation stays
  unreferenced and is reclaimed by reconcile; the winner's mappings/Ready/pointer
  commit atomically. No automatic re-embedding, no fingerprint identity.
- Jobs construct `EmbeddingExecution` from a pre-issued
  `context.operation_owner()` (jobs.rs:38); direct search constructs it from
  `AppState::intelligence_lease()` (app/intelligence.rs:24-33) combined with
  the 2-search permit.

## 6. Import auto-continuation seam

Narrow scheduler extension (ruling): a crate-private optional
`AfterTerminal` collaborator registered on `JobScheduler` before `start`
(alongside `register`, jobs.rs:165). In `worker`, after `handler.run` returns
Ok but before the terminal commit, the worker reserves one child token through
the still-active `JobContext` (jobs.rs:38), then commits the Succeeded
transition, then awaits the collaborator with a compact immutable notice
(parent Import id, document UUID, version, SHA256, parser identity). Only a
committed Succeeded triggers it; cancellation/stopping/child-capacity failure
drops the notice without downgrading the successful parse. The hook is awaited
in the owned worker, never detached; the SQL-only `terminal_tx` stays free of
ConfigIo.

Admission inside the hook:

1. One new ConfigIo gate on `ServiceConfigStore` (extension, config.rs):
   `selected_embedding_snapshot()` reading `settings.embedding_profile_id` and
   the full public profile in one callback. No selection -> keep Parsed and
   record `auto_index_deferred` in `knowledge_repairs`; no automatic paid
   retry.
2. Under the same gate, enter SQLite then the generation fence: recheck parent
   job `succeeded`, current document UUID/version/SHA256/parser association,
   status `parsed`; skip deleted/replaced/already-indexing documents and
   obsolete associations.
3. Admit a `JobKind::Reindex` job with `JobAdmission::Manual { exclusion_key:
   "document:{id}" }` and payload `{"version":1,"profiles":[profile],
   "input":{"document_id":...,"document_version":...}}` in one transaction
   (mirrors `enqueue_with_profiles`, jobs.rs:332-353), publish/wake only after
   commit. Queue full (32) or exclusion conflict -> keep Parsed plus the
   deferred repair reason; the document stays available for the manual
   "选择 Embedding 模型后建立索引" action.

Restart: uncontinued Parsed documents stay Parsed; startup reconciliation never
replays the hook. A fence alone is not proof Reindex is registered; validate
the registered handler capability at composition time (app/mod.rs:414-418).

## 7. Search readiness preflight, candidates, hydration, budget, provenance,
cancellation, no-paid-call behavior

Command `search_knowledge(request: KnowledgeSearch)` ->
`KnowledgeService::search(request)`; `KnowledgeResults { hits, identity:
Option<String>, unavailable_documents: u32 }`.

Validation first (zero network): query length 1..=4096 Unicode scalars;
limit 1..=20 (default 5); if `selected_segment_ids` is non-empty,
`session_id` must be Some and every ID must exist in `translation_segments`
for that session (bounded count, <= 200); failure -> Configuration before any
embedding. The exact submitted query is the only text embedded; an edited query
clears the exact-selection IDs at the frontend boundary.

Preflight (zero HTTP on known-bad readiness):

1. Count documents. Zero total -> `{ hits: [], identity: None,
   unavailable_documents: 0 }`.
2. Otherwise resolve the binding for the selected embedding profile (section
   5). No selection or no healthy published space -> `IndexRepairRequired`
   with zero embedding calls.
3. There must exist at least one current Ready/searchable document in that
   space; otherwise `IndexRepairRequired`, zero embedding calls.

Execute: acquire the 2-search permit (replacement cancels the older outstanding
request through a retained watch sender); build `EmbeddingExecution` from the
AppState intelligence lease + permit; `embeddings.embed(profile, [query],
execution)`; `owner.open_existing(active_generation)`; `owner.search(ticket,
vector, min(limit*5, 100))`.

Hydration, one row at a time (authoritative SQLite):

- For each candidate `(vector_id, score, ChunkMetadata)`: join
  `knowledge_generation_mappings` (published=1) to the current chunk/document
  rows and `knowledge_index_generations.active_generation == mapping.generation`;
  require `document.status='ready'`, `document_version`/`content_hash`/chunk
  `index_state='ready'` match the metadata; borrow ValueRefs under bounds
  (title <= 32768 UTF-8 bytes, chunk <= 2400 scalars / 9600 UTF-8 bytes,
  canonical location JSON <= 16 MiB, heading raw UTF-8 <= 16 MiB); verify
  `ChunkMetadata::matches_location` (codec.rs:69-75). Any mismatch excludes
  the document and marks it repair-required locally (no paid reembedding).
- Scores stay finite cosine similarities; never percentages.

Budget: checked count-only `serde_json::to_writer` over the compact hit stream
including escapes, scores, commas and the envelope, caching per-hit counts;
admit the eligible score prefix; stop at the first eligible overflow (one
valid-but-unfit hit -> `ResourceLimit`); never truncate fields, never fill from
lower scores, never count a budget omission as unavailable. Recheck
`unavailable_documents` and the whole envelope; remove only trailing whole
hits if the final envelope exceeds 32 MiB. Recheck readiness at the end: if no
healthy searchable document remains, return `IndexRepairRequired` instead of
empty success. `identity` = `Some(lowercase64hex)` for the resolved space;
never an invented empty-string or fingerprint.

Cancellation: the per-request watch receiver flows into `JsonHttp::post` and
into the owner envelope's `cancelled` flag (lifecycle.rs:144); a replacement
search cancels its predecessor; the dropped observer never releases the
entered envelope lease. No translation provider ever receives a knowledge
query. No automatic query on subtitle/keystroke events.

Frontend defense (Task 8-facing): decoders in knowledge-wire.ts for the three
DTOs with per-field bounds plus a conservative 32 MiB aggregate estimator over
decoded fields (TextEncoder sums + fixed overhead), explicitly not assuming
JavaScript/Rust float byte equality.

## 8. Reindex / delete / rebuild / reconcile state machines and crash recovery

### Reindex (manual or continuation)

Admission: `Reindex` job, Manual `document:{id}`, payload as in section 6.

1. `context.operation_owner()` issued once and retained through the whole
   operation and cleanup (jobs.rs:38).
2. SQLite: `documents.status='indexing'`, chunk rows of the current version
   `index_state='indexing'`; desired version/hash recorded. If the document is
   missing/deleting/parsing -> Cancelled/Configuration without index work.
3. Resolve binding (section 5); for None dimension, the first valid embedding
   response fixes the actual dimension.
4. Embed current-version chunks in batches (section 5); map returned indexes to
   inputs; encode `ChunkMetadata` from authoritative SQLite rows.
5. Owner: `create`/`open_existing` + `enqueue_open` + `add` batches <= 200 +
   `persist` -> receipt. Stage mapping rows `published=0` is NOT required for
   plain reindex; the plan carries the final ids directly.
6. `publish(ticket, receipt, ready_plan)`: mappings published=1, pointer/state,
   documents Ready, chunk vector ids/identity, repairs cleared — all in the one
   transaction. Then `acknowledge(ticket, ack)` (lost ack recoverable only via
   `recover_ack`, repository.rs:103-109).
7. Job Succeeded. CommitPoint test seams: after SQLite intent, after vector
   mutation, after vector persist, before/inside SQLite ready update.

Crash rules: before persist, the document stays non-Ready and search excludes
it; after vector persist but before the ready transaction, the search of the
published generation cannot see un-published ids, and on restart the unsealed/
dirty source fails closed -> `IndexRepairRequired`, preserving original text
and managed files; the retained desired text/repair marker enables an explicit
Reindex. Replacement during reindex is excluded by the shared `document:{id}`
key; replacement never returns old-version hits while new rows are partial.

### Delete

1. Command `delete_document(id)`: admission transaction sets
   `documents.status='deleting'` and chunk `index_state='deleting'`; job
   payload { document_id }; the durable cleanup intent is (operation UUID,
   identity, generation) in the payload plus paged vector IDs in
   `knowledge_index_work_items` written from the published mapping before any
   SrvDB mutation (batches <= 200, ordinals bounded).
2. Owner: `open_existing` if indexed; `delete(ticket, ids<=200)`; `persist`;
   `publish(ticket, receipt, delete_plan)`: tombstone persistence is already
   durable; the plan removes document/chunk rows (cascade) and sets
   `knowledge_managed_files.state='delete_pending'`, retaining work items and
   mappings. `acknowledge`.
3. After commit: bounded managed-file cleanup (the existing `cleanup_rows`
   pattern, handler.rs:169-189, run outside SQLite); then explicit cleanup
   acknowledgment: paged deletion of the operation's work items and mapping
   rows. Capacity is NOT reclaimed (tombstones) until a generation rebuild.
4. Crash at any point: on restart, `reconcile_startup` resumes documents still
   `deleting`: tombstone any remaining ids (delete returns false for already
   deleted, owner.rs:334-359), persist, then complete row removal and file
   cleanup; the durable intent makes the resume idempotent. If a crash left an
   orphan vector, hydration excludes it and reconciliation tombstones it with
   zero embedding calls. No identity-row deletion API is built in 5C; identity
   deletion stays blocked by the RESTRICT FKs while work/mappings remain.

### Rebuild (local, no embedding)

Trigger: explicit local reconcile, 12 GiB metadata watermark
(seal.rs:17), or capacity pressure from tombstones.

1. Source = the published Clean active generation. `open_existing` -> new
   intent -> `persist` (source now Awaiting) -> `begin_maintenance`
   (owner.rs:399-410) -> `copy_vectors` pages of <= 200 preserving f32 bits
   (owner.rs:434-480) -> `end_maintenance`.
2. Staging `Generation` with a fresh UUID, same identity: `create` + open (the
   other idle identity is evicted first; total engines <= 2) + `add` with
   `ChunkMetadata` recomputed from the authoritative SQLite mapping rows +
   current chunk locations + `persist`.
3. `publish(ticket, receipt, swap_plan)`: staging mappings published=1 with
   their new vector ids, chunk rows repointed, pointer
   `active_generation=staging`, `state='active'`; all in one transaction.
   `acknowledge(staging)`.
4. Publish+ack the source's pending refresh receipt (seal refresh only), then
   `close` the old generation via extension E2 and delete its managed
   directory only after the closed receipt is published/acknowledged. Then
   paged removal of the old mapping/work rows. The only good index is never
   overwritten: the old directory survives every failure before step 3
   commits; failure after the swap leaves a durable old-directory cleanup.
5. No bare `compact` (it densely renumbers IDs and returns only a removed
   count), no third engine, no re-embedding.

### Startup reconcile (`reconcile_local`)

Runs under the app lock after index owner readiness and before jobs start
(section 2, M10). For each identity row with a clean published seal:
`open_existing`; `scan` `[0, count)` in <= 200-id steps with checked u64
arithmetic; distinguish tombstones (skip) from live IDs; decode metadata; for
every live record resolve the published mapping and the current Ready chunk/
document/hash; mismatches -> mark the owning document `repair_required` +
repair reason; live IDs with no mapping -> orphan -> local tombstone (zero
embedding calls); missing metadata on a live ID -> inverse mapping -> repair;
then `persist` + `publish(reconcile_plan)` + `acknowledge`. Documents marked
Indexing/Deleting are resumed per the state machines above. Dirty/unsealed
generations fail locally before any open (5B behavior) and surface as
`IndexRepairRequired`, preserving all files and text.

## 9. Owned files, ordered TDD slices, focused tests, acceptance commands

Task-owned paths (5C):

- `src-tauri/src/knowledge/embedding.rs` (transport + EmbeddingExecution),
  `src-tauri/src/knowledge/service.rs` (KnowledgeService, Reindex/Delete
  handlers, AfterTerminal collaborator), `src-tauri/src/knowledge/search.rs`
  (preflight/hydration/budget), `src-tauri/src/knowledge/reconcile.rs`,
  `src-tauri/src/knowledge/identity.rs` (semantic_key), `src-tauri/src/
  knowledge/types.rs` (DTOs), `src-tauri/src/knowledge/mod.rs` (module
  registration).
- `src-tauri/src/knowledge/index.rs` + `index/{lifecycle,owner,repository}.rs`
  (E1-E4 extensions only; no 5B rewrite), `src-tauri/src/knowledge/
  index/tests.rs` + new `src-tauri/src/knowledge/index_tests.rs`-style
  KnowledgeHarness and `src-tauri/src/knowledge/reconcile_tests.rs`.
- `src-tauri/src/intelligence/jobs.rs` (AfterTerminal slot + worker call site),
  `src-tauri/src/intelligence/config.rs` (selected_embedding_snapshot),
  `src-tauri/src/app/mod.rs` (composition/registration),
  `src-tauri/src/app/intelligence.rs` (startup reconcile call),
  `src-tauri/src/commands/knowledge.rs` (three commands + COMMANDS list).
- `src-tauri/migrations/0005_knowledge_index.sql`,
  `src-tauri/src/storage/sqlite.rs` (MIGRATIONS entry).
- `src/lib/knowledge-wire.ts` + tests (DTO decoders, 16 MiB heading fix).

TDD slices in order:

1. Migration/wire RED: migration 5 empty + bounds + partial-unique fences;
   KnowledgeSearch/Hit/Results decoders; heading 16 MiB fixture.
2. Embedding transport RED/GREEN: JSON-byte batching, index mapping rejection,
   dimension fixing, L2 normalization, OnceTransient retry, zero HTTP on
   resolver failure, full-profile snapshot equality, shared 4-request gate.
3. Reindex RED/GREEN: the brief's crash-injection matrix
   (crash_after_vector_persist_never_exposes..., replacement new vector ID,
   deletion across reopen, stale hash/missing metadata/missing vector/wrong
   dimension/same model different endpoint, NaN/infinity/zero vectors,
   out-of-order indexes, cancellation during import, duplicate retry, orphan
   vector, persisted generation swap, saved-profile edit vs old space).
4. Delete/reconcile/rebuild RED/GREEN: cleanup-intent durability across
   document row removal, tombstone capacity, bit-preserving copy, pointer swap
   mapping regression, first-ID-zero/last-count-minus-one scan, old generation
   deletion only after swap.
5. Search RED/GREEN: empty-library zero-call success, repair-preflight zero
   calls, candidate/hydration bounds, prefix overflow ResourceLimit, trailing
   trim, unavailable count, late readiness loss -> repair, identity
   None/Some64hex, selected partial/multi-row query with foreign-session
   rejection, replacement cancellation, two-outstanding cap.
6. Import continuation RED/GREEN: hook only on committed Succeeded, queue
   full/exclusion/no-profile fallbacks keep Parsed + deferred reason, stale
   notice never alters newer associations, shutdown drain.
7. Full gates.

`KnowledgeHarness` extends `IntelligenceHarness`
(src-tauri/src/intelligence/test_support.rs:15) with a real temp SrvDB
`IndexOwner` (started with a lease), `DocumentService` with a fixture parser,
a deterministic axis Embeddings (128-dimensional, normalized, observed call
count), `import_text/name`, `index`, `search`, `crash_at(CommitPoint)`,
`reopen`, `delete`, `embedding_calls`. `CommitPoint` is a test-only injected
callback at SQLite intent / vector mutation / vector persist / SQLite ready
update; it must not alter production transaction logic.

Acceptance commands (one Cargo owner, offline/locked local CARGO_HOME as in
5B):

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
```

Frontend: `npx vitest run src/lib/knowledge-wire.test.ts` (existing harness)
and the normal desktop/frontend build gates used by 4B. Commit:
`feat: index and search local documents with recoverable SrvDB state`.

## 10. External / future-owner gaps not implemented in 5C

- Workbench UI consumption of the three commands, tool panels, settings page
  embedding selection flows, deferred-repair presentation: Task 8.
- Frontend aggregate defense/desktop-port plumbing beyond knowledge-wire
  decoders (wire.ts/desktop.ts additions): Task 8; 5C only fixes the accepted
  16 MiB heading bound and ships decoder tests.
- Identity (binding) row deletion, migration of identity files, managed-file
  history GC beyond delete/rebuild ack: future owner; RESTRICT FKs keep
  deletion blocked while work/mappings/seals exist.
- Real paid-service acceptance, keyring, Windows/macOS compile/signing,
  performance/RSS measurement at 200000 records / 4096 dimensions: external
  gates. 5C states operational caps only, never RSS guarantees.
- Probe dimension auto-persistence semantics beyond the M11 ruling, and any
  research/summary (migration 6+) work: Tasks 6/7.
- 4A/4B parser, documents, migration 1-4 behavior: unchanged and not re-tested
  as 5C scope; only additive migration 5 and the documented handler seams are
  touched.

## Controller decisions required before 5C source

D1. Accept the typed `PublicationPlan` extension through
`IndexOwner::publish`/`IndexRepository::publish` (E1) and facade close (E2),
with the rule that Ready/mapping/pointer/state changes occur only inside that
one fenced transaction.
D2. Approve migration 5 exactly as section 3 (semantic_key, requested_dimension,
updated_at_ms, two partial unique indexes, status indexes; typed state values;
no table rebuild, no trusted backfill).
D3. Approve binding semantics: `semantic_key` canonical JSON of
{profile_id, normalized base_url, model, normalization_version}; record the
requestedDimension option; winner-takes-first with loser-fails-fence for
concurrent first indexing (section 5).
D4. Approve `EmbeddingExecution`/`Embeddings` signatures and the AppResult
return, shared 4-request gate, OnceTransient retry, and 8 MiB/128 KiB/32-text
bounds as in section 5.
D5. Approve the AfterTerminal collaborator seam in jobs.rs and the new
`selected_embedding_snapshot()` config gate; fallback keeps Parsed plus a
`auto_index_deferred` repair reason and never downgrades the parent.
D6. Approve `reconcile_owned(lease)` startup ordering in `start_intelligence`
(after index readiness, before jobs start) with zero paid re-embedding.
D7. Approve the 16 MiB heading correction in knowledge-wire.ts plus the
conservative frontend aggregate estimator.
D8. Approve delete payload/work-item cleanup-ack ordering and rebuild
close-then-delete ordering (section 8), including publishing the source
refresh receipt before closing the retired generation.
D9. Approve `KnowledgeService::new(store, config, jobs, embeddings, index,
data_dir)` (data_dir added for managed-file cleanup) and the
two-outstanding/cancel-on-replacement search gate.
D10. Approve the four test-only CommitPoint seams and the KnowledgeHarness
shape of section 9.
