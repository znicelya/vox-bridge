# Task brief — workbench-intelligence — Task 5

Prepared, NOT dispatched. Actual BASE, reviewed producer handoffs and ownership supplied at dispatch. Read global-constraints.md. Read task-5-srvdb-owner-handoff.md and ledgered Auto/fixed-pool/generation-overlap rulings; fixed managed-file anomaly caps must be settled at the architecture checkpoint before writable opens. Do not invent unfinished earlier producer APIs or redo completed tasks.

### Task 5: Embeddings, SrvDB consistency and document search

**Files:** Create knowledge/{embedding,index,reconcile,search,index_tests,reconcile_tests}.rs. Extend knowledge store/documents/commands, Cargo manifests, AppState and job registration. Add small SrvDB integration fixtures/tests; use the actual crate in default tests with 128-dimensional synthetic vectors.

**Interfaces:**

```rust
pub struct EmbeddingIdentity {
    pub profile_id: Uuid, pub base_url: String, pub model: String,
    pub dimensions: u32, pub normalization_version: u32,
}
pub struct KnowledgeHit {
    pub document_id: Uuid, pub document_version: u32, pub chunk_id: Uuid,
    pub title: String, pub location: TextLocation, pub text: String, pub score: f32,
}
pub struct KnowledgeSearch {
    pub query: String, pub selected_segment_ids: Vec<Uuid>,
    pub session_id: Option<Uuid>, pub limit: u32,
}
pub struct KnowledgeResults {
    pub hits: Vec<KnowledgeHit>, pub identity: Option<String>,
    pub unavailable_documents: u32,
}
#[async_trait]
pub trait Embeddings: Send + Sync {
    async fn embed(&self, profile: &ModelProfile, inputs: &[String],
        cancel: watch::Receiver<bool>) -> Result<Vec<Vec<f32>>, IntelligenceErrorCode>;
}
impl KnowledgeService {
    pub async fn search(&self, request: KnowledgeSearch) -> AppResult<KnowledgeResults>;
    pub async fn reindex(&self, document_id: Uuid) -> AppResult<JobView>;
    pub async fn delete(&self, document_id: Uuid) -> AppResult<JobView>;
    pub async fn reconcile_local(&self) -> AppResult<()>;
}
```

KnowledgeService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, embeddings:Arc<dyn Embeddings>, index:Arc<IndexOwner>) -> Self; IndexOwner::new(data_dir:PathBuf) -> AppResult<Self> constructs an inert facade; `start_owned(OperationLease)` starts one bounded worker generation with its own base process lease. Cached engines retain that base lease independently of per-job child slots. Close drains actual jobs and admitted direct operations before closing the index queue, then drains accepted index work and drops engines/pool on the owner before acknowledging completion; observer timeout keeps admission/rearm fenced and cleanup available.

Commands `search_knowledge(request)`, `reindex_document(document_id)`, `delete_document(document_id)` match these methods. The submitted query is the authoritative explicit text to embed, with length 1–4096 Unicode scalars. Supporting selected transcript IDs must belong to the supplied session; validate their membership/provenance without replacing or expanding the query to whole transcript rows. Retain the exact selected query when IDs are present; an edited query clears the exact-selection IDs at the frontend boundary. Add command-boundary tests selecting a few words inside one longer row and text across multiple rows; the recording Embeddings port must receive only the submitted query, and a foreign-session ID must fail before any embedding call. No automatic query on every subtitle or keystroke. Search max limit 20, default 5; at most two outstanding search requests, cancellable on replacement.

- [ ] **1. Write RED real-index persistence/recovery tests.** `KnowledgeHarness` extends IntelligenceHarness with a real temp SrvDB owner, DocumentService and deterministic Embeddings returning normalized 128-dimensional axes. Its methods import_text(name,text), index(id), search(query), crash_at(CommitPoint), reopen(), delete(id) use real SQLite/index operations; embedding_calls() returns the deterministic embedding port's observed request count. `CommitPoint` is a test-only injected callback at SQLite intent, vector mutation, vector persist and SQLite ready update; it must not alter production transaction logic.

```rust
#[tokio::test]
async fn crash_after_vector_persist_never_exposes_an_uncommitted_document() {
    let mut h = KnowledgeHarness::new().unwrap();
    let id = h.import_text("pump.md", "额定电压 24 V").await.unwrap();
    h.crash_at(CommitPoint::AfterVectorPersist);
    assert!(h.index(id).await.is_err());
    let h = h.reopen().await.unwrap();
    let before = h.embedding_calls();
    assert!(matches!(h.search("额定电压").await,
        Err(AppError::Intelligence(IntelligenceErrorCode::IndexRepairRequired))));
    assert_eq!(h.embedding_calls(), before);
    h.index(id).await.unwrap();
    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
}
```

Also test replacement returns/persists a new vector ID; deletion across reopen; stale hash, missing metadata, missing vector, wrong dimension, same model name at different endpoint/profile, NaN/infinity/zero vectors, out-of-order embedding indexes, cancellation during import, duplicate retry, orphan vector and persisted generation swap. Check independent saved profile edit does not reinterpret an old vector space.

- [ ] **2. Implement embedding transport and identity.** OpenAiCompatibleEmbeddings::new(config:Arc<ServiceConfigStore>, http:Arc<dyn JsonHttp>) -> Self stores the collaborators; embed uses an ephemeral scoped config carrying an already-issued actual-completion owner, resolves config.resolve_snapshot_credentials(profile.clone()) under one ConfigIo gate, moves its SecretString into JsonRequest, and maps resolver failure to Configuration before invoking HTTP. POST normalized `/embeddings` with model/input, encoding_format float and optional dimensions. Batch <=32 texts and <=128KiB JSON, concurrent requests <=4 globally; response <=8MiB. Match `data[i].index` to input, reject duplicate/missing/out-of-range indexes, non-finite/empty/zero vectors and inconsistent dimensions. First valid response fixes actual dimension; it must be 128–4096 and match configured dimension if provided. Hash canonical JSON of EmbeddingIdentity to a lowercase hex directory key; keys are excluded. L2-normalize with checked finite norm. Avoid holding SQLite/index locks during network calls.

- [ ] **3. Use actual SrvDB APIs through one owner.**

```rust
use srvdb::{DatabaseConfig, SrvDB, Vector, VectorEngine};
let config = DatabaseConfig::new(identity.dimensions as usize)
    .map_err(|_| AppError::Configuration("unsupported embedding dimension".into()))?;
let mut db = SrvDB::new_with_config(index_dir.to_str().ok_or_else(
    || AppError::Configuration("index directory is not valid UTF-8".into())
)?, config).map_err(index_error)?;
let metadata_json = serde_json::to_string(&metadata)
    .map_err(|_| AppError::Internal("index metadata could not be encoded".into()))?;
let id = db.add(&Vector::new(vector), &metadata_json)
    .map_err(index_error)?;
db.persist().map_err(index_error)?;
```

`index_error` maps local categories without logging content. Define strict metadata schema2/codec `chunk_location_ref_v1`: document ID/version, chunk ID, content hash, inline nullable page plus paragraph/start/end, and SHA-256 of full canonical SQLite TextLocation JSON (page/paragraph/heading/start_char/end_char order, explicit nulls). Encoded vector metadata is at most 16384 bytes; authoritative hydration preserves the complete SQLite heading/text. Start Flat full-precision with quantization disabled and mode None, then call `set_mode(IndexMode::Auto)` on every identity/generation open; default construction otherwise leaves automatic migration inactive. Use the pinned documented automatic behavior without claiming a fixed HNSW mode. Run every SrvDB open/search/mutation/persist/rebuild validation inside one owner-scoped Rayon pool with exactly two workers, counting the dedicated owner separately. One dedicated blocking index worker owns one open database per active identity, max two open identities with idle LRU close; command queue 16, no unbounded channel. All `add/replace/delete/persist/search` happen on that owner; no duplicate directory opens. One published generation per identity may temporarily coexist with its staging rebuild generation; evict the other idle identity before that overlap so total SrvDB engine instances remains at most two. A private VectorStorage reader may open the existing nonempty source vector file only while its source SrvDB retains the directory lock, persistence/count/dimension checks pass, and a maintenance lease fences all source mutations/remaps/close. Copy borrowed vectors immediately in at most 200-record batches, preserving their full-precision values; never reopen a second SrvDB on the source directory.

- [ ] **4. Make SQLite-first operations recoverable.** In SQLite mark affected document/index rows Indexing/RepairRequired and record desired version/hash before SrvDB mutation. Batch embeddings then add/replace vectors, retain returned IDs, call `persist()`, and only then transactionally store IDs and mark Ready. Until this final transaction, search excludes the document. On failure keep desired text and repair marker; do not delete original or managed source. Replacement never returns old-version hits while new rows are partial.

Deletion first marks Deleting plus a durable cleanup intent containing prior vector IDs/identity, excludes it from search, deletes/tombstones vectors, persists, then removes SQLite rows and managed copies. If a crash leaves an orphan vector, later hydration excludes it and reconciliation deletes it. Cleanup intent must survive removal of document rows, so store it in intelligence job payload or a non-cascading cleanup table until persisted completion.

On startup under the app lock, check all Ready chunk IDs/hashes/model identities in bounded batches of 200. Avoid unbounded `all_metadata()` in normal operation: scan vector IDs over the half-open range `0..count()` in batches, use `get_metadata` and skip tombstones, with checked u64 iteration and idle yields. This matches the pinned RecordFile::append returning the prior count, flat append_batch returning `first..first+n`, and delete rejecting `id >= count()`; ID 0 is valid and ID count is excluded. Snapshot count while holding the serialized index owner so mutations do not move the scan boundary. Cap each open database to 200000 stored records INCLUDING tombstones; a bounded generation rebuild recovers capacity after deletion, and new imports fail visibly until space is available. SrvDB's inherent open-memory/index cost remains bounded by this supported limit. Unmatched/malformed metadata marks the document RepairRequired; do not return that document's results. Local orphan cleanup needs no embedding call. Reembedding is an explicit reindex job. A whole-index rebuild writes a new generation directory, persists and validates it, then atomically switches the SQLite generation pointer; only afterwards delete the old managed generation. Never overwrite the only good index during rebuild. Do not call bare SrvDB::compact on an active generation: it renumbers live IDs densely and returns only a removed count. The new generation must publish every new chunk/vector mapping in SQLite with its pointer swap. Add a first-ID-zero/last-ID-count-minus-one/tombstone scan regression and a generation-rebuild mapping regression.

- [ ] **5. Search and hydrate authoritatively.** Before any embedding request, inspect local document/index readiness for the selected embedding identity. If the library has documents but none is healthy/searchable because its index is missing, partial or RepairRequired, return AppError::Intelligence(IndexRepairRequired) with zero embedding calls. A truly empty library returns an empty result locally. With at least one healthy searchable document, embed on explicit search, request up to `min(limit*5,100)` candidates, then validate metadata ID/version/hash against SQLite Ready rows and active identity before returning. Scores are finite cosine similarities, not percentages/probabilities. Return <=20 hits with source title/page/heading and full bounded chunk text. Keep candidate IDs/scores compact and hydrate one row at a time through bounded borrowed SQLite values before copies: title <=32768 UTF-8 bytes, chunk <=2400 Unicode scalars/9600 UTF-8 bytes, complete canonical location JSON <=16MiB. Heading wire bound is16MiB raw UTF-8; the existing8MiB body bound remains. Cap the actual compact serde-serialized KnowledgeResults JSON at32MiB, including escaped fields, envelope and punctuation; use a checked count-only writer on borrowed values before admitting/copying whole hits, preserve the eligible score prefix, and stop at the first eligible overflow. Never truncate fields or count budget omission as an unavailable document. A single otherwise-valid hit that cannot fit returns ResourceLimit. Recheck readiness/unavailable count and final serialization budget, dropping only trailing whole hits if the envelope grows. Add a frontend aggregate defense without assuming JavaScript/Rust float serialization byte equality. `identity=None` represents an empty/unbound local result; a resolved indexed space returns `Some(lowercase64hex)`, never an invented empty-string/fingerprint identity. If readiness becomes invalid during the request and no healthy searchable documents remain, return IndexRepairRequired rather than empty success; the preflight test above covers the already-known-invalid case before any metered call. Mixed healthy/broken collections may return healthy hits plus `unavailable_documents` and a local repair action. Selection search sends only the explicit selected text to the configured Embedding service; no translation provider receives knowledge queries.

- [ ] **6. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
```

Commit `feat: index and search local documents with recoverable SrvDB state`.

## Additional mandatory architecture preflight

Read task-5-managed-caps-handoff.md beforesourceimplementation. Itestablisheswhyfilecapsalonecannotboundeagerredbtombstone/metadata allocations beforeSrvDBreturns. Atarchitecturecheckpoint,confirmfixedvector/metadata/graph/header/directorylimits compatiblewith200000storedrecords/128–4096dims, theactualTextLocationJSONcontract,andtrustedSQLitegenerationseal+dirtyintent lifecycleusingreviewedmigration4storagefrom4B. Noarbitraryunsealedredbopen/post-hoccountcheckclaimed safe. Preserveoriginalfiles/text andexplicitrepair onunsafeunsealedrestart;noautomaticpaidreembedding. Freshuniqueemptycreation andknowntrustedlivepartialpersistretry remainlocal; vector-preservingfacade stillrequirestheretainedpersistedSrvDBlock/maintenancelease. Theseproposals needcontroller rulingbeforeimplementation;neitherreportnorfilecaps areRSS/smallRAM/nativeperformance evidence.

## Additional prepared producer handoff

Read task-5-seal-format-handoff.md and reviewed4B task-4b-work-tables-checkpoint.md/report at dispatch. The accepted m4 seal/intent envelopes fit bounded typed provenance; bulk prior-ID cleanup/rebuild must use paged durable knowledge_index_work_items and generation mappings rather than IDs in64KiBjob/16KiBintent JSON. Work/mappings outlive current document/chunk/job rows and RESTRICT generation-seal deletion until explicit cleanup. Task5 owns real publication/cleanup lifetimes, active-generation/current-Ready/version/hash hydration and identity deletion policy.

A concrete legal6000-CJKMarkdownheading makes full-locationmetadata18318B;16KiBper-value full-location encoding is insufficient. At Task5 architecture choose full-fidelity bounded reference/digest metadata with canonical fullSQLiteTextLocation hydration (proposed schema2/382B), or explicitly larger full-location/value/file caps. Do not silently truncate heading, invent a new parser restriction or reduce200000records/128–4096dimensions. The proposed seal/intent formats1141/671B fit existing64/16KiB; raw full-location alternative8MiBvalue implies8TiBfile envelope with the original factor, so this is a real policy decision, not a small constant bump. These are static proposals pending the Task5 author/controller checkpoint, not implementation or RAM evidence.

At the actual Task5 checkpoint, any reference metadata must still carry the inline nullable page (the approved spec explicitly requires per-vector page metadata). The fixed-width paragraph/start_char/end_char may also be retained inline; digest comparison covers the full canonical SQLite TextLocation including heading. A digest-only location object cannot silently remove the spec page field. No final codec decision is made by this preparation note.

## Mandatory future handler credential ownership

Read task-5-job-credential-ownership-handoff.md before any model transport implementation. Actual reviewed start_owned lease is Arc<File>; AppState's separate bounded Operation also owns an intelligence_operations permit, but handlers receive neither a native-work token nor a drain port. ConfigIo keeps a supplied scoped owner through actual entered blocking completion; an unscoped credential observer may return Configuration after its10s timeout while secret_get/DEK creation continues. A physical lock clone alone does not preserve same-AppState generation rearm fencing.

At Task5 checkpoint implement the smallest counted generation-child operation token issued by JobContext and scoped into ephemeral ServiceConfigStore/ConfigIo. Refuse new tokens under stopping fence; retain actual process lease through entered native work; generation waits all issued-token drain before publishing completion/dropping its lease. No handler captures AppState/SchedulerArc or stores a scoped config long term. Cover controlled credential timeout/cancel/late completion/close/rearm with injected native boundary, not actual keyring. This is future handler integration, not a claim reviewed3A/3B are defective.

resolve_snapshot_credentials does not currently exist. Existing resolve_credentials reads the current saved profile plus secret under its config gate; model ports must validate the admitted immutable public profile under that same gate before resolving credentials/HTTP. Add the narrow crate-private snapshot resolver rather than inventing an existing method or reinterpreting the old job using newly edited profile fields. Actual naming/schema is author/controller checkpoint.

The same actual-completion ownership principle applies to queued synchronous IndexOwner requests: an async observer timeout/drop cannot release a job/generation/process owner while the dedicated worker/pool call still mutates/persists/drops mappings. At the Task5 checkpoint explain the owned request/cleanup token carrier for index calls, including direct AppState search operations and shutdown/rearm, alongside credential scoping. Keep queue16/owner+2poolworkers/<=2engine limits.

## Current accepted producer checkpoint, supersedes preparation notes

Task5A is independently reviewed and committeddf57179. JobContext::operation_owner and ServiceConfigStore::resolve_snapshot_credentials now exist with accepted one-gate semantics. Metadata schema2/reference codec, fixed caps/provenance, local discovery binding and live-dirty behavior are accepted in progress.md. Root canonical refinementsc8f7770 establish inert owned-start, nullable KnowledgeResults.identity and exact32MiB/full16MiB-heading hydration policy. The older proposal/does-not-exist wording above is historical; never reimplement5A. Actual5B/5C scopes are supplied in their separate briefs.
