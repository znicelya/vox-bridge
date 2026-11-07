# Task5C frozen staged-source review package

Recorded BASE: 1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee
Frozen cached tree: 76677da1a712b77a10f17fd59b1bfcc2c31553a2
Source paths: 53
Manifest SHA256: 172d891e12bb5dcd0997d27957439696f6ec827f61cd8c5b6c34fc7171f4222c

This task is not committed yet; the frozen staged tree is the review HEAD.

## Commit range


## Diff stat

 .../plans/2026-11-04-workbench-intelligence.md     |  14 +-
 src-tauri/migrations/0005_knowledge_index.sql      |  20 +
 src-tauri/src/app/document_test_support.rs         |  20 +
 src-tauri/src/app/intelligence.rs                  |  31 +
 src-tauri/src/app/intelligence/tests.rs            |  51 +-
 .../src/app/intelligence/tests/child_operations.rs |  14 +-
 src-tauri/src/app/intelligence/tests/knowledge.rs  | 176 +++++
 src-tauri/src/app/lifecycle.rs                     |   1 +
 src-tauri/src/app/mod.rs                           |  68 +-
 src-tauri/src/commands/knowledge.rs                |  40 +-
 src-tauri/src/commands/knowledge/tests.rs          | 439 ++++++++++-
 src-tauri/src/intelligence/config.rs               | 131 +++-
 src-tauri/src/intelligence/jobs.rs                 | 287 ++++++--
 src-tauri/src/intelligence/jobs/test_support.rs    |  14 +
 src-tauri/src/intelligence/jobs/tests.rs           |  35 +-
 .../src/intelligence/jobs/tests/continuations.rs   | 193 +++++
 src-tauri/src/knowledge/document_tests.rs          | 331 +++++++--
 src-tauri/src/knowledge/documents.rs               |  39 +-
 src-tauri/src/knowledge/documents/handler.rs       | 276 +++++--
 src-tauri/src/knowledge/embedding.rs               | 390 ++++++++++
 src-tauri/src/knowledge/embedding_tests.rs         | 275 +++++++
 src-tauri/src/knowledge/identity.rs                |  20 +
 src-tauri/src/knowledge/index.rs                   |  16 +-
 src-tauri/src/knowledge/index/codec.rs             |  17 +-
 src-tauri/src/knowledge/index/lifecycle.rs         | 363 +++++++++-
 src-tauri/src/knowledge/index/owner.rs             | 315 +++++++-
 src-tauri/src/knowledge/index/preflight.rs         |   4 +-
 src-tauri/src/knowledge/index/publication.rs       | 734 +++++++++++++++++++
 src-tauri/src/knowledge/index/recovery.rs          | 260 +++++++
 src-tauri/src/knowledge/index/repository.rs        | 171 ++++-
 src-tauri/src/knowledge/index/seal.rs              |  64 +-
 src-tauri/src/knowledge/index/tests/lifecycle.rs   |  74 +-
 src-tauri/src/knowledge/index_tests.rs             | 489 +++++++++++++
 src-tauri/src/knowledge/index_tests/binding.rs     | 187 +++++
 .../src/knowledge/index_tests/continuation.rs      | 136 ++++
 src-tauri/src/knowledge/index_tests/publication.rs | 529 ++++++++++++++
 src-tauri/src/knowledge/index_tests/recovery.rs    | 415 +++++++++++
 src-tauri/src/knowledge/index_tests/search.rs      | 803 +++++++++++++++++++++
 src-tauri/src/knowledge/mod.rs                     |  10 +
 src-tauri/src/knowledge/reconcile.rs               | 545 ++++++++++++++
 src-tauri/src/knowledge/search.rs                  | 403 +++++++++++
 src-tauri/src/knowledge/service.rs                 | 295 ++++++++
 src-tauri/src/knowledge/service/continuation.rs    | 447 ++++++++++++
 src-tauri/src/knowledge/service/delete.rs          | 305 ++++++++
 src-tauri/src/knowledge/service/recovery.rs        | 323 +++++++++
 src-tauri/src/knowledge/service/reindex.rs         | 204 ++++++
 src-tauri/src/knowledge/service/searches.rs        | 110 +++
 src-tauri/src/knowledge/service/state.rs           | 437 +++++++++++
 src-tauri/src/knowledge/types.rs                   |  32 +
 src-tauri/src/storage/sqlite.rs                    | 152 +++-
 src-tauri/src/storage/sqlite_tests.rs              |   4 +-
 src/lib/knowledge-wire.test.ts                     | 181 ++++-
 src/lib/knowledge-wire.ts                          | 187 ++++-
 53 files changed, 10657 insertions(+), 420 deletions(-)

## Full frozen diff

diff --git a/docs/superpowers/plans/2026-11-04-workbench-intelligence.md b/docs/superpowers/plans/2026-11-04-workbench-intelligence.md
index 578663c..e43ada1 100644
--- a/docs/superpowers/plans/2026-11-04-workbench-intelligence.md
+++ b/docs/superpowers/plans/2026-11-04-workbench-intelligence.md
@@ -596,21 +596,21 @@ After parse, persist text/location/hash in a SQLite transaction and mark Parsed.
 ```bash
 cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge::parser
 cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge::documents
 cargo test --manifest-path src-tauri/Cargo.toml --test document_worker
 ```
 
 Register the offline worker test target explicitly in Cargo.toml. Commit `feat: parse managed documents with bounded worker isolation`.
 
 ### Task 5: Embeddings, SrvDB consistency and document search
 
-**Files:** Create knowledge/{embedding,index,reconcile,search,index_tests,reconcile_tests}.rs. Extend knowledge store/documents/commands, Cargo manifests, AppState and job registration. Add small SrvDB integration fixtures/tests; use the actual crate in default tests with 128-dimensional synthetic vectors.
+**Files:** Create knowledge/{embedding,index,reconcile,search,index_tests,reconcile_tests}.rs and migrations/0005_knowledge_index.sql for the accepted additive binding indexes. Extend knowledge store/documents/commands, Cargo manifests, AppState and job registration. Add small SrvDB integration fixtures/tests; use the actual crate in default tests with 128-dimensional synthetic vectors.
 
 **Interfaces:**
 
 ```rust
 pub struct EmbeddingIdentity {
     pub profile_id: Uuid, pub base_url: String, pub model: String,
     pub dimensions: u32, pub normalization_version: u32,
 }
 pub struct KnowledgeHit {
     pub document_id: Uuid, pub document_version: u32, pub chunk_id: Uuid,
@@ -694,21 +694,21 @@ On startup under the app lock, check all Ready chunk IDs/hashes/model identities
 
 ```bash
 cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge
 cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
 ```
 
 Commit `feat: index and search local documents with recoverable SrvDB state`.
 
 ### Task 6: Automatic/manual summaries with complete input coverage
 
-**Files:** Create llm/{mod,openai_compatible,tests}.rs, summary/{mod,types,store,planner,tests}.rs, migrations/0005_summaries.sql and commands/summary.rs. Modify session/orchestrator final-commit notification, AppState/jobs registration and export mapping. Do not change audio stop ordering.
+**Files:** Create llm/{mod,openai_compatible,tests}.rs, summary/{mod,types,store,planner,tests}.rs, migrations/0006_summaries.sql and commands/summary.rs. Modify session/orchestrator final-commit notification, AppState/jobs registration and export mapping. Do not change audio stop ordering.
 
 **Interfaces and strict schema:**
 
 ```rust
 pub struct LlmRequest {
     pub system: String, pub user: String, pub max_output_tokens: u32,
 }
 pub struct LlmResponse { pub content: String, pub response_hash: String }
 #[async_trait]
 pub trait LlmProvider: Send + Sync {
@@ -775,27 +775,27 @@ async fn closed_session_channel_does_not_prevent_one_automatic_summary() {
     assert_eq!(h.summary_versions(id).unwrap()[0].status, JobStatus::Succeeded);
 }
 ```
 
 Add tests for successful manual rerun after a Succeeded version and manual retry after each Failed/Cancelled/Interrupted version, with strictly increasing versions and distinct job UUIDs; two concurrent manual summary requests must create only one new version. Replay after_terminal after an automatic job finishes and after reopening storage; it must keep the original job/version. A failed automatic job followed by manual retry must retain one automatic permanent marker and add exactly one manual version. Deleting the successful automatic summary version must not allow after_terminal to launch another automatic request. Inject a summary-version insert failure and assert neither a job nor a permanent marker remains after rollback.
 
 Delete the highest version, reopen storage, then manually generate again: the new version must exceed the deleted version. Also delete every version and verify the next version still advances; a rolled-back admission consumes no version. Cover checked u32 exhaustion with a safe local failure before any job/version insertion.
 
 Add delayed LLM proving stop already returned, failed JSON raw/hash persisted, selected-model switch creates a new version, cancellation, deletion during an active summary, storage failure, no configured LLM, empty transcript, and a transcript larger than one request whose middle sentinel appears in the recorded map requests. Assert coverage set equals the complete final snapshot IDs; never test only the first/last lines.
 
-- [ ] **2. Extend version storage safely.** Version 5 adds profile_id, model, input_hash, input_final_count, snapshot_last_rowid, error_code and raw_response_hash to existing session_summaries. Add `summary_responses(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, ordinal INTEGER, phase TEXT, raw_text TEXT NOT NULL, response_hash TEXT NOT NULL, PRIMARY KEY(summary_id,ordinal))` and `summary_coverage(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, segment_id TEXT NOT NULL, part_index INTEGER NOT NULL, part_count INTEGER NOT NULL, PRIMARY KEY(summary_id,segment_id,part_index), CHECK(part_count > 0), CHECK(part_index >= 0 AND part_index < part_count))`. Add `summary_version_counters(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, last_version INTEGER NOT NULL CHECK(last_version >= 0))`, seeded from each existing session's MAX(version); new sessions allocate from zero on first admission. Counter increment uses checked u32 range inside the admission transaction. Summary deletion leaves the counter; session deletion cascades it. Keep old structured_json/status rows readable as legacy versions; missing metadata remains None.
+- [ ] **2. Extend version storage safely.** Version 6 adds profile_id, model, input_hash, input_final_count, snapshot_last_rowid, error_code and raw_response_hash to existing session_summaries. Add `summary_responses(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, ordinal INTEGER, phase TEXT, raw_text TEXT NOT NULL, response_hash TEXT NOT NULL, PRIMARY KEY(summary_id,ordinal))` and `summary_coverage(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, segment_id TEXT NOT NULL, part_index INTEGER NOT NULL, part_count INTEGER NOT NULL, PRIMARY KEY(summary_id,segment_id,part_index), CHECK(part_count > 0), CHECK(part_index >= 0 AND part_index < part_count))`. Add `summary_version_counters(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, last_version INTEGER NOT NULL CHECK(last_version >= 0))`, seeded from each existing session's MAX(version); new sessions allocate from zero on first admission. Counter increment uses checked u32 range inside the admission transaction. Summary deletion leaves the counter; session deletion cascades it. Keep old structured_json/status rows readable as legacy versions; missing metadata remains None.
 
 `summary_coverage` part indexes are zero based; an unsplit final uses `(segment_id,0,1)`. Validate each segment's consistent part count against the immutable manifest; success requires the exact complete set of expected part tuples, while `input_final_count` remains the number of distinct final segment IDs. Persist supplied map-input coverage with its retained raw response before parsing, so a failed attempt cannot claim uncalled planned parts. Add a middle-part omission regression that fails even when the distinct segment-ID set matches.
 
 Use create_summary_job to allocate the next durable counter value together with job admission, create a Queued version before cloud work, and update its status/JSON on success/failure. Raw response is written before parsing. Store each map/reduce response <=2MiB and <=32MiB total per summary; exceeding the cap fails visibly while preserving prior responses. Return hashes/status/parsed data to UI; do not put raw model content into generic diagnostics/logs. Summary deletion cascades responses/coverage and rejects or cancels an in-flight version with bounded completion before removal. Export includes structured summaries and safe model/version metadata; raw responses remain local detail data, outside default export.
 
-- [ ] **3. Implement complete final snapshot and bounded hierarchy.** Capture a SQLite read snapshot of final rows with session-filtered rowid upper bound, each row retaining ID, direction, role/language and text. Max 16MiB normalized input and 100000 rows; exceeding limit creates ContextTooLarge, never truncates. No interim or audio. Order by storage insertion order; use direction-specific labels, not inferred speaker names. Check borrowed SQLite text/metadata lengths before eagerly allocating legacy rows and charge the bounded normalized encoding, including escaping and retained identity metadata. Release the read transaction before any network await. Add a migration-5 index on `translation_segments(session_id)` and verify the ordered session/high-water query plan, rather than treating a page limit as a bound on an unrelated table scan or sort. Native kind comes from stored details; legacy missing kind remains explicitly unknown in the new snapshot and is never inferred from language.
+- [ ] **3. Implement complete final snapshot and bounded hierarchy.** Capture a SQLite read snapshot of final rows with session-filtered rowid upper bound, each row retaining ID, direction, role/language and text. Max 16MiB normalized input and 100000 rows; exceeding limit creates ContextTooLarge, never truncates. No interim or audio. Order by storage insertion order; use direction-specific labels, not inferred speaker names. Check borrowed SQLite text/metadata lengths before eagerly allocating legacy rows and charge the bounded normalized encoding, including escaping and retained identity metadata. Release the read transaction before any network await. Add a migration-6 index on `translation_segments(session_id)` and verify the ordered session/high-water query plan, rather than treating a page limit as a bound on an unrelated table scan or sort. Native kind comes from stored details; legacy missing kind remains explicitly unknown in the new snapshot and is never inferred from language.
 
 Budget each request with a conservative UTF-8-byte estimate plus 1024 prompt/schema overhead against `max_context_tokens - max_output_tokens`; document this as an estimate for arbitrary compatible tokenizers, not exact token accounting. Provider context-limit errors fail safely. Split oversized individual rows at Unicode boundaries with `(segment_id,part_index,part_count)`; every part appears in coverage. Map chunks use the same strict schema. Hierarchically reduce bounded intermediate summaries until one fits, max 64 requests and 8 levels; if no reduction progress/context limit, fail ContextTooLarge. Never omit a chunk to meet a limit. Preserve request coverage independently of what the model chooses to mention; do not promise semantic completeness beyond the supplied input.
 
 OpenAiCompatibleLlm::new(config:Arc<ServiceConfigStore>, http:Arc<dyn JsonHttp>) -> Self uses the same crate-private resolver and opaque JsonRequest bearer; its complete method cannot read a plaintext key. ResearchService and explicit probes use that resolver for their own saved profile ID as well.
 
 Chat POST `/chat/completions`: model, system/user messages, temperature 0, max_tokens, non-streaming, no proprietary tools/schema capability requirement; the system instruction requests the strict JSON and the local parser verifies it. System instruction treats transcript text as quoted data and requests no invented participants/actions/dates. LLM output never triggers tools or network. Parse `choices[0].message.content` as a bounded string; missing choices/refusal/wrong types fail InvalidResponse. Secrets resolve backend-only at execution.
 
 - [ ] **4. Wire exactly-once automatic scheduling after durable termination.** Add an injected terminal observer or callback on the lifecycle owner's successful Completed/Failed commit; it receives only session ID and cannot delay audio cleanup/network stop. Schedule with JobAdmission::AutomaticSummary and its permanent key `auto-summary:<session UUID>` using create_summary_job. Repeated observer calls return the same job/version even after that job is terminal; a duplicate does not reserve another version. Existing session bus closes normally; intelligence events/polling carry progress. At startup, local reconciliation uses the same eligible Completed-or-Failed terminal predicate, including sessions marked Failed by interrupted runtime recovery with retained final text. With automatic-summary enabled and no permanent automatic marker/job/version, it creates a visible local Interrupted/retry record rather than silently reissuing a paid call after uncertain crash history. Add completed and failed terminal-commit crash-window tests plus a restart-interrupted session with retained finals; all must create one local retry record and make zero LLM calls, including after a second reopen. New normal terminal sessions start automatically when final text exists, including a failed session with retained text; an empty terminal session records a local NoFinalText result without calling a model. Missing model/queue-full records a failed summary version with safe code and retry control. Manual regeneration or retry uses the same in-flight exclusion key `summary:<session UUID>` with permanent_key=NULL, creates a NEW job UUID/version after the prior job is terminal, and never overwrites prior output. A failed/interrupted automatic job retains its permanent marker; clicking Retry creates a manual job, not a second automatic job. Removing a summary version does not remove the automatic marker. Missing model, queue-full or NoFinalText creates the failed automatic marker and failed version atomically so repeated terminal callbacks cannot keep creating failures. Stop result is independent of LLM success.
 
 - [ ] **5. Verify and commit.**
@@ -804,21 +804,21 @@ Chat POST `/chat/completions`: model, system/user messages, temperature 0, max_t
 cargo test --manifest-path src-tauri/Cargo.toml --lib summary
 cargo test --manifest-path src-tauri/Cargo.toml --lib llm
 cargo test --manifest-path src-tauri/Cargo.toml --lib session
 cargo test --manifest-path src-tauri/Cargo.toml --lib export
 ```
 
 Commit `feat: summarize complete session transcripts with durable versions`.
 
 ### Task 7: Explicitly confirmed Tavily research and cited overview
 
-**Files:** Create research/{mod,types,entities,tavily,store,tests}.rs, migrations/0006_research.sql and commands/research.rs. Extend AppState/jobs registration and LLM integration; preserve stored sources independently of overview success.
+**Files:** Create research/{mod,types,entities,tavily,store,tests}.rs, migrations/0007_research.sql and commands/research.rs. Extend AppState/jobs registration and LLM integration; preserve stored sources independently of overview success.
 
 **Interfaces:**
 
 ```rust
 pub struct EntityCandidate {
     pub id: Uuid, pub text: String, pub session_id: Uuid,
     pub supporting_segment_ids: Vec<Uuid>, pub kind: String,
 }
 pub struct ResearchRequest {
     pub query: String, pub session_id: Option<Uuid>,
@@ -870,21 +870,21 @@ async fn unconfirmed_candidate_never_queues_a_tavily_request() {
         supporting_segment_ids: vec![], confirmed: false,
     }).await;
     assert!(result.is_err());
     assert_eq!(h.http.request_count(), 0);
     assert_eq!(h.store.unfinished_job_count().unwrap(), 0);
 }
 ```
 
 ResearchHarness uses IntelligenceHarness/RecordingHttp and real source storage. Add suggestions only local, unsupported URL schemes, foreign-session segment IDs, empty/no-result response, 429/timeout, oversized/malformed body, duplicate source URLs, invalid citation ID, overview failure leaving sources intact, prompt-injection text rendered as text and no tool calls, cancel/retry and source deletion cascades.
 
-- [ ] **2. Add source/provenance migration.** Version 6 adds query, profile_id, error_code, response_hash and confirmed_at_ms to research_runs; nullable excerpt, nullable published_at, nullable fetched_at_ms, origin (legacy `legacy`) and ordinal to research_sources. Retain the foundation source title column as nullable. Add `research_support(run_id,segment_id)` and versioned `research_overviews` with parsed_json, raw_text/hash, profile/model/status/error, created time and unique(run_id,version). Preserve legacy sources with title=None/excerpt=None/fetched_at_ms=None and origin=Legacy when those fields were absent; never invent an empty excerpt, page title or fetch time. Native Tavily writes Some validated title/excerpt/fetched time and origin=Native; optional published_at remains None if not supplied. Source metadata is saved before optional overview begins. Add a foundation fixture with title=NULL and no provenance columns, migrate/reopen twice, and assert nullable values and original URL survive unchanged. Wire fixture/UI tests must cover this legacy source beside a complete native source.
+- [ ] **2. Add source/provenance migration.** Version 7 adds query, profile_id, error_code, response_hash and confirmed_at_ms to research_runs; nullable excerpt, nullable published_at, nullable fetched_at_ms, origin (legacy `legacy`) and ordinal to research_sources. Retain the foundation source title column as nullable. Add `research_support(run_id,segment_id)` and versioned `research_overviews` with parsed_json, raw_text/hash, profile/model/status/error, created time and unique(run_id,version). Preserve legacy sources with title=None/excerpt=None/fetched_at_ms=None and origin=Legacy when those fields were absent; never invent an empty excerpt, page title or fetch time. Native Tavily writes Some validated title/excerpt/fetched time and origin=Native; optional published_at remains None if not supplied. Source metadata is saved before optional overview begins. Add a foundation fixture with title=NULL and no provenance columns, migrate/reopen twice, and assert nullable values and original URL survive unchanged. Wire fixture/UI tests must cover this legacy source beside a complete native source.
 
 - [ ] **3. Implement local candidates and confirmation.** Read only final subtitles, cap the most recent 200 rows/128KiB. Extract HTTPS/HTTP domain references and bounded company-like spans, including Chinese suffixes 公司/集团/科技/有限公司 and English company suffixes. Return <=12 deduplicated candidates with supporting IDs. Candidate generation uses deterministic rules, not an automatic LLM/network call; confidence is not a fabricated percentage. Empty result keeps manual company/URL input available.
 
 `start_research` requires confirmed=true, query trimmed 2–256 characters, <=8 supporting IDs belonging to the supplied session. Manual query with no session is supported. Confirmation is the user's explicit button action; selecting a subtitle or typing cannot queue the job. Normalize URL queries for display and allow http/https only; discard URL credentials/fragments and reject embedded credentials rather than leaking them. Never crawl the entered website directly; Tavily owns search.
 
 - [ ] **4. Implement Tavily and citations.** POST selected endpoint with `Authorization: Bearer <backend key>` and body `{query, search_depth:"basic", max_results:8, include_answer:false, include_raw_content:false}`. This release uses basic depth with eight-result cap; no unsupported depth setting is exposed. Use Task 3 HTTP limits/cancellation; no secret in JSON/logs. Accept documented `results` entries with url/title/content and optional published_date. Canonicalize valid http/https URLs for deduplication, cap URL 2048 bytes/title 300 chars/excerpt 8000 chars, skip invalid entries with bounded local counts. Store request hash and response hash, actual fetched time and original source URL. No results is successful empty data, not a network failure.
 
 Optional overview sends only stored source IDs/title/url/excerpt to the selected LLM; exclude legacy rows with absent/empty excerpts, never send UI fallback text as evidence. If no source has usable stored evidence, return a local InvalidResponse with re-search guidance before any LLM request. Strict schema `{facts:[{text,source_ids}],unknowns:[...]}` with <=30 facts/unknowns, text <=2000 chars. Every fact requires at least one source ID from this run; reject invented IDs and missing citations, and show links from stored URLs. Unknowns may contain explicit unanswered questions but cannot be promoted to facts. Persist a failed raw overview response with hash and retain all source rows. Opening a result uses the system browser only on user click and revalidates http/https; never render remote HTML or auto-load remote previews.
 
 - [ ] **5. Verify and commit.**
@@ -1050,17 +1050,17 @@ git diff --check
 ```
 
 Update README with actual setup/configuration, live/call route selection, final/history/export behaviour, metered connection tests, local managed data, model/index replacement/recovery, scan/OCR limits and manual Tavily confirmation. Keep release gates for signed Windows/macOS drivers, PulseAudio/pipewire-pulse subprocess association/drain, minimum OSes, real Volcengine and chosen intelligence endpoints. Do not label the application release-ready on fixture/browser tests alone.
 
 Commit `test: verify workbench intelligence flows and production presentation`.
 
 ## Controller preflight checklist
 
 - [ ] Reconcile runtime Task 6's final DTO/serde/command shapes with Task 1 wire fixtures and Task 2 controls.
 - [x] Check every task pair sharing storage migration dispatcher, AppState/commands, job interfaces, DTOs or frontend routing; record the table in this plan's SDD ledger. Current ten-task preflight covers 45 pairs and ten self-checks (2026-11-06); final runtime reconciliation remains separate.
-- [ ] Keep migration order 0003 intelligence → 0004 documents → 0005 summaries → 0006 research; confirm no unrelated task has consumed these version numbers.
+- [ ] Keep migration order 0003 intelligence → 0004 documents → 0005 knowledge bindings → 0006 summaries → 0007 research; confirm no unrelated task has consumed these version numbers.
 - [ ] Verify parser-worker OS limits/packaged executable invocation against platform process APIs before Task 4; preserve minimum-OS gates.
 - [ ] Confirm SrvDB dependency/API/dimension/ID semantics and config-profile snapshots survive actual integration tests.
 - [ ] Validate entire final-transcript coverage and local failed raw-response retention; no closed-session event dependency.
 - [ ] Check fixture-only code is excluded from production and credentials never appear in public DTOs/artifacts.
 - [ ] Verify legacy/new accounting diagnostic fixtures preserve rate units, finalized/unknown-tail flags and forward-compatible unknown variants without inferring acoustic timing.
 - [ ] Complete controller spec coverage and type/interface self-review before execution; this refinement report is not that review.
diff --git a/src-tauri/migrations/0005_knowledge_index.sql b/src-tauri/migrations/0005_knowledge_index.sql
new file mode 100644
index 0000000..f3d41f6
--- /dev/null
+++ b/src-tauri/migrations/0005_knowledge_index.sql
@@ -0,0 +1,20 @@
+-- A semantic binding is established only by a fenced writer publication.
+-- Legacy rows remain unbound, with no invented trusted generation.
+ALTER TABLE knowledge_index_generations ADD COLUMN semantic_key TEXT;
+ALTER TABLE knowledge_index_generations ADD COLUMN requested_dimension INTEGER;
+ALTER TABLE knowledge_index_generations ADD COLUMN updated_at_ms INTEGER;
+CREATE UNIQUE INDEX knowledge_binding_unspecified
+    ON knowledge_index_generations(semantic_key)
+    WHERE semantic_key IS NOT NULL AND requested_dimension IS NULL;
+CREATE UNIQUE INDEX knowledge_binding_explicit
+    ON knowledge_index_generations(semantic_key,requested_dimension)
+    WHERE requested_dimension IS NOT NULL;
+CREATE INDEX documents_status ON documents(status);
+CREATE INDEX document_chunks_ready ON document_chunks(index_state,embedding_identity);
+-- Compact per-row publication evidence; old rows deliberately stay untrusted.
+ALTER TABLE knowledge_index_work_items ADD COLUMN document_fence_json TEXT
+    CHECK (document_fence_json IS NULL OR (typeof(document_fence_json)='text' AND length(CAST(document_fence_json AS BLOB))<=1024));
+ALTER TABLE knowledge_index_work_items ADD COLUMN location_sha256 TEXT
+    CHECK (location_sha256 IS NULL OR (typeof(location_sha256)='text' AND length(location_sha256)=64 AND location_sha256 NOT GLOB '*[^0-9a-f]*'));
+ALTER TABLE knowledge_index_work_items ADD COLUMN previous_vector_id TEXT
+    CHECK (previous_vector_id IS NULL OR (typeof(previous_vector_id)='text' AND length(previous_vector_id) BETWEEN 1 AND 20 AND previous_vector_id NOT GLOB '*[^0-9]*' AND (length(previous_vector_id)=1 OR substr(previous_vector_id,1,1)!='0') AND (length(previous_vector_id)<20 OR previous_vector_id<='18446744073709551615')));
diff --git a/src-tauri/src/app/document_test_support.rs b/src-tauri/src/app/document_test_support.rs
index 5f9b934..d1a2d77 100644
--- a/src-tauri/src/app/document_test_support.rs
+++ b/src-tauri/src/app/document_test_support.rs
@@ -10,10 +10,30 @@ pub(crate) fn with_parser(
         path.into(),
         AppState::lock_data_dir(path).unwrap(),
         secrets.clone(),
         fixture.audio.clone(),
         Arc::new(ProviderFactory::new(secrets)),
         Arc::new(UnavailableDriverOperations),
         parser,
     )
     .unwrap()
 }
+
+pub(crate) fn with_knowledge_ports(
+    path: &Path,
+    parser: Arc<dyn crate::knowledge::parser::DocumentParser>,
+    embeddings: Arc<dyn crate::knowledge::embedding::Embeddings>,
+) -> AppState {
+    let secrets = Arc::new(SecretStore::for_tests(path).unwrap());
+    let fixture = crate::app::tests::fixture();
+    AppState::compose_with_embeddings(
+        path.into(),
+        AppState::lock_data_dir(path).unwrap(),
+        secrets.clone(),
+        fixture.audio.clone(),
+        Arc::new(ProviderFactory::new(secrets)),
+        Arc::new(UnavailableDriverOperations),
+        parser,
+        Some(embeddings),
+    )
+    .unwrap()
+}
diff --git a/src-tauri/src/app/intelligence.rs b/src-tauri/src/app/intelligence.rs
index 9ff3065..0143f0c 100644
--- a/src-tauri/src/app/intelligence.rs
+++ b/src-tauri/src/app/intelligence.rs
@@ -20,20 +20,33 @@ impl AppState {
             Err(failure) => {
                 let _ = self.probes.shutdown().await;
                 let _ = self.shutdown_index_after_startup_failure().await;
                 return Err(failure);
             }
         };
         if let Err(failure) = self.check_epoch(epoch) {
             let _ = self.shutdown_index_after_startup_failure().await;
             return Err(failure);
         }
+        let local = match self
+            .knowledge
+            .reopen_searches()
+            .and_then(|()| self.intelligence_lease())
+        {
+            Ok(lease) => self.knowledge.reconcile_owned(lease).await,
+            Err(failure) => Err(failure),
+        };
+        if let Err(failure) = local {
+            let _ = self.knowledge.shutdown_searches().await;
+            let _ = self.shutdown_index_after_startup_failure().await;
+            return Err(failure);
+        }
         let jobs = self.with_open_epoch(|| self.jobs.start_owned(self._process_lock.clone()));
         if jobs.is_err() {
             let _ = self.shutdown_index_after_startup_failure().await;
         }
         jobs
     }
     async fn shutdown_index_after_startup_failure(&self) -> AppResult<()> {
         let _direct_operations = self
             .intelligence_operations
             .clone()
@@ -58,20 +71,38 @@ impl AppState {
                 _process: self._process_lock.clone(),
                 _permit: permit,
             }) as OperationLease)
         })
     }
     pub fn get_managed_data_location(&self) -> ManagedDataLocation {
         ManagedDataLocation {
             data_dir: self.data_dir().to_path_buf(),
         }
     }
+    pub async fn search_knowledge(
+        &self,
+        request: crate::knowledge::types::KnowledgeSearch,
+    ) -> AppResult<crate::knowledge::types::KnowledgeResults> {
+        self.knowledge
+            .search_owned(request, self.intelligence_lease()?)
+            .await
+    }
+    pub async fn reindex_document(&self, id: Uuid) -> AppResult<JobView> {
+        self.knowledge
+            .reindex_owned(id, self.intelligence_lease()?)
+            .await
+    }
+    pub async fn delete_document(&self, id: Uuid) -> AppResult<JobView> {
+        self.knowledge
+            .delete_owned(id, self.intelligence_lease()?)
+            .await
+    }
     pub async fn list_model_profiles(&self) -> AppResult<Vec<ModelProfileView>> {
         self.service_config
             .scoped(self.intelligence_lease()?)
             .list()
             .await
     }
     pub async fn save_model_profile(
         &self,
         request: SaveModelProfile,
     ) -> AppResult<ModelProfileView> {
diff --git a/src-tauri/src/app/intelligence/tests.rs b/src-tauri/src/app/intelligence/tests.rs
index 739dc1c..df9d19e 100644
--- a/src-tauri/src/app/intelligence/tests.rs
+++ b/src-tauri/src/app/intelligence/tests.rs
@@ -1,12 +1,13 @@
 use super::*;
 mod child_operations;
+mod knowledge;
 use crate::intelligence::{
     jobs::{JobContext, JobHandler},
     test_support::payload,
 };
 use std::time::Duration;
 struct Wait;
 #[async_trait::async_trait]
 impl JobHandler for Wait {
     async fn run(
         &self,
@@ -43,29 +44,29 @@ impl JobHandler for BlockCleanup {
         Ok(())
     }
 }
 
 #[tokio::test]
 async fn intelligence_app_close_keeps_index_owned_until_job_cleanup_finishes() {
     let dir = tempfile::tempdir().unwrap();
     let app = AppState::new(dir.path().to_path_buf()).unwrap();
     let entered = Arc::new(tokio::sync::Notify::new());
     let release = Arc::new(tokio::sync::Notify::new());
-    app.jobs
-        .register(
-            JobKind::Reindex,
-            Arc::new(BlockCleanup {
-                entered: entered.clone(),
-                release: release.clone(),
-            }),
-        )
-        .unwrap();
+    crate::intelligence::jobs::test_support::replace_pre_start_handler(
+        &app.jobs,
+        JobKind::Reindex,
+        Arc::new(BlockCleanup {
+            entered: entered.clone(),
+            release: release.clone(),
+        }),
+    )
+    .unwrap();
     app.start_intelligence().await.unwrap();
     assert!(app.is_index_running_for_test());
     let close = {
         let app = app.clone();
         tokio::spawn(async move { app.close_all().await })
     };
     tokio::time::timeout(Duration::from_secs(1), entered.notified())
         .await
         .unwrap();
     assert!(
@@ -76,29 +77,29 @@ async fn intelligence_app_close_keeps_index_owned_until_job_cleanup_finishes() {
     close.await.unwrap().unwrap();
     assert!(!app.is_index_running_for_test());
 }
 
 #[tokio::test]
 async fn intelligence_app_close_timeout_keeps_index_for_retry_until_jobs_drain() {
     let dir = tempfile::tempdir().unwrap();
     let app = AppState::new(dir.path().to_path_buf()).unwrap();
     let entered = Arc::new(tokio::sync::Notify::new());
     let release = Arc::new(tokio::sync::Notify::new());
-    app.jobs
-        .register(
-            JobKind::Reindex,
-            Arc::new(BlockCleanup {
-                entered: entered.clone(),
-                release: release.clone(),
-            }),
-        )
-        .unwrap();
+    crate::intelligence::jobs::test_support::replace_pre_start_handler(
+        &app.jobs,
+        JobKind::Reindex,
+        Arc::new(BlockCleanup {
+            entered: entered.clone(),
+            release: release.clone(),
+        }),
+    )
+    .unwrap();
     app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(30));
     app.start_intelligence().await.unwrap();
     let first_close = tokio::time::timeout(Duration::from_secs(1), app.close_all())
         .await
         .unwrap();
     assert!(first_close.is_err());
     tokio::time::timeout(Duration::from_secs(1), entered.notified())
         .await
         .unwrap();
     assert!(
@@ -230,21 +231,26 @@ fn intelligence_app_inert_construction_projects_empty_library_directory_and_reco
     assert_eq!(
         app.store.get_job(job.id).unwrap().unwrap().status,
         JobStatus::Interrupted
     );
     assert!(!dir.path().join("secrets.dek.lock").exists());
 }
 #[tokio::test]
 async fn intelligence_app_close_cancels_jobs_and_restarts_only_on_explicit_host_start() {
     let dir = tempfile::tempdir().unwrap();
     let app = AppState::new(dir.path().to_path_buf()).unwrap();
-    app.jobs.register(JobKind::Reindex, Arc::new(Wait)).unwrap();
+    crate::intelligence::jobs::test_support::replace_pre_start_handler(
+        &app.jobs,
+        JobKind::Reindex,
+        Arc::new(Wait),
+    )
+    .unwrap();
     app.start_intelligence().await.unwrap();
     let id = Uuid::new_v4();
     let job = app
         .jobs
         .enqueue(
             JobKind::Reindex,
             None,
             Some(id),
             JobAdmission::Manual {
                 exclusion_key: format!("document:{id}"),
@@ -352,21 +358,26 @@ async fn intelligence_app_entered_clear_retains_process_and_close_barrier_after_
         })
         .await
         .unwrap();
         assert_eq!(h.audio.open_calls(), 0);
     }
 }
 #[tokio::test]
 async fn intelligence_app_dropping_external_handles_releases_process_after_worker_cleanup() {
     let dir = tempfile::tempdir().unwrap();
     let app = AppState::new(dir.path().to_path_buf()).unwrap();
-    app.jobs.register(JobKind::Reindex, Arc::new(Wait)).unwrap();
+    crate::intelligence::jobs::test_support::replace_pre_start_handler(
+        &app.jobs,
+        JobKind::Reindex,
+        Arc::new(Wait),
+    )
+    .unwrap();
     app.start_intelligence().await.unwrap();
     let id = Uuid::new_v4();
     app.jobs
         .enqueue(
             JobKind::Reindex,
             None,
             Some(id),
             JobAdmission::Manual {
                 exclusion_key: format!("document:{id}"),
             },
diff --git a/src-tauri/src/app/intelligence/tests/child_operations.rs b/src-tauri/src/app/intelligence/tests/child_operations.rs
index c5d7ba8..960d33b 100644
--- a/src-tauri/src/app/intelligence/tests/child_operations.rs
+++ b/src-tauri/src/app/intelligence/tests/child_operations.rs
@@ -46,23 +46,22 @@ impl JobHandler for EnteredConfig {
                 Ok(())
             })
             .await
         });
         if self.abort_observer {
             self.abort.notified().await;
             observer.abort();
             assert!(observer.await.unwrap_err().is_cancelled());
         } else {
             let result = observer.await.unwrap();
-            assert!(
-                matches!(result, Err(AppError::Configuration(message)) if message.contains("timed out"))
-            );
+            assert!(matches!(result,
+ Err(AppError::Configuration(message)) if message.contains("timed out")));
         }
         self.observed.notify_one();
         Err(IntelligenceErrorCode::Configuration)
     }
 }
 async fn entered(
     abort_observer: bool,
 ) -> (tempfile::TempDir, AppState, Arc<EnteredConfig>, Release) {
     let dir = tempfile::tempdir().unwrap();
     let fixture = crate::app::tests::fixture();
@@ -80,23 +79,26 @@ async fn entered(
     let (entered, entry) = oneshot::channel();
     let (release, wait) = blocking::channel();
     let handler = Arc::new(EnteredConfig {
         io,
         channels: StdMutex::new(Some((entered, wait))),
         abort_observer,
         abort: Notify::new(),
         observed: Notify::new(),
         completed: Arc::new(AtomicBool::new(false)),
     });
-    app.jobs
-        .register(JobKind::Reindex, handler.clone())
-        .unwrap();
+    crate::intelligence::jobs::test_support::replace_pre_start_handler(
+        &app.jobs,
+        JobKind::Reindex,
+        handler.clone(),
+    )
+    .unwrap();
     app.start_intelligence().await.unwrap();
     let id = Uuid::new_v4();
     let job = app
         .jobs
         .enqueue(
             JobKind::Reindex,
             None,
             Some(id),
             JobAdmission::Manual {
                 exclusion_key: format!("document:{id}"),
diff --git a/src-tauri/src/app/intelligence/tests/knowledge.rs b/src-tauri/src/app/intelligence/tests/knowledge.rs
new file mode 100644
index 0000000..bc136f5
--- /dev/null
+++ b/src-tauri/src/app/intelligence/tests/knowledge.rs
@@ -0,0 +1,176 @@
+use super::*;
+use crate::knowledge::{
+    embedding::{EmbeddingExecution, Embeddings},
+    parser::DocumentParser,
+    types::*,
+};
+use std::sync::atomic::{AtomicBool, Ordering};
+struct Parser;
+#[async_trait::async_trait]
+impl DocumentParser for Parser {
+    async fn parse(
+        &self,
+        path: std::path::PathBuf,
+        _: DocumentKind,
+        _: watch::Receiver<bool>,
+    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
+        let text = std::fs::read_to_string(path).unwrap();
+        Ok(vec![TextBlock {
+            location: TextLocation {
+                page: None,
+                paragraph: 1,
+                heading: None,
+                start_char: 0,
+                end_char: text.chars().count() as u32,
+            },
+            text,
+        }])
+    }
+}
+#[derive(Default)]
+struct EnteredEmbedding {
+    block: AtomicBool,
+    entered: tokio::sync::Notify,
+    release: tokio::sync::Notify,
+}
+#[async_trait::async_trait]
+impl Embeddings for EnteredEmbedding {
+    async fn embed(
+        &self,
+        _: &ModelProfile,
+        inputs: &[String],
+        _: EmbeddingExecution,
+    ) -> AppResult<Vec<Vec<f32>>> {
+        if self.block.load(Ordering::SeqCst) {
+            self.entered.notify_one();
+            self.release.notified().await;
+        }
+        Ok(inputs
+            .iter()
+            .map(|_| {
+                let mut v = vec![0.; 128];
+                v[0] = 1.;
+                v
+            })
+            .collect())
+    }
+}
+async fn entered() -> (
+    tempfile::TempDir,
+    AppState,
+    Arc<EnteredEmbedding>,
+    tokio::task::JoinHandle<AppResult<KnowledgeResults>>,
+) {
+    let dir = tempfile::tempdir().unwrap();
+    let embeddings = Arc::new(EnteredEmbedding::default());
+    let app = crate::app::document_test_support::with_knowledge_ports(
+        dir.path(),
+        Arc::new(Parser),
+        embeddings.clone(),
+    );
+    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(40));
+    let p = ModelProfile {
+        kind: ServiceKind::Embedding,
+        max_context_tokens: None,
+        max_output_tokens: None,
+        dimensions: Some(128),
+        ..crate::intelligence::test_support::test_llm_profile()
+    };
+    app.save_model_profile(SaveModelProfile {
+        profile: p.clone(),
+        credential: CredentialChange::Replace {
+            api_key: "synthetic".into(),
+        },
+    })
+    .await
+    .unwrap();
+    let mut settings = app.get_intelligence_settings().await.unwrap();
+    settings.embedding_profile_id = Some(p.id);
+    app.save_intelligence_settings(settings).await.unwrap();
+    app.start_intelligence().await.unwrap();
+    let source = dir.path().join("pump.txt");
+    std::fs::write(&source, "rated voltage24V").unwrap();
+    let job = app.import_document(source).await.unwrap();
+    let document = job.object_id.unwrap();
+    tokio::time::timeout(Duration::from_secs(3), async {
+        loop {
+            if app.get_document(document).unwrap().unwrap().summary.status == DocumentStatus::Ready
+            {
+                break;
+            }
+            tokio::task::yield_now().await;
+        }
+    })
+    .await
+    .unwrap();
+    embeddings.block.store(true, Ordering::SeqCst);
+    let observer = {
+        let app = app.clone();
+        tokio::spawn(async move {
+            app.search_knowledge(KnowledgeSearch {
+                query: "voltage".into(),
+                selected_segment_ids: vec![],
+                session_id: None,
+                limit: 5,
+            })
+            .await
+        })
+    };
+    embeddings.entered.notified().await;
+    (dir, app, embeddings, observer)
+}
+#[tokio::test]
+async fn knowledge_app_close_and_rearm_wait_for_entered_search_after_observer_loss() {
+    let (_dir, app, embeddings, observer) = entered().await;
+    observer.abort();
+    assert!(observer.await.unwrap_err().is_cancelled());
+    let close_pending = app.close_all().await.is_err();
+    let rearm_denied = app.reopen_window().await.is_err();
+    embeddings.block.store(false, Ordering::SeqCst);
+    embeddings.release.notify_waiters();
+    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_secs(2));
+    tokio::time::timeout(Duration::from_secs(3), async {
+        while app.close_all().await.is_err() {
+            tokio::task::yield_now().await;
+        }
+    })
+    .await
+    .unwrap();
+    assert!(close_pending);
+    assert!(rearm_denied);
+    app.reopen_window().await.unwrap();
+    app.close_all().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_app_process_lock_outlives_search_observer_until_entered_work_finishes() {
+    use fs2::FileExt;
+    let (dir, app, embeddings, observer) = entered().await;
+    let process = Arc::downgrade(&app._process_lock);
+    observer.abort();
+    assert!(observer.await.unwrap_err().is_cancelled());
+    drop(app);
+    let lock = std::fs::OpenOptions::new()
+        .read(true)
+        .write(true)
+        .open(dir.path().join("app.lock"))
+        .unwrap();
+    let held = lock.try_lock_exclusive().is_err();
+    if !held {
+        lock.unlock().unwrap();
+    }
+    embeddings.block.store(false, Ordering::SeqCst);
+    embeddings.release.notify_waiters();
+    tokio::time::timeout(Duration::from_secs(3), async {
+        while process.upgrade().is_some() {
+            tokio::task::yield_now().await;
+        }
+    })
+    .await
+    .unwrap();
+    lock.try_lock_exclusive().unwrap();
+    lock.unlock().unwrap();
+    assert!(
+        held,
+        "entered search must own physical app lock after observer loss"
+    );
+}
diff --git a/src-tauri/src/app/lifecycle.rs b/src-tauri/src/app/lifecycle.rs
index 493c789..2a3b390 100644
--- a/src-tauri/src/app/lifecycle.rs
+++ b/src-tauri/src/app/lifecycle.rs
@@ -143,20 +143,21 @@ impl AppState {
         let _startup = self.intelligence_startup.lock().await;
         let (jobs, probes, subscriptions) = tokio::join!(
             self.jobs.shutdown(),
             self.probes.shutdown(),
             self.intelligence_subscriptions.close_all()
         );
         // Job cleanup may still need already-issued index ownership. The
         // independent index sender remains alive until the scheduler confirms
         // that workers and child operations have actually drained.
         jobs?;
+        self.knowledge.shutdown_searches().await?;
         let _direct_operations = self
             .intelligence_operations
             .clone()
             .acquire_many_owned(16)
             .await
             .map_err(|_| closing_error())?;
         self.index_owner.shutdown().await?;
         probes?;
         subscriptions
     }
diff --git a/src-tauri/src/app/mod.rs b/src-tauri/src/app/mod.rs
index 54664f5..e92dfdd 100644
--- a/src-tauri/src/app/mod.rs
+++ b/src-tauri/src/app/mod.rs
@@ -74,20 +74,21 @@ pub struct AppState {
     close_epoch: watch::Sender<u64>,
     service_config: Arc<crate::intelligence::config::ServiceConfigStore>,
     jobs: Arc<crate::intelligence::jobs::JobScheduler>,
     json_http: Arc<dyn crate::intelligence::http::JsonHttp>,
     probes: Arc<crate::intelligence::probes::ProbeOwner>,
     intelligence_subscriptions: crate::intelligence::subscriptions::IntelligenceSubscriptions,
     intelligence_operations: Arc<Semaphore>,
     intelligence_startup: Arc<Mutex<()>>,
     index_owner: crate::knowledge::index::IndexOwner,
     documents: Arc<crate::knowledge::documents::DocumentService>,
+    knowledge: Arc<crate::knowledge::service::KnowledgeService>,
 }
 
 impl AppState {
     pub fn new(data_dir: PathBuf) -> AppResult<Self> {
         Self::with_document_parser(
             data_dir,
             Arc::new(crate::knowledge::parser::ProcessDocumentParser::new()?),
         )
     }
     /// Backend composition injection, retaining the production process-lock and
@@ -186,20 +187,35 @@ impl AppState {
         Ok(lock)
     }
     fn compose(
         data_dir: PathBuf,
         lock: File,
         secrets: Arc<SecretStore>,
         audio: Arc<dyn AudioBackend>,
         factory: Arc<ProviderFactory>,
         driver: Arc<dyn DriverOperations>,
         parser: Arc<dyn crate::knowledge::parser::DocumentParser>,
+    ) -> AppResult<Self> {
+        Self::compose_with_embeddings(
+            data_dir, lock, secrets, audio, factory, driver, parser, None,
+        )
+    }
+    #[allow(clippy::too_many_arguments)]
+    fn compose_with_embeddings(
+        data_dir: PathBuf,
+        lock: File,
+        secrets: Arc<SecretStore>,
+        audio: Arc<dyn AudioBackend>,
+        factory: Arc<ProviderFactory>,
+        driver: Arc<dyn DriverOperations>,
+        parser: Arc<dyn crate::knowledge::parser::DocumentParser>,
+        embeddings: Option<Arc<dyn crate::knowledge::embedding::Embeddings>>,
     ) -> AppResult<Self> {
         let store = Arc::new(LocalStore::open(&data_dir)?);
         let now = SystemTime::now()
             .duration_since(UNIX_EPOCH)
             .unwrap_or_default()
             .as_secs() as i64;
         store.interrupt_incomplete_sessions(now)?;
         let event_bus = EventBus::new();
         let config_io = ConfigIo::new(ConfigStore::new(&data_dir, secrets)?);
         let service_config = Arc::new(crate::intelligence::config::ServiceConfigStore::new(
@@ -209,23 +225,39 @@ impl AppState {
         let jobs = Arc::new(crate::intelligence::jobs::JobScheduler::new(store.clone()));
         let index_owner =
             crate::knowledge::index::IndexOwner::new(data_dir.join("indexes"), store.clone());
         let documents = Arc::new(crate::knowledge::documents::DocumentService::new(
             store.clone(),
             service_config.clone(),
             jobs.clone(),
             parser,
             data_dir.clone(),
         ));
-        register_intelligence_handlers(&documents)?;
-        jobs.reconcile()?;
         let json_http = Arc::new(crate::intelligence::http::ReqwestJsonHttp::new()?);
+        let embeddings = embeddings.unwrap_or_else(|| {
+            Arc::new(
+                crate::knowledge::embedding::OpenAiCompatibleEmbeddings::new(
+                    service_config.clone(),
+                    json_http.clone(),
+                ),
+            )
+        });
+        let knowledge = Arc::new(crate::knowledge::service::KnowledgeService::new(
+            store.clone(),
+            service_config.clone(),
+            jobs.clone(),
+            embeddings,
+            index_owner.clone(),
+            data_dir.clone(),
+        ));
+        register_intelligence_handlers(&documents, &knowledge)?;
+        jobs.reconcile()?;
         let runtime = Arc::new(TranslationRuntime::new(
             factory.clone(),
             audio.clone(),
             store.clone(),
             event_bus.clone(),
         ));
         let orchestrator =
             SessionOrchestrator::with_runtime(store.clone(), event_bus.clone(), runtime.clone());
         let (close_epoch, _) = watch::channel(0);
         Ok(Self {
@@ -249,20 +281,21 @@ impl AppState {
             service_config,
             jobs,
             json_http,
             probes: Arc::new(crate::intelligence::probes::ProbeOwner::new()),
             intelligence_subscriptions:
                 crate::intelligence::subscriptions::IntelligenceSubscriptions::new(),
             intelligence_operations: Arc::new(Semaphore::new(16)),
             intelligence_startup: Arc::new(Mutex::new(())),
             index_owner,
             documents,
+            knowledge,
         })
     }
     pub fn data_dir(&self) -> &Path {
         self.config_io.data_dir()
     }
     pub fn event_bus(&self) -> &EventBus {
         &self.event_bus
     }
     pub fn store(&self) -> &LocalStore {
         &self.store
@@ -306,32 +339,34 @@ impl AppState {
         })
     }
     pub async fn create_session(&self, launch: SessionLaunch) -> AppResult<SessionRecord> {
         let mut closed = self.close_epoch.subscribe();
         let mut driver = self.driver_active.subscribe();
         if *driver.borrow_and_update() {
             return Err(crate::audio::platform::driver::busy_error());
         }
         let epoch = self.admitted_epoch()?;
         tokio::select! {
-            biased;
-            _ = closed.changed() => Err(lifecycle::closing_error()),
-            _ = driver.changed() => Err(crate::audio::platform::driver::busy_error()),
-            result = async {
-                let _guard = self.admission.lock().await;
-                self.check_epoch(epoch)?;
-                let id = launch.provider_config_id;
-                let saved = self.load_launch_provider_config(id).await?;
-                self.check_epoch(epoch)?;
-                self.orchestrator.start(launch, saved).await
-            } => result,
-        }
+                   biased;
+                   _ = closed.changed() => Err(lifecycle::closing_error()),
+                   _ = driver.changed() => Err(crate::audio::platform::driver::busy_error()),
+                   result = async {
+                       let _guard = self.admission.lock().await;
+                       self.check_epoch(epoch)?;
+                       let id = launch.provider_config_id;
+                       let saved = self.load_launch_provider_config(id).await?;
+                       self.check_epoch(epoch)?;
+                       self.orchestrator.start(launch,
+        saved).await
+                   }
+        => result,
+               }
     }
     pub async fn stop_session(&self, session_id: Uuid) -> AppResult<SessionRecord> {
         self.orchestrator.stop_session(session_id).await
     }
     pub async fn resume_session_direction(
         &self,
         session_id: Uuid,
         direction: SessionDirection,
     ) -> AppResult<()> {
         self.admitted_epoch()?;
@@ -406,13 +441,16 @@ pub(crate) mod driver_test_support {
         app.driver_observation_timeout = std::time::Duration::from_millis(30);
         app.close_state.lock().unwrap().observation_timeout =
             Some(std::time::Duration::from_millis(30));
     }
 }
 
 /// Tasks 4/6/7 construct handlers from store/config/ports here before the first
 /// start. Handlers never retain AppState or Arc<JobScheduler>.
 fn register_intelligence_handlers(
     documents: &crate::knowledge::documents::DocumentService,
+    knowledge: &crate::knowledge::service::KnowledgeService,
 ) -> AppResult<()> {
-    documents.register_import_handler()
+    documents.register_import_handler()?;
+    knowledge.register_handlers()?;
+    knowledge.register_continuation()
 }
diff --git a/src-tauri/src/commands/knowledge.rs b/src-tauri/src/commands/knowledge.rs
index e5ad00b..2413f56 100644
--- a/src-tauri/src/commands/knowledge.rs
+++ b/src-tauri/src/commands/knowledge.rs
@@ -1,14 +1,14 @@
 use crate::{
     app::AppState,
     intelligence::types::JobView,
-    knowledge::types::{DocumentDetail, DocumentPage},
+    knowledge::types::{DocumentDetail, DocumentPage, KnowledgeResults, KnowledgeSearch},
 };
 use std::path::PathBuf;
 use tauri::State;
 use uuid::Uuid;
 #[tauri::command]
 pub async fn import_document(state: State<'_, AppState>, path: PathBuf) -> Result<JobView, String> {
     state
         .import_document(path)
         .await
         .map_err(super::command_error)
@@ -36,27 +36,63 @@ pub fn list_documents(
 }
 #[tauri::command]
 pub fn get_document(
     state: State<'_, AppState>,
     document_id: Uuid,
 ) -> Result<Option<DocumentDetail>, String> {
     state
         .get_document(document_id)
         .map_err(super::command_error)
 }
+#[tauri::command]
+pub async fn search_knowledge(
+    state: State<'_, AppState>,
+    request: KnowledgeSearch,
+) -> Result<KnowledgeResults, String> {
+    state
+        .search_knowledge(request)
+        .await
+        .map_err(super::command_error)
+}
+#[tauri::command]
+pub async fn reindex_document(
+    state: State<'_, AppState>,
+    document_id: Uuid,
+) -> Result<JobView, String> {
+    state
+        .reindex_document(document_id)
+        .await
+        .map_err(super::command_error)
+}
+#[tauri::command]
+pub async fn delete_document(
+    state: State<'_, AppState>,
+    document_id: Uuid,
+) -> Result<JobView, String> {
+    state
+        .delete_document(document_id)
+        .await
+        .map_err(super::command_error)
+}
 pub(crate) const COMMANDS: &[&str] = &[
     "import_document",
     "replace_document",
     "list_documents",
     "get_document",
+    "search_knowledge",
+    "reindex_document",
+    "delete_document",
 ];
 pub(crate) fn handle<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
     let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
         import_document,
         replace_document,
         list_documents,
-        get_document
+        get_document,
+        search_knowledge,
+        reindex_document,
+        delete_document
     ];
     handler(invoke)
 }
 #[cfg(test)]
 mod tests;
diff --git a/src-tauri/src/commands/knowledge/tests.rs b/src-tauri/src/commands/knowledge/tests.rs
index 3e57ba9..2631028 100644
--- a/src-tauri/src/commands/knowledge/tests.rs
+++ b/src-tauri/src/commands/knowledge/tests.rs
@@ -31,64 +31,124 @@ fn documents_commands_real_dispatch_preserves_legacy_dtos_and_exact_camelcase_ar
     )
     .unwrap();
     let state = AppState::with_dependencies(
         dir.path().into(),
         secrets.clone(),
         fixture.audio.clone(),
         Arc::new(crate::providers::factory::ProviderFactory::new(secrets)),
     )
     .unwrap();
     let id = Uuid::new_v4();
-    state.store().transaction(|tx|{tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'legacy-hash','legacy-reference',NULL,37)",[id.to_string()])?;Ok(())}).unwrap();
+    state
+        .store()
+        .transaction(|tx| {
+            tx.execute(
+                "INSERT INTO documents (id,sha256,uri,title,\
+                    created_at) VALUES (?1,'legacy-hash',\
+                    'legacy-reference',NULL,37)",
+                [id.to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
     let app = tauri::test::mock_builder()
         .manage(state)
         .invoke_handler(handle)
         .build(tauri::test::mock_context(tauri::test::noop_assets()))
         .unwrap();
     let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
         .build()
         .unwrap();
     let call = |cmd: &str, args: Value| {
         tauri::test::get_ipc_response(&window, request(cmd, args))
             .map(|v| v.deserialize::<Value>().unwrap())
     };
-    let page = call("list_documents", json!({"after":null,"limit":64})).unwrap();
+    let page = call(
+        "list_documents",
+        json!({
+        "after":null,
+        "limit":64}
+        ),
+    )
+    .unwrap();
     assert_eq!(
         page["rows"].as_array().unwrap().len(),
         1,
         "legacy row must be projected by actual command"
     );
-    let detail = call("get_document", json!({"documentId":id})).unwrap();
+    let detail = call(
+        "get_document",
+        json!({
+        "documentId":id}
+        ),
+    )
+    .unwrap();
     assert_eq!(detail["legacy_uri"], "legacy-reference");
     assert_eq!(detail["summary"]["title"], Value::Null);
     assert_eq!(detail["summary"]["updated_at_ms"], Value::Null);
     assert_eq!(detail["summary"]["origin"], "legacy");
     assert_eq!(detail["original_path"], Value::Null);
     assert_eq!(detail["managed_path"], Value::Null);
     assert_eq!(
-        call("get_document", json!({"documentId":Uuid::new_v4()})).unwrap(),
+        call(
+            "get_document",
+            json!({
+            "documentId":Uuid::new_v4()}
+            )
+        )
+        .unwrap(),
         Value::Null
     );
-    assert!(call("get_document", json!({"document_id":id})).is_err());
-    assert!(call("get_document", json!({"documentId":"invalid"})).is_err());
-    assert!(call("list_documents", json!({"after":null,"limit":0})).is_err());
+    assert!(
+        call(
+            "get_document",
+            json!({
+            "document_id":id}
+            )
+        )
+        .is_err()
+    );
+    assert!(
+        call(
+            "get_document",
+            json!({
+            "documentId":"invalid"}
+            )
+        )
+        .is_err()
+    );
+    assert!(
+        call(
+            "list_documents",
+            json!({
+            "after":null,
+            "limit":0}
+            )
+        )
+        .is_err()
+    );
     assert!(
         call(
             "import_document",
-            json!({"path":dir.path().join("unread.txt")})
+            json!({
+            "path":dir.path().join("unread.txt")}
+            )
         )
         .is_err()
     );
     assert!(
         call(
             "replace_document",
-            json!({"documentId":id,"path":dir.path().join("unread.txt")})
+            json!({
+            "documentId":id,
+            "path":dir.path().join("unread.txt")}
+            )
         )
         .is_err()
     );
     assert!(!dir.path().join("knowledge").exists());
 }
 
 #[test]
 fn documents_commands_real_import_replace_and_duplicate_use_registered_document_service() {
     use crate::intelligence::types::{IntelligenceErrorCode as Code, JobStatus};
     use crate::knowledge::{
@@ -144,54 +204,401 @@ fn documents_commands_real_import_replace_and_duplicate_use_registered_document_
                     }
                     tokio::task::yield_now().await;
                 }
             })
             .await
             .unwrap();
         })
     };
     let path = dir.path().join("source.txt");
     std::fs::write(&path, "first canonical text").unwrap();
-    let job = call("import_document", json!({"path":path})).unwrap();
+    let job = call(
+        "import_document",
+        json!({
+        "path":path}
+        ),
+    )
+    .unwrap();
     let job_id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
     let document = Uuid::parse_str(job["object_id"].as_str().unwrap()).unwrap();
     wait(job_id);
-    let detail = call("get_document", json!({"documentId":document})).unwrap();
+    let detail = call(
+        "get_document",
+        json!({
+        "documentId":document}
+        ),
+    )
+    .unwrap();
     assert_eq!(detail["summary"]["status"], "parsed");
     assert_eq!(detail["summary"]["version"], 1);
     assert_eq!(detail["summary"]["origin"], "native");
     assert!(detail["managed_path"].is_string());
     assert_eq!(detail["legacy_uri"], Value::Null);
     let replacement = dir.path().join("replacement.txt");
     std::fs::write(&replacement, "replacement canonical text").unwrap();
     let next = call(
         "replace_document",
-        json!({"documentId":document,"path":replacement}),
+        json!({
+        "documentId":document,
+        "path":replacement}
+        ),
     )
     .unwrap();
     wait(Uuid::parse_str(next["id"].as_str().unwrap()).unwrap());
     assert_ne!(next["id"], job["id"]);
     assert_eq!(next["object_id"], job["object_id"]);
     assert_eq!(
         call(
             "replace_document",
-            json!({"documentId":document,"path":replacement})
+            json!({
+            "documentId":document,
+            "path":replacement}
+            )
         )
         .unwrap()["id"],
         next["id"]
     );
     assert_eq!(
-        call("import_document", json!({"path":replacement})).unwrap()["id"],
+        call(
+            "import_document",
+            json!({
+            "path":replacement}
+            )
+        )
+        .unwrap()["id"],
         next["id"]
     );
-    let detail = call("get_document", json!({"documentId":document})).unwrap();
+    let detail = call(
+        "get_document",
+        json!({
+        "documentId":document}
+        ),
+    )
+    .unwrap();
     assert_eq!(detail["summary"]["version"], 2);
     assert_eq!(detail["summary"]["chunk_count"], 1);
     assert_eq!(
-        call("list_documents", json!({"after":null,"limit":64})).unwrap()["rows"]
+        call(
+            "list_documents",
+            json!({
+            "after":null,
+            "limit":64}
+            )
+        )
+        .unwrap()["rows"]
             .as_array()
             .unwrap()
             .len(),
         1
     );
     tauri::async_runtime::block_on(observer.close_all()).unwrap();
 }
+#[test]
+fn knowledge_commands_dispatch_empty_search_and_validate_exact_boundaries() {
+    let dir = tempfile::tempdir().unwrap();
+    let state = crate::app::document_test_support::with_parser(dir.path(), Arc::new(CommandParser));
+    let observer = state.clone();
+    let app = tauri::test::mock_builder()
+        .manage(state)
+        .invoke_handler(handle)
+        .build(tauri::test::mock_context(tauri::test::noop_assets()))
+        .unwrap();
+    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
+        .build()
+        .unwrap();
+    let call = |cmd: &str, args: Value| {
+        tauri::test::get_ipc_response(&window, request(cmd, args))
+            .map(|v| v.deserialize::<Value>().unwrap())
+    };
+    let empty = call(
+        "search_knowledge",
+        json!({
+        "request":{
+        "query":"voltage",
+        "selected_segment_ids":[],
+        "session_id":null,
+        "limit":5}
+        }
+        ),
+    )
+    .unwrap();
+    assert_eq!(
+        empty,
+        json!({
+        "hits":[],
+        "identity":null,
+        "unavailable_documents":0}
+        )
+    );
+    assert!(
+        call(
+            "search_knowledge",
+            json!({
+            "request":{
+            "query":"",
+            "selected_segment_ids":[],
+            "session_id":null,
+            "limit":5}
+            }
+            )
+        )
+        .is_err()
+    );
+    assert!(
+        call(
+            "search_knowledge",
+            json!({
+            "request":{
+            "query":"voltage",
+            "selected_segment_ids":[],
+            "session_id":null,
+            "limit":21}
+            }
+            )
+        )
+        .is_err()
+    );
+    assert!(
+        call(
+            "reindex_document",
+            json!({
+            "documentId":Uuid::new_v4()}
+            )
+        )
+        .is_err()
+    );
+    assert!(
+        call(
+            "delete_document",
+            json!({
+            "document_id":Uuid::new_v4()}
+            )
+        )
+        .is_err()
+    );
+    tauri::async_runtime::block_on(observer.close_all()).unwrap();
+}
+struct CommandParser;
+#[async_trait::async_trait]
+impl crate::knowledge::parser::DocumentParser for CommandParser {
+    async fn parse(
+        &self,
+        path: PathBuf,
+        _: crate::knowledge::types::DocumentKind,
+        _: tokio::sync::watch::Receiver<bool>,
+    ) -> Result<
+        Vec<crate::knowledge::types::TextBlock>,
+        crate::intelligence::types::IntelligenceErrorCode,
+    > {
+        let text = std::fs::read_to_string(path).unwrap();
+        let end = text.chars().count() as u32;
+        Ok(vec![crate::knowledge::types::TextBlock {
+            text,
+            location: crate::knowledge::types::TextLocation {
+                page: None,
+                paragraph: 1,
+                heading: None,
+                start_char: 0,
+                end_char: end,
+            },
+        }])
+    }
+}
+#[derive(Default)]
+struct CommandAxes(std::sync::Mutex<Vec<Vec<String>>>);
+#[async_trait::async_trait]
+impl crate::knowledge::embedding::Embeddings for CommandAxes {
+    async fn embed(
+        &self,
+        _: &crate::intelligence::types::ModelProfile,
+        inputs: &[String],
+        _: crate::knowledge::embedding::EmbeddingExecution,
+    ) -> crate::error::AppResult<Vec<Vec<f32>>> {
+        self.0.lock().unwrap().push(inputs.to_vec());
+        Ok(inputs
+            .iter()
+            .map(|_| {
+                let mut vector = vec![0.; 128];
+                vector[0] = 1.;
+                vector
+            })
+            .collect())
+    }
+}
+#[test]
+fn knowledge_commands_selected_partial_multi_row_and_foreign_session_reach_owned_service() {
+    use crate::intelligence::{test_support::test_llm_profile, types::*};
+    let dir = tempfile::tempdir().unwrap();
+    let embeddings = Arc::new(CommandAxes::default());
+    let state = crate::app::document_test_support::with_knowledge_ports(
+        dir.path(),
+        Arc::new(CommandParser),
+        embeddings.clone(),
+    );
+    let observer = state.clone();
+    tauri::async_runtime::block_on(async {
+        let profile = ModelProfile {
+            kind: ServiceKind::Embedding,
+            max_context_tokens: None,
+            max_output_tokens: None,
+            dimensions: Some(128),
+            ..test_llm_profile()
+        };
+        state
+            .save_model_profile(SaveModelProfile {
+                profile: profile.clone(),
+                credential: crate::app::CredentialChange::Replace {
+                    api_key: "synthetic".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let mut settings = state.get_intelligence_settings().await.unwrap();
+        settings.embedding_profile_id = Some(profile.id);
+        state.save_intelligence_settings(settings).await.unwrap();
+        state.start_intelligence().await.unwrap();
+    });
+    let app = tauri::test::mock_builder()
+        .manage(state)
+        .invoke_handler(handle)
+        .build(tauri::test::mock_context(tauri::test::noop_assets()))
+        .unwrap();
+    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
+        .build()
+        .unwrap();
+    let call = |cmd: &str, args: Value| {
+        tauri::test::get_ipc_response(&window, request(cmd, args))
+            .map(|v| v.deserialize::<Value>().unwrap())
+    };
+    let path = dir.path().join("pump.txt");
+    std::fs::write(&path, "rated voltage 24V").unwrap();
+    let job = call(
+        "import_document",
+        json!({
+        "path":path}
+        ),
+    )
+    .unwrap();
+    let document = Uuid::parse_str(job["object_id"].as_str().unwrap()).unwrap();
+    tauri::async_runtime::block_on(async {
+        tokio::time::timeout(std::time::Duration::from_secs(5), async {
+            loop {
+                if observer
+                    .get_document(document)
+                    .unwrap()
+                    .unwrap()
+                    .summary
+                    .status
+                    == crate::knowledge::types::DocumentStatus::Ready
+                {
+                    break;
+                }
+                tokio::task::yield_now().await;
+            }
+        })
+        .await
+        .unwrap();
+    });
+    let session = Uuid::new_v4();
+    let foreign = Uuid::new_v4();
+    for id in [session, foreign] {
+        observer
+            .store()
+            .create_session(&crate::storage::StoredSession {
+                id,
+                started_at: 1,
+                mode: "live".into(),
+                direction: "source_to_target".into(),
+                source_language: "en".into(),
+                target_language: "zh".into(),
+            })
+            .unwrap();
+    }
+    let mut segments = Vec::new();
+    for (sequence, text) in [
+        (
+            0,
+            "A longer row containing few selected words and extra context",
+        ),
+        (1, "A second longer row with additional words"),
+    ] {
+        let id = Uuid::new_v4();
+        observer
+            .store()
+            .append_final_segment(&crate::storage::StoredTranscriptSegment {
+                id,
+                session_id: session,
+                direction: "source_to_target".into(),
+                sequence,
+                occurred_at: 1,
+                language: "en".into(),
+                text: text.into(),
+                is_final: true,
+                details: None,
+                occurred_at_ms: None,
+            })
+            .unwrap();
+        segments.push(id);
+    }
+    for (query, ids) in [
+        ("few selected words", vec![segments[0]]),
+        ("extra context\nA second", segments.clone()),
+    ] {
+        let result = call(
+            "search_knowledge",
+            json!({
+            "request":{
+            "query":query,
+            "selected_segment_ids":ids,
+            "session_id":session,
+            "limit":5}
+            }
+            ),
+        )
+        .unwrap();
+        assert_eq!(result["hits"][0]["document_id"], document.to_string());
+        assert_eq!(embeddings.0.lock().unwrap().last().unwrap(), &[query]);
+    }
+    let calls = embeddings.0.lock().unwrap().len();
+    assert!(
+        call(
+            "search_knowledge",
+            json!({
+            "request":{
+            "query":"few words",
+            "selected_segment_ids":segments,
+            "session_id":foreign,
+            "limit":5}
+            }
+            )
+        )
+        .is_err()
+    );
+    assert_eq!(embeddings.0.lock().unwrap().len(), calls);
+    let reindex = call(
+        "reindex_document",
+        json!({
+        "documentId":document}
+        ),
+    )
+    .unwrap();
+    assert_eq!(reindex["kind"], "reindex");
+    tauri::async_runtime::block_on(async {
+        let id = Uuid::parse_str(reindex["id"].as_str().unwrap()).unwrap();
+        tokio::time::timeout(std::time::Duration::from_secs(5), async {
+            while observer.get_job(id).unwrap().unwrap().status != JobStatus::Succeeded {
+                tokio::task::yield_now().await;
+            }
+        })
+        .await
+        .unwrap();
+    });
+    let deletion = call(
+        "delete_document",
+        json!({
+        "documentId":document}
+        ),
+    )
+    .unwrap();
+    assert_eq!(deletion["kind"], "delete_document");
+    tauri::async_runtime::block_on(observer.close_all()).unwrap();
+}
diff --git a/src-tauri/src/intelligence/config.rs b/src-tauri/src/intelligence/config.rs
index f41d42b..331e8fe 100644
--- a/src-tauri/src/intelligence/config.rs
+++ b/src-tauri/src/intelligence/config.rs
@@ -61,48 +61,65 @@ impl ServiceConfigStore {
             .map_err(safe_config_error)
     }
     pub(crate) async fn persist_probe(
         &self,
         profile: ModelProfile,
         revision: Uuid,
         result: ConnectionProbe,
         dimensions: Option<u32>,
     ) -> AppResult<()> {
         let store = self.store.clone();
-        self.io.run(move |tx| {
-            let mut doc = read(tx)?;
-            let Some(saved) = doc.profiles.iter_mut().find(|p| p.profile.id == profile.id) else {
-                return Ok(());
-            };
-            if saved.revision != revision || saved.profile != profile { return Ok(()); }
-            if let Some(dim) = dimensions {
-                if profile.kind != ServiceKind::Embedding
-                    || !(128..=4096).contains(&dim)
-                    || profile.dimensions.is_some_and(|old| old != dim)
-                { return Err(configuration()); }
-                if profile.dimensions.is_none()
-                    && !store.transaction(|db| profile_in_use(db, profile.id))?
-                {
-                    saved.profile.dimensions = Some(dim);
-                    saved.revision = Uuid::new_v4();
-                    write(tx, &doc)?;
+        self.io
+            .run(move |tx| {
+                let mut doc = read(tx)?;
+                let Some(saved) = doc.profiles.iter_mut().find(|p| p.profile.id == profile.id)
+                else {
+                    return Ok(());
+                };
+                if saved.revision != revision || saved.profile != profile {
+                    return Ok(());
                 }
-            }
-            store.transaction(|db| {
-                let encoded = serde_json::to_string(&result).map_err(|_| configuration())?;
-                db.execute(
-                    "INSERT OR REPLACE INTO service_probes(profile_id,checked_at_ms,result_json) VALUES(?1,?2,?3)",
-                    rusqlite::params![profile.id.to_string(), result.checked_at_ms, encoded],
-                )?;
-                Ok(())
-            }).map_err(super::store::safe_store_error)
-        }).await.map_err(safe_config_error)
+                if let Some(dim) = dimensions {
+                    if profile.kind != ServiceKind::Embedding
+                        || !(128..=4096).contains(&dim)
+                        || profile.dimensions.is_some_and(|old| old != dim)
+                    {
+                        return Err(configuration());
+                    }
+                    if profile.dimensions.is_none()
+                        && !store.transaction(|db| profile_in_use(db, profile.id))?
+                    {
+                        saved.profile.dimensions = Some(dim);
+                        saved.revision = Uuid::new_v4();
+                        write(tx, &doc)?;
+                    }
+                }
+                store
+                    .transaction(|db| {
+                        let encoded =
+                            serde_json::to_string(&result).map_err(|_| configuration())?;
+                        db.execute(
+                            "INSERT OR REPLACE INTO \
+                    service_probes(profile_id,checked_at_ms,\
+                    result_json) VALUES(?1,?2,?3)",
+                            rusqlite::params![
+                                profile.id.to_string(),
+                                result.checked_at_ms,
+                                encoded
+                            ],
+                        )?;
+                        Ok(())
+                    })
+                    .map_err(super::store::safe_store_error)
+            })
+            .await
+            .map_err(safe_config_error)
     }
     pub(crate) fn new(io: ConfigIo, store: Arc<LocalStore>) -> Self {
         let path = io.data_dir().join("service-configs.json");
         Self { io, store, path }
     }
     pub fn path(&self) -> &Path {
         &self.path
     }
     pub async fn save(&self, request: SaveModelProfile) -> AppResult<ModelProfileView> {
         let SaveModelProfile {
@@ -241,20 +258,73 @@ impl ServiceConfigStore {
             })
             .await
             .map_err(safe_config_error)
     }
     pub async fn settings(&self) -> AppResult<IntelligenceSettings> {
         self.io
             .run(move |tx| Ok(read(tx)?.settings))
             .await
             .map_err(safe_config_error)
     }
+    pub(crate) async fn selected_embedding_snapshot(&self) -> AppResult<Option<ModelProfile>> {
+        self.io
+            .run(move |tx| {
+                let doc = read(tx)?;
+                doc.settings
+                    .embedding_profile_id
+                    .map(|id| {
+                        doc.profiles
+                            .iter()
+                            .find(|p| {
+                                p.profile.id == id && p.profile.kind == ServiceKind::Embedding
+                            })
+                            .map(|p| p.profile.clone())
+                            .ok_or_else(configuration)
+                    })
+                    .transpose()
+            })
+            .await
+            .map_err(safe_config_error)
+    }
+    pub(crate) async fn with_selected_embedding_completion<T: Send + 'static>(
+        &self,
+        operation: impl FnOnce(&rusqlite::Transaction<'_>, Option<ModelProfile>) -> AppResult<T>
+        + Send
+        + 'static,
+        completed: impl FnOnce(&T) + Send + 'static,
+    ) -> AppResult<T> {
+        let store = self.store.clone();
+        self.io
+            .run(move |tx| {
+                let doc = read(tx)?;
+                let profile = doc
+                    .settings
+                    .embedding_profile_id
+                    .map(|id| {
+                        doc.profiles
+                            .iter()
+                            .find(|p| {
+                                p.profile.id == id && p.profile.kind == ServiceKind::Embedding
+                            })
+                            .map(|p| p.profile.clone())
+                            .ok_or_else(configuration)
+                    })
+                    .transpose()?;
+                let result = store
+                    .transaction(|db| operation(db, profile))
+                    .map_err(super::store::safe_store_error)?;
+                completed(&result);
+                Ok(result)
+            })
+            .await
+            .map_err(safe_config_error)
+    }
     pub async fn save_settings(
         &self,
         value: IntelligenceSettings,
     ) -> AppResult<IntelligenceSettings> {
         self.io
             .run(move |tx| {
                 let mut doc = read(tx)?;
                 validate_settings(&doc, &value)?;
                 doc.settings = value.clone();
                 write(tx, &doc)?;
@@ -283,27 +353,20 @@ impl ServiceConfigStore {
                     return Err(configuration());
                 }
                 let bytes = tx
                     .secret_get(SERVICE, &key(id))?
                     .ok_or_else(configuration)?;
                 SecretString::from_store_bytes(bytes)
             })
             .await
             .map_err(safe_config_error)
     }
-    #[cfg_attr(
-        not(test),
-        expect(
-            dead_code,
-            reason = "Reserved for Task 5 immutable model credential execution"
-        )
-    )]
     pub(crate) async fn resolve_snapshot_credentials(
         &self,
         expected: ModelProfile,
     ) -> AppResult<SecretString> {
         self.io
             .run(move |tx| {
                 let doc = read(tx)?;
                 let saved = doc
                     .profiles
                     .iter()
diff --git a/src-tauri/src/intelligence/jobs.rs b/src-tauri/src/intelligence/jobs.rs
index 4bfa5bf..413b2e6 100644
--- a/src-tauri/src/intelligence/jobs.rs
+++ b/src-tauri/src/intelligence/jobs.rs
@@ -20,28 +20,51 @@ use std::{
 use tokio::sync::{Notify, Semaphore, broadcast, mpsc, watch};
 use uuid::Uuid;
 mod child_operations;
 use child_operations::ChildOperations;
 pub struct JobContext {
     pub id: Uuid,
     pub cancel: watch::Receiver<bool>,
     fence: JobAdmissionFence,
     children: ChildOperations,
 }
+#[derive(Clone)]
+pub(crate) struct JobPublicationFence {
+    id: Uuid,
+    cancel: watch::Receiver<bool>,
+    fence: JobAdmissionFence,
+}
+impl JobPublicationFence {
+    pub(crate) fn commit<T>(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        operation: impl FnOnce() -> AppResult<T>,
+    ) -> AppResult<T> {
+        self.fence.run(|| {
+            if *self.cancel.borrow()
+                || store::job_in_transaction(tx, self.id)?
+                    .is_none_or(|job| job.status != JobStatus::Running)
+            {
+                return Err(error(IntelligenceErrorCode::Cancelled));
+            }
+            operation()
+        })
+    }
+}
 impl JobContext {
-    #[cfg_attr(
-        not(test),
-        expect(
-            dead_code,
-            reason = "Reserved for Task 5 native and index operation ownership"
-        )
-    )]
+    pub(crate) fn publication_fence(&self) -> JobPublicationFence {
+        JobPublicationFence {
+            id: self.id,
+            cancel: self.cancel.clone(),
+            fence: self.fence.clone(),
+        }
+    }
     pub(crate) fn operation_owner(&self) -> AppResult<OperationLease> {
         // Only control is held here. The returned token crosses config/index
         // boundaries after this guard is released; never consult SQLite here.
         let control = self.fence.control.lock().unwrap();
         if control.stopping || self.fence.closed.load(Ordering::Acquire) {
             return Err(error(IntelligenceErrorCode::Busy));
         }
         if *self.cancel.borrow() || !control.active.contains_key(&self.id) {
             return Err(error(IntelligenceErrorCode::Cancelled));
         }
@@ -86,20 +109,46 @@ pub trait JobHandler: Send + Sync {
     async fn cleanup(&self, _reaped: Option<Uuid>) -> AppResult<()> {
         Ok(())
     }
     async fn cleanup_after_stop(&self) -> AppResult<()> {
         self.cleanup(None).await
     }
     fn reconcile_startup(&self) -> AppResult<()> {
         Ok(())
     }
 }
+pub(crate) struct AfterTerminalNotice {
+    pub(crate) parent_id: Uuid,
+    pub(crate) document_id: Uuid,
+    pub(crate) version: u32,
+    pub(crate) sha256: String,
+    pub(crate) parser: String,
+}
+#[async_trait::async_trait]
+pub(crate) trait AfterTerminal: Send + Sync {
+    fn notice(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        job: &JobView,
+    ) -> AppResult<Option<AfterTerminalNotice>>;
+    fn deferred(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        notice: &AfterTerminalNotice,
+    ) -> AppResult<()>;
+    async fn succeeded(
+        &self,
+        notice: AfterTerminalNotice,
+        owner: OperationLease,
+        fence: JobAdmissionFence,
+    );
+}
 #[derive(Clone)]
 pub(crate) struct JobEvents(broadcast::Sender<IntelligenceEvent>);
 impl JobEvents {
     pub(crate) fn publish(&self, job: JobView) {
         let _ = self.0.send(IntelligenceEvent::JobChanged { job });
     }
 }
 /// Generation fence for Task 6's config→SQLite atomic job/result transaction.
 /// Call `run` only inside the SQLite transaction, never acquire it before SQLite.
 #[derive(Clone)]
@@ -131,20 +180,21 @@ struct Generation {
     id: Uuid,
     cancel: watch::Sender<bool>,
     completion: watch::Receiver<bool>,
     control: Arc<Mutex<Control>>,
 }
 #[derive(Default)]
 struct State {
     frozen: bool,
     handlers: Handlers,
     generation: Option<Generation>,
+    after_terminal: Option<Arc<dyn AfterTerminal>>,
 }
 pub struct JobScheduler {
     store: Arc<LocalStore>,
     state: Mutex<State>,
     wake: Arc<Notify>,
     events: broadcast::Sender<IntelligenceEvent>,
     parser: Arc<Semaphore>,
     shutdown_timeout: Duration,
     closed: Arc<AtomicBool>,
     signal: watch::Sender<bool>,
@@ -163,36 +213,52 @@ impl JobScheduler {
         }
     }
     pub fn register(&self, kind: JobKind, handler: Arc<dyn JobHandler>) -> AppResult<()> {
         let mut s = self.state.lock().unwrap();
         if s.frozen || s.handlers.contains_key(&kind) {
             return Err(error(IntelligenceErrorCode::Configuration));
         }
         s.handlers.insert(kind, handler);
         Ok(())
     }
+    pub(crate) fn register_after_terminal(&self, hook: Arc<dyn AfterTerminal>) -> AppResult<()> {
+        let mut state = self.state.lock().unwrap();
+        if state.frozen
+            || state.after_terminal.is_some()
+            || !state.handlers.contains_key(&JobKind::Import)
+            || !state.handlers.contains_key(&JobKind::Reindex)
+        {
+            return Err(error(IntelligenceErrorCode::Configuration));
+        }
+        state.after_terminal = Some(hook);
+        Ok(())
+    }
+    pub(crate) fn wake_port(&self) -> Arc<Notify> {
+        self.wake.clone()
+    }
     pub fn start(&self) -> AppResult<()> {
         self.start_owned(Arc::new(()))
     }
     pub(crate) fn start_owned(&self, lease: OperationLease) -> AppResult<()> {
         let runtime = tokio::runtime::Handle::try_current()
             .map_err(|_| error(IntelligenceErrorCode::Configuration))?;
         let mut s = self.state.lock().unwrap();
         if let Some(g) = &s.generation {
             return if self.closed.load(Ordering::Acquire) || g.control.lock().unwrap().stopping {
                 Err(error(IntelligenceErrorCode::Busy))
             } else {
                 Ok(())
             };
         }
         s.frozen = true;
         let handlers = Arc::new(s.handlers.clone());
+        let after_terminal = s.after_terminal.clone();
         let control = Arc::new(Mutex::new(Control::default()));
         self.closed.store(false, Ordering::Release);
         self.signal.send_replace(false);
         let cancel = self.signal.clone();
         let stop = cancel.subscribe();
         let (done, completion) = watch::channel(false);
         let wake = self.wake.clone();
         let store = self.store.clone();
         let events = self.events.clone();
         let worker_control = control.clone();
@@ -207,31 +273,34 @@ impl JobScheduler {
                 workers.spawn(worker(
                     store.clone(),
                     handlers.clone(),
                     worker_control.clone(),
                     rx.clone(),
                     wake.clone(),
                     events.clone(),
                     stop.clone(),
                     closed.clone(),
                     children.clone(),
+                    after_terminal.clone(),
                 ));
             }
             let mut stop = stop;
             loop {
                 tokio::select! {
-                    biased;
-                    _ = cancelled(&mut stop) => break,
-                    _ = wake.notified() => {
-                        for _ in 0..2 { let _ = tx.try_send(()); }
-                    }
-                }
+                                                  biased;
+                                                  _ = cancelled(&mut stop) => break,
+                                                  _ = wake.notified() => {
+                                                      for _ in 0..2 {
+                let _ = tx.try_send(());
+                               }
+                                                  }
+                                              }
             }
             {
                 let mut control = worker_control.lock().unwrap();
                 control.stopping = true;
                 for cancel in control.active.values() {
                     cancel.send_replace(true);
                 }
             }
             drop(tx);
             while workers.join_next().await.is_some() {}
@@ -331,25 +400,38 @@ impl JobScheduler {
         object_id: Option<Uuid>,
         admission: JobAdmission,
         ids: Vec<Uuid>,
         input: serde_json::Value,
     ) -> AppResult<JobView> {
         let fence = self.admission_fence(kind)?;
         // Shutdown can begin while config entry waits. A shared generation control
         // guard fences commit against shutdown without capturing Arc<JobScheduler>.
         let result = config
             .with_profile_snapshot(ids, move |tx, profiles| {
-                fence.run(|| store::admit_job(tx, JobDraft {
-                id: Uuid::new_v4(), kind, session_id, object_id, admission,
-                payload: serde_json::json!({ "version": 1, "profiles": profiles, "input": input }),
-                initial_failure: None,
-            }))
+                fence.run(|| {
+                    store::admit_job(
+                        tx,
+                        JobDraft {
+                            id: Uuid::new_v4(),
+                            kind,
+                            session_id,
+                            object_id,
+                            admission,
+                            payload: serde_json::json!({
+                             "version": 1,
+                             "profiles": profiles,
+                             "input": input }
+                            ),
+                            initial_failure: None,
+                        },
+                    )
+                })
             })
             .await?;
         let view = result.view().clone();
         self.publish(view.clone());
         self.wake();
         Ok(view)
     }
     pub async fn cancel(&self, id: Uuid) -> AppResult<JobView> {
         let s = self.state.lock().unwrap();
         self.store
@@ -523,26 +605,33 @@ fn check_admission(s: &State, kind: JobKind) -> AppResult<()> {
 async fn worker(
     store: Arc<LocalStore>,
     handlers: Arc<Handlers>,
     control: Arc<Mutex<Control>>,
     rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
     wake: Arc<Notify>,
     events: broadcast::Sender<IntelligenceEvent>,
     mut stop: watch::Receiver<bool>,
     closed: Arc<AtomicBool>,
     children: ChildOperations,
+    after_terminal: Option<Arc<dyn AfterTerminal>>,
 ) {
     use futures_util::FutureExt;
     loop {
-        tokio::select! {biased;
-        _=cancelled(&mut stop)=>return,token=async{rx.lock().await.recv().await}=>if token.is_none(){return;
-        }}
+        tokio::select! {
+        biased;
+                _=cancelled(&mut stop)=>return,
+        token=async{
+        rx.lock().await.recv().await}
+        =>if token.is_none(){
+        return;
+                }
+        }
         let claimed = match claim(&store, &control, &closed) {
             Ok(v) => v,
             Err(_) => {
                 control.lock().unwrap().stopping = true;
                 return;
             }
         };
         let Some((job, payload, cancel)) = claimed else {
             continue;
         };
@@ -559,78 +648,138 @@ async fn worker(
                     },
                     children: children.clone(),
                 },
                 payload,
             ))
             .catch_unwind()
             .await
             .unwrap_or(Err(IntelligenceErrorCode::Interrupted)),
             None => Err(IntelligenceErrorCode::Configuration),
         };
+        let notice = if result.is_ok() {
+            after_terminal
+                .as_ref()
+                .and_then(|hook| store.transaction(|tx| hook.notice(tx, &job)).ok().flatten())
+        } else {
+            None
+        };
+        // Reservation happens while this context is still active and Running.
+        // ConfigIo never acquires an owner from a terminal context.
+        let continuation = notice.as_ref().and_then(|_| {
+            JobContext {
+                id: job.id,
+                cancel: cancel.clone(),
+                fence: JobAdmissionFence {
+                    control: control.clone(),
+                    closed: closed.clone(),
+                    cancel: stop.clone(),
+                },
+                children: children.clone(),
+            }
+            .operation_owner()
+            .ok()
+        });
         let finished = store.transaction(|tx| {
             let mut c = control.lock().unwrap();
+            let was_running = store::job_in_transaction(tx, job.id)?
+                .is_some_and(|job| job.status == JobStatus::Running);
             let is_cancelled = *cancel.borrow();
             let (status, code) = if is_cancelled {
                 (JobStatus::Cancelled, Some(IntelligenceErrorCode::Cancelled))
             } else {
                 match result {
                     Ok(()) => (JobStatus::Succeeded, None),
                     Err(IntelligenceErrorCode::Cancelled) => {
                         (JobStatus::Cancelled, Some(IntelligenceErrorCode::Cancelled))
                     }
                     Err(IntelligenceErrorCode::Interrupted) => (
                         JobStatus::Interrupted,
                         Some(IntelligenceErrorCode::Interrupted),
                     ),
                     Err(e) => (JobStatus::Failed, Some(e)),
                 }
             };
             transition_tx(tx, job.id, "running", status, code, &handlers, false)?;
             c.active.remove(&job.id);
-            Ok(())
+            let succeeded = was_running && status == JobStatus::Succeeded;
+            if succeeded && continuation.is_none() {
+                if let (Some(hook), Some(notice)) = (&after_terminal, &notice) {
+                    hook.deferred(tx, notice)?;
+                }
+            }
+            Ok(succeeded)
         });
         if finished.is_ok() {
             if let Ok(Some(job)) = store.get_job(job.id) {
                 let _ = events.send(IntelligenceEvent::JobChanged { job });
             }
         }
+        if matches!(finished, Ok(true)) {
+            if let (Some(hook), Some(notice), Some(owner)) = (&after_terminal, notice, continuation)
+            {
+                hook.succeeded(
+                    notice,
+                    owner,
+                    JobAdmissionFence {
+                        control: control.clone(),
+                        closed: closed.clone(),
+                        cancel: stop.clone(),
+                    },
+                )
+                .await;
+            }
+        }
         if let Some(handler) = handlers.get(&job.kind) {
             let _ = handler.cleanup(Some(job.id)).await;
         }
         wake.notify_one();
     }
 }
 fn claim(
     store: &LocalStore,
     control: &Mutex<Control>,
     closed: &AtomicBool,
 ) -> AppResult<Option<(JobView, serde_json::Value, watch::Receiver<bool>)>> {
     use rusqlite::OptionalExtension;
-    let claimed = store.transaction(|tx| {
-        let mut control = control.lock().unwrap();
-        if control.stopping || closed.load(Ordering::Acquire) { return Ok(None); }
-        let row: Option<(String, String)> = tx.query_row(
-            "SELECT id,payload_json FROM intelligence_jobs WHERE status='queued' ORDER BY rowid LIMIT 1",
-            [], |row| Ok((row.get(0)?, row.get(1)?)),
-        ).optional()?;
-        let Some((id, payload)) = row else { return Ok(None); };
-        let id = Uuid::parse_str(&id).map_err(|_| error(IntelligenceErrorCode::Storage))?;
-        let decoded = store::decode_payload(&payload)?;
-        tx.execute(
-            "UPDATE intelligence_jobs SET status='running',updated_at_ms=?1 WHERE id=?2 AND status='queued'",
-            rusqlite::params![now_ms(), id.to_string()],
-        )?;
-        let (cancel, receiver) = watch::channel(false);
-        control.active.insert(id, cancel);
-        let payload = serde_json::to_value(decoded).map_err(|_| error(IntelligenceErrorCode::Storage))?;
-        Ok(Some((id, payload, receiver)))
-    }).map_err(store::safe_store_error)?;
+    let claimed = store
+        .transaction(|tx| {
+            let mut control = control.lock().unwrap();
+            if control.stopping || closed.load(Ordering::Acquire) {
+                return Ok(None);
+            }
+            let row: Option<(String, String)> = tx
+                .query_row(
+                    "SELECT id,payload_json FROM \
+                    intelligence_jobs WHERE status='queued' \
+                    ORDER BY rowid LIMIT 1",
+                    [],
+                    |row| Ok((row.get(0)?, row.get(1)?)),
+                )
+                .optional()?;
+            let Some((id, payload)) = row else {
+                return Ok(None);
+            };
+            let id = Uuid::parse_str(&id).map_err(|_| error(IntelligenceErrorCode::Storage))?;
+            let decoded = store::decode_payload(&payload)?;
+            tx.execute(
+                "UPDATE intelligence_jobs SET \
+                    status='running',updated_at_ms=?1 WHERE \
+                    id=?2 AND status='queued'",
+                rusqlite::params![now_ms(), id.to_string()],
+            )?;
+            let (cancel, receiver) = watch::channel(false);
+            control.active.insert(id, cancel);
+            let payload =
+                serde_json::to_value(decoded).map_err(|_| error(IntelligenceErrorCode::Storage))?;
+            Ok(Some((id, payload, receiver)))
+        })
+        .map_err(store::safe_store_error)?;
     claimed
         .map(|(id, payload, cancel)| {
             Ok((
                 store
                     .get_job(id)?
                     .ok_or_else(|| error(IntelligenceErrorCode::Storage))?,
                 payload,
                 cancel,
             ))
         })
@@ -638,44 +787,80 @@ fn claim(
 }
 fn transition_tx(
     tx: &rusqlite::Transaction<'_>,
     id: Uuid,
     from: &str,
     status: JobStatus,
     code: Option<IntelligenceErrorCode>,
     handlers: &Handlers,
     awaiting_reap: bool,
 ) -> AppResult<()> {
-    let changed=tx.execute(
-        "UPDATE intelligence_jobs SET status=?1,error_code=?2,updated_at_ms=?3 WHERE id=?4 AND status=?5",
-        rusqlite::params![serde_json::to_value(status).unwrap().as_str(), code.map(IntelligenceErrorCode::as_str), now_ms(), id.to_string(), from],
+    let changed = tx.execute(
+        "UPDATE intelligence_jobs SET status=?1,\
+                    error_code=?2,updated_at_ms=?3 WHERE id=?4 \
+                    AND status=?5",
+        rusqlite::params![
+            serde_json::to_value(status).unwrap().as_str(),
+            code.map(IntelligenceErrorCode::as_str),
+            now_ms(),
+            id.to_string(),
+            from
+        ],
     )?;
     if changed != 0 {
         let job = store::job_in_transaction(tx, id)?
             .ok_or_else(|| error(IntelligenceErrorCode::Storage))?;
         if let Some(handler) = handlers.get(&job.kind) {
             handler.terminal_tx(tx, id, status, code, awaiting_reap)?;
         }
     }
     Ok(())
 }
 fn interrupt(store: &LocalStore, handlers: &Handlers) -> AppResult<Vec<JobView>> {
-    store.transaction(|tx| {
-        let pending=tx.prepare("SELECT id,status FROM intelligence_jobs WHERE status IN ('queued','running') LIMIT 33")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
-        if pending.len()>store::MAX_UNFINISHED{return Err(error(IntelligenceErrorCode::ResourceLimit));}
-        let mut changed=Vec::with_capacity(pending.len());
-        for (id,status) in pending {let id=Uuid::parse_str(&id).map_err(|_|error(IntelligenceErrorCode::Storage))?;transition_tx(tx,id,&status,JobStatus::Interrupted,Some(IntelligenceErrorCode::Interrupted),handlers,status=="running")?;changed.push(store::job_in_transaction(tx,id)?.ok_or_else(||error(IntelligenceErrorCode::Storage))?);}
-        Ok(changed)
-    }).map_err(store::safe_store_error)
+    store
+        .transaction(|tx| {
+            let pending = tx
+                .prepare(
+                    "SELECT id,status FROM intelligence_jobs \
+                    WHERE status IN ('queued','running') LIMIT 33",
+                )?
+                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
+                .collect::<Result<Vec<_>, _>>()?;
+            if pending.len() > store::MAX_UNFINISHED {
+                return Err(error(IntelligenceErrorCode::ResourceLimit));
+            }
+            let mut changed = Vec::with_capacity(pending.len());
+            for (id, status) in pending {
+                let id = Uuid::parse_str(&id).map_err(|_| error(IntelligenceErrorCode::Storage))?;
+                transition_tx(
+                    tx,
+                    id,
+                    &status,
+                    JobStatus::Interrupted,
+                    Some(IntelligenceErrorCode::Interrupted),
+                    handlers,
+                    status == "running",
+                )?;
+                changed.push(
+                    store::job_in_transaction(tx, id)?
+                        .ok_or_else(|| error(IntelligenceErrorCode::Storage))?,
+                );
+            }
+            Ok(changed)
+        })
+        .map_err(store::safe_store_error)
 }
 pub(crate) fn now_ms() -> i64 {
     std::time::SystemTime::now()
         .duration_since(std::time::UNIX_EPOCH)
         .unwrap_or_default()
         .as_millis()
         .min(i64::MAX as u128) as i64
 }
 fn error(code: IntelligenceErrorCode) -> AppError {
     AppError::Intelligence(code)
 }
 #[cfg(test)]
 mod tests;
+
+#[cfg(test)]
+pub(crate) mod test_support;
diff --git a/src-tauri/src/intelligence/jobs/test_support.rs b/src-tauri/src/intelligence/jobs/test_support.rs
new file mode 100644
index 0000000..a6e2e7a
--- /dev/null
+++ b/src-tauri/src/intelligence/jobs/test_support.rs
@@ -0,0 +1,14 @@
+//! Test construction only: replace an occupied handler before the first start.
+use super::*;
+pub(crate) fn replace_pre_start_handler(
+    scheduler: &JobScheduler,
+    kind: JobKind,
+    handler: Arc<dyn JobHandler>,
+) -> AppResult<()> {
+    let mut state = scheduler.state.lock().unwrap();
+    if state.frozen || state.generation.is_some() || !state.handlers.contains_key(&kind) {
+        return Err(error(IntelligenceErrorCode::Configuration));
+    }
+    state.handlers.insert(kind, handler);
+    Ok(())
+}
diff --git a/src-tauri/src/intelligence/jobs/tests.rs b/src-tauri/src/intelligence/jobs/tests.rs
index e8c4f25..b24fe1d 100644
--- a/src-tauri/src/intelligence/jobs/tests.rs
+++ b/src-tauri/src/intelligence/jobs/tests.rs
@@ -1,32 +1,39 @@
 use super::*;
 mod child_operations;
+mod continuations;
 mod documents;
 use crate::intelligence::test_support::*;
 use std::{
     sync::atomic::{AtomicUsize, Ordering},
     time::Duration,
 };
 struct Handler {
     calls: Arc<AtomicUsize>,
     release: Arc<tokio::sync::Notify>,
     fail: bool,
 }
 #[async_trait::async_trait]
 impl JobHandler for Handler {
     async fn run(
         &self,
         mut context: JobContext,
         _: serde_json::Value,
     ) -> Result<(), IntelligenceErrorCode> {
         self.calls.fetch_add(1, Ordering::SeqCst);
-        tokio::select! {_=super::super::http::cancelled(&mut context.cancel)=>Err(IntelligenceErrorCode::Cancelled),_=self.release.notified()=>if self.fail{Err(IntelligenceErrorCode::InvalidResponse)}else{Ok(())}}
+        tokio::select! {
+        _=super::super::http::cancelled(&mut context.cancel)=>Err(IntelligenceErrorCode::Cancelled),
+        _=self.release.notified()=>if self.fail{
+        Err(IntelligenceErrorCode::InvalidResponse)}
+        else{
+        Ok(())}
+        }
     }
 }
 fn handler() -> Arc<Handler> {
     Arc::new(Handler {
         calls: Arc::default(),
         release: Arc::default(),
         fail: false,
     })
 }
 async fn enqueue(jobs: &JobScheduler, id: Uuid) -> AppResult<JobView> {
@@ -259,21 +266,23 @@ async fn intelligence_jobs_snapshot_admission_retains_public_identity() {
         .jobs
         .enqueue_with_profiles(
             &h.config,
             JobKind::Import,
             None,
             Some(id),
             JobAdmission::Manual {
                 exclusion_key: format!("document:{id}"),
             },
             vec![p.id],
-            serde_json::json!({"document_id":id}),
+            serde_json::json!({
+            "document_id":id}
+            ),
         )
         .await
         .unwrap();
     assert!(h.config.delete(p.id).await.is_err());
     let raw: String = h
         .store
         .transaction(|tx| {
             Ok(tx.query_row(
                 "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                 [job.id.to_string()],
@@ -336,10 +345,32 @@ async fn intelligence_jobs_actual_config_snapshot_and_shutdown_share_one_order()
         .await
         .unwrap()
         .unwrap()
         .unwrap();
     release.join().unwrap();
     assert_eq!(
         h.store.get_job(admitted.view().id).unwrap().unwrap().status,
         JobStatus::Cancelled
     );
 }
+
+#[tokio::test]
+async fn handler_fixture_replacement_is_only_available_before_first_start() {
+    let h = IntelligenceHarness::new();
+    assert!(
+        super::test_support::replace_pre_start_handler(&h.jobs, JobKind::Import, handler())
+            .is_err()
+    );
+    h.jobs.register(JobKind::Import, handler()).unwrap();
+    super::test_support::replace_pre_start_handler(&h.jobs, JobKind::Import, handler()).unwrap();
+    assert!(h.jobs.register(JobKind::Import, handler()).is_err());
+    h.jobs.start().unwrap();
+    assert!(
+        super::test_support::replace_pre_start_handler(&h.jobs, JobKind::Import, handler())
+            .is_err()
+    );
+    h.jobs.shutdown().await.unwrap();
+    assert!(
+        super::test_support::replace_pre_start_handler(&h.jobs, JobKind::Import, handler())
+            .is_err()
+    );
+}
diff --git a/src-tauri/src/intelligence/jobs/tests/continuations.rs b/src-tauri/src/intelligence/jobs/tests/continuations.rs
new file mode 100644
index 0000000..f16273d
--- /dev/null
+++ b/src-tauri/src/intelligence/jobs/tests/continuations.rs
@@ -0,0 +1,193 @@
+use super::*;
+struct Immediate;
+#[async_trait::async_trait]
+impl JobHandler for Immediate {
+    async fn run(&self, _: JobContext, _: serde_json::Value) -> Result<(), IntelligenceErrorCode> {
+        Ok(())
+    }
+}
+struct Hook {
+    io: crate::config::ConfigIo,
+    store: Arc<crate::storage::LocalStore>,
+    channels: std::sync::Mutex<
+        Option<(
+            tokio::sync::oneshot::Sender<()>,
+            std::sync::mpsc::Receiver<()>,
+        )>,
+    >,
+    calls: AtomicUsize,
+    completed: Arc<AtomicUsize>,
+}
+#[async_trait::async_trait]
+impl AfterTerminal for Hook {
+    fn notice(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        job: &JobView,
+    ) -> AppResult<Option<AfterTerminalNotice>> {
+        if job.kind != JobKind::Import {
+            return Ok(None);
+        }
+        let _ = tx;
+        Ok(Some(AfterTerminalNotice {
+            parent_id: job.id,
+            document_id: job.object_id.unwrap(),
+            version: 1,
+            sha256: "a".repeat(64),
+            parser: "fixture".into(),
+        }))
+    }
+    fn deferred(&self, _: &rusqlite::Transaction<'_>, _: &AfterTerminalNotice) -> AppResult<()> {
+        Ok(())
+    }
+    async fn succeeded(
+        &self,
+        notice: AfterTerminalNotice,
+        owner: OperationLease,
+        fence: JobAdmissionFence,
+    ) {
+        assert_eq!(
+            self.store
+                .get_job(notice.parent_id)
+                .unwrap()
+                .unwrap()
+                .status,
+            JobStatus::Succeeded
+        );
+        self.calls.fetch_add(1, Ordering::SeqCst);
+        self.store
+            .transaction(|tx| {
+                fence.run(|| {
+                    store::admit_job(
+                        tx,
+                        store::JobDraft {
+                            id: Uuid::new_v4(),
+                            kind: JobKind::Reindex,
+                            session_id: None,
+                            object_id: Some(notice.document_id),
+                            admission: JobAdmission::Manual {
+                                exclusion_key: format!("document:{}", notice.document_id),
+                            },
+                            payload: payload(vec![]),
+                            initial_failure: None,
+                        },
+                    )?;
+                    Ok(())
+                })
+            })
+            .unwrap();
+        let (entered, release) = self.channels.lock().unwrap().take().unwrap();
+        let completed = self.completed.clone();
+        self.io
+            .with_operation_owner(owner)
+            .run(move |_| {
+                let _ = entered.send(());
+                release.recv_timeout(Duration::from_secs(5)).unwrap();
+                completed.fetch_add(1, Ordering::SeqCst);
+                Ok(())
+            })
+            .await
+            .unwrap();
+    }
+}
+#[tokio::test]
+async fn intelligence_after_terminal_reserves_before_success_releases_exclusion_and_drains_actual_child()
+ {
+    let h = IntelligenceHarness::new();
+    h.jobs
+        .register(JobKind::Import, Arc::new(Immediate))
+        .unwrap();
+    h.jobs
+        .register(JobKind::Reindex, Arc::new(Immediate))
+        .unwrap();
+    let (entered, wait) = tokio::sync::oneshot::channel();
+    let (release, receive) = std::sync::mpsc::channel();
+    let hook = Arc::new(Hook {
+        io: h.io.clone(),
+        store: h.store.clone(),
+        channels: std::sync::Mutex::new(Some((entered, receive))),
+        calls: AtomicUsize::new(0),
+        completed: Arc::new(AtomicUsize::new(0)),
+    });
+    h.jobs.register_after_terminal(hook.clone()).unwrap();
+    h.jobs.start().unwrap();
+    let parent = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
+    wait.await.unwrap();
+    assert_eq!(
+        h.store.get_job(parent.id).unwrap().unwrap().status,
+        JobStatus::Succeeded
+    );
+    let mut shutdown = Box::pin(h.jobs.shutdown());
+    let pending = tokio::time::timeout(Duration::from_millis(40), shutdown.as_mut())
+        .await
+        .is_err();
+    release.send(()).unwrap();
+    tokio::time::timeout(Duration::from_secs(3), shutdown.as_mut())
+        .await
+        .unwrap()
+        .unwrap();
+    assert!(
+        pending,
+        "terminal parent does not release the reserved entered child operation"
+    );
+    assert_eq!(hook.completed.load(Ordering::SeqCst), 1);
+    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
+    h.jobs.start().unwrap();
+    h.jobs.shutdown().await.unwrap();
+    assert_eq!(
+        hook.calls.load(Ordering::SeqCst),
+        1,
+        "startup must not replay successful parent notices"
+    );
+}
+#[tokio::test]
+async fn intelligence_after_terminal_does_not_run_success_hook_for_failed_or_cancelled_parent() {
+    for scenario in ["handler_failure", "preterminal_failure", "cancelled"] {
+        let h = IntelligenceHarness::new();
+        let mut handler = handler();
+        Arc::get_mut(&mut handler).unwrap().fail = scenario == "handler_failure";
+        h.jobs.register(JobKind::Import, handler.clone()).unwrap();
+        h.jobs
+            .register(JobKind::Reindex, Arc::new(Immediate))
+            .unwrap();
+        let (entered, _wait) = tokio::sync::oneshot::channel();
+        let (_release, receive) = std::sync::mpsc::channel();
+        let hook = Arc::new(Hook {
+            io: h.io.clone(),
+            store: h.store.clone(),
+            channels: std::sync::Mutex::new(Some((entered, receive))),
+            calls: AtomicUsize::new(0),
+            completed: Arc::new(AtomicUsize::new(0)),
+        });
+        h.jobs.register_after_terminal(hook.clone()).unwrap();
+        h.jobs.start().unwrap();
+        let parent = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
+        status(&h, parent.id, JobStatus::Running).await;
+        if scenario == "cancelled" {
+            h.jobs.cancel(parent.id).await.unwrap();
+        } else if scenario == "preterminal_failure" {
+            h.store
+                .transaction(|tx| {
+                    tx.execute(
+                        "UPDATE intelligence_jobs SET status='failed' WHERE id=?1",
+                        [parent.id.to_string()],
+                    )?;
+                    Ok(())
+                })
+                .unwrap();
+        }
+        handler.release.notify_one();
+        status(
+            &h,
+            parent.id,
+            if scenario == "cancelled" {
+                JobStatus::Cancelled
+            } else {
+                JobStatus::Failed
+            },
+        )
+        .await;
+        h.jobs.shutdown().await.unwrap();
+        assert_eq!(hook.calls.load(Ordering::SeqCst), 0);
+    }
+}
diff --git a/src-tauri/src/knowledge/document_tests.rs b/src-tauri/src/knowledge/document_tests.rs
index 6807baf..614bf9c 100644
--- a/src-tauri/src/knowledge/document_tests.rs
+++ b/src-tauri/src/knowledge/document_tests.rs
@@ -1,105 +1,318 @@
 use crate::{error::AppResult, storage::LocalStore};
 use rusqlite::{Connection, params};
 use uuid::Uuid;
 
+#[test]
+fn knowledge_migration5_binding_fences_and_reopen_preserve_untrusted_legacy_rows() {
+    let directory = tempfile::tempdir().unwrap();
+    for pass in 0..2 {
+        let store = LocalStore::open(directory.path()).unwrap();
+        store
+            .transaction(|tx| {
+                let version: u32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
+                assert_eq!(version, 5);
+                if pass == 0 {
+                    let key = "a".repeat(64);
+                    tx.execute(
+                        "INSERT INTO \
+                    knowledge_index_generations(identity,state,\
+                    semantic_key) VALUES ('first','building',?1)",
+                        [&key],
+                    )?;
+                    assert!(
+                        tx.execute(
+                            "INSERT INTO \
+                    knowledge_index_generations(identity,state,\
+                    semantic_key) VALUES ('duplicate','building',\
+                    ?1)",
+                            [&key]
+                        )
+                        .is_err()
+                    );
+                    tx.execute(
+                        "INSERT INTO \
+                    knowledge_index_generations(identity,state,\
+                    semantic_key,requested_dimension) VALUES \
+                    ('explicit','building',?1,128)",
+                        [&key],
+                    )?;
+                    assert!(
+                        tx.execute(
+                            "INSERT INTO \
+                    knowledge_index_generations(identity,state,\
+                    semantic_key,requested_dimension) VALUES \
+                    ('duplicate-explicit','building',?1,128)",
+                            [&key]
+                        )
+                        .is_err()
+                    );
+                    tx.execute(
+                        "INSERT INTO \
+                    knowledge_index_generations(identity,state,\
+                    semantic_key,requested_dimension) VALUES \
+                    ('other-dimension','building',?1,256)",
+                        [&key],
+                    )?;
+                }
+                let trusted: u32 = tx.query_row(
+                    "SELECT count(*) FROM knowledge_index_generation_seals",
+                    [],
+                    |r| r.get(0),
+                )?;
+                assert_eq!(trusted, 0);
+                Ok(())
+            })
+            .unwrap();
+    }
+}
+
 #[test]
 fn documents_migration_preserves_legacy_nullable_metadata_and_chunks_across_reopen() {
     let directory = tempfile::tempdir().unwrap();
     let document = Uuid::new_v4().to_string();
     let chunk = Uuid::new_v4().to_string();
     let connection = Connection::open(directory.path().join("vox-bridge.sqlite")).unwrap();
     connection
         .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
         .unwrap();
     connection
         .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
         .unwrap();
     connection.pragma_update(None, "user_version", 2).unwrap();
-    connection.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'legacy-hash','legacy-reference',NULL,37)", [&document]).unwrap();
-    connection.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text) VALUES (?1,?2,0,'legacy canonical text')", params![chunk,document]).unwrap();
+    connection
+        .execute(
+            "INSERT INTO documents (id,sha256,uri,title,\
+                    created_at) VALUES (?1,'legacy-hash',\
+                    'legacy-reference',NULL,37)",
+            [&document],
+        )
+        .unwrap();
+    connection
+        .execute(
+            "INSERT INTO document_chunks (id,document_id,\
+                    chunk_index,text) VALUES (?1,?2,0,'legacy \
+                    canonical text')",
+            params![chunk, document],
+        )
+        .unwrap();
     drop(connection);
     for _ in 0..2 {
         let store = LocalStore::open(directory.path()).unwrap();
-        store.transaction(|tx| -> AppResult<()> {
-            let old: (String, String, Option<String>, i64) = tx.query_row(
-                "SELECT sha256,uri,title,created_at FROM documents WHERE id=?1", [&document],
-                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
-            )?;
-            assert_eq!(old,("legacy-hash".into(),"legacy-reference".into(),None,37));
-            let metadata: (String,String,u32,Option<String>,Option<String>,String,Option<i64>) = tx.query_row(
-                "SELECT kind,origin,version,original_path,managed_path,status,updated_at_ms FROM documents WHERE id=?1", [&document],
-                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
-            )?;
-            assert_eq!(metadata,("unknown".into(),"legacy".into(),1,None,None,"repair_required".into(),None));
-            let retained: (String,String,u32,Option<String>,String) = tx.query_row(
-                "SELECT id,text,chunk_index,vector_id,index_state FROM document_chunks WHERE document_id=?1", [&document],
-                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
-            )?;
-            assert_eq!(retained,(chunk.clone(),"legacy canonical text".into(),0,None,"repair_required".into()));
-            let version: u32 = tx.pragma_query_value(None,"user_version",|row|row.get(0))?;
-            assert_eq!(version,4);
-            Ok(())
-        }).unwrap();
+        store
+            .transaction(|tx| -> AppResult<()> {
+                let old: (String, String, Option<String>, i64) = tx.query_row(
+                    "SELECT sha256,uri,title,created_at FROM documents WHERE id=?1",
+                    [&document],
+                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
+                )?;
+                assert_eq!(
+                    old,
+                    ("legacy-hash".into(), "legacy-reference".into(), None, 37)
+                );
+                let metadata: (
+                    String,
+                    String,
+                    u32,
+                    Option<String>,
+                    Option<String>,
+                    String,
+                    Option<i64>,
+                ) = tx.query_row(
+                    "SELECT kind,origin,version,original_path,\
+                    managed_path,status,updated_at_ms FROM \
+                    documents WHERE id=?1",
+                    [&document],
+                    |row| {
+                        Ok((
+                            row.get(0)?,
+                            row.get(1)?,
+                            row.get(2)?,
+                            row.get(3)?,
+                            row.get(4)?,
+                            row.get(5)?,
+                            row.get(6)?,
+                        ))
+                    },
+                )?;
+                assert_eq!(
+                    metadata,
+                    (
+                        "unknown".into(),
+                        "legacy".into(),
+                        1,
+                        None,
+                        None,
+                        "repair_required".into(),
+                        None
+                    )
+                );
+                let retained: (String, String, u32, Option<String>, String) = tx.query_row(
+                    "SELECT id,text,chunk_index,vector_id,\
+                    index_state FROM document_chunks WHERE \
+                    document_id=?1",
+                    [&document],
+                    |row| {
+                        Ok((
+                            row.get(0)?,
+                            row.get(1)?,
+                            row.get(2)?,
+                            row.get(3)?,
+                            row.get(4)?,
+                        ))
+                    },
+                )?;
+                assert_eq!(
+                    retained,
+                    (
+                        chunk.clone(),
+                        "legacy canonical text".into(),
+                        0,
+                        None,
+                        "repair_required".into()
+                    )
+                );
+                let version: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
+                assert_eq!(version, 5);
+                Ok(())
+            })
+            .unwrap();
         assert!(
             !directory.path().join("knowledge").exists(),
             "legacy migration cannot create/copy managed sources"
         );
     }
 }
 
 #[test]
 fn documents_migration_generation_seals_reject_untrusted_json_and_cascade_cleanup() {
     let store = LocalStore::in_memory().unwrap();
-    store.transaction(|tx| {
-        tx.execute("INSERT INTO knowledge_index_generations (identity,profile_id,dimension,active_generation,state) VALUES ('identity',NULL,128,NULL,'repair_required')",[])?;
-        let insert="INSERT INTO knowledge_index_generation_seals (identity,generation,seal_json,dirty,intent_json,updated_at_ms) VALUES ('identity',?1,?2,?3,?4,17)";
-        for (i,(seal,dirty,intent)) in [
-            (None,0,None), (Some("{}"),0,None), (Some("null"),0,None), (Some("{"),0,None),
-            (Some("{\"version\":null}"),0,None), (Some("{\"version\":true}"),0,None), (Some("{\"version\":1.0}"),0,None),
-            (Some("{\"version\":2}"),0,None), (Some("{\"version\":1}"),0,Some("{\"version\":1}")),
-            (None,1,None), (None,1,Some("{}")), (None,1,Some("null")), (None,1,Some("{")),
-            (None,2,Some("{\"version\":1}")),
-        ].into_iter().enumerate() {
-            assert!(tx.execute(insert,params![format!("bad-{i}"),seal,dirty,intent]).is_err(),"invalid seal/intent {i} was accepted");
-        }
-        let large=format!("{{\"version\":1,\"data\":\"{}\"}}","x".repeat(64*1024));
-        assert!(tx.execute(insert,params!["oversize-seal",large,0,None::<String>]).is_err());
-        let large=format!("{{\"version\":1,\"data\":\"{}\"}}","中".repeat(6*1024));
-        assert!(tx.execute(insert,params!["oversize-intent",None::<String>,1,large]).is_err(),"limit counts encoded bytes");
-        tx.execute(insert,params!["fresh",None::<String>,1,"{\"version\":1}"])?;
-        tx.execute(insert,params!["clean","{\"version\":1}",0,None::<String>])?;
-        tx.execute(insert,params!["dirty","{\"version\":1}",1,"{\"version\":1}"])?;
-        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_index_generation_seals",[],|row|row.get::<_,u32>(0))?,3);
-        tx.execute("DELETE FROM knowledge_index_generations WHERE identity='identity'",[])?;
-        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_index_generation_seals",[],|row|row.get::<_,u32>(0))?,0);
-        assert!(tx.execute(insert,params!["orphan",None::<String>,1,"{\"version\":1}"]).is_err());
-        Ok(())
-    }).unwrap();
+    store
+        .transaction(|tx| {
+            tx.execute(
+                "INSERT INTO knowledge_index_generations \
+                    (identity,profile_id,dimension,\
+                    active_generation,state) VALUES ('identity',\
+                    NULL,128,NULL,'repair_required')",
+                [],
+            )?;
+            let insert = "INSERT INTO knowledge_index_generation_seals \
+                    (identity,generation,seal_json,dirty,\
+                    intent_json,updated_at_ms) VALUES \
+                    ('identity',?1,?2,?3,?4,17)";
+            for (i, (seal, dirty, intent)) in [
+                (None, 0, None),
+                (Some("{}"), 0, None),
+                (Some("null"), 0, None),
+                (Some("{"), 0, None),
+                (Some("{\"version\":null}"), 0, None),
+                (Some("{\"version\":true}"), 0, None),
+                (Some("{\"version\":1.0}"), 0, None),
+                (Some("{\"version\":2}"), 0, None),
+                (Some("{\"version\":1}"), 0, Some("{\"version\":1}")),
+                (None, 1, None),
+                (None, 1, Some("{}")),
+                (None, 1, Some("null")),
+                (None, 1, Some("{")),
+                (None, 2, Some("{\"version\":1}")),
+            ]
+            .into_iter()
+            .enumerate()
+            {
+                assert!(
+                    tx.execute(insert, params![format!("bad-{i}"), seal, dirty, intent])
+                        .is_err(),
+                    "invalid seal/intent {i} was accepted"
+                );
+            }
+            let large = format!("{{\"version\":1,\"data\":\"{}\"}}", "x".repeat(64 * 1024));
+            assert!(
+                tx.execute(insert, params!["oversize-seal", large, 0, None::<String>])
+                    .is_err()
+            );
+            let large = format!("{{\"version\":1,\"data\":\"{}\"}}", "中".repeat(6 * 1024));
+            assert!(
+                tx.execute(insert, params!["oversize-intent", None::<String>, 1, large])
+                    .is_err(),
+                "limit counts encoded bytes"
+            );
+            tx.execute(
+                insert,
+                params!["fresh", None::<String>, 1, "{\"version\":1}"],
+            )?;
+            tx.execute(
+                insert,
+                params!["clean", "{\"version\":1}", 0, None::<String>],
+            )?;
+            tx.execute(
+                insert,
+                params!["dirty", "{\"version\":1}", 1, "{\"version\":1}"],
+            )?;
+            assert_eq!(
+                tx.query_row(
+                    "SELECT count(*) FROM knowledge_index_generation_seals",
+                    [],
+                    |row| row.get::<_, u32>(0)
+                )?,
+                3
+            );
+            tx.execute(
+                "DELETE FROM knowledge_index_generations WHERE identity='identity'",
+                [],
+            )?;
+            assert_eq!(
+                tx.query_row(
+                    "SELECT count(*) FROM knowledge_index_generation_seals",
+                    [],
+                    |row| row.get::<_, u32>(0)
+                )?,
+                0
+            );
+            assert!(
+                tx.execute(
+                    insert,
+                    params!["orphan", None::<String>, 1, "{\"version\":1}"]
+                )
+                .is_err()
+            );
+            Ok(())
+        })
+        .unwrap();
 }
 
 #[test]
 fn documents_projection_pages_legacy_rows_without_rewriting_nullable_metadata() {
     use super::{
         store,
         types::{DocumentKind, DocumentStatus},
     };
     use crate::intelligence::types::RecordOrigin;
     let local = LocalStore::in_memory().unwrap();
     let ids: Vec<_> = (0..3).map(|_| Uuid::new_v4()).collect();
-    local.transaction(|tx| {
-        for id in &ids {
-            tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'original-hash','legacy-reference',NULL,37)",[id.to_string()])?;
-        }
-        tx.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text) VALUES (?1,?2,0,'retained')",params![Uuid::new_v4().to_string(),ids[0].to_string()])?;
-        Ok(())
-    }).unwrap();
+    local
+        .transaction(|tx| {
+            for id in &ids {
+                tx.execute(
+                    "INSERT INTO documents (id,sha256,uri,title,\
+                    created_at) VALUES (?1,'original-hash',\
+                    'legacy-reference',NULL,37)",
+                    [id.to_string()],
+                )?;
+            }
+            tx.execute(
+                "INSERT INTO document_chunks (id,document_id,\
+                    chunk_index,text) VALUES (?1,?2,0,'retained')",
+                params![Uuid::new_v4().to_string(), ids[0].to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
     let detail = store::get(&local, ids[0])
         .unwrap()
         .expect("legacy row must remain visible");
     assert_eq!(detail.summary.title, None);
     assert_eq!(detail.summary.updated_at_ms, None);
     assert_eq!(detail.summary.created_at_ms, 37);
     assert_eq!(detail.summary.chunk_count, 1);
     assert_eq!(detail.summary.origin, RecordOrigin::Legacy);
     assert_eq!(detail.summary.kind, DocumentKind::Unknown);
     assert_eq!(detail.summary.status, DocumentStatus::RepairRequired);
@@ -131,21 +344,29 @@ fn documents_migration4_failure_rolls_back_added_columns_and_preserves_legacy_ro
     connection
         .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
         .unwrap();
     connection
         .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
         .unwrap();
     connection
         .execute_batch(include_str!("../../migrations/0003_intelligence.sql"))
         .unwrap();
     connection.pragma_update(None, "user_version", 3).unwrap();
-    connection.execute_batch("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES ('retained','hash','legacy-reference',NULL,37); CREATE TABLE knowledge_index_generation_seals (conflict TEXT);").unwrap();
+    connection
+        .execute_batch(
+            "INSERT INTO documents (id,sha256,uri,title,\
+                    created_at) VALUES ('retained','hash',\
+                    'legacy-reference',NULL,37); CREATE TABLE \
+                    knowledge_index_generation_seals (conflict \
+                    TEXT);",
+        )
+        .unwrap();
     assert!(LocalStore::open(directory.path()).is_err());
     assert_eq!(
         connection
             .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
             .unwrap(),
         3
     );
     assert!(connection.prepare("SELECT kind FROM documents").is_err());
     assert!(
         connection
diff --git a/src-tauri/src/knowledge/documents.rs b/src-tauri/src/knowledge/documents.rs
index 2029d37..6f5bea0 100644
--- a/src-tauri/src/knowledge/documents.rs
+++ b/src-tauri/src/knowledge/documents.rs
@@ -12,20 +12,32 @@ use crate::{
         jobs::{JobHandler, JobScheduler, OperationLease},
         types::{IntelligenceErrorCode as Code, JobKind, JobView},
     },
     storage::LocalStore,
 };
 use std::{path::PathBuf, sync::Arc, time::Duration};
 use tokio::sync::{Semaphore, oneshot, watch};
 use uuid::Uuid;
 mod admission;
 mod handler;
+pub(crate) async fn cleanup_managed(
+    store: Arc<LocalStore>,
+    data_dir: PathBuf,
+    owner: OperationLease,
+) -> AppResult<()> {
+    tokio::task::spawn_blocking(move || {
+        let _owner = owner;
+        handler::cleanup_rows(&store, &data_dir, None, false)
+    })
+    .await
+    .map_err(|_| error(Code::Interrupted))?
+}
 #[derive(Clone)]
 pub struct DocumentService {
     store: Arc<LocalStore>,
     _config: Arc<ServiceConfigStore>,
     jobs: Arc<JobScheduler>,
     parser: Arc<dyn DocumentParser>,
     data_dir: PathBuf,
     pub(crate) copier: Arc<dyn FileCopier>,
     file_gate: Arc<Semaphore>,
     pub(crate) observation_timeout: Duration,
@@ -127,25 +139,30 @@ impl DocumentService {
                 cancel: cancelled.clone(),
             };
             let blocking_lease = _lease.clone();
             let blocking_permit = _permit.clone();
             let mut work = tokio::task::spawn_blocking(move || {
                 let _lease = blocking_lease;
                 let _permit = blocking_permit;
                 copier.copy(request)
             });
             let mut observer_cancel = cancelled.clone();
-            let copied=tokio::select! {biased;
-                _=crate::intelligence::http::cancelled(&mut stop)=>{cancel.send_replace(true);work.await},
+            let copied=tokio::select! {
+biased;
+                _=crate::intelligence::http::cancelled(&mut stop)=>{
+cancel.send_replace(true);
+work.await}
+,
                 _=crate::intelligence::http::cancelled(&mut observer_cancel)=>work.await,
                 value=&mut work=>value,
-            }.map_err(|_|error(Code::Interrupted)).and_then(|value|value);
+            }
+.map_err(|_|error(Code::Interrupted)).and_then(|value|value);
             let candidate_owned = copied.is_ok();
             let admitted = match copied {
                 Ok(copy) if !*cancelled.borrow() && !*stop.borrow() => admission::admit(
                     &owner.store,
                     &fence,
                     replacement,
                     file_id,
                     &copy,
                     &cancelled,
                 ),
@@ -153,21 +170,35 @@ impl DocumentService {
                 Err(error) => Err(error),
             };
             let result = match admitted {
                 Ok((job, true)) => {
                     owner.jobs.event_port().publish(job.clone());
                     owner.jobs.wake();
                     Ok(job)
                 }
                 other => {
                     let cleaned = if candidate_owned {
-                        owner.store.transaction(|tx|{tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE file_id=?1 AND state='copying'",[file_id.to_string()])?;Ok(())}).map_err(safe).and_then(|_|copying::cleanup_candidate(&owner.store,&owner.data_dir,file_id))
+                        owner
+                            .store
+                            .transaction(|tx| {
+                                tx.execute(
+                                    "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE file_id=?1 AND \
+                    state='copying'",
+                                    [file_id.to_string()],
+                                )?;
+                                Ok(())
+                            })
+                            .map_err(safe)
+                            .and_then(|_| {
+                                copying::cleanup_candidate(&owner.store, &owner.data_dir, file_id)
+                            })
                     } else {
                         Ok(())
                     };
                     match cleaned {
                         Err(e) => Err(e),
                         Ok(()) => other.map(|(job, _)| job),
                     }
                 }
             };
             let _ = reply.send(result);
diff --git a/src-tauri/src/knowledge/documents/handler.rs b/src-tauri/src/knowledge/documents/handler.rs
index fead151..6d41718 100644
--- a/src-tauri/src/knowledge/documents/handler.rs
+++ b/src-tauri/src/knowledge/documents/handler.rs
@@ -27,70 +27,152 @@ fn code(error: crate::error::AppError) -> Code {
         _ => Code::Storage,
     }
 }
 impl ImportHandler {
     fn owns(
         &self,
         tx: &rusqlite::Transaction<'_>,
         context: &JobContext,
         input: &Input,
     ) -> AppResult<()> {
-        let owns:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM documents d JOIN knowledge_document_imports i ON i.document_id=d.id AND i.document_version=d.version WHERE d.id=?1 AND d.version=?2 AND d.import_job_id=?3 AND d.parser_identity=?4 AND d.status='parsing' AND i.job_id=d.import_job_id AND i.parser_identity=d.parser_identity AND i.sha256=d.sha256)",params![input.document_id.to_string(),input.document_version,context.id.to_string(),PARSER_ID],|r|r.get(0))?;
+        let owns: bool = tx.query_row(
+            "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
+                    knowledge_document_imports i ON \
+                    i.document_id=d.id AND \
+                    i.document_version=d.version WHERE d.id=?1 \
+                    AND d.version=?2 AND d.import_job_id=?3 AND \
+                    d.parser_identity=?4 AND d.status='parsing' \
+                    AND i.job_id=d.import_job_id AND \
+                    i.parser_identity=d.parser_identity AND \
+                    i.sha256=d.sha256)",
+            params![
+                input.document_id.to_string(),
+                input.document_version,
+                context.id.to_string(),
+                PARSER_ID
+            ],
+            |r| r.get(0),
+        )?;
         if owns {
             Ok(())
         } else {
             Err(error(Code::Cancelled))
         }
     }
     fn progress(&self, context: &JobContext, input: &Input, total: u32) -> AppResult<()> {
-        let job=self.store.transaction(|tx|context.commit(tx,||{
-            self.owns(tx,context,input)?;
-            tx.execute("UPDATE intelligence_jobs SET total_units=?1,updated_at_ms=?2 WHERE id=?3 AND status='running' AND completed_units=0",params![total,crate::intelligence::jobs::now_ms(),context.id.to_string()])?;
-            job_in_transaction(tx,context.id)?.ok_or_else(||error(Code::Storage))
-        })).map_err(safe)?;
+        let job = self
+            .store
+            .transaction(|tx| {
+                context.commit(tx, || {
+                    self.owns(tx, context, input)?;
+                    tx.execute(
+                        "UPDATE intelligence_jobs SET total_units=?1,\
+                    updated_at_ms=?2 WHERE id=?3 AND \
+                    status='running' AND completed_units=0",
+                        params![
+                            total,
+                            crate::intelligence::jobs::now_ms(),
+                            context.id.to_string()
+                        ],
+                    )?;
+                    job_in_transaction(tx, context.id)?.ok_or_else(|| error(Code::Storage))
+                })
+            })
+            .map_err(safe)?;
         self.events.publish(job);
         Ok(())
     }
     fn chunks(&self, context: &JobContext, input: &Input, chunks: Vec<TextBlock>) -> AppResult<()> {
-        let job=self.store.transaction(|tx|context.commit(tx,||{
-            self.owns(tx,context,input)?;
-            tx.execute("DELETE FROM document_chunks WHERE document_id=?1",[input.document_id.to_string()])?;
-            let mut statement=tx.prepare("INSERT INTO document_chunks (id,document_id,chunk_index,text,location_json,content_hash,document_version,index_state) VALUES (?1,?2,?3,?4,?5,?6,?7,'parsed')")?;
-            for (index,chunk) in chunks.iter().enumerate(){statement.execute(params![Uuid::new_v4().to_string(),input.document_id.to_string(),index as u32,chunk.text,serde_json::to_string(&chunk.location).map_err(|_|error(Code::Storage))?,chunk_hash(&chunk.text),input.document_version])?;}
-            tx.execute("UPDATE documents SET status='parsed',error_code=NULL,index_identity=NULL,updated_at_ms=?1 WHERE id=?2",params![crate::intelligence::jobs::now_ms(),input.document_id.to_string()])?;
-            tx.execute("DELETE FROM knowledge_repairs WHERE document_id=?1",[input.document_id.to_string()])?;
-            tx.execute("UPDATE knowledge_managed_files SET state='retained' WHERE job_id=?1 AND state='copying'",[context.id.to_string()])?;
-            tx.execute("UPDATE intelligence_jobs SET completed_units=?1,total_units=?1,updated_at_ms=?2 WHERE id=?3 AND status='running'",params![chunks.len() as u32,crate::intelligence::jobs::now_ms(),context.id.to_string()])?;
-            job_in_transaction(tx,context.id)?.ok_or_else(||error(Code::Storage))
-        })).map_err(safe)?;
+        let job = self
+            .store
+            .transaction(|tx| {
+                context.commit(tx, || {
+                    self.owns(tx, context, input)?;
+                    tx.execute(
+                        "DELETE FROM document_chunks WHERE document_id=?1",
+                        [input.document_id.to_string()],
+                    )?;
+                    let mut statement = tx.prepare(
+                        "INSERT INTO document_chunks (id,document_id,\
+                    chunk_index,text,location_json,content_hash,\
+                    document_version,index_state) VALUES (?1,?2,\
+                    ?3,?4,?5,?6,?7,'parsed')",
+                    )?;
+                    for (index, chunk) in chunks.iter().enumerate() {
+                        statement.execute(params![
+                            Uuid::new_v4().to_string(),
+                            input.document_id.to_string(),
+                            index as u32,
+                            chunk.text,
+                            serde_json::to_string(&chunk.location)
+                                .map_err(|_| error(Code::Storage))?,
+                            chunk_hash(&chunk.text),
+                            input.document_version
+                        ])?;
+                    }
+                    tx.execute(
+                        "UPDATE documents SET status='parsed',\
+                    error_code=NULL,index_identity=NULL,\
+                    updated_at_ms=?1 WHERE id=?2",
+                        params![
+                            crate::intelligence::jobs::now_ms(),
+                            input.document_id.to_string()
+                        ],
+                    )?;
+                    tx.execute(
+                        "DELETE FROM knowledge_repairs WHERE document_id=?1",
+                        [input.document_id.to_string()],
+                    )?;
+                    tx.execute(
+                        "UPDATE knowledge_managed_files SET \
+                    state='retained' WHERE job_id=?1 AND \
+                    state='copying'",
+                        [context.id.to_string()],
+                    )?;
+                    tx.execute(
+                        "UPDATE intelligence_jobs SET \
+                    completed_units=?1,total_units=?1,\
+                    updated_at_ms=?2 WHERE id=?3 AND \
+                    status='running'",
+                        params![
+                            chunks.len() as u32,
+                            crate::intelligence::jobs::now_ms(),
+                            context.id.to_string()
+                        ],
+                    )?;
+                    job_in_transaction(tx, context.id)?.ok_or_else(|| error(Code::Storage))
+                })
+            })
+            .map_err(safe)?;
         self.events.publish(job);
         Ok(())
     }
 }
 #[async_trait::async_trait]
 impl JobHandler for ImportHandler {
     async fn run(&self, mut context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
         let envelope = crate::intelligence::store::decode_payload(
             &serde_json::to_string(&payload).map_err(|_| Code::ParseFailed)?,
         )
         .map_err(code)?;
         if !envelope.profiles.is_empty() {
             return Err(Code::Configuration);
         }
         let input: Input = serde_json::from_value(envelope.input).map_err(|_| Code::ParseFailed)?;
         if input.document_version == 0 || input.parser_identity != PARSER_ID {
             return Err(Code::ParseFailed);
         }
-        let _permit = tokio::select! {biased;
-            _=crate::intelligence::http::cancelled(&mut context.cancel)=>return Err(Code::Cancelled),
-            permit=self.gate.clone().acquire_owned()=>permit.map_err(|_|Code::Interrupted)?,
-        };
+        let _permit = tokio::select! {
+        biased;
+                    _=crate::intelligence::http::cancelled(&mut context.cancel)=>return Err(Code::Cancelled),
+                    permit=self.gate.clone().acquire_owned()=>permit.map_err(|_|Code::Interrupted)?,
+                };
         self.store
             .transaction(|tx| context.commit(tx, || self.owns(tx, &context, &input)))
             .map_err(code)?;
         let detail = store::get(&self.store, input.document_id)
             .map_err(code)?
             .ok_or(Code::Storage)?;
         let path = detail.managed_path.ok_or(Code::UnsupportedDocument)?;
         let blocks = self
             .parser
             .parse(path, detail.summary.kind, context.cancel.clone())
@@ -108,23 +190,39 @@ impl JobHandler for ImportHandler {
         &self,
         tx: &rusqlite::Transaction<'_>,
         id: Uuid,
         status: JobStatus,
         code: Option<Code>,
         awaiting_reap: bool,
     ) -> AppResult<()> {
         if status == JobStatus::Succeeded {
             return Ok(());
         }
-        tx.execute("UPDATE documents SET status='failed',error_code=?1,managed_path=NULL,index_identity=NULL,updated_at_ms=?2 WHERE import_job_id=?3 AND status='parsing' AND EXISTS(SELECT 1 FROM knowledge_document_imports i WHERE i.document_id=documents.id AND i.document_version=documents.version AND i.job_id=?3)",params![code.map(Code::as_str),crate::intelligence::jobs::now_ms(),id.to_string()])?;
         tx.execute(
-            "UPDATE knowledge_managed_files SET state=?1 WHERE job_id=?2 AND state='copying'",
+            "UPDATE documents SET status='failed',\
+                    error_code=?1,managed_path=NULL,\
+                    index_identity=NULL,updated_at_ms=?2 WHERE \
+                    import_job_id=?3 AND status='parsing' AND \
+                    EXISTS(SELECT 1 FROM \
+                    knowledge_document_imports i WHERE \
+                    i.document_id=documents.id AND \
+                    i.document_version=documents.version AND \
+                    i.job_id=?3)",
+            params![
+                code.map(Code::as_str),
+                crate::intelligence::jobs::now_ms(),
+                id.to_string()
+            ],
+        )?;
+        tx.execute(
+            "UPDATE knowledge_managed_files SET state=?1 \
+                    WHERE job_id=?2 AND state='copying'",
             params![
                 if awaiting_reap {
                     "awaiting_reap"
                 } else {
                     "delete_pending"
                 },
                 id.to_string()
             ],
         )?;
         Ok(())
@@ -137,53 +235,143 @@ impl JobHandler for ImportHandler {
             .map_err(|_| error(Code::Interrupted))?
     }
     async fn cleanup_after_stop(&self) -> AppResult<()> {
         let store = self.store.clone();
         let data_dir = self.data_dir.clone();
         tokio::task::spawn_blocking(move || cleanup_rows(&store, &data_dir, None, true))
             .await
             .map_err(|_| error(Code::Interrupted))?
     }
     fn reconcile_startup(&self) -> AppResult<()> {
-        self.store.transaction(|tx|{
-            // Older terminal mutations may not have called the domain hook. Match
-            // only the current version/job association; never replay its handler.
-            let terminal=tx.prepare("SELECT j.id,j.status,j.error_code FROM documents d JOIN knowledge_document_imports i ON i.document_id=d.id AND i.document_version=d.version AND i.job_id=d.import_job_id JOIN intelligence_jobs j ON j.id=i.job_id WHERE d.status='parsing' AND j.status IN ('failed','cancelled','interrupted') LIMIT 33")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
-            if terminal.len()>32{return Err(error(Code::ResourceLimit));}
-            for (id,status,raw_code) in terminal {
-                let id=Uuid::parse_str(&id).map_err(|_|error(Code::Storage))?;
-                let status:JobStatus=serde_json::from_value(serde_json::Value::String(status)).map_err(|_|error(Code::Storage))?;
-                let code=raw_code.map(|c|serde_json::from_value(serde_json::Value::String(c)).map_err(|_|error(Code::Storage))).transpose()?;
-                self.terminal_tx(tx,id,status,code,false)?;
-            }
-            tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE state IN ('copying','awaiting_reap')",[])?;Ok(())
-        }).map_err(safe)?;
+        self.store
+            .transaction(|tx| {
+                // Older terminal mutations may not have called the domain hook. Match
+                // only the current version/job association; never replay its handler.
+                let terminal = tx
+                    .prepare(
+                        "SELECT j.id,j.status,j.error_code FROM \
+                    documents d JOIN knowledge_document_imports \
+                    i ON i.document_id=d.id AND \
+                    i.document_version=d.version AND \
+                    i.job_id=d.import_job_id JOIN \
+                    intelligence_jobs j ON j.id=i.job_id WHERE \
+                    d.status='parsing' AND j.status IN ('failed',\
+                    'cancelled','interrupted') LIMIT 33",
+                    )?
+                    .query_map([], |r| {
+                        Ok((
+                            r.get::<_, String>(0)?,
+                            r.get::<_, String>(1)?,
+                            r.get::<_, Option<String>>(2)?,
+                        ))
+                    })?
+                    .collect::<Result<Vec<_>, _>>()?;
+                if terminal.len() > 32 {
+                    return Err(error(Code::ResourceLimit));
+                }
+                for (id, status, raw_code) in terminal {
+                    let id = Uuid::parse_str(&id).map_err(|_| error(Code::Storage))?;
+                    let status: JobStatus =
+                        serde_json::from_value(serde_json::Value::String(status))
+                            .map_err(|_| error(Code::Storage))?;
+                    let code = raw_code
+                        .map(|c| {
+                            serde_json::from_value(serde_json::Value::String(c))
+                                .map_err(|_| error(Code::Storage))
+                        })
+                        .transpose()?;
+                    self.terminal_tx(tx, id, status, code, false)?;
+                }
+                tx.execute(
+                    "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE state IN \
+                    ('copying','awaiting_reap')",
+                    [],
+                )?;
+                Ok(())
+            })
+            .map_err(safe)?;
         cleanup_rows(&self.store, &self.data_dir, None, true)
     }
 }
-fn cleanup_rows(
+pub(crate) fn cleanup_rows(
     store: &LocalStore,
     data_dir: &std::path::Path,
     reaped: Option<Uuid>,
     all_reaped: bool,
 ) -> AppResult<()> {
-    store.transaction(|tx|{if all_reaped {tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE state='awaiting_reap'",[])?;}else if let Some(id)=reaped {tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE job_id=?1 AND state='awaiting_reap'",[id.to_string()])?;}Ok(())}).map_err(safe)?;
+    store
+        .transaction(|tx| {
+            if all_reaped {
+                tx.execute(
+                    "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE \
+                    state='awaiting_reap'",
+                    [],
+                )?;
+            } else if let Some(id) = reaped {
+                tx.execute(
+                    "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE job_id=?1 AND \
+                    state='awaiting_reap'",
+                    [id.to_string()],
+                )?;
+            }
+            Ok(())
+        })
+        .map_err(safe)?;
     // Each bounded page releases SQLite before filesystem I/O; no full history allocation.
     for _ in 0..1000 {
-        let ids=store.transaction(|tx|Ok(tx.prepare("SELECT file_id FROM knowledge_managed_files WHERE state='delete_pending' ORDER BY created_at_ms,file_id LIMIT 200")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?)).map_err(safe)?;
+        let ids = store
+            .transaction(|tx| {
+                Ok(tx
+                    .prepare(
+                        "SELECT file_id FROM knowledge_managed_files \
+                    WHERE state='delete_pending' ORDER BY \
+                    created_at_ms,file_id LIMIT 200",
+                    )?
+                    .query_map([], |r| r.get::<_, String>(0))?
+                    .collect::<Result<Vec<_>, _>>()?)
+            })
+            .map_err(safe)?;
         if ids.is_empty() {
             return Ok(());
         }
         for raw_id in ids {
             let id = Uuid::parse_str(&raw_id).map_err(|_| error(Code::Storage))?;
             if let Err(failure) = copying::cleanup_candidate(store, data_dir, id) {
-                store.transaction(|tx|{
-                    tx.execute("UPDATE documents SET status='repair_required',error_code='storage',updated_at_ms=?1 WHERE status IN ('failed','repair_required') AND EXISTS(SELECT 1 FROM knowledge_managed_files f WHERE f.file_id=?2 AND f.document_id=documents.id AND f.document_version=documents.version AND f.job_id=documents.import_job_id)",params![crate::intelligence::jobs::now_ms(),raw_id])?;
-                    tx.execute("INSERT INTO knowledge_repairs (document_id,reason) SELECT d.id,'managed_copy_cleanup_failed' FROM documents d JOIN knowledge_managed_files f ON f.document_id=d.id AND f.document_version=d.version AND f.job_id=d.import_job_id WHERE f.file_id=?1 AND d.status='repair_required' ON CONFLICT(document_id) DO UPDATE SET reason=excluded.reason",[raw_id])?;
-                    Ok(())
-                }).map_err(safe)?;
+                store
+                    .transaction(|tx| {
+                        tx.execute(
+                            "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='storage',updated_at_ms=?1 WHERE \
+                    status IN ('failed','repair_required') AND \
+                    EXISTS(SELECT 1 FROM knowledge_managed_files \
+                    f WHERE f.file_id=?2 AND \
+                    f.document_id=documents.id AND \
+                    f.document_version=documents.version AND \
+                    f.job_id=documents.import_job_id)",
+                            params![crate::intelligence::jobs::now_ms(), raw_id],
+                        )?;
+                        tx.execute(
+                            "INSERT INTO knowledge_repairs (document_id,\
+                    reason) SELECT d.id,\
+                    'managed_copy_cleanup_failed' FROM documents \
+                    d JOIN knowledge_managed_files f ON \
+                    f.document_id=d.id AND \
+                    f.document_version=d.version AND \
+                    f.job_id=d.import_job_id WHERE f.file_id=?1 \
+                    AND d.status='repair_required' ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+                            [raw_id],
+                        )?;
+                        Ok(())
+                    })
+                    .map_err(safe)?;
                 return Err(failure);
             }
         }
     }
     Err(error(Code::ResourceLimit))
 }
diff --git a/src-tauri/src/knowledge/embedding.rs b/src-tauri/src/knowledge/embedding.rs
new file mode 100644
index 0000000..e2309da
--- /dev/null
+++ b/src-tauri/src/knowledge/embedding.rs
@@ -0,0 +1,390 @@
+//! Each metered request retains its actual-completion owner.
+use crate::{
+    error::{AppError, AppResult},
+    intelligence::{
+        config::{ServiceConfigStore, endpoint, validate_profile},
+        http::{HttpRetryPolicy, JsonHttp, JsonRequest},
+        jobs::OperationLease,
+        types::{IntelligenceErrorCode as Code, ModelProfile, ServiceKind},
+    },
+};
+use std::{
+    io::{self, Write},
+    sync::Arc,
+    time::Duration,
+};
+use tokio::sync::{Semaphore, watch};
+pub(crate) struct EmbeddingExecution {
+    owner: OperationLease,
+    cancel: watch::Receiver<bool>,
+}
+impl EmbeddingExecution {
+    pub(crate) fn new(owner: OperationLease, cancel: watch::Receiver<bool>) -> Self {
+        Self { owner, cancel }
+    }
+}
+#[async_trait::async_trait]
+pub(crate) trait Embeddings: Send + Sync {
+    async fn embed(
+        &self,
+        profile: &ModelProfile,
+        inputs: &[String],
+        execution: EmbeddingExecution,
+    ) -> AppResult<Vec<Vec<f32>>>;
+}
+pub(crate) struct OpenAiCompatibleEmbeddings {
+    config: Arc<ServiceConfigStore>,
+    http: Arc<dyn JsonHttp>,
+    requests: Arc<Semaphore>,
+}
+impl OpenAiCompatibleEmbeddings {
+    pub(crate) fn new(config: Arc<ServiceConfigStore>, http: Arc<dyn JsonHttp>) -> Self {
+        Self {
+            config,
+            http,
+            requests: Arc::new(Semaphore::new(4)),
+        }
+    }
+}
+pub(crate) fn error(code: Code) -> AppError {
+    AppError::Intelligence(code)
+}
+#[derive(Default)]
+pub(crate) struct JsonCount(pub(crate) usize);
+impl Write for JsonCount {
+    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
+        self.0 = self
+            .0
+            .checked_add(bytes.len())
+            .ok_or_else(|| io::Error::other("JSON budget overflow"))?;
+        Ok(bytes.len())
+    }
+    fn flush(&mut self) -> io::Result<()> {
+        Ok(())
+    }
+}
+pub(crate) fn json_bytes(value: &impl serde::Serialize) -> AppResult<usize> {
+    let mut c = JsonCount::default();
+    serde_json::to_writer(&mut c, value).map_err(|_| error(Code::ResourceLimit))?;
+    Ok(c.0)
+}
+pub(crate) fn request_body(
+    profile: &ModelProfile,
+    inputs: &[String],
+) -> AppResult<serde_json::Value> {
+    if inputs.is_empty() || inputs.len() > 32 {
+        return Err(error(Code::ResourceLimit));
+    }
+    let mut body = serde_json::json!({
+    "model":profile.model.as_ref().ok_or_else(||error(Code::Configuration))?,
+    "input":inputs,
+    "encoding_format":"float"}
+    );
+    if let Some(d) = profile.dimensions {
+        body["dimensions"] = d.into();
+    }
+    if json_bytes(&body)? > 128 * 1024 {
+        return Err(error(Code::ResourceLimit));
+    }
+    Ok(body)
+}
+#[async_trait::async_trait]
+impl Embeddings for OpenAiCompatibleEmbeddings {
+    async fn embed(
+        &self,
+        profile: &ModelProfile,
+        inputs: &[String],
+        execution: EmbeddingExecution,
+    ) -> AppResult<Vec<Vec<f32>>> {
+        validate_profile(profile)?;
+        if profile.kind != ServiceKind::Embedding {
+            return Err(error(Code::Configuration));
+        }
+        let body = request_body(profile, inputs)?;
+        let mut cancel = execution.cancel.clone();
+        let _permit = tokio::select! {
+        biased;
+                _=crate::intelligence::http::cancelled(&mut cancel)=>return Err(error(Code::Cancelled)),
+        p=self.requests.clone().acquire_owned()=>p.map_err(|_|error(Code::Interrupted))?};
+        let bearer = self
+            .config
+            .scoped(execution.owner.clone())
+            .resolve_snapshot_credentials(profile.clone())
+            .await
+            .map_err(|_| error(Code::Configuration))?;
+        let response = self
+            .http
+            .post(
+                JsonRequest {
+                    endpoint: endpoint(profile)?,
+                    bearer,
+                    body,
+                    timeout: Duration::from_millis(u64::from(profile.timeout_ms)),
+                    max_response_bytes: 8 * 1024 * 1024,
+                    retry_policy: HttpRetryPolicy::OnceTransient,
+                },
+                execution.cancel.clone(),
+            )
+            .await
+            .map_err(error)?;
+        if response.body.len() > 8 * 1024 * 1024 {
+            return Err(error(Code::ResponseTooLarge));
+        }
+        if !(200..300).contains(&response.status) {
+            return Err(error(match response.status {
+                401 | 403 => Code::Authentication,
+                429 => Code::RateLimited,
+                _ => Code::Network,
+            }));
+        }
+        let invalid = || error(Code::InvalidResponse);
+        let value: serde_json::Value =
+            serde_json::from_slice(&response.body).map_err(|_| invalid())?;
+        let data = value["data"]
+            .as_array()
+            .filter(|d| d.len() == inputs.len())
+            .ok_or_else(invalid)?;
+        let mut vectors = vec![None; inputs.len()];
+        let mut dimension = None;
+        for row in data {
+            let index = row["index"]
+                .as_u64()
+                .and_then(|i| usize::try_from(i).ok())
+                .filter(|i| *i < inputs.len())
+                .ok_or_else(invalid)?;
+            if vectors[index].is_some() {
+                return Err(invalid());
+            }
+            let values = row["embedding"]
+                .as_array()
+                .filter(|v| (128..=4096).contains(&v.len()))
+                .ok_or_else(invalid)?;
+            if profile
+                .dimensions
+                .is_some_and(|d| d as usize != values.len())
+                || dimension.is_some_and(|d| d != values.len())
+            {
+                return Err(invalid());
+            }
+            dimension = Some(values.len());
+            let mut vector = Vec::with_capacity(values.len());
+            let mut norm = 0.0f64;
+            for v in values {
+                let f = v.as_f64().ok_or_else(invalid)? as f32;
+                if !f.is_finite() {
+                    return Err(invalid());
+                }
+                norm += f64::from(f) * f64::from(f);
+                vector.push(f);
+            }
+            if !norm.is_finite() || norm <= 0.0 {
+                return Err(invalid());
+            }
+            let norm = norm.sqrt();
+            for v in &mut vector {
+                *v = (f64::from(*v) / norm) as f32;
+            }
+            if !vector.iter().all(|v| v.is_finite()) || !vector.iter().any(|v| *v != 0.0) {
+                return Err(invalid());
+            }
+            vectors[index] = Some(vector);
+        }
+        vectors.into_iter().map(|v| v.ok_or_else(invalid)).collect()
+    }
+}
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use crate::{
+        app::CredentialChange,
+        intelligence::{test_support::*, types::*},
+    };
+    fn profile() -> ModelProfile {
+        ModelProfile {
+            kind: ServiceKind::Embedding,
+            max_context_tokens: None,
+            max_output_tokens: None,
+            dimensions: None,
+            ..test_llm_profile()
+        }
+    }
+    #[tokio::test]
+    async fn knowledge_embeddings_restore_response_order_and_normalize_full_precision() {
+        let h = IntelligenceHarness::new();
+        let p = profile();
+        h.config
+            .save(SaveModelProfile {
+                profile: p.clone(),
+                credential: CredentialChange::Replace {
+                    api_key: "synthetic-embedding-key".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let mut a = vec![0.0; 128];
+        a[0] = 3.0;
+        a[1] = 4.0;
+        let mut b = vec![0.0; 128];
+        b[2] = 2.0;
+        h.http
+            .responses
+            .lock()
+            .unwrap()
+            .push_back(Ok(serde_json::json!({
+            "data":[{
+            "index":1,
+            "embedding":b}
+            ,
+            {
+            "index":0,
+            "embedding":a}
+            ]}
+            )));
+        let transport = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
+        let (_cancel, rx) = tokio::sync::watch::channel(false);
+        let vectors = transport
+            .embed(
+                &p,
+                &["first".into(), "second".into()],
+                EmbeddingExecution::new(std::sync::Arc::new(()), rx),
+            )
+            .await
+            .unwrap();
+        assert_eq!(&vectors[0][0..2], &[0.6, 0.8]);
+        assert_eq!(vectors[1][2], 1.0);
+        let calls = h.http.requests.lock().unwrap();
+        assert_eq!(
+            calls[0].body,
+            serde_json::json!({
+            "model":"fixture-model",
+            "input":["first",
+            "second"],
+            "encoding_format":"float"}
+            )
+        );
+        assert!(calls[0].retry == crate::intelligence::http::HttpRetryPolicy::OnceTransient);
+    }
+    #[tokio::test]
+    async fn knowledge_embeddings_reject_snapshot_edits_before_http_and_invalid_indexes() {
+        let h = IntelligenceHarness::new();
+        let p = profile();
+        h.config
+            .save(SaveModelProfile {
+                profile: p.clone(),
+                credential: CredentialChange::Replace {
+                    api_key: "synthetic-embedding-key".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
+        let mut edited = p.clone();
+        edited.timeout_ms = 2000;
+        let (_c, rx) = tokio::sync::watch::channel(false);
+        assert!(matches!(
+            t.embed(
+                &edited,
+                &["x".into()],
+                EmbeddingExecution::new(std::sync::Arc::new(()), rx)
+            )
+            .await,
+            Err(crate::error::AppError::Intelligence(
+                IntelligenceErrorCode::Configuration
+            ))
+        ));
+        assert!(h.http.requests.lock().unwrap().is_empty());
+        for data in [
+            serde_json::json!([]),
+            serde_json::json!([{
+            "index":1,
+            "embedding":vec![1.0;
+            128]}
+            ]),
+            serde_json::json!([{
+            "index":0,
+            "embedding":vec![0.0;
+            128]}
+            ]),
+            serde_json::json!([{
+            "index":0,
+            "embedding":vec![1.0;
+            127]}
+            ]),
+            serde_json::json!([{
+            "index":0,
+            "embedding":vec![1.0;
+            128]}
+            ,
+            {
+            "index":0,
+            "embedding":vec![1.0;
+            128]}
+            ]),
+        ] {
+            h.http
+                .responses
+                .lock()
+                .unwrap()
+                .push_back(Ok(serde_json::json!({
+                "data":data}
+                )));
+            let (_c, rx) = tokio::sync::watch::channel(false);
+            assert!(
+                t.embed(
+                    &p,
+                    &["x".into()],
+                    EmbeddingExecution::new(std::sync::Arc::new(()), rx)
+                )
+                .await
+                .is_err()
+            );
+        }
+    }
+    #[test]
+    fn knowledge_embedding_semantics_bind_endpoint_profile_model_without_actual_dimension() {
+        let p = profile();
+        let a = crate::knowledge::identity::EmbeddingIdentity::from_profile(&p, 128).unwrap();
+        let b = crate::knowledge::identity::EmbeddingIdentity::from_profile(&p, 256).unwrap();
+        assert_eq!(a.semantic_key().unwrap(), b.semantic_key().unwrap());
+        assert_ne!(a.key().unwrap(), b.key().unwrap());
+        let mut other = p;
+        other.base_url = "http://127.0.0.1:23456/v1".into();
+        assert_ne!(
+            a.semantic_key().unwrap(),
+            crate::knowledge::identity::EmbeddingIdentity::from_profile(&other, 128)
+                .unwrap()
+                .semantic_key()
+                .unwrap()
+        );
+    }
+    #[tokio::test]
+    async fn knowledge_embeddings_enforce_json_bytes_and_text_count_before_http() {
+        let h = IntelligenceHarness::new();
+        let p = profile();
+        h.config
+            .save(SaveModelProfile {
+                profile: p.clone(),
+                credential: CredentialChange::Replace {
+                    api_key: "synthetic-embedding-key".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
+        for inputs in [vec![], vec!["x".into(); 33], vec!["\t".repeat(65536)]] {
+            let (_c, rx) = tokio::sync::watch::channel(false);
+            assert!(matches!(
+                t.embed(
+                    &p,
+                    &inputs,
+                    EmbeddingExecution::new(std::sync::Arc::new(()), rx)
+                )
+                .await,
+                Err(crate::error::AppError::Intelligence(
+                    IntelligenceErrorCode::ResourceLimit
+                ))
+            ));
+        }
+        assert!(h.http.requests.lock().unwrap().is_empty());
+    }
+}
diff --git a/src-tauri/src/knowledge/embedding_tests.rs b/src-tauri/src/knowledge/embedding_tests.rs
new file mode 100644
index 0000000..f4421d1
--- /dev/null
+++ b/src-tauri/src/knowledge/embedding_tests.rs
@@ -0,0 +1,275 @@
+use super::embedding::*;
+use crate::{
+    app::CredentialChange,
+    intelligence::{
+        http::{JsonHttp, JsonRequest, JsonResponse},
+        test_support::*,
+        types::*,
+    },
+};
+use std::sync::{
+    Arc,
+    atomic::{AtomicUsize, Ordering},
+};
+fn profile() -> ModelProfile {
+    ModelProfile {
+        kind: ServiceKind::Embedding,
+        max_context_tokens: None,
+        max_output_tokens: None,
+        dimensions: None,
+        ..test_llm_profile()
+    }
+}
+#[tokio::test]
+async fn knowledge_embedding_rejects_dimension_inconsistency_duplicate_missing_nonfinite_and_extremes()
+ {
+    let h = IntelligenceHarness::new();
+    let p = profile();
+    h.config
+        .save(SaveModelProfile {
+            profile: p.clone(),
+            credential: CredentialChange::Replace {
+                api_key: "synthetic".into(),
+            },
+        })
+        .await
+        .unwrap();
+    let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), h.http.clone());
+    for data in [
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![1.;
+        128]}
+        ,
+        {
+        "index":0,
+        "embedding":vec![1.;
+        128]}
+        ]),
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![1.;
+        128]}
+        ]),
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![1.;
+        128]}
+        ,
+        {
+        "index":1,
+        "embedding":vec![1.;
+        256]}
+        ]),
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![1e100;
+        128]}
+        ,
+        {
+        "index":1,
+        "embedding":vec![1.;
+        128]}
+        ]),
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![1.;
+        4097]}
+        ,
+        {
+        "index":1,
+        "embedding":vec![1.;
+        4097]}
+        ]),
+        serde_json::json!([{
+        "index":0,
+        "embedding":vec![0.;
+        128]}
+        ,
+        {
+        "index":1,
+        "embedding":vec![1.;
+        128]}
+        ]),
+    ] {
+        h.http
+            .responses
+            .lock()
+            .unwrap()
+            .push_back(Ok(serde_json::json!({
+            "data":data}
+            )));
+        let (_c, rx) = tokio::sync::watch::channel(false);
+        assert!(matches!(
+            t.embed(
+                &p,
+                &["first".into(), "second".into()],
+                EmbeddingExecution::new(Arc::new(()), rx)
+            )
+            .await,
+            Err(crate::error::AppError::Intelligence(
+                IntelligenceErrorCode::InvalidResponse
+            ))
+        ));
+    }
+    let mut configured = p.clone();
+    configured.dimensions = Some(128);
+    h.config
+        .save(SaveModelProfile {
+            profile: configured.clone(),
+            credential: CredentialChange::Keep,
+        })
+        .await
+        .unwrap();
+    h.http
+        .responses
+        .lock()
+        .unwrap()
+        .push_back(Ok(serde_json::json!({
+        "data":[{
+        "index":0,
+        "embedding":vec![1.;
+        256]}
+        ]}
+        )));
+    let (_c, rx) = tokio::sync::watch::channel(false);
+    assert!(
+        t.embed(
+            &configured,
+            &["first".into()],
+            EmbeddingExecution::new(Arc::new(()), rx)
+        )
+        .await
+        .is_err()
+    );
+}
+struct BlockHttp {
+    entered: AtomicUsize,
+    active: AtomicUsize,
+    max: AtomicUsize,
+    notice: tokio::sync::Notify,
+    release: tokio::sync::Semaphore,
+    oversize: bool,
+}
+#[async_trait::async_trait]
+impl JsonHttp for BlockHttp {
+    async fn post(
+        &self,
+        request: JsonRequest,
+        _cancel: tokio::sync::watch::Receiver<bool>,
+    ) -> Result<JsonResponse, IntelligenceErrorCode> {
+        assert_eq!(request.max_response_bytes, 8 * 1024 * 1024);
+        assert!(matches!(
+            request.retry_policy,
+            crate::intelligence::http::HttpRetryPolicy::OnceTransient
+        ));
+        self.entered.fetch_add(1, Ordering::SeqCst);
+        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
+        self.max.fetch_max(active, Ordering::SeqCst);
+        self.notice.notify_one();
+        let _release = self.release.acquire().await.unwrap();
+        self.active.fetch_sub(1, Ordering::SeqCst);
+        Ok(JsonResponse {
+            status: 200,
+            body: if self.oversize {
+                vec![b' '; 8 * 1024 * 1024 + 1]
+            } else {
+                serde_json::to_vec(&serde_json::json!({
+                "data":[{
+                "index":0,
+                "embedding":vec![1.;
+                128]}
+                ]}
+                ))
+                .unwrap()
+            },
+        })
+    }
+}
+#[tokio::test]
+async fn knowledge_embedding_shared_four_request_gate_is_held_until_http_finishes() {
+    let h = IntelligenceHarness::new();
+    let p = profile();
+    h.config
+        .save(SaveModelProfile {
+            profile: p.clone(),
+            credential: CredentialChange::Replace {
+                api_key: "synthetic".into(),
+            },
+        })
+        .await
+        .unwrap();
+    let http = Arc::new(BlockHttp {
+        entered: AtomicUsize::new(0),
+        active: AtomicUsize::new(0),
+        max: AtomicUsize::new(0),
+        notice: tokio::sync::Notify::new(),
+        release: tokio::sync::Semaphore::new(0),
+        oversize: false,
+    });
+    let t = Arc::new(OpenAiCompatibleEmbeddings::new(
+        h.config.clone(),
+        http.clone(),
+    ));
+    let owner = Arc::new(());
+    let weak = Arc::downgrade(&owner);
+    let mut tasks = Vec::new();
+    for _ in 0..5 {
+        let t = t.clone();
+        let p = p.clone();
+        let owner = owner.clone();
+        tasks.push(tokio::spawn(async move {
+            let (_c, rx) = tokio::sync::watch::channel(false);
+            t.embed(&p, &["x".into()], EmbeddingExecution::new(owner, rx))
+                .await
+        }));
+    }
+    drop(owner);
+    tokio::time::timeout(std::time::Duration::from_secs(3), async {
+        while http.entered.load(Ordering::SeqCst) < 4 {
+            http.notice.notified().await;
+        }
+    })
+    .await
+    .unwrap();
+    assert_eq!(http.entered.load(Ordering::SeqCst), 4);
+    assert!(weak.upgrade().is_some());
+    http.release.add_permits(5);
+    for task in tasks {
+        task.await.unwrap().unwrap();
+    }
+    assert_eq!(http.max.load(Ordering::SeqCst), 4);
+    assert!(weak.upgrade().is_none());
+}
+#[tokio::test]
+async fn knowledge_embedding_eight_mib_response_limit_is_checked_even_for_a_port_ignoring_the_bound()
+ {
+    let h = IntelligenceHarness::new();
+    let p = profile();
+    h.config
+        .save(SaveModelProfile {
+            profile: p.clone(),
+            credential: CredentialChange::Replace {
+                api_key: "synthetic".into(),
+            },
+        })
+        .await
+        .unwrap();
+    let http = Arc::new(BlockHttp {
+        entered: AtomicUsize::new(0),
+        active: AtomicUsize::new(0),
+        max: AtomicUsize::new(0),
+        notice: tokio::sync::Notify::new(),
+        release: tokio::sync::Semaphore::new(1),
+        oversize: true,
+    });
+    let t = OpenAiCompatibleEmbeddings::new(h.config.clone(), http);
+    let (_c, rx) = tokio::sync::watch::channel(false);
+    assert!(matches!(
+        t.embed(&p, &["x".into()], EmbeddingExecution::new(Arc::new(()), rx))
+            .await,
+        Err(crate::error::AppError::Intelligence(
+            IntelligenceErrorCode::ResponseTooLarge
+        ))
+    ));
+}
diff --git a/src-tauri/src/knowledge/identity.rs b/src-tauri/src/knowledge/identity.rs
index 6bdef1f..494896e 100644
--- a/src-tauri/src/knowledge/identity.rs
+++ b/src-tauri/src/knowledge/identity.rs
@@ -12,20 +12,40 @@ use uuid::Uuid;
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
 pub struct EmbeddingIdentity {
     pub profile_id: Uuid,
     pub base_url: String,
     pub model: String,
     pub dimensions: u32,
     pub normalization_version: u32,
 }
 impl EmbeddingIdentity {
+    pub(crate) fn semantic_key(&self) -> AppResult<String> {
+        self.key()?;
+        #[derive(Serialize)]
+        struct Semantic<'a> {
+            profile_id: Uuid,
+            base_url: &'a str,
+            model: &'a str,
+            normalization_version: u32,
+        }
+        let value = Semantic {
+            profile_id: self.profile_id,
+            base_url: &self.base_url,
+            model: &self.model,
+            normalization_version: self.normalization_version,
+        };
+        Ok(format!(
+            "{:x}",
+            Sha256::digest(serde_json::to_vec(&value).map_err(|_| invalid())?)
+        ))
+    }
     pub fn from_profile(profile: &ModelProfile, actual_dimension: u32) -> AppResult<Self> {
         validate_profile(profile)?;
         if profile.kind != ServiceKind::Embedding
             || !(128..=4096).contains(&actual_dimension)
             || profile
                 .dimensions
                 .is_some_and(|dimension| dimension != actual_dimension)
         {
             return Err(invalid());
         }
diff --git a/src-tauri/src/knowledge/index.rs b/src-tauri/src/knowledge/index.rs
index b39cf3a..c685185 100644
--- a/src-tauri/src/knowledge/index.rs
+++ b/src-tauri/src/knowledge/index.rs
@@ -1,20 +1,26 @@
-// The frozen 5B owner ports are exercised by tests now and consumed by 5C.
-#![cfg_attr(not(test), allow(dead_code))]
-
 mod codec;
 mod lifecycle;
 mod owner;
 mod preflight;
+mod publication;
+mod recovery;
 mod redb_header;
 mod repository;
 mod seal;
-#[allow(unused_imports)]
-pub(crate) use codec::ChunkMetadata;
+pub(crate) use codec::{ChunkMetadata, location_digest};
 pub(crate) use lifecycle::IndexOwner;
+pub(crate) use lifecycle::WorkTicket;
+pub(crate) use publication::{DocumentFence, PublicationPlan, ReadyPublication};
+pub(crate) use recovery::RecoveryEvidence;
+pub(crate) use seal::{Generation, Seal};
 
 use crate::{error::AppError, intelligence::types::IntelligenceErrorCode};
 fn repair() -> AppError {
     AppError::Intelligence(IntelligenceErrorCode::IndexRepairRequired)
 }
 #[cfg(test)]
 mod tests;
+
+pub(crate) fn recovery_descriptor(json: &str) -> crate::error::AppResult<Generation> {
+    Ok(seal::Intent::decode(json)?.descriptor)
+}
diff --git a/src-tauri/src/knowledge/index/codec.rs b/src-tauri/src/knowledge/index/codec.rs
index 151a6f7..0e3d79f 100644
--- a/src-tauri/src/knowledge/index/codec.rs
+++ b/src-tauri/src/knowledge/index/codec.rs
@@ -6,25 +6,25 @@ use crate::{
 use serde::{Deserialize, Serialize};
 use sha2::{Digest, Sha256};
 use std::io::{self, Write};
 use uuid::Uuid;
 
 #[derive(Clone, Debug, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
 pub(crate) struct ChunkMetadata {
     schema_version: u32,
     #[serde(deserialize_with = "canonical_uuid")]
-    document_id: Uuid,
-    document_version: u32,
+    pub(crate) document_id: Uuid,
+    pub(crate) document_version: u32,
     #[serde(deserialize_with = "canonical_uuid")]
-    chunk_id: Uuid,
-    content_hash: String,
+    pub(crate) chunk_id: Uuid,
+    pub(crate) content_hash: String,
     location: LocationReference,
 }
 #[derive(Clone, Debug, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
 struct LocationReference {
     kind: String,
     codec: String,
     #[serde(deserialize_with = "required_nullable")]
     page: Option<u32>,
     paragraph: u32,
@@ -75,20 +75,23 @@ impl ChunkMetadata {
         metadata.validate()?;
         Ok(metadata)
     }
     pub(crate) fn matches_location(&self, location: &TextLocation) -> AppResult<bool> {
         Ok(self.location.page == location.page
             && self.location.paragraph == location.paragraph
             && self.location.start_char == location.start_char
             && self.location.end_char == location.end_char
             && self.location.sha256 == location_digest(location)?)
     }
+    pub(crate) fn location_sha256(&self) -> &str {
+        &self.location.sha256
+    }
     fn validate(&self) -> AppResult<()> {
         if self.schema_version != 2
             || self.document_version == 0
             || !is_hash(&self.content_hash)
             || self.location.kind != "sqlite_chunk_location"
             || self.location.codec != "text_location_v1"
             || !is_hash(&self.location.sha256)
             || self.location.end_char < self.location.start_char
         {
             return Err(invalid());
@@ -103,29 +106,31 @@ pub(super) fn is_hash(value: &str) -> bool {
             .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
 }
 fn invalid() -> AppError {
     AppError::Intelligence(Code::IndexRepairRequired)
 }
 fn required_nullable<'de, D: serde::Deserializer<'de>>(
     deserializer: D,
 ) -> Result<Option<u32>, D::Error> {
     Option::deserialize(deserializer)
 }
-fn canonical_uuid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
+pub(super) fn canonical_uuid<'de, D: serde::Deserializer<'de>>(
+    deserializer: D,
+) -> Result<Uuid, D::Error> {
     let raw = String::deserialize(deserializer)?;
     let id = Uuid::parse_str(&raw).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
     if id.to_string() != raw {
         return Err(serde::de::Error::custom("noncanonical UUID"));
     }
     Ok(id)
 }
-fn location_digest(location: &TextLocation) -> AppResult<String> {
+pub(crate) fn location_digest(location: &TextLocation) -> AppResult<String> {
     struct HashWriter(Sha256);
     impl Write for HashWriter {
         fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
             self.0.update(bytes);
             Ok(bytes.len())
         }
         fn flush(&mut self) -> io::Result<()> {
             Ok(())
         }
     }
diff --git a/src-tauri/src/knowledge/index/lifecycle.rs b/src-tauri/src/knowledge/index/lifecycle.rs
index adda194..82ef5c0 100644
--- a/src-tauri/src/knowledge/index/lifecycle.rs
+++ b/src-tauri/src/knowledge/index/lifecycle.rs
@@ -50,21 +50,59 @@ pub(crate) struct Pending<T> {
     result: tokio::sync::oneshot::Receiver<AppResult<Reply>>,
     project: fn(Reply) -> AppResult<T>,
 }
 pub(crate) struct MaintenanceTicket {
     owner: IndexOwner,
     work: WorkTicket,
     token: Uuid,
     active: AtomicBool,
     release_requested: Arc<AtomicBool>,
 }
+pub(crate) struct RecoveryTicket {
+    owner: IndexOwner,
+    work: WorkTicket,
+    token: Uuid,
+    active: AtomicBool,
+    release_requested: Arc<AtomicBool>,
+}
+impl RecoveryTicket {
+    pub(crate) fn token(&self) -> Uuid {
+        self.token
+    }
+}
+impl Drop for RecoveryTicket {
+    fn drop(&mut self) {
+        if self.active.load(Ordering::Acquire) {
+            self.release_requested.store(true, Ordering::Release);
+            let _ =
+                self.owner
+                    .enqueue_admitted(&self.work, Command::EndRecovery(self.token), |r| match r {
+                        Reply::MaintenanceEnded => Ok(()),
+                        _ => Err(error(Code::Interrupted)),
+                    });
+        }
+    }
+}
 impl WorkTicket {
+    pub(crate) fn operation_id(&self) -> Uuid {
+        self.intent.body().operation_id
+    }
+    pub(crate) fn generation(&self) -> &seal::Generation {
+        &self.intent.body().descriptor
+    }
+    pub(crate) fn cleanup(&self) -> Self {
+        Self {
+            intent: self.intent.clone(),
+            lease: self.lease.clone(),
+            cancelled: Arc::new(AtomicBool::new(false)),
+        }
+    }
     pub(crate) fn cancel(&self) {
         self.cancelled.store(true, Ordering::Release);
     }
 }
 impl<T> Pending<T> {
     pub(crate) async fn observe(self) -> AppResult<T> {
         let reply = tokio::time::timeout(STOP_TIMEOUT, self.result)
             .await
             .map_err(|_| error(Code::Timeout))?
             .map_err(|_| error(Code::Interrupted))??;
@@ -89,22 +127,48 @@ impl Drop for MaintenanceTicket {
         if self.active.load(Ordering::Acquire) {
             self.release_requested.store(true, Ordering::Release);
             // The accepted envelope retains the operation token even when no
             // caller remains to observe its release response.
             let _ = self.enqueue_end();
         }
     }
 }
 
 enum Command {
+    BeginRecovery(super::recovery::RecoveryEvidence, Arc<AtomicBool>),
+    CopyRecovery {
+        token: Uuid,
+        start: u64,
+        limit: u32,
+    },
+    VerifyRecovery(Uuid),
+    EndRecovery(Uuid),
+    RetireRecovery(super::recovery::RecoveryFence),
+    RemoveGeneration(seal::Generation),
     Open,
+    PublishPlan(PersistedReceipt, super::publication::PublicationPlan),
+    Retire,
+    Candidates {
+        vector: Vec<f32>,
+        limit: u32,
+    },
+    #[cfg(test)]
+    CorruptMetadata {
+        id: u64,
+        vector: Vec<f32>,
+        metadata: String,
+    },
     Add(Vec<(Vec<f32>, ChunkMetadata)>),
+    ScanRecords {
+        start: u64,
+        limit: u32,
+    },
     Scan {
         start: u64,
         limit: u32,
     },
     Delete(Vec<u64>),
     Replace {
         id: u64,
         vector: Vec<f32>,
         metadata: ChunkMetadata,
     },
@@ -117,23 +181,25 @@ enum Command {
         token: Uuid,
         start: u64,
         limit: u32,
     },
     EndMaintenance(Uuid),
     Persist,
     Publish(PersistedReceipt),
     Acknowledge(PublicationAck),
 }
 enum Reply {
+    Candidates(Vec<super::owner::Candidate>),
     Opened(u64),
     Added(Vec<u64>),
     Scanned(Vec<(u64, Option<ChunkMetadata>)>),
+    Records(Vec<super::owner::RecordCandidate>),
     Deleted(Vec<bool>),
     Replaced(u64),
     Searched(Vec<(u64, f32, ChunkMetadata)>),
     MaintenanceStarted(Uuid),
     VectorsCopied(Vec<(u64, Vec<f32>)>),
     MaintenanceEnded,
     Persisted(PersistedReceipt),
     Published(PublicationAck),
     Acknowledged,
 }
@@ -375,20 +441,35 @@ impl IndexOwner {
         rows: Vec<(Vec<f32>, ChunkMetadata)>,
     ) -> AppResult<Vec<u64>> {
         self.enqueue(ticket, Command::Add(rows), |reply| match reply {
             Reply::Added(ids) => Ok(ids),
             _ => Err(error(Code::Interrupted)),
         })?
         .observe()
         .await
     }
 
+    pub(crate) async fn scan_records(
+        &self,
+        ticket: &WorkTicket,
+        start: u64,
+        limit: u32,
+    ) -> AppResult<Vec<super::owner::RecordCandidate>> {
+        self.enqueue(ticket, Command::ScanRecords { start, limit }, |r| match r {
+            Reply::Records(rows) => Ok(rows),
+            _ => Err(error(Code::Interrupted)),
+        })?
+        .observe()
+        .await
+    }
+    // Retained strict compatibility port; 5C consumes scan_records.
+    #[cfg_attr(not(test), allow(dead_code))]
     pub(crate) async fn scan(
         &self,
         ticket: &WorkTicket,
         start: u64,
         limit: u32,
     ) -> AppResult<Vec<(u64, Option<ChunkMetadata>)>> {
         self.enqueue(
             ticket,
             Command::Scan { start, limit },
             |reply| match reply {
@@ -425,61 +506,188 @@ impl IndexOwner {
             },
             |reply| match reply {
                 Reply::Replaced(id) => Ok(id),
                 _ => Err(error(Code::Interrupted)),
             },
         )?
         .observe()
         .await
     }
 
+    pub(crate) async fn candidates(
+        &self,
+        ticket: &WorkTicket,
+        vector: Vec<f32>,
+        limit: u32,
+    ) -> AppResult<Vec<super::owner::Candidate>> {
+        self.enqueue(
+            ticket,
+            Command::Candidates { vector, limit },
+            |reply| match reply {
+                Reply::Candidates(c) => Ok(c),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?
+        .observe()
+        .await
+    }
+    // Retained strict compatibility port; 5C consumes candidates.
+    #[cfg_attr(not(test), allow(dead_code))]
     pub(crate) async fn search(
         &self,
         ticket: &WorkTicket,
         vector: Vec<f32>,
         limit: u32,
     ) -> AppResult<Vec<(u64, f32, ChunkMetadata)>> {
         self.enqueue(
             ticket,
             Command::Search { vector, limit },
             |reply| match reply {
                 Reply::Searched(hits) => Ok(hits),
                 _ => Err(error(Code::Interrupted)),
             },
         )?
         .observe()
         .await
     }
 
+    pub(crate) fn recovery_evidence(
+        &self,
+        identity: &str,
+        generation: Uuid,
+    ) -> AppResult<super::recovery::RecoveryEvidence> {
+        self.inner
+            .repository
+            .recovery_evidence(identity, generation)
+    }
+    pub(crate) async fn begin_recovery(
+        &self,
+        ticket: &WorkTicket,
+        evidence: super::recovery::RecoveryEvidence,
+    ) -> AppResult<RecoveryTicket> {
+        // Create the drop guard before awaiting entry. A disappeared observer
+        // requests release even if the reader is still being opened in the pool.
+        let mut guard = RecoveryTicket {
+            owner: self.clone(),
+            work: ticket.clone(),
+            token: Uuid::nil(),
+            active: AtomicBool::new(true),
+            release_requested: Arc::new(AtomicBool::new(false)),
+        };
+        guard.token = self
+            .enqueue(
+                ticket,
+                Command::BeginRecovery(evidence, guard.release_requested.clone()),
+                |r| match r {
+                    Reply::MaintenanceStarted(t) => Ok(t),
+                    _ => Err(error(Code::Interrupted)),
+                },
+            )?
+            .observe()
+            .await?;
+        Ok(guard)
+    }
+    pub(crate) async fn copy_recovery(
+        &self,
+        reader: &RecoveryTicket,
+        start: u64,
+        limit: u32,
+    ) -> AppResult<Vec<(u64, Vec<f32>)>> {
+        if !Arc::ptr_eq(&self.inner, &reader.owner.inner) {
+            return Err(repair());
+        }
+        self.enqueue(
+            &reader.work,
+            Command::CopyRecovery {
+                token: reader.token,
+                start,
+                limit,
+            },
+            |r| match r {
+                Reply::VectorsCopied(v) => Ok(v),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?
+        .observe()
+        .await
+    }
+    pub(crate) async fn verify_recovery(&self, reader: &RecoveryTicket) -> AppResult<()> {
+        self.enqueue(
+            &reader.work,
+            Command::VerifyRecovery(reader.token),
+            |r| match r {
+                Reply::MaintenanceEnded => Ok(()),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?
+        .observe()
+        .await
+    }
+    pub(crate) async fn end_recovery(&self, reader: RecoveryTicket) -> AppResult<()> {
+        let pending = self.enqueue_admitted(
+            &reader.work,
+            Command::EndRecovery(reader.token),
+            |r| match r {
+                Reply::MaintenanceEnded => Ok(()),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?;
+        reader.active.store(false, Ordering::Release);
+        pending.observe().await
+    }
+    pub(crate) async fn remove_generation(
+        &self,
+        ticket: &WorkTicket,
+        source: seal::Generation,
+    ) -> AppResult<()> {
+        self.enqueue_admitted(ticket, Command::RemoveGeneration(source), |r| match r {
+            Reply::MaintenanceEnded => Ok(()),
+            _ => Err(error(Code::Interrupted)),
+        })?
+        .observe()
+        .await
+    }
+    pub(crate) async fn retire_recovery(
+        &self,
+        ticket: &WorkTicket,
+        fence: super::recovery::RecoveryFence,
+    ) -> AppResult<()> {
+        self.enqueue_admitted(ticket, Command::RetireRecovery(fence), |r| match r {
+            Reply::MaintenanceEnded => Ok(()),
+            _ => Err(error(Code::Interrupted)),
+        })?
+        .observe()
+        .await
+    }
     pub(crate) async fn begin_maintenance(
         &self,
         ticket: &WorkTicket,
     ) -> AppResult<MaintenanceTicket> {
-        let release_requested = Arc::new(AtomicBool::new(false));
-        let token = self
+        let mut guard = MaintenanceTicket {
+            owner: self.clone(),
+            work: ticket.clone(),
+            token: Uuid::nil(),
+            active: AtomicBool::new(true),
+            release_requested: Arc::new(AtomicBool::new(false)),
+        };
+        guard.token = self
             .enqueue(
                 ticket,
-                Command::BeginMaintenance(release_requested.clone()),
+                Command::BeginMaintenance(guard.release_requested.clone()),
                 |reply| match reply {
                     Reply::MaintenanceStarted(token) => Ok(token),
                     _ => Err(error(Code::Interrupted)),
                 },
             )?
             .observe()
             .await?;
-        Ok(MaintenanceTicket {
-            owner: self.clone(),
-            work: ticket.clone(),
-            token,
-            active: AtomicBool::new(true),
-            release_requested,
-        })
+        Ok(guard)
     }
 
     pub(crate) async fn copy_vectors(
         &self,
         maintenance: &MaintenanceTicket,
         start: u64,
         limit: u32,
     ) -> AppResult<Vec<(u64, Vec<f32>)>> {
         if !Arc::ptr_eq(&self.inner, &maintenance.owner.inner) {
             return Err(repair());
@@ -544,20 +752,77 @@ impl IndexOwner {
             ticket,
             Command::Publish(receipt.clone()),
             |reply| match reply {
                 Reply::Published(ack) => Ok(ack),
                 _ => Err(error(Code::Interrupted)),
             },
         )?
         .observe()
         .await
     }
+    pub(crate) async fn publish_plan(
+        &self,
+        ticket: &WorkTicket,
+        receipt: &PersistedReceipt,
+        plan: super::publication::PublicationPlan,
+    ) -> AppResult<PublicationAck> {
+        if ticket.intent.body() != receipt.intent().body() {
+            return Err(repair());
+        }
+        let body = receipt.intent().body();
+        self.ensure_transition(
+            &(body.identity.clone(), body.descriptor.generation),
+            body.operation_id,
+            body.owner_epoch,
+        )?;
+        self.enqueue(
+            ticket,
+            Command::PublishPlan(receipt.clone(), plan),
+            |reply| match reply {
+                Reply::Published(ack) => Ok(ack),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?
+        .observe()
+        .await
+    }
+    pub(crate) async fn retire(&self, ticket: &WorkTicket) -> AppResult<()> {
+        self.enqueue_admitted(ticket, Command::Retire, |reply| match reply {
+            Reply::Acknowledged => Ok(()),
+            _ => Err(error(Code::Interrupted)),
+        })?
+        .observe()
+        .await
+    }
+    #[cfg(test)]
+    pub(crate) async fn corrupt_metadata(
+        &self,
+        ticket: &WorkTicket,
+        id: u64,
+        vector: Vec<f32>,
+        metadata: String,
+    ) -> AppResult<u64> {
+        self.enqueue(
+            ticket,
+            Command::CorruptMetadata {
+                id,
+                vector,
+                metadata,
+            },
+            |reply| match reply {
+                Reply::Replaced(id) => Ok(id),
+                _ => Err(error(Code::Interrupted)),
+            },
+        )?
+        .observe()
+        .await
+    }
 
     pub(crate) async fn acknowledge(
         &self,
         ticket: &WorkTicket,
         ack: PublicationAck,
     ) -> AppResult<()> {
         if ticket.intent.body() != ack.intent().body() {
             return Err(repair());
         }
         self.inner.repository.validate_ack(&ack)?;
@@ -736,36 +1001,46 @@ impl IndexOwner {
                 #[cfg(test)]
                 Some(self.inner.hooks.clone()),
             );
         }
         Some(observation)
     }
     pub(crate) async fn shutdown(&self) -> AppResult<()> {
         let Some(mut observation) = self.signal_stop(None) else {
             return Ok(());
         };
-        let result = tokio::time::timeout(STOP_TIMEOUT,async {
+        let result = tokio::time::timeout(STOP_TIMEOUT, async {
             loop {
-                let attempt=observation.attempts.borrow_and_update().clone();
-                if attempt.number==observation.attempt {
-                    if let Some(Err(failure))=attempt.result {
+                let attempt = observation.attempts.borrow_and_update().clone();
+                if attempt.number == observation.attempt {
+                    if let Some(Err(failure)) = attempt.result {
                         observation.stop.observed_failure(observation.attempt);
                         return Err(error(failure));
                     }
                 }
-                if let Some(done)=*observation.done.borrow_and_update(){return done.map_err(error);}
+                if let Some(done) = *observation.done.borrow_and_update() {
+                    return done.map_err(error);
+                }
                 tokio::select! {
-                    changed=observation.done.changed()=>if changed.is_err(){return Err(error(Code::Interrupted));},
-                    changed=observation.attempts.changed()=>if changed.is_err(){return Err(error(Code::Interrupted));},
+                                    changed=observation.done.changed()=>if changed.is_err(){
+                return Err(error(Code::Interrupted));
                 }
+                ,
+                                    changed=observation.attempts.changed()=>if changed.is_err(){
+                return Err(error(Code::Interrupted));
+                }
+                ,
+                                }
             }
-        }).await.map_err(|_|error(Code::Timeout))??;
+        })
+        .await
+        .map_err(|_| error(Code::Timeout))??;
         #[cfg(test)]
         {
             let gate = self
                 .inner
                 .hooks
                 .stop_return
                 .lock()
                 .unwrap_or_else(|poisoned| poisoned.into_inner())
                 .take();
             if let Some(gate) = gate {
@@ -928,85 +1203,131 @@ fn run_owner(
             }
         }
         if hooks.fail_ready.swap(false, Ordering::AcqRel) {
             ready.send_replace(Some(Err(Code::Interrupted)));
             return Err(Code::Interrupted);
         }
     }
     ready.send_replace(Some(Ok(())));
     for envelope in receiver {
         if envelope.cancelled.load(Ordering::Acquire)
-            && !matches!(&envelope.command, Command::EndMaintenance(_))
+            && !matches!(
+                &envelope.command,
+                Command::EndMaintenance(_) | Command::EndRecovery(_)
+            )
         {
             let _ = envelope.reply.send(Err(error(Code::Cancelled)));
             continue;
         }
         runtime.release_requested_maintenance();
         #[cfg(test)]
         {
             let gate = hooks.command.lock().unwrap().take();
             if let Some(gate) = gate {
                 let _ = gate.entered.send(());
                 if gate.release.recv_timeout(Duration::from_secs(10)).is_err() {
                     let _ = envelope.reply.send(Err(error(Code::Interrupted)));
                     continue;
                 }
             }
         }
         // Cancellation is checked only before entry. Observer loss cannot
         // discard entered synchronous work or move engine Drop to its caller.
         let result = match envelope.command {
+            Command::BeginRecovery(evidence, release) => runtime
+                .begin_recovery(&envelope.intent, evidence, release, envelope._lease.clone())
+                .map(Reply::MaintenanceStarted),
+            Command::CopyRecovery {
+                token,
+                start,
+                limit,
+            } => runtime
+                .copy_recovery(&envelope.intent, token, start, limit)
+                .map(Reply::VectorsCopied),
+            Command::VerifyRecovery(token) => runtime
+                .verify_recovery(&envelope.intent, token)
+                .map(|()| Reply::MaintenanceEnded),
+            Command::EndRecovery(token) => runtime
+                .end_recovery(token)
+                .map(|()| Reply::MaintenanceEnded),
+            Command::RemoveGeneration(source) => runtime
+                .remove_generation(&envelope.intent, &source)
+                .map(|()| Reply::MaintenanceEnded),
+            Command::RetireRecovery(fence) => runtime
+                .retire_recovery(&envelope.intent, &fence)
+                .map(|()| Reply::MaintenanceEnded),
             Command::Open => runtime.open(&envelope.intent).map(Reply::Opened),
             Command::Add(rows) => runtime.add(&envelope.intent, rows).map(Reply::Added),
+            Command::ScanRecords { start, limit } => runtime
+                .scan_records(&envelope.intent, start, limit)
+                .map(Reply::Records),
             Command::Scan { start, limit } => runtime
                 .scan(&envelope.intent, start, limit)
                 .map(Reply::Scanned),
             Command::Delete(ids) => runtime.delete(&envelope.intent, ids).map(Reply::Deleted),
             Command::Replace {
                 id,
                 vector,
                 metadata,
             } => runtime
                 .replace(&envelope.intent, id, vector, metadata)
                 .map(Reply::Replaced),
             Command::Search { vector, limit } => runtime
                 .search(&envelope.intent, vector, limit)
                 .map(Reply::Searched),
             Command::BeginMaintenance(release_requested) => runtime
-                .begin_maintenance(&envelope.intent, release_requested)
+                .begin_maintenance(&envelope.intent, release_requested, envelope._lease.clone())
                 .map(Reply::MaintenanceStarted),
             Command::CopyVectors {
                 token,
                 start,
                 limit,
             } => runtime
                 .copy_vectors(&envelope.intent, token, start, limit)
                 .map(Reply::VectorsCopied),
             Command::EndMaintenance(token) => runtime
                 .end_maintenance(&envelope.intent, token)
                 .map(|()| Reply::MaintenanceEnded),
             Command::Persist => runtime.persist(&envelope.intent).map(Reply::Persisted),
             Command::Publish(receipt) => runtime.publish(&receipt).map(Reply::Published),
+            Command::PublishPlan(receipt, plan) => {
+                runtime.publish_plan(&receipt, &plan).map(Reply::Published)
+            }
+            Command::Retire => runtime
+                .retire(&envelope.intent)
+                .map(|()| Reply::Acknowledged),
+            Command::Candidates { vector, limit } => runtime
+                .candidates(&envelope.intent, vector, limit)
+                .map(Reply::Candidates),
+            #[cfg(test)]
+            Command::CorruptMetadata {
+                id,
+                vector,
+                metadata,
+            } => runtime
+                .corrupt_metadata(&envelope.intent, id, vector, metadata)
+                .map(Reply::Replaced),
             Command::Acknowledge(ack) => runtime.acknowledge(&ack).map(|()| Reply::Acknowledged),
         };
         let _ = envelope.reply.send(result);
         runtime.release_requested_maintenance();
     }
     let outcome = loop {
         let attempt = stop.current();
         let result = runtime.shutdown().map_err(code);
         stop.publish(attempt, result);
         if result.is_ok() || !stop.wait_retry(attempt) {
             break result;
         }
     };
-    drop(runtime); // Engines, pool termination and both Rayon joins precede exit.
+    drop(runtime);
+    // Engines, pool termination and both Rayon joins precede exit.
     outcome
 }
 /// Exactly one payload can join this owner, including on runtime teardown.
 struct JoinPayload {
     handle: Option<JoinHandle<Outcome>>,
     base_lease: Option<OperationLease>,
     done: watch::Sender<Completion>,
     stop: Arc<StopControl>,
 }
 impl JoinPayload {
diff --git a/src-tauri/src/knowledge/index/owner.rs b/src-tauri/src/knowledge/index/owner.rs
index 6d9bdd8..cd3bc73 100644
--- a/src-tauri/src/knowledge/index/owner.rs
+++ b/src-tauri/src/knowledge/index/owner.rs
@@ -17,20 +17,32 @@ use srvdb::{DatabaseConfig, IndexMode, SrvDB, Vector, VectorEngine, VectorStorag
 use std::{
     collections::BTreeMap,
     path::PathBuf,
     sync::{
         Arc, Mutex,
         atomic::{AtomicBool, Ordering},
     },
     thread::JoinHandle,
 };
 use uuid::Uuid;
+#[derive(Clone)]
+pub(crate) struct RecordCandidate {
+    pub(crate) id: u64,
+    pub(crate) deleted: bool,
+    pub(crate) metadata: Option<ChunkMetadata>,
+}
+#[derive(Clone)]
+pub(crate) struct Candidate {
+    pub(crate) id: u64,
+    pub(crate) score: f32,
+    pub(crate) metadata: Option<ChunkMetadata>,
+}
 
 #[derive(Clone, Debug)]
 pub(crate) struct PersistedReceipt {
     intent: DurableIntent,
     seal_json: String,
 }
 impl PersistedReceipt {
     pub(super) fn intent(&self) -> &DurableIntent {
         &self.intent
     }
@@ -50,35 +62,46 @@ struct Closing {
     dropped: bool,
     receipt: Option<PersistedReceipt>,
 }
 struct Entry {
     db: SrvDB,
     intent: DurableIntent,
     phase: Phase,
     used: u64,
 }
 struct Maintenance {
+    _lease: crate::intelligence::jobs::OperationLease,
     token: Uuid,
     key: Key,
     snapshot_end: u64,
     release_requested: Arc<AtomicBool>,
 }
+struct Recovery {
+    token: Uuid,
+    target: Key,
+    fence: super::recovery::RecoveryFence,
+    reader: super::recovery::VectorReader,
+    verified: bool,
+    release_requested: Arc<AtomicBool>,
+    _lease: crate::intelligence::jobs::OperationLease,
+}
 type Key = (String, Uuid);
 pub(super) struct Runtime {
     root: PathBuf,
     repository: Arc<IndexRepository>,
     epoch: Uuid,
     entries: BTreeMap<Key, Entry>,
     closing: BTreeMap<Key, Closing>,
     pool: PoolOwner,
     sequence: u64,
     maintenance: Option<Maintenance>,
+    recovery: Option<Recovery>,
     #[cfg(test)]
     pub(super) close_fault: Option<CloseFault>,
     #[cfg(test)]
     pub(super) fail_once_after_persist: bool,
 }
 #[cfg(test)]
 #[derive(Clone, Copy)]
 pub(super) enum CloseFault {
     Marker,
     Sync,
@@ -91,20 +114,21 @@ impl Runtime {
     ) -> AppResult<Self> {
         Ok(Self {
             root,
             repository,
             epoch,
             entries: BTreeMap::new(),
             closing: BTreeMap::new(),
             pool: PoolOwner::new()?,
             sequence: 0,
             maintenance: None,
+            recovery: None,
             #[cfg(test)]
             close_fault: None,
             #[cfg(test)]
             fail_once_after_persist: false,
         })
     }
     pub(super) fn open(&mut self, intent: &DurableIntent) -> AppResult<u64> {
         self.validate(intent)?;
         let key = key(intent);
         self.ensure_mutable(&key)?;
@@ -245,20 +269,86 @@ impl Runtime {
         let entry = entry_mut(&mut self.entries, receipt.intent())?;
         match &entry.phase {
             Phase::Awaiting(encoded) if encoded == receipt.seal_json() => {}
             _ => return Err(repair()),
         }
         match self.repository.publish(receipt) {
             Ok(ack) => Ok(ack),
             Err(failure) => self.repository.recover_ack(receipt).map_err(|_| failure),
         }
     }
+    pub(super) fn publish_plan(
+        &mut self,
+        receipt: &PersistedReceipt,
+        plan: &super::publication::PublicationPlan,
+    ) -> AppResult<PublicationAck> {
+        self.validate(receipt.intent())?;
+        if let super::publication::PublicationPlan::Recovery { token, fence, .. } = plan {
+            let reader = self
+                .recovery
+                .as_mut()
+                .filter(|r| {
+                    r.token == *token
+                        && r.target == key(receipt.intent())
+                        && r.fence == *fence
+                        && r.verified
+                })
+                .ok_or_else(busy)?;
+            self.pool.install(|| reader.reader.verify())?;
+        }
+        let entry = entry_mut(&mut self.entries, receipt.intent())?;
+        match &entry.phase {
+            Phase::Awaiting(encoded) if encoded == receipt.seal_json() => {}
+            _ => return Err(repair()),
+        }
+        match self.repository.publish_plan(receipt, plan) {
+            Ok(ack) => Ok(ack),
+            Err(failure) => self.repository.recover_ack(receipt).map_err(|_| failure),
+        }
+    }
+    pub(super) fn retire(&mut self, intent: &DurableIntent) -> AppResult<()> {
+        if intent.body().owner_epoch != self.epoch {
+            return Err(repair());
+        }
+        let key = key(intent);
+        let entry = self.entries.get(&key).ok_or_else(repair)?;
+        if entry.intent.body() != intent.body() || !matches!(entry.phase, Phase::Clean(_)) {
+            return Err(busy());
+        }
+        self.close_and_publish(&key)
+    }
+    #[cfg(test)]
+    pub(super) fn corrupt_metadata(
+        &mut self,
+        intent: &DurableIntent,
+        id: u64,
+        vector: Vec<f32>,
+        metadata: String,
+    ) -> AppResult<u64> {
+        self.validate(intent)?;
+        self.ensure_mutable(&key(intent))?;
+        if metadata.len() > 16384 {
+            return Err(repair());
+        }
+        validate_vector(&vector, intent.body().descriptor.identity.dimensions)?;
+        let entry = entry_mut(&mut self.entries, intent)?;
+        if !matches!(entry.phase, Phase::Dirty) {
+            return Err(busy());
+        }
+        self.pool.install(|| {
+            reserve(entry.db.count(), 1)?;
+            entry
+                .db
+                .replace(id, &Vector::new(vector), &metadata)
+                .map_err(|_| repair())
+        })
+    }
     pub(super) fn acknowledge(&mut self, ack: &PublicationAck) -> AppResult<()> {
         if ack.intent().body().owner_epoch != self.epoch {
             return Err(repair());
         }
         self.repository.validate_ack(ack)?;
         if let Some(closing) = self.closing.get(&key(ack.intent())) {
             if closing.intent.body() != ack.intent().body()
                 || closing
                     .receipt
                     .as_ref()
@@ -281,43 +371,66 @@ impl Runtime {
             _ => return Err(repair()),
         }
         Ok(())
     }
     pub(super) fn scan(
         &mut self,
         intent: &DurableIntent,
         start: u64,
         limit: u32,
     ) -> AppResult<Vec<(u64, Option<ChunkMetadata>)>> {
+        self.scan_records(intent, start, limit)?
+            .into_iter()
+            .map(|r| {
+                if !r.deleted && r.metadata.is_none() {
+                    return Err(repair());
+                }
+                Ok((r.id, r.metadata))
+            })
+            .collect()
+    }
+    pub(super) fn scan_records(
+        &mut self,
+        intent: &DurableIntent,
+        start: u64,
+        limit: u32,
+    ) -> AppResult<Vec<RecordCandidate>> {
         self.validate(intent)?;
         validate_batch(limit as usize)?;
         let entry = entry_mut(&mut self.entries, intent)?;
         self.pool.install(|| {
             let count = entry.db.count();
             if start > count {
                 return Err(repair());
             }
             let end = start
                 .checked_add(u64::from(limit))
                 .ok_or_else(repair)?
                 .min(count);
             (start..end)
                 .map(|id| {
-                    if entry.db.is_deleted(id) {
-                        return Ok((id, None));
-                    }
-                    let encoded = entry
-                        .db
-                        .get_metadata(id)
-                        .map_err(|_| repair())?
-                        .ok_or_else(repair)?;
-                    Ok((id, Some(ChunkMetadata::decode(&encoded)?)))
+                    let deleted = entry.db.is_deleted(id);
+                    let metadata = if deleted {
+                        None
+                    } else {
+                        entry
+                            .db
+                            .get_metadata(id)
+                            .map_err(|_| repair())?
+                            .as_deref()
+                            .and_then(|s| ChunkMetadata::decode(s).ok())
+                    };
+                    Ok(RecordCandidate {
+                        id,
+                        deleted,
+                        metadata,
+                    })
                 })
                 .collect()
         })
     }
     pub(super) fn delete(&mut self, intent: &DurableIntent, ids: Vec<u64>) -> AppResult<Vec<bool>> {
         self.validate(intent)?;
         self.ensure_mutable(&key(intent))?;
         validate_batch(ids.len())?;
         let entry = entry_mut(&mut self.entries, intent)?;
         if !matches!(entry.phase, Phase::Dirty) {
@@ -362,54 +475,216 @@ impl Runtime {
                 .replace(id, &Vector::new(vector), &metadata)
                 .map_err(|_| repair())
         })
     }
     pub(super) fn search(
         &mut self,
         intent: &DurableIntent,
         vector: Vec<f32>,
         limit: u32,
     ) -> AppResult<Vec<(u64, f32, ChunkMetadata)>> {
+        self.candidates(intent, vector, limit)?
+            .into_iter()
+            .map(|hit| Ok((hit.id, hit.score, hit.metadata.ok_or_else(repair)?)))
+            .collect()
+    }
+    pub(super) fn candidates(
+        &mut self,
+        intent: &DurableIntent,
+        vector: Vec<f32>,
+        limit: u32,
+    ) -> AppResult<Vec<Candidate>> {
         self.validate(intent)?;
         validate_batch(limit as usize)?;
         validate_vector(&vector, intent.body().descriptor.identity.dimensions)?;
         let entry = entry_mut(&mut self.entries, intent)?;
         self.pool.install(|| {
             let hits = entry
                 .db
                 .search(&Vector::new(vector), limit as usize)
                 .map_err(|_| repair())?;
             if hits.len() > limit as usize {
                 return Err(repair());
             }
             hits.into_iter()
                 .map(|hit| {
                     if hit.id >= entry.db.count()
                         || entry.db.is_deleted(hit.id)
                         || !hit.score.is_finite()
                     {
                         return Err(repair());
                     }
-                    let metadata =
-                        ChunkMetadata::decode(hit.metadata.as_deref().ok_or_else(repair)?)?;
-                    Ok((hit.id, hit.score, metadata))
+                    let metadata = hit
+                        .metadata
+                        .as_deref()
+                        .and_then(|json| ChunkMetadata::decode(json).ok());
+                    Ok(Candidate {
+                        id: hit.id,
+                        score: hit.score,
+                        metadata,
+                    })
                 })
                 .collect()
         })
     }
+    pub(super) fn begin_recovery(
+        &mut self,
+        intent: &DurableIntent,
+        evidence: super::recovery::RecoveryEvidence,
+        release_requested: Arc<AtomicBool>,
+        lease: crate::intelligence::jobs::OperationLease,
+    ) -> AppResult<Uuid> {
+        self.validate(intent)?;
+        if self.maintenance.is_some()
+            || self.recovery.is_some()
+            || intent.body().identity != evidence.source().key()?
+            || intent.body().descriptor.generation == evidence.source().generation
+        {
+            return Err(busy());
+        }
+        self.repository.validate_recovery(&evidence.fence, true)?;
+        let fence = evidence.fence.clone();
+        let token = Uuid::new_v4();
+        let reader = self
+            .pool
+            .install(|| super::recovery::VectorReader::open(&self.root, evidence))?;
+        self.recovery = Some(Recovery {
+            token,
+            target: key(intent),
+            fence,
+            reader,
+            verified: false,
+            release_requested,
+            _lease: lease,
+        });
+        Ok(token)
+    }
+    pub(super) fn copy_recovery(
+        &mut self,
+        intent: &DurableIntent,
+        token: Uuid,
+        start: u64,
+        limit: u32,
+    ) -> AppResult<Vec<(u64, Vec<f32>)>> {
+        self.validate(intent)?;
+        let recovery = self
+            .recovery
+            .as_mut()
+            .filter(|r| r.token == token && r.target == key(intent))
+            .ok_or_else(busy)?;
+        recovery.verified = false;
+        self.pool.install(|| recovery.reader.copy(start, limit))
+    }
+    pub(super) fn verify_recovery(&mut self, intent: &DurableIntent, token: Uuid) -> AppResult<()> {
+        self.validate(intent)?;
+        let recovery = self
+            .recovery
+            .as_mut()
+            .filter(|r| r.token == token && r.target == key(intent))
+            .ok_or_else(busy)?;
+        self.repository.validate_recovery(&recovery.fence, true)?;
+        self.pool.install(|| recovery.reader.verify())?;
+        recovery.verified = true;
+        Ok(())
+    }
+    pub(super) fn end_recovery(&mut self, token: Uuid) -> AppResult<()> {
+        if self.recovery.as_ref().is_some_and(|r| r.token != token) {
+            return Err(busy());
+        }
+        let reader = self.recovery.take();
+        self.pool.install(|| drop(reader));
+        Ok(())
+    }
+    pub(super) fn remove_generation(
+        &mut self,
+        intent: &DurableIntent,
+        source: &seal::Generation,
+    ) -> AppResult<()> {
+        if intent.body().owner_epoch != self.epoch
+            || self.recovery.is_some()
+            || self.maintenance.is_some()
+        {
+            return Err(busy());
+        }
+        self.repository
+            .can_retire(&intent.body().descriptor, source)?;
+        let target = entry_mut(&mut self.entries, intent)?;
+        if !matches!(target.phase, Phase::Clean(_)) {
+            return Err(busy());
+        }
+        let source_key = (source.key()?, source.generation);
+        if let Some(entry) = self.entries.get(&source_key) {
+            if !matches!(entry.phase, Phase::Clean(_)) {
+                return Err(busy());
+            }
+            self.close_and_publish(&source_key)?;
+        }
+        let path = preflight::generation_path(&self.root, source)?;
+        self.pool.install(|| {
+            if !path.exists() {
+                return Ok(());
+            }
+            preflight::directories(&path, false)?;
+            let _lock = super::recovery::exclusive_lock(&path)?;
+            for (n, entry) in std::fs::read_dir(&path).map_err(|_| repair())?.enumerate() {
+                if n >= 16 {
+                    return Err(repair());
+                }
+                let entry = entry.map_err(|_| repair())?;
+                let meta = std::fs::symlink_metadata(entry.path()).map_err(|_| repair())?;
+                if !meta.is_file()
+                    || meta.file_type().is_symlink()
+                    || !matches!(
+                        entry.file_name().to_str(),
+                        Some("srvdb.lock" | "vectors.bin" | "metadata.db" | "hnsw.graph")
+                    )
+                {
+                    return Err(repair());
+                }
+            }
+            std::fs::remove_dir_all(path).map_err(|_| repair())
+        })
+    }
+    pub(super) fn retire_recovery(
+        &mut self,
+        intent: &DurableIntent,
+        fence: &super::recovery::RecoveryFence,
+    ) -> AppResult<()> {
+        if intent.body().owner_epoch != self.epoch
+            || self.recovery.is_some()
+            || self
+                .entries
+                .contains_key(&(fence.source.key()?, fence.source.generation))
+        {
+            return Err(busy());
+        }
+        self.repository.validate_recovery(fence, false)?;
+        let path = preflight::generation_path(&self.root, &fence.source)?;
+        self.pool.install(|| {
+            if !path.exists() {
+                return Ok(());
+            }
+            preflight::directories(&path, false)?;
+            let _lock = super::recovery::exclusive_lock(&path)?;
+            // Only an application-managed, now-unreferenced generation can be
+            // retired. No dirty database decoder is entered during removal.
+            std::fs::remove_dir_all(&path).map_err(|_| repair())
+        })
+    }
     pub(super) fn begin_maintenance(
         &mut self,
         intent: &DurableIntent,
         release_requested: Arc<AtomicBool>,
+        lease: crate::intelligence::jobs::OperationLease,
     ) -> AppResult<Uuid> {
         self.validate(intent)?;
-        if self.maintenance.is_some() {
+        if self.maintenance.is_some() || self.recovery.is_some() {
             return Err(busy());
         }
         let source_key = key(intent);
         let entry = entry_mut(&mut self.entries, intent)?;
         if !matches!(entry.phase, Phase::Awaiting(_)) {
             return Err(busy());
         }
         let count = entry.db.count();
         let dimension = intent.body().descriptor.identity.dimensions;
         let path = preflight::generation_path(&self.root, &intent.body().descriptor)?;
@@ -417,20 +692,21 @@ impl Runtime {
         self.pool.install(|| {
             let reader =
                 VectorStorage::new(&reader_path, dimension as usize).map_err(|_| repair())?;
             if reader.count() != count || reader.dimension() != dimension as usize {
                 return Err(repair());
             }
             Ok(())
         })?;
         let token = Uuid::new_v4();
         self.maintenance = Some(Maintenance {
+            _lease: lease,
             token,
             key: source_key,
             snapshot_end: count,
             release_requested,
         });
         Ok(token)
     }
     pub(super) fn copy_vectors(
         &mut self,
         intent: &DurableIntent,
@@ -486,28 +762,37 @@ impl Runtime {
             .maintenance
             .as_ref()
             .is_none_or(|maintenance| maintenance.token != token || maintenance.key != source_key)
         {
             return Err(busy());
         }
         self.maintenance = None;
         Ok(())
     }
     pub(super) fn release_requested_maintenance(&mut self) {
+        if self
+            .recovery
+            .as_ref()
+            .is_some_and(|r| r.release_requested.load(Ordering::Acquire))
+        {
+            let reader = self.recovery.take();
+            self.pool.install(|| drop(reader));
+        }
         if self
             .maintenance
             .as_ref()
             .is_some_and(|maintenance| maintenance.release_requested.load(Ordering::Acquire))
         {
             self.maintenance = None;
         }
     }
+    #[cfg(test)]
     pub(super) fn close_generation(
         &mut self,
         generation: &seal::Generation,
     ) -> AppResult<PersistedReceipt> {
         self.close_key(&(generation.key()?, generation.generation))
     }
     fn close_key(&mut self, key: &Key) -> AppResult<PersistedReceipt> {
         self.ensure_mutable(key)?;
         if !self.closing.contains_key(key) {
             let entry = self.entries.get(key).ok_or_else(repair)?;
@@ -594,20 +879,22 @@ impl Runtime {
             Ok(ack) => ack,
             Err(error) => match self.repository.recover_ack(&receipt) {
                 Ok(ack) => ack,
                 Err(_) => return Err(error),
             },
         };
         self.acknowledge(&ack)
     }
     pub(super) fn shutdown(&mut self) -> AppResult<()> {
         self.maintenance = None;
+        let recovery = self.recovery.take();
+        self.pool.install(|| drop(recovery));
         for key in self.entries.keys().cloned().collect::<Vec<_>>() {
             let awaiting = self.entries.get(&key).and_then(|e| match &e.phase {
                 Phase::Awaiting(encoded) => Some(PersistedReceipt {
                     intent: e.intent.clone(),
                     seal_json: encoded.clone(),
                 }),
                 _ => None,
             });
             if let Some(receipt) = awaiting {
                 if let Ok(ack) = self.repository.recover_ack(&receipt) {
@@ -648,20 +935,22 @@ impl Runtime {
         Ok(())
     }
 }
 impl Drop for Runtime {
     fn drop(&mut self) {
         if !std::thread::panicking() {
             let _ = self.shutdown();
         }
         // Exceptional final-owner fallback cannot leave live mappings behind.
         // A failed close publishes no new seal; old exact hashes still fence reopen.
+        let recovery = self.recovery.take();
+        self.pool.install(|| drop(recovery));
         let entries = std::mem::take(&mut self.entries);
         self.pool.install(|| drop(entries));
     }
 }
 fn key(intent: &DurableIntent) -> Key {
     (
         intent.body().identity.clone(),
         intent.body().descriptor.generation,
     )
 }
diff --git a/src-tauri/src/knowledge/index/preflight.rs b/src-tauri/src/knowledge/index/preflight.rs
index 9c1ce0b..6be0ae2 100644
--- a/src-tauri/src/knowledge/index/preflight.rs
+++ b/src-tauri/src/knowledge/index/preflight.rs
@@ -88,21 +88,21 @@ pub(super) fn inspect(path: &Path, dimension: u32, count: u64) -> AppResult<Vec<
             let sha256 = hash_file(&path.join(name), bytes, name == "hnsw.graph")?;
             result.push(FileSeal {
                 name: name.into(),
                 bytes,
                 sha256,
             });
         }
     }
     Ok(result)
 }
-fn directories(path: &Path, create: bool) -> AppResult<()> {
+pub(super) fn directories(path: &Path, create: bool) -> AppResult<()> {
     let mut current = PathBuf::new();
     for component in path.components() {
         if matches!(component, std::path::Component::ParentDir) {
             return Err(repair());
         }
         current.push(component);
         match fs::symlink_metadata(&current) {
             Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
             Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                 fs::create_dir(&current).map_err(|_| repair())?
@@ -128,21 +128,21 @@ fn check_vector_header(path: &Path, dimension: u32, count: u64, len: u64) -> App
         || count > RECORD_CAP
         || stored != count
         || header[16..] != [0; 8]
         || len < logical
         || len > vector_cap(dimension)?
     {
         return Err(repair());
     }
     Ok(())
 }
-fn hash_file(path: &Path, expected: u64, graph: bool) -> AppResult<String> {
+pub(super) fn hash_file(path: &Path, expected: u64, graph: bool) -> AppResult<String> {
     let mut file = File::open(path).map_err(|_| repair())?;
     if file.metadata().map_err(|_| repair())?.len() != expected {
         return Err(repair());
     }
     let mut hash = Sha256::new();
     let mut crc = crc32fast::Hasher::new();
     let mut remaining = expected;
     let mut graph_crc = None;
     if graph {
         let mut header = [0; 24];
diff --git a/src-tauri/src/knowledge/index/publication.rs b/src-tauri/src/knowledge/index/publication.rs
new file mode 100644
index 0000000..aeb4010
--- /dev/null
+++ b/src-tauri/src/knowledge/index/publication.rs
@@ -0,0 +1,734 @@
+//! Compact plans reference durable rows, never a whole-document vector array.
+use super::{
+    repair,
+    seal::{Generation, Intent},
+};
+use crate::{
+    error::AppResult,
+    intelligence::jobs::{JobPublicationFence, now_ms},
+};
+use rusqlite::{Transaction, params};
+use serde::{Deserialize, Serialize};
+use uuid::Uuid;
+#[derive(Clone, Debug, Serialize, Deserialize)]
+#[serde(deny_unknown_fields)]
+pub(crate) struct DocumentFence {
+    #[serde(deserialize_with = "super::codec::canonical_uuid")]
+    pub(crate) id: Uuid,
+    pub(crate) version: u32,
+    pub(crate) sha256: String,
+    pub(crate) parser: String,
+    #[serde(deserialize_with = "optional_canonical_uuid")]
+    pub(crate) import_job: Option<Uuid>,
+}
+impl DocumentFence {
+    pub(crate) fn validate(&self) -> AppResult<()> {
+        if self.version == 0
+            || !super::codec::is_hash(&self.sha256)
+            || self.parser.is_empty()
+            || self.parser.len() > 128
+        {
+            return Err(repair());
+        }
+        Ok(())
+    }
+    pub(crate) fn matches(&self, tx: &Transaction<'_>, status: &str) -> AppResult<bool> {
+        self.validate()?;
+        Ok(tx.query_row(
+            "SELECT EXISTS(SELECT 1 FROM documents WHERE \
+                    id=?1 AND version=?2 AND sha256=?3 AND \
+                    parser_identity=?4 AND import_job_id IS ?5 \
+                    AND status=?6)",
+            params![
+                self.id.to_string(),
+                self.version,
+                self.sha256,
+                self.parser,
+                self.import_job.map(|i| i.to_string()),
+                status
+            ],
+            |r| r.get(0),
+        )?)
+    }
+}
+#[derive(Clone)]
+pub(crate) struct ReadyPublication {
+    pub(crate) operation: Uuid,
+    pub(crate) work: Uuid,
+    pub(crate) document: DocumentFence,
+    pub(crate) expected_generation: Option<Uuid>,
+    pub(crate) count: u32,
+    pub(crate) job: JobPublicationFence,
+}
+impl ReadyPublication {
+    fn ready(&self, tx: &Transaction<'_>, generation: &Generation) -> AppResult<()> {
+        if !self.document.matches(tx, "indexing")? {
+            return Err(repair());
+        }
+        let identity = generation.key()?;
+        let generation_id = generation.generation.to_string();
+        let operation = self.work.to_string();
+        let prior: Option<String> = tx.query_row(
+            "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE identity=?1",
+            [&identity],
+            |r| r.get(0),
+        )?;
+        if prior != self.expected_generation.map(|g| g.to_string()) {
+            return Err(repair());
+        }
+        let count: u32 = tx.query_row(
+            "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
+            [&operation],
+            |r| r.get(0),
+        )?;
+        let chunks: u32 = tx.query_row(
+            "SELECT count(*) FROM document_chunks WHERE \
+                    document_id=?1 AND document_version=?2",
+            params![self.document.id.to_string(), self.document.version],
+            |r| r.get(0),
+        )?;
+        if count != self.count || chunks != self.count {
+            return Err(repair());
+        }
+        let mut after = -1i64;
+        let mut seen = 0u32;
+        loop {
+            let mut query = tx.prepare(
+                "SELECT ordinal,identity,generation,vector_id,\
+                    document_id,document_version,chunk_id,\
+                    content_hash FROM knowledge_index_work_items \
+                    WHERE operation_id=?1 AND ordinal>?2 ORDER \
+                    BY ordinal LIMIT 200",
+            )?;
+            let mut rows = query.query(params![operation, after])?;
+            let mut page = 0;
+            while let Some(r) = rows.next()? {
+                let ordinal: i64 = r.get(0)?;
+                let vector: String = r.get(3)?;
+                let chunk: String = r.get(6)?;
+                let hash: String = r.get(7)?;
+                staged_associations(tx, self.work, ordinal, false)?;
+                if ordinal != i64::from(seen)
+                    || r.get::<_, String>(1)? != identity
+                    || r.get::<_, String>(2)? != generation_id
+                    || r.get::<_, String>(4)? != self.document.id.to_string()
+                    || r.get::<_, u32>(5)? != self.document.version
+                {
+                    return Err(repair());
+                }
+                let valid: bool = tx.query_row(
+                    "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings m JOIN \
+                    document_chunks c ON c.id=m.chunk_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.chunk_id=?3 AND m.vector_id=?4 AND \
+                    m.document_id=?5 AND m.document_version=?6 \
+                    AND m.content_hash=?7 AND m.published=0 AND \
+                    c.document_id=m.document_id AND \
+                    c.document_version=m.document_version AND \
+                    c.content_hash=m.content_hash AND \
+                    c.index_state='indexing')",
+                    params![
+                        identity,
+                        generation_id,
+                        chunk,
+                        vector,
+                        self.document.id.to_string(),
+                        self.document.version,
+                        hash
+                    ],
+                    |r| r.get(0),
+                )?;
+                if !valid {
+                    return Err(repair());
+                }
+                tx.execute(
+                    "UPDATE knowledge_generation_mappings SET \
+                    published=1 WHERE identity=?1 AND \
+                    generation=?2 AND chunk_id=?3",
+                    params![identity, generation_id, chunk],
+                )?;
+                if tx.execute(
+                    "UPDATE document_chunks SET vector_id=?1,\
+                    embedding_identity=?2,index_state='ready' \
+                    WHERE id=?3 AND document_version=?4 AND \
+                    content_hash=?5",
+                    params![vector, identity, chunk, self.document.version, hash],
+                )? != 1
+                {
+                    return Err(repair());
+                }
+                after = ordinal;
+                seen = seen.checked_add(1).ok_or_else(repair)?;
+                page += 1;
+            }
+            if page == 0 {
+                break;
+            }
+        }
+        if seen != self.count {
+            return Err(repair());
+        }
+        let semantic = generation.identity.semantic_key()?;
+        let bound: Option<String> = tx.query_row(
+            "SELECT semantic_key FROM knowledge_index_generations WHERE identity=?1",
+            [&identity],
+            |r| r.get(0),
+        )?;
+        if bound.is_some_and(|b| b != semantic) {
+            return Err(repair());
+        }
+        tx.execute(
+            "UPDATE knowledge_index_generations SET \
+                    semantic_key=?1,requested_dimension=?2,\
+                    active_generation=?3,state='active',\
+                    updated_at_ms=?4 WHERE identity=?5",
+            params![
+                semantic,
+                generation.requested_dimension,
+                generation_id,
+                now_ms(),
+                identity
+            ],
+        )
+        .map_err(|_| repair())?;
+        // A dirty source may be recovered into a fresh generation. Other documents
+        // retain their text and are explicitly repair-required until reindexed.
+        if self
+            .expected_generation
+            .is_some_and(|old| old != generation.generation)
+        {
+            tx.execute(
+                "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    index_identity=?1 AND id!=?2 AND \
+                    status='ready'",
+                params![identity, self.document.id.to_string()],
+            )?;
+            tx.execute(
+                "INSERT INTO knowledge_repairs(document_id,\
+                    reason) SELECT id,'unsealed_generation' FROM \
+                    documents WHERE index_identity=?1 AND \
+                    status='repair_required' ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+                [&identity],
+            )?;
+        }
+        tx.execute(
+            "UPDATE documents SET status='ready',\
+                    index_identity=?1,error_code=NULL,\
+                    updated_at_ms=?2 WHERE id=?3",
+            params![identity, now_ms(), self.document.id.to_string()],
+        )?;
+        tx.execute(
+            "DELETE FROM knowledge_repairs WHERE document_id=?1",
+            [self.document.id.to_string()],
+        )?;
+        // Removal is in the same publication transaction; uncommitted rows stay
+        // durable on any rollback and never qualify for authoritative hydration.
+        tx.execute(
+            "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
+            [operation],
+        )?;
+        Ok(())
+    }
+}
+#[derive(Clone)]
+pub(crate) enum PublicationPlan {
+    Ready(ReadyPublication),
+    Recovery {
+        operation: Uuid,
+        work: Uuid,
+        token: Uuid,
+        fence: super::recovery::RecoveryFence,
+        count: u32,
+        delete_work: Uuid,
+        document: DocumentFence,
+    },
+    Delete {
+        operation: Uuid,
+        work: Uuid,
+        document: DocumentFence,
+        job: Option<JobPublicationFence>,
+    },
+    Cleanup {
+        operation: Uuid,
+        work: Uuid,
+    },
+    Swap {
+        operation: Uuid,
+        work: Uuid,
+        source: Uuid,
+        count: u32,
+    },
+}
+impl PublicationPlan {
+    pub(super) fn apply(&self, tx: &Transaction<'_>, intent: &Intent) -> AppResult<()> {
+        match self {
+            Self::Recovery {
+                operation,
+                work,
+                token: _,
+                fence,
+                count,
+                delete_work,
+                document,
+            } => {
+                if *operation != intent.operation_id
+                    || *count > 200000
+                    || fence.source.key()? != intent.identity
+                    || fence.source.generation == intent.descriptor.generation
+                {
+                    return Err(repair());
+                }
+                super::recovery::validate(tx, fence, true)?;
+                if !document.matches(tx, "deleting")? {
+                    return Err(repair());
+                }
+                let source = fence.source.generation.to_string();
+                let target = intent.descriptor.generation.to_string();
+                let invalid: bool = tx.query_row(
+                    "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_index_work_items WHERE \
+                    operation_id=?1 AND (identity!=?2 OR \
+                    generation!=?3 OR document_id IS NOT ?4))",
+                    params![
+                        delete_work.to_string(),
+                        intent.identity,
+                        source,
+                        document.id.to_string()
+                    ],
+                    |r| r.get(0),
+                )?;
+                if invalid {
+                    return Err(repair());
+                }
+                let seen = visit(tx, *work, |r| {
+                    let ordinal: i64 = r.get(0)?;
+                    let chunk: String = r.get(6)?;
+                    let hash: String = r.get(7)?;
+                    let vector: String = r.get(3)?;
+                    if r.get::<_, String>(1)? != intent.identity || r.get::<_, String>(2)? != target
+                    {
+                        return Err(repair());
+                    }
+                    staged_associations(tx, *work, ordinal, true)?;
+                    let valid: bool = tx.query_row(
+                        "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings old JOIN \
+                    knowledge_generation_mappings new ON \
+                    new.chunk_id=old.chunk_id JOIN \
+                    knowledge_index_work_items w ON \
+                    w.chunk_id=new.chunk_id AND \
+                    w.operation_id=?7 WHERE old.identity=?1 AND \
+                    old.generation=?2 AND old.published=1 AND \
+                    new.identity=?1 AND new.generation=?3 AND \
+                    new.chunk_id=?4 AND new.vector_id=?5 AND \
+                    new.content_hash=?6 AND new.published=0 AND \
+                    old.document_id=new.document_id AND \
+                    old.document_version=new.document_version \
+                    AND old.content_hash=new.content_hash AND \
+                    old.vector_id=w.previous_vector_id)",
+                        params![
+                            intent.identity,
+                            source,
+                            target,
+                            chunk,
+                            vector,
+                            hash,
+                            work.to_string()
+                        ],
+                        |r| r.get(0),
+                    )?;
+                    if !valid {
+                        return Err(repair());
+                    }
+                    tx.execute(
+                        "UPDATE knowledge_generation_mappings SET \
+                    published=1 WHERE identity=?1 AND \
+                    generation=?2 AND chunk_id=?3",
+                        params![intent.identity, target, chunk],
+                    )?;
+                    tx.execute(
+                        "UPDATE document_chunks SET vector_id=?1,\
+                    embedding_identity=?2,index_state='ready' \
+                    WHERE id=?3",
+                        params![vector, intent.identity, chunk],
+                    )?;
+                    Ok(())
+                })?;
+                if seen != *count {
+                    return Err(repair());
+                }
+                complete_survivors(tx, &intent.identity, &target, true)?;
+                // Whole-document Ready requires complete destination coverage.
+                tx.execute(
+                    "UPDATE documents SET status='ready',\
+                    error_code=NULL WHERE index_identity=?1 AND \
+                    (status='ready' OR (status='repair_required' \
+                    AND EXISTS(SELECT 1 FROM knowledge_repairs r \
+                    WHERE r.document_id=documents.id AND \
+                    r.reason='unsealed_generation'))) AND \
+                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
+                    c.document_id=documents.id AND \
+                    c.document_version=documents.version) AND \
+                    NOT EXISTS(SELECT 1 FROM document_chunks c \
+                    WHERE c.document_id=documents.id AND \
+                    c.document_version=documents.version AND NOT \
+                    EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings m WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.published=1 AND m.chunk_id=c.id AND \
+                    m.document_id=documents.id AND \
+                    m.document_version=documents.version AND \
+                    m.content_hash=c.content_hash AND \
+                    m.vector_id=c.vector_id))",
+                    params![intent.identity, target],
+                )?;
+                tx.execute(
+                    "DELETE FROM knowledge_repairs WHERE \
+                    document_id IN (SELECT id FROM documents \
+                    WHERE index_identity=?1 AND status='ready')",
+                    [&intent.identity],
+                )?;
+                tx.execute(
+                    "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE document_id=?1",
+                    [document.id.to_string()],
+                )?;
+                if tx.execute(
+                    "DELETE FROM documents WHERE id=?1 AND version=?2",
+                    params![document.id.to_string(), document.version],
+                )? != 1
+                {
+                    return Err(repair());
+                }
+                tx.execute(
+                    "UPDATE knowledge_index_generations SET \
+                    active_generation=?1,state='active',\
+                    updated_at_ms=?2 WHERE identity=?3",
+                    params![target, now_ms(), intent.identity],
+                )?;
+                tx.execute(
+                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
+                    [work.to_string()],
+                )?;
+                Ok(())
+            }
+            Self::Ready(plan) => {
+                if plan.operation != intent.operation_id || plan.count == 0 || plan.count > 10000 {
+                    return Err(repair());
+                }
+                plan.job.commit(tx, || plan.ready(tx, &intent.descriptor))
+            }
+            Self::Delete {
+                operation,
+                work,
+                document,
+                job,
+            } => {
+                if *operation != intent.operation_id {
+                    return Err(repair());
+                }
+                let remove = || {
+                    if !document.matches(tx, "deleting")? {
+                        return Err(repair());
+                    }
+                    let foreign: bool = tx.query_row(
+                        "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_index_work_items WHERE \
+                    operation_id=?1 AND (identity!=?2 OR \
+                    generation!=?3 OR document_id IS NOT ?4))",
+                        params![
+                            work.to_string(),
+                            intent.identity,
+                            intent.descriptor.generation.to_string(),
+                            document.id.to_string()
+                        ],
+                        |r| r.get(0),
+                    )?;
+                    if foreign {
+                        return Err(repair());
+                    }
+                    tx.execute(
+                        "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE document_id=?1",
+                        [document.id.to_string()],
+                    )?;
+                    if tx.execute(
+                        "DELETE FROM documents WHERE id=?1 AND version=?2",
+                        params![document.id.to_string(), document.version],
+                    )? != 1
+                    {
+                        return Err(repair());
+                    }
+                    Ok(())
+                };
+                match job {
+                    Some(j) => j.commit(tx, remove),
+                    None => remove(),
+                }
+            }
+            Self::Cleanup { operation, work } => {
+                if *operation != intent.operation_id {
+                    return Err(repair());
+                }
+                visit(tx, *work, |r| {
+                    if r.get::<_, String>(1)? != intent.identity
+                        || r.get::<_, String>(2)? != intent.descriptor.generation.to_string()
+                    {
+                        return Err(repair());
+                    }
+                    tx.execute(
+                        "DELETE FROM knowledge_generation_mappings \
+                    WHERE identity=?1 AND generation=?2 AND \
+                    vector_id=?3",
+                        params![
+                            intent.identity,
+                            intent.descriptor.generation.to_string(),
+                            r.get::<_, String>(3)?
+                        ],
+                    )?;
+                    Ok(())
+                })?;
+                tx.execute(
+                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
+                    [work.to_string()],
+                )?;
+                Ok(())
+            }
+            Self::Swap {
+                operation,
+                work,
+                source,
+                count,
+            } => {
+                if *operation != intent.operation_id
+                    || *count > 200000
+                    || *source == intent.descriptor.generation
+                {
+                    return Err(repair());
+                }
+                let target = intent.descriptor.generation.to_string();
+                let source = source.to_string();
+                let active: Option<String> = tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE identity=?1",
+                    [&intent.identity],
+                    |r| r.get(0),
+                )?;
+                if active.as_deref() != Some(&source) {
+                    return Err(repair());
+                }
+                let seen = visit(tx, *work, |r| {
+                    let chunk: String = r.get(6)?;
+                    let hash: String = r.get(7)?;
+                    let vector: String = r.get(3)?;
+                    staged_associations(tx, *work, r.get(0)?, true)?;
+                    if r.get::<_, String>(1)? != intent.identity || r.get::<_, String>(2)? != target
+                    {
+                        return Err(repair());
+                    }
+                    let valid: bool = tx.query_row(
+                        "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings old JOIN \
+                    knowledge_generation_mappings new ON \
+                    new.chunk_id=old.chunk_id JOIN \
+                    document_chunks c ON c.id=old.chunk_id JOIN \
+                    documents d ON d.id=c.document_id WHERE \
+                    old.identity=?1 AND old.generation=?2 AND \
+                    old.published=1 AND new.identity=?1 AND \
+                    new.generation=?3 AND new.chunk_id=?4 AND \
+                    new.vector_id=?5 AND new.content_hash=?6 AND \
+                    new.published=0 AND \
+                    old.document_id=new.document_id AND \
+                    old.document_version=new.document_version \
+                    AND old.content_hash=new.content_hash AND \
+                    d.version=new.document_version AND \
+                    d.status='ready' AND d.index_identity=?1 AND \
+                    c.document_version=d.version AND \
+                    c.content_hash=new.content_hash AND \
+                    c.vector_id=old.vector_id AND \
+                    c.index_state='ready')",
+                        params![intent.identity, source, target, chunk, vector, hash],
+                        |r| r.get(0),
+                    )?;
+                    if !valid {
+                        return Err(repair());
+                    }
+                    tx.execute(
+                        "UPDATE knowledge_generation_mappings SET \
+                    published=1 WHERE identity=?1 AND \
+                    generation=?2 AND chunk_id=?3",
+                        params![intent.identity, target, chunk],
+                    )?;
+                    tx.execute(
+                        "UPDATE document_chunks SET vector_id=?1,embedding_identity=?2 WHERE id=?3",
+                        params![vector, intent.identity, chunk],
+                    )?;
+                    Ok(())
+                })?;
+                if seen != *count {
+                    return Err(repair());
+                }
+                complete_survivors(tx, &intent.identity, &target, false)?;
+                tx.execute(
+                    "UPDATE knowledge_index_generations SET \
+                    active_generation=?1,state='active',\
+                    updated_at_ms=?2 WHERE identity=?3",
+                    params![target, now_ms(), intent.identity],
+                )?;
+                tx.execute(
+                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
+                    [work.to_string()],
+                )?;
+                Ok(())
+            }
+        }
+    }
+}
+fn visit(
+    tx: &Transaction<'_>,
+    operation: Uuid,
+    mut action: impl FnMut(&rusqlite::Row<'_>) -> AppResult<()>,
+) -> AppResult<u32> {
+    let mut after = -1i64;
+    let mut count = 0u32;
+    loop {
+        let mut q = tx.prepare(
+            "SELECT ordinal,identity,generation,vector_id,\
+                    document_id,document_version,chunk_id,\
+                    content_hash FROM knowledge_index_work_items \
+                    WHERE operation_id=?1 AND ordinal>?2 ORDER \
+                    BY ordinal LIMIT 200",
+        )?;
+        let mut rows = q.query(params![operation.to_string(), after])?;
+        let mut page = 0;
+        while let Some(r) = rows.next()? {
+            let ordinal: i64 = r.get(0)?;
+            if ordinal <= after || count >= 200000 {
+                return Err(repair());
+            }
+            action(r)?;
+            after = ordinal;
+            count += 1;
+            page += 1;
+        }
+        if page == 0 {
+            return Ok(count);
+        }
+    }
+}
+
+fn staged_associations(
+    tx: &Transaction<'_>,
+    operation: Uuid,
+    ordinal: i64,
+    survivor: bool,
+) -> AppResult<()> {
+    use crate::knowledge::{service::state, types::TextLocation};
+    let mut q = tx.prepare(
+        "SELECT w.document_fence_json,\
+                    w.location_sha256,c.text,c.location_json,\
+                    c.content_hash,d.status,w.previous_vector_id,\
+                    c.vector_id,c.index_state,w.document_id,\
+                    w.document_version FROM \
+                    knowledge_index_work_items w JOIN \
+                    document_chunks c ON c.id=w.chunk_id JOIN \
+                    documents d ON d.id=w.document_id WHERE \
+                    w.operation_id=?1 AND w.ordinal=?2 AND \
+                    c.document_id=w.document_id AND \
+                    c.document_version=w.document_version AND \
+                    c.content_hash=w.content_hash",
+    )?;
+    let mut rows = q.query(params![operation.to_string(), ordinal])?;
+    let r = rows.next()?.ok_or_else(repair)?;
+    let f: DocumentFence = serde_json::from_str(state::text(r, 0, 1024)?).map_err(|_| repair())?;
+    let status = state::text(r, 5, 32)?;
+    if f.id.to_string() != state::text(r, 9, 36)?
+        || f.version != r.get::<_, u32>(10)?
+        || !f.matches(tx, status)?
+    {
+        return Err(repair());
+    }
+    if survivor {
+        let quarantined: bool = tx.query_row(
+            "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_repairs WHERE document_id=?1 AND \
+                    reason='unsealed_generation')",
+            [f.id.to_string()],
+            |r| r.get(0),
+        )?;
+        if (status != "ready" && (status != "repair_required" || !quarantined))
+            || state::text(r, 8, 32)? != "ready"
+            || state::text(r, 6, 20)? != state::text(r, 7, 20)?
+        {
+            return Err(repair());
+        }
+    } else if status != "indexing" || state::text(r, 8, 32)? != "indexing" {
+        return Err(repair());
+    }
+    if crate::knowledge::chunk_hash(state::text(r, 2, 9600)?) != state::text(r, 4, 64)? {
+        return Err(repair());
+    }
+    let location: TextLocation =
+        serde_json::from_str(state::text(r, 3, 16 * 1024 * 1024)?).map_err(|_| repair())?;
+    if super::codec::location_digest(&location)? != state::text(r, 1, 64)? {
+        return Err(repair());
+    }
+    Ok(())
+}
+
+fn optional_canonical_uuid<'de, D: serde::Deserializer<'de>>(
+    d: D,
+) -> Result<Option<Uuid>, D::Error> {
+    let raw = Option::<String>::deserialize(d)?;
+    raw.map(|s| {
+        let id = Uuid::parse_str(&s).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
+        if id.to_string() != s {
+            return Err(serde::de::Error::custom("noncanonical UUID"));
+        }
+        Ok(id)
+    })
+    .transpose()
+}
+
+fn complete_survivors(
+    tx: &Transaction<'_>,
+    identity: &str,
+    target: &str,
+    restore_unsealed: bool,
+) -> AppResult<()> {
+    let incomplete: bool = tx.query_row(
+        "SELECT EXISTS(SELECT 1 FROM documents d \
+                    WHERE d.index_identity=?1 AND \
+                    (d.status='ready' OR (?3 AND \
+                    d.status='repair_required' AND EXISTS(SELECT \
+                    1 FROM knowledge_repairs r WHERE \
+                    r.document_id=d.id AND \
+                    r.reason='unsealed_generation'))) AND (NOT \
+                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
+                    c.document_id=d.id AND \
+                    c.document_version=d.version) OR \
+                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
+                    c.document_id=d.id AND \
+                    c.document_version=d.version AND NOT \
+                    EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings m WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.published=1 AND m.chunk_id=c.id AND \
+                    m.document_id=d.id AND \
+                    m.document_version=d.version AND \
+                    m.content_hash=c.content_hash AND \
+                    m.vector_id=c.vector_id))))",
+        params![identity, target, restore_unsealed],
+        |r| r.get(0),
+    )?;
+    if incomplete {
+        return Err(repair());
+    }
+    Ok(())
+}
diff --git a/src-tauri/src/knowledge/index/recovery.rs b/src-tauri/src/knowledge/index/recovery.rs
new file mode 100644
index 0000000..653c5b4
--- /dev/null
+++ b/src-tauri/src/knowledge/index/recovery.rs
@@ -0,0 +1,260 @@
+//! Witnessed vector-only recovery. Never enters dirty metadata or SrvDB.
+use super::{
+    preflight, repair,
+    seal::{Generation, Intent, IntentKind, Seal, digest},
+};
+use crate::{error::AppResult, intelligence::types::IntelligenceErrorCode as Code};
+use rusqlite::{Transaction, params};
+use sha2::{Digest, Sha256};
+use std::{
+    fs::{File, OpenOptions},
+    io::{Read, Seek, SeekFrom},
+    path::{Path, PathBuf},
+};
+use uuid::Uuid;
+#[derive(Clone, Debug, PartialEq, Eq)]
+pub(crate) struct RecoveryFence {
+    pub(crate) source: Generation,
+    pub(super) seal_sha256: String,
+    pub(super) intent_sha256: String,
+}
+#[derive(Clone)]
+pub(crate) struct RecoveryEvidence {
+    pub(crate) fence: RecoveryFence,
+    pub(super) seal: Seal,
+}
+impl RecoveryEvidence {
+    pub(crate) fn source(&self) -> &Generation {
+        &self.fence.source
+    }
+    pub(crate) fn count(&self) -> u64 {
+        self.seal.stored_count
+    }
+}
+pub(super) fn evidence(
+    tx: &Transaction<'_>,
+    identity: &str,
+    generation: Uuid,
+) -> AppResult<RecoveryEvidence> {
+    let (seal_json, intent_json): (String, String) = tx
+        .query_row(
+            "SELECT seal_json,intent_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2 AND dirty=1",
+            params![identity, generation.to_string()],
+            |r| Ok((r.get(0)?, r.get(1)?)),
+        )
+        .map_err(|_| repair())?;
+    let seal = Seal::decode(&seal_json)?;
+    let intent = Intent::decode(&intent_json)?;
+    if intent.operation != IntentKind::Open
+        || intent.prior_seal_sha256.as_deref() != Some(&digest(&seal_json))
+        || intent.descriptor != seal.descriptor
+        || intent.expected_stored_count != seal.stored_count
+        || seal.descriptor.key()? != identity
+        || seal.descriptor.generation != generation
+    {
+        return Err(repair());
+    }
+    let fence = RecoveryFence {
+        source: seal.descriptor.clone(),
+        seal_sha256: digest(&seal_json),
+        intent_sha256: digest(&intent_json),
+    };
+    validate(tx, &fence, true)?;
+    Ok(RecoveryEvidence { fence, seal })
+}
+pub(super) fn validate(tx: &Transaction<'_>, fence: &RecoveryFence, active: bool) -> AppResult<()> {
+    let identity = fence.source.key()?;
+    let source = fence.source.generation.to_string();
+    let (seal, intent): (String, String) = tx
+        .query_row(
+            "SELECT seal_json,intent_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2 AND dirty=1",
+            params![identity, source],
+            |r| Ok((r.get(0)?, r.get(1)?)),
+        )
+        .map_err(|_| repair())?;
+    if digest(&seal) != fence.seal_sha256 || digest(&intent) != fence.intent_sha256 {
+        return Err(repair());
+    }
+    let pointer: Option<String> = tx.query_row(
+        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE identity=?1",
+        [identity],
+        |r| r.get(0),
+    )?;
+    if (pointer.as_deref() == Some(&source)) != active {
+        return Err(repair());
+    }
+    Ok(())
+}
+pub(super) struct VectorReader {
+    path: PathBuf,
+    file: File,
+    _lock: File,
+    evidence: RecoveryEvidence,
+}
+impl VectorReader {
+    pub(super) fn open(root: &Path, evidence: RecoveryEvidence) -> AppResult<Self> {
+        let path = preflight::generation_path(root, evidence.source())?;
+        preflight::directories(&path, false)?;
+        safe_entries(&path, evidence.source().identity.dimensions)?;
+        let lock = exclusive_lock(&path)?;
+        regular(&path.join("vectors.bin"), evidence.seal.files[0].bytes)?;
+        let file = File::open(path.join("vectors.bin")).map_err(|_| repair())?;
+        let mut reader = Self {
+            path,
+            file,
+            _lock: lock,
+            evidence,
+        };
+        reader.verify()?;
+        Ok(reader)
+    }
+    pub(super) fn verify(&mut self) -> AppResult<()> {
+        safe_entries(&self.path, self.evidence.source().identity.dimensions)?;
+        let expected = &self.evidence.seal.files[0];
+        regular(&self.path.join("vectors.bin"), expected.bytes)?;
+        if self.file.metadata().map_err(|_| repair())?.len() != expected.bytes {
+            return Err(repair());
+        }
+        self.file.seek(SeekFrom::Start(0)).map_err(|_| repair())?;
+        let mut header = [0u8; 24];
+        self.file.read_exact(&mut header).map_err(|_| repair())?;
+        let dimension = self.evidence.source().identity.dimensions;
+        let count = self.evidence.count();
+        let logical = count
+            .checked_mul(u64::from(dimension) * 4)
+            .and_then(|n| n.checked_add(24))
+            .ok_or_else(repair)?;
+        if header[..4] != 0x53764442u32.to_le_bytes()
+            || header[4..6] != 4u16.to_le_bytes()
+            || u32::from(u16::from_le_bytes(
+                header[6..8].try_into().map_err(|_| repair())?,
+            )) != dimension
+            || u64::from_le_bytes(header[8..16].try_into().map_err(|_| repair())?) != count
+            || header[16..] != [0; 8]
+            || expected.bytes < logical
+        {
+            return Err(repair());
+        }
+        self.file.seek(SeekFrom::Start(0)).map_err(|_| repair())?;
+        let mut hash = Sha256::new();
+        let mut remaining = expected.bytes;
+        let mut buffer = [0u8; 65536];
+        while remaining > 0 {
+            let n = remaining.min(buffer.len() as u64) as usize;
+            self.file
+                .read_exact(&mut buffer[..n])
+                .map_err(|_| repair())?;
+            hash.update(&buffer[..n]);
+            remaining -= n as u64;
+        }
+        let mut extra = [0];
+        if self.file.read(&mut extra).map_err(|_| repair())? != 0
+            || format!("{:x}", hash.finalize()) != expected.sha256
+            || self.file.metadata().map_err(|_| repair())?.len() != expected.bytes
+        {
+            return Err(repair());
+        }
+        // Recheck the named file as well as the retained descriptor. A replacement
+        // path cannot be certified by hashing the now-unlinked original handle.
+        if preflight::hash_file(&self.path.join("vectors.bin"), expected.bytes, false)?
+            != expected.sha256
+        {
+            return Err(repair());
+        }
+        Ok(())
+    }
+    pub(super) fn copy(&mut self, start: u64, limit: u32) -> AppResult<Vec<(u64, Vec<f32>)>> {
+        if limit == 0 || limit > 200 || start > self.evidence.count() {
+            return Err(repair());
+        }
+        let end = start
+            .checked_add(u64::from(limit))
+            .ok_or_else(repair)?
+            .min(self.evidence.count());
+        let dimension = self.evidence.source().identity.dimensions;
+        let mut bytes = vec![0u8; dimension as usize * 4];
+        let mut out = Vec::with_capacity((end - start) as usize);
+        for id in start..end {
+            let offset = id
+                .checked_mul(u64::from(dimension) * 4)
+                .and_then(|n| n.checked_add(24))
+                .ok_or_else(repair)?;
+            self.file
+                .seek(SeekFrom::Start(offset))
+                .map_err(|_| repair())?;
+            self.file.read_exact(&mut bytes).map_err(|_| repair())?;
+            let vector = bytes
+                .chunks_exact(4)
+                .map(|b| f32::from_le_bytes(b.try_into().expect("four bytes")))
+                .collect();
+            out.push((id, vector));
+        }
+        Ok(out)
+    }
+}
+pub(super) fn exclusive_lock(path: &Path) -> AppResult<File> {
+    let m = std::fs::symlink_metadata(path.join("srvdb.lock")).map_err(|_| repair())?;
+    if !m.is_file() || m.file_type().is_symlink() || m.len() > 4096 {
+        return Err(repair());
+    }
+    let lock = OpenOptions::new()
+        .read(true)
+        .write(true)
+        .open(path.join("srvdb.lock"))
+        .map_err(|_| repair())?;
+    lock.try_lock().map_err(|e| match e {
+        std::fs::TryLockError::WouldBlock => crate::error::AppError::Intelligence(Code::Busy),
+        _ => repair(),
+    })?;
+    Ok(lock)
+}
+fn regular(path: &Path, length: u64) -> AppResult<()> {
+    let m = std::fs::symlink_metadata(path).map_err(|_| repair())?;
+    if !m.is_file() || m.file_type().is_symlink() || m.len() != length {
+        return Err(repair());
+    }
+    Ok(())
+}
+
+fn safe_entries(path: &Path, dimension: u32) -> AppResult<()> {
+    let mut vectors = false;
+    let mut metadata = false;
+    for (ordinal, entry) in std::fs::read_dir(path).map_err(|_| repair())?.enumerate() {
+        if ordinal >= 16 {
+            return Err(repair());
+        }
+        let entry = entry.map_err(|_| repair())?;
+        let name = entry.file_name();
+        let name = name.to_str().ok_or_else(repair)?;
+        let cap = match name {
+            "vectors.bin" => {
+                vectors = true;
+                super::seal::vector_cap(dimension)?
+            }
+            "metadata.db" => {
+                metadata = true;
+                super::seal::METADATA_CAP
+            }
+            "hnsw.graph" => super::seal::GRAPH_CAP,
+            "srvdb.lock" => 4096,
+            _ => return Err(repair()),
+        };
+        let m = std::fs::symlink_metadata(entry.path()).map_err(|_| repair())?;
+        if !m.is_file()
+            || m.file_type().is_symlink()
+            || m.len() > cap
+            || (name == "metadata.db" && m.len() == 0)
+        {
+            return Err(repair());
+        }
+    }
+    if !vectors || !metadata {
+        return Err(repair());
+    }
+    Ok(())
+}
diff --git a/src-tauri/src/knowledge/index/repository.rs b/src-tauri/src/knowledge/index/repository.rs
index b3f9690..2f46a85 100644
--- a/src-tauri/src/knowledge/index/repository.rs
+++ b/src-tauri/src/knowledge/index/repository.rs
@@ -36,77 +36,195 @@ impl PublicationAck {
         &self.intent
     }
     pub(super) fn seal_json(&self) -> &str {
         &self.seal_json
     }
 }
 impl IndexRepository {
     pub(crate) fn new(store: Arc<LocalStore>) -> Self {
         Self { store }
     }
+    pub(super) fn can_retire(&self, target: &Generation, source: &Generation) -> AppResult<()> {
+        self.store.transaction(|tx| {
+            let valid: bool = tx.query_row(
+                "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_index_generations WHERE \
+                    identity=?1 AND active_generation=?2 AND \
+                    active_generation!=?3)",
+                params![
+                    target.key()?,
+                    target.generation.to_string(),
+                    source.generation.to_string()
+                ],
+                |r| r.get(0),
+            )?;
+            if target.key()? != source.key()? || !valid {
+                return Err(repair());
+            }
+            Ok(())
+        })
+    }
+    pub(super) fn recovery_evidence(
+        &self,
+        identity: &str,
+        generation: Uuid,
+    ) -> AppResult<super::recovery::RecoveryEvidence> {
+        self.store
+            .transaction(|tx| super::recovery::evidence(tx, identity, generation))
+    }
+    pub(super) fn validate_recovery(
+        &self,
+        fence: &super::recovery::RecoveryFence,
+        active: bool,
+    ) -> AppResult<()> {
+        self.store
+            .transaction(|tx| super::recovery::validate(tx, fence, active))
+    }
     pub(super) fn create(&self, generation: Generation, epoch: Uuid) -> AppResult<DurableIntent> {
         let body = Intent::new(generation, epoch, None)?;
         let intent = DurableIntent {
             encoded: body.encode()?,
             body,
             prior: None,
         };
         self.store.transaction(|tx| {
-            tx.execute("INSERT INTO knowledge_index_generations(identity,profile_id,dimension,state) VALUES (?1,?2,?3,'building') ON CONFLICT(identity) DO NOTHING",params![intent.body.identity,intent.body.descriptor.identity.profile_id.to_string(),intent.body.descriptor.identity.dimensions])?;
-            if read_row(tx,&intent.body)?.is_some(){return Err(repair());}
-            tx.execute("INSERT INTO knowledge_index_generation_seals(identity,generation,seal_json,dirty,intent_json,updated_at_ms) VALUES (?1,?2,NULL,1,?3,?4)",params![intent.body.identity,intent.body.descriptor.generation.to_string(),intent.encoded,now_ms()])?;
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_index_generations(identity,\
+                    profile_id,dimension,state) VALUES (?1,?2,?3,\
+                    'building') ON CONFLICT(identity) DO NOTHING",
+                params![
+                    intent.body.identity,
+                    intent.body.descriptor.identity.profile_id.to_string(),
+                    intent.body.descriptor.identity.dimensions
+                ],
+            )?;
+            if read_row(tx, &intent.body)?.is_some() {
+                return Err(repair());
+            }
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_index_generation_seals(identity,\
+                    generation,seal_json,dirty,intent_json,\
+                    updated_at_ms) VALUES (?1,?2,NULL,1,?3,?4)",
+                params![
+                    intent.body.identity,
+                    intent.body.descriptor.generation.to_string(),
+                    intent.encoded,
+                    now_ms()
+                ],
+            )?;
             Ok(())
         })?;
         Ok(intent)
     }
     pub(super) fn open(&self, generation: Generation, epoch: Uuid) -> AppResult<DurableIntent> {
         let identity = generation.key()?;
         self.store.transaction(|tx| {
-            let row:Option<Row>=tx.query_row("SELECT seal_json,dirty,intent_json FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",params![identity,generation.generation.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
-            let Some((Some(prior),0,None))=row else {return Err(repair());};
-            let seal=Seal::decode(&prior)?;
-            if seal.descriptor!=generation {return Err(repair());}
-            let body=Intent::new(generation,epoch,Some(&prior))?;
-            let intent=DurableIntent{encoded:body.encode()?,body,prior:Some(prior)};
-            tx.execute("UPDATE knowledge_index_generation_seals SET dirty=1,intent_json=?3,updated_at_ms=?4 WHERE identity=?1 AND generation=?2",params![intent.body.identity,intent.body.descriptor.generation.to_string(),intent.encoded,now_ms()])?;
+            let row: Option<Row> = tx
+                .query_row(
+                    "SELECT seal_json,dirty,intent_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2",
+                    params![identity, generation.generation.to_string()],
+                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
+                )
+                .optional()?;
+            let Some((Some(prior), 0, None)) = row else {
+                return Err(repair());
+            };
+            let seal = Seal::decode(&prior)?;
+            if seal.descriptor != generation {
+                return Err(repair());
+            }
+            let body = Intent::new(generation, epoch, Some(&prior))?;
+            let intent = DurableIntent {
+                encoded: body.encode()?,
+                body,
+                prior: Some(prior),
+            };
+            tx.execute(
+                "UPDATE knowledge_index_generation_seals SET \
+                    dirty=1,intent_json=?3,updated_at_ms=?4 \
+                    WHERE identity=?1 AND generation=?2",
+                params![
+                    intent.body.identity,
+                    intent.body.descriptor.generation.to_string(),
+                    intent.encoded,
+                    now_ms()
+                ],
+            )?;
             Ok(intent)
         })
     }
     pub(super) fn close(
         &self,
         receipt: &PersistedReceipt,
         epoch: Uuid,
     ) -> AppResult<DurableIntent> {
         self.store.transaction(|tx| {
-            let Some((Some(prior),0,None))=read_row(tx,receipt.intent().body())? else{return Err(repair());};
-            if prior!=receipt.seal_json(){return Err(repair());}
-            let body=Intent::close(receipt.intent().body().descriptor.clone(),epoch,&prior)?;
-            let intent=DurableIntent{encoded:body.encode()?,body,prior:Some(prior)};
-            tx.execute("UPDATE knowledge_index_generation_seals SET dirty=1,intent_json=?3,updated_at_ms=?4 WHERE identity=?1 AND generation=?2",params![intent.body.identity,intent.body.descriptor.generation.to_string(),intent.encoded,now_ms()])?;
+            let Some((Some(prior), 0, None)) = read_row(tx, receipt.intent().body())? else {
+                return Err(repair());
+            };
+            if prior != receipt.seal_json() {
+                return Err(repair());
+            }
+            let body = Intent::close(receipt.intent().body().descriptor.clone(), epoch, &prior)?;
+            let intent = DurableIntent {
+                encoded: body.encode()?,
+                body,
+                prior: Some(prior),
+            };
+            tx.execute(
+                "UPDATE knowledge_index_generation_seals SET \
+                    dirty=1,intent_json=?3,updated_at_ms=?4 \
+                    WHERE identity=?1 AND generation=?2",
+                params![
+                    intent.body.identity,
+                    intent.body.descriptor.generation.to_string(),
+                    intent.encoded,
+                    now_ms()
+                ],
+            )?;
             Ok(intent)
         })
     }
     pub(super) fn validate(&self, intent: &DurableIntent) -> AppResult<()> {
         self.store.transaction(|tx| validate_intent(tx, intent))
     }
     /// Future mapping/pointer/Ready publication belongs in this same transaction
     /// through a typed publication plan. This layer does not alter document state.
     pub(crate) fn publish(&self, receipt: &PersistedReceipt) -> AppResult<PublicationAck> {
         self.store.transaction(|tx| {
             validate_intent(tx, receipt.intent())?;
             publish_seal(tx, receipt)
         })?;
         Ok(PublicationAck {
             intent: receipt.intent().clone(),
             seal_json: receipt.seal_json().to_owned(),
         })
     }
+    pub(super) fn publish_plan(
+        &self,
+        receipt: &PersistedReceipt,
+        plan: &super::publication::PublicationPlan,
+    ) -> AppResult<PublicationAck> {
+        self.store.transaction(|tx| {
+            validate_intent(tx, receipt.intent())?;
+            plan.apply(tx, receipt.intent().body())?;
+            publish_seal(tx, receipt)
+        })?;
+        Ok(PublicationAck {
+            intent: receipt.intent().clone(),
+            seal_json: receipt.seal_json().to_owned(),
+        })
+    }
     pub(super) fn recover_ack(&self, receipt: &PersistedReceipt) -> AppResult<PublicationAck> {
         let ack = PublicationAck {
             intent: receipt.intent().clone(),
             seal_json: receipt.seal_json().to_owned(),
         };
         self.validate_ack(&ack)?;
         Ok(ack)
     }
     pub(super) fn validate_ack(&self, ack: &PublicationAck) -> AppResult<()> {
         self.store.transaction(|tx| {
@@ -121,34 +239,53 @@ impl IndexRepository {
                 || decoded.descriptor != ack.intent.body.descriptor
             {
                 return Err(repair());
             }
             Ok(())
         })
     }
 }
 type Row = (Option<String>, i64, Option<String>);
 fn read_row(tx: &Transaction<'_>, intent: &Intent) -> AppResult<Option<Row>> {
-    Ok(tx.query_row("SELECT seal_json,dirty,intent_json FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",params![intent.identity,intent.descriptor.generation.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?)
+    Ok(tx
+        .query_row(
+            "SELECT seal_json,dirty,intent_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2",
+            params![intent.identity, intent.descriptor.generation.to_string()],
+            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
+        )
+        .optional()?)
 }
 fn validate_intent(tx: &Transaction<'_>, intent: &DurableIntent) -> AppResult<()> {
     let Some((prior, 1, Some(encoded))) = read_row(tx, &intent.body)? else {
         return Err(repair());
     };
     if encoded != intent.encoded
         || Intent::decode(&encoded)? != intent.body
         || prior != intent.prior
         || prior.as_deref().map(digest) != intent.body.prior_seal_sha256
     {
         return Err(repair());
     }
     Ok(())
 }
 fn publish_seal(tx: &Transaction<'_>, receipt: &PersistedReceipt) -> AppResult<()> {
     let intent = receipt.intent().body();
     let seal = Seal::decode(receipt.seal_json())?;
     if seal.operation_id != intent.operation_id || seal.descriptor != intent.descriptor {
         return Err(repair());
     }
-    tx.execute("UPDATE knowledge_index_generation_seals SET seal_json=?3,dirty=0,intent_json=NULL,updated_at_ms=?4 WHERE identity=?1 AND generation=?2",params![intent.identity,intent.descriptor.generation.to_string(),receipt.seal_json(),now_ms()])?;
+    tx.execute(
+        "UPDATE knowledge_index_generation_seals SET \
+                    seal_json=?3,dirty=0,intent_json=NULL,\
+                    updated_at_ms=?4 WHERE identity=?1 AND \
+                    generation=?2",
+        params![
+            intent.identity,
+            intent.descriptor.generation.to_string(),
+            receipt.seal_json(),
+            now_ms()
+        ],
+    )?;
     Ok(())
 }
diff --git a/src-tauri/src/knowledge/index/seal.rs b/src-tauri/src/knowledge/index/seal.rs
index 737c9af..9c06833 100644
--- a/src-tauri/src/knowledge/index/seal.rs
+++ b/src-tauri/src/knowledge/index/seal.rs
@@ -14,21 +14,21 @@ pub(super) const GRAPH_CAP: u64 = 536_870_912;
 const PIN: &str = "1eec46feb2b7e6a6e286fa3361a20cd771dd4e04";
 
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
 pub(crate) struct Generation {
     pub identity: EmbeddingIdentity,
     pub requested_dimension: Option<u32>,
     pub generation: Uuid,
 }
 impl Generation {
-    pub(super) fn key(&self) -> AppResult<String> {
+    pub(crate) fn key(&self) -> AppResult<String> {
         if self
             .requested_dimension
             .is_some_and(|d| d != self.identity.dimensions)
         {
             return Err(repair());
         }
         self.identity.key().map_err(|_| repair())
     }
 }
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
@@ -87,30 +87,30 @@ pub(super) fn vector_cap(dimension: u32) -> AppResult<u64> {
 }
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
 pub(super) struct FileSeal {
     pub name: String,
     pub bytes: u64,
     pub sha256: String,
 }
 #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
 #[serde(deny_unknown_fields)]
-pub(super) struct Seal {
+pub(crate) struct Seal {
     version: u32,
     kind: String,
     policy: Policy,
-    pub identity: String,
-    pub descriptor: Generation,
-    pub operation_id: Uuid,
-    pub stored_count: u64,
-    pub metadata_header: HeaderSnapshot,
-    pub files: Vec<FileSeal>,
+    pub(super) identity: String,
+    pub(crate) descriptor: Generation,
+    pub(super) operation_id: Uuid,
+    pub(crate) stored_count: u64,
+    pub(super) metadata_header: HeaderSnapshot,
+    pub(super) files: Vec<FileSeal>,
 }
 impl Seal {
     pub(super) fn new(
         intent: &Intent,
         count: u64,
         files: Vec<FileSeal>,
         metadata_header: HeaderSnapshot,
     ) -> AppResult<Self> {
         let seal = Self {
             version: 1,
@@ -123,21 +123,27 @@ impl Seal {
             metadata_header,
             files,
         };
         seal.validate()?;
         Ok(seal)
     }
     pub(super) fn encode(&self) -> AppResult<String> {
         self.validate()?;
         encode(self, 65536)
     }
-    pub(super) fn decode(json: &str) -> AppResult<Self> {
+    pub(crate) fn needs_rebuild(&self, live: u64, pending: u64) -> AppResult<bool> {
+        rebuild_decision(self.stored_count, live, pending, self.files[1].bytes)
+    }
+    pub(crate) fn ensure_capacity(&self, pending: u64) -> AppResult<()> {
+        capacity(self.stored_count, pending, self.files[1].bytes)
+    }
+    pub(crate) fn decode(json: &str) -> AppResult<Self> {
         let seal: Self = decode(json, 65536)?;
         seal.validate()?;
         Ok(seal)
     }
     fn validate(&self) -> AppResult<()> {
         if self.version != 1
             || self.kind != "srvdb_generation_seal"
             || self.identity != self.descriptor.key()?
             || self.policy != Policy::current(self.descriptor.identity.dimensions)?
             || self.stored_count > RECORD_CAP
@@ -211,21 +217,21 @@ impl Intent {
     }
     pub(super) fn close(descriptor: Generation, epoch: Uuid, prior: &str) -> AppResult<Self> {
         let mut intent = Self::new(descriptor, epoch, Some(prior))?;
         intent.operation = IntentKind::Close;
         Ok(intent)
     }
     pub(super) fn encode(&self) -> AppResult<String> {
         self.validate()?;
         encode(self, 16384)
     }
-    pub(super) fn decode(json: &str) -> AppResult<Self> {
+    pub(crate) fn decode(json: &str) -> AppResult<Self> {
         let intent: Self = decode(json, 16384)?;
         intent.validate()?;
         Ok(intent)
     }
     fn validate(&self) -> AppResult<()> {
         if self.version != 1
             || self.kind != "srvdb_generation_intent"
             || self.identity != self.descriptor.key()?
             || self.policy != Policy::current(self.descriptor.identity.dimensions)?
             || self.expected_stored_count > RECORD_CAP
@@ -251,10 +257,48 @@ fn encode(value: &impl Serialize, cap: usize) -> AppResult<String> {
 fn decode<T: serde::de::DeserializeOwned + Serialize>(json: &str, cap: usize) -> AppResult<T> {
     if json.len() > cap {
         return Err(repair());
     }
     let value: T = serde_json::from_str(json).map_err(|_| repair())?;
     if encode(&value, cap)? != json {
         return Err(repair());
     }
     Ok(value)
 }
+
+fn rebuild_decision(stored: u64, live: u64, pending: u64, metadata: u64) -> AppResult<bool> {
+    if live > stored || stored > RECORD_CAP {
+        return Err(repair());
+    }
+    let unused = stored - live;
+    Ok(metadata >= METADATA_WATERMARK
+        || (unused > 0
+            && (unused >= stored.div_ceil(2)
+                || stored.checked_add(pending).is_none_or(|n| n > RECORD_CAP))))
+}
+fn capacity(stored: u64, pending: u64, metadata: u64) -> AppResult<()> {
+    if stored.checked_add(pending).is_none_or(|n| n > RECORD_CAP) || metadata >= METADATA_WATERMARK
+    {
+        return Err(crate::error::AppError::Intelligence(
+            crate::intelligence::types::IntelligenceErrorCode::ResourceLimit,
+        ));
+    }
+    Ok(())
+}
+#[cfg(test)]
+mod trigger_tests {
+    use super::*;
+    #[test]
+    fn knowledge_rebuild_certified_count_and_metadata_threshold_arithmetic() {
+        assert!(!rebuild_decision(0, 0, 0, 1).unwrap());
+        assert!(!rebuild_decision(3, 2, 0, 1).unwrap());
+        assert!(rebuild_decision(4, 2, 0, 1).unwrap());
+        assert!(!rebuild_decision(200000, 200000, 1, 1).unwrap());
+        assert!(rebuild_decision(200000, 199999, 1, 1).unwrap());
+        assert!(capacity(200000, 1, 1).is_err());
+        assert!(capacity(199999, 1, 1).is_ok());
+        assert!(capacity(1, u64::MAX, 1).is_err());
+        assert!(!rebuild_decision(0, 0, 0, METADATA_WATERMARK - 1).unwrap());
+        assert!(rebuild_decision(0, 0, 0, METADATA_WATERMARK).unwrap());
+        assert!(capacity(0, 0, METADATA_WATERMARK).is_err());
+    }
+}
diff --git a/src-tauri/src/knowledge/index/tests/lifecycle.rs b/src-tauri/src/knowledge/index/tests/lifecycle.rs
index 19a170c..f4b3ca1 100644
--- a/src-tauri/src/knowledge/index/tests/lifecycle.rs
+++ b/src-tauri/src/knowledge/index/tests/lifecycle.rs
@@ -521,21 +521,33 @@ async fn index_lifecycle_failed_close_retains_owner_then_later_observer_retries_
             vec![(vec![1.0; 128], super::persistence::metadata())],
         )
         .await
         .unwrap();
     let receipt = owner.persist(&ticket).await.unwrap();
     owner
         .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
         .await
         .unwrap();
     drop(ticket);
-    store.transaction(|tx|{tx.execute_batch("CREATE TRIGGER fail_owner_close BEFORE UPDATE ON knowledge_index_generation_seals WHEN NEW.dirty=1 AND json_extract(NEW.intent_json,'$.operation')='close' BEGIN SELECT RAISE(ABORT,'close failure'); END;")?;Ok(())}).unwrap();
+    store
+        .transaction(|tx| {
+            tx.execute_batch(
+                "CREATE TRIGGER fail_owner_close BEFORE \
+                    UPDATE ON knowledge_index_generation_seals \
+                    WHEN NEW.dirty=1 AND \
+                    json_extract(NEW.intent_json,\
+                    '$.operation')='close' BEGIN SELECT \
+                    RAISE(ABORT,'close failure'); END;",
+            )?;
+            Ok(())
+        })
+        .unwrap();
     assert!(matches!(
         owner.shutdown().await,
         Err(crate::error::AppError::Intelligence(
             crate::intelligence::types::IntelligenceErrorCode::Storage
         ))
     ));
     assert!(
         probe.try_lock_exclusive().is_err(),
         "normal failed close retains real owner/resources for retry"
     );
@@ -579,24 +591,26 @@ async fn index_lifecycle_join_spawn_failure_forces_failed_close_without_deadlock
         .unwrap();
     let receipt = owner.persist(&ticket).await.unwrap();
     owner
         .acknowledge(&ticket, owner.publish(&ticket, &receipt).await.unwrap())
         .await
         .unwrap();
     drop(ticket);
     store
         .transaction(|tx| {
             tx.execute_batch(
-                "CREATE TRIGGER fail_join_spawn_close \
-                 BEFORE UPDATE ON knowledge_index_generation_seals \
-                 WHEN NEW.dirty=1 AND json_extract(NEW.intent_json,'$.operation')='close' \
-                 BEGIN SELECT RAISE(ABORT,'close failure'); END;",
+                "CREATE TRIGGER fail_join_spawn_close BEFORE \
+                    UPDATE ON knowledge_index_generation_seals \
+                    WHEN NEW.dirty=1 AND \
+                    json_extract(NEW.intent_json,\
+                    '$.operation')='close' BEGIN SELECT \
+                    RAISE(ABORT,'close failure'); END;",
             )?;
             Ok(())
         })
         .unwrap();
     owner
         .hooks()
         .fail_join_spawn
         .store(true, std::sync::atomic::Ordering::Release);
     assert!(matches!(
         owner.shutdown().await,
@@ -677,10 +691,60 @@ async fn index_lifecycle_stale_receipt_cannot_publish_after_owner_shutdown() {
                         row.get::<_, Option<String>>(2)?,
                     ))
                 },
             )?)
         })
         .unwrap();
     assert_eq!(row.0, 1);
     assert!(row.1.is_none());
     assert!(row.2.is_some());
 }
+#[tokio::test]
+async fn index_lifecycle_dropped_begin_maintenance_observer_releases_entered_fence() {
+    let dir = tempfile::tempdir().unwrap();
+    let owner = IndexOwner::new(
+        dir.path().join("indexes"),
+        Arc::new(LocalStore::in_memory().unwrap()),
+    );
+    owner.start_owned(Arc::new(())).await.unwrap();
+    let source = owner
+        .create(super::persistence::generation(), Arc::new(()))
+        .unwrap();
+    owner
+        .enqueue_open(&source)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    owner
+        .add(
+            &source,
+            vec![(vec![1.; 128], super::persistence::metadata())],
+        )
+        .await
+        .unwrap();
+    owner.persist(&source).await.unwrap();
+    let (entered, release) = block_command(&owner);
+    let begin = {
+        let owner = owner.clone();
+        let source = source.clone();
+        tokio::spawn(async move { owner.begin_maintenance(&source).await })
+    };
+    tokio::task::spawn_blocking(move || entered.recv_timeout(std::time::Duration::from_secs(2)))
+        .await
+        .unwrap()
+        .unwrap();
+    begin.abort();
+    assert!(matches!(begin.await,
+Err(error) if error.is_cancelled()));
+    drop(release);
+    let next = owner.begin_maintenance(&source).await;
+    let released = next.is_ok();
+    if let Ok(ticket) = next {
+        owner.end_maintenance(ticket).await.unwrap();
+    }
+    owner.shutdown().await.unwrap();
+    assert!(
+        released,
+        "lost begin observer stranded the source maintenance fence"
+    );
+}
diff --git a/src-tauri/src/knowledge/index_tests.rs b/src-tauri/src/knowledge/index_tests.rs
new file mode 100644
index 0000000..cd3f427
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests.rs
@@ -0,0 +1,489 @@
+use super::{
+    embedding::{EmbeddingExecution, Embeddings},
+    service::{CommitPoint, KnowledgeService},
+    types::*,
+};
+use crate::{
+    app::CredentialChange,
+    error::{AppError, AppResult},
+    intelligence::{test_support::*, types::*},
+};
+use std::sync::{
+    Arc,
+    atomic::{AtomicUsize, Ordering},
+};
+use uuid::Uuid;
+
+#[derive(Default)]
+struct Axes {
+    vectors: std::sync::Mutex<std::collections::VecDeque<Vec<f32>>>,
+    calls: AtomicUsize,
+    requests: std::sync::Mutex<Vec<Vec<String>>>,
+    block: std::sync::atomic::AtomicBool,
+    entered: tokio::sync::Notify,
+    release: tokio::sync::Notify,
+    after: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
+}
+#[async_trait::async_trait]
+impl Embeddings for Axes {
+    async fn embed(
+        &self,
+        _: &ModelProfile,
+        inputs: &[String],
+        _execution: EmbeddingExecution,
+    ) -> AppResult<Vec<Vec<f32>>> {
+        self.calls.fetch_add(1, Ordering::SeqCst);
+        self.requests.lock().unwrap().push(inputs.to_vec());
+        if self.block.load(Ordering::SeqCst) {
+            self.entered.notify_one();
+            self.release.notified().await;
+        }
+        if let Some(after) = self.after.lock().unwrap().take() {
+            after();
+        }
+        Ok(inputs
+            .iter()
+            .map(|_| {
+                self.vectors.lock().unwrap().pop_front().unwrap_or_else(|| {
+                    let mut v = vec![0.0; 128];
+                    v[0] = 1.0;
+                    v
+                })
+            })
+            .collect())
+    }
+}
+struct KnowledgeHarness {
+    h: IntelligenceHarness,
+    service: KnowledgeService,
+    index: super::index::IndexOwner,
+    embeddings: Arc<Axes>,
+}
+impl KnowledgeHarness {
+    async fn new() -> Self {
+        Self::with_auto(false).await
+    }
+    async fn with_auto(auto: bool) -> Self {
+        let h = IntelligenceHarness::new();
+        let embeddings = Arc::new(Axes::default());
+        let profile = ModelProfile {
+            kind: ServiceKind::Embedding,
+            max_context_tokens: None,
+            max_output_tokens: None,
+            dimensions: Some(128),
+            ..test_llm_profile()
+        };
+        h.config
+            .save(SaveModelProfile {
+                profile: profile.clone(),
+                credential: CredentialChange::Replace {
+                    api_key: "synthetic-embedding-key".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let mut settings = h.config.settings().await.unwrap();
+        settings.embedding_profile_id = Some(profile.id);
+        h.config.save_settings(settings).await.unwrap();
+        let index =
+            super::index::IndexOwner::new(h.temp_dir.path().join("indexes"), h.store.clone());
+        index.start_owned(Arc::new(())).await.unwrap();
+        let service = KnowledgeService::new(
+            h.store.clone(),
+            h.config.clone(),
+            h.jobs.clone(),
+            embeddings.clone(),
+            index.clone(),
+            h.temp_dir.path().to_path_buf(),
+        );
+        service.register_handlers().unwrap();
+        if auto {
+            let documents = super::documents::DocumentService::new(
+                h.store.clone(),
+                h.config.clone(),
+                h.jobs.clone(),
+                Arc::new(FixtureParser),
+                h.temp_dir.path().to_path_buf(),
+            );
+            documents.register_import_handler().unwrap();
+            service.register_continuation().unwrap();
+        }
+        h.jobs.start().unwrap();
+        Self {
+            h,
+            service,
+            index,
+            embeddings,
+        }
+    }
+    fn import_text(&self, title: &str, text: &str) -> Uuid {
+        let id = Uuid::new_v4();
+        let chunk = Uuid::new_v4();
+        let location = TextLocation {
+            page: None,
+            paragraph: 1,
+            heading: None,
+            start_char: 0,
+            end_char: text.chars().count() as u32,
+        };
+        self.h
+            .store
+            .transaction(|tx| {
+                tx.execute(
+                    "INSERT INTO documents(id,sha256,uri,title,\
+                    created_at,origin,kind,status,\
+                    parser_identity) VALUES (?1,?2,'',?3,1,\
+                    'native','text','parsed',?4)",
+                    rusqlite::params![id.to_string(), "a".repeat(64), title, super::PARSER_ID],
+                )?;
+                tx.execute(
+                    "INSERT INTO document_chunks(id,document_id,\
+                    chunk_index,text,location_json,content_hash,\
+                    document_version,index_state) VALUES (?1,?2,\
+                    0,?3,?4,?5,1,'parsed')",
+                    rusqlite::params![
+                        chunk.to_string(),
+                        id.to_string(),
+                        text,
+                        serde_json::to_string(&location).unwrap(),
+                        super::chunk_hash(text)
+                    ],
+                )?;
+                Ok(())
+            })
+            .unwrap();
+        id
+    }
+    async fn wait(&self, id: Uuid) -> JobView {
+        tokio::time::timeout(std::time::Duration::from_secs(15), async {
+            loop {
+                let j = self.h.store.get_job(id).unwrap().unwrap();
+                if !matches!(j.status, JobStatus::Queued | JobStatus::Running) {
+                    return j;
+                }
+                tokio::task::yield_now().await;
+            }
+        })
+        .await
+        .unwrap()
+    }
+    async fn index(&self, id: Uuid) -> JobView {
+        let j = self.service.reindex_owned(id, Arc::new(())).await.unwrap();
+        self.wait(j.id).await
+    }
+    async fn search(&self, query: &str) -> AppResult<KnowledgeResults> {
+        self.service
+            .search_owned(
+                KnowledgeSearch {
+                    query: query.into(),
+                    selected_segment_ids: vec![],
+                    session_id: None,
+                    limit: 5,
+                },
+                Arc::new(()),
+            )
+            .await
+    }
+    async fn close(&self) {
+        self.h.jobs.shutdown().await.unwrap();
+        self.index.shutdown().await.unwrap();
+    }
+}
+#[tokio::test]
+async fn knowledge_reindex_publishes_authoritative_ready_mapping_and_consecutive_searches_reopen_cleanly()
+ {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump.md", "额定电压 24 V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    for _ in 0..2 {
+        let result = h.search("额定电压").await.unwrap();
+        assert_eq!(result.hits[0].document_id, id);
+        assert_eq!(result.hits[0].text, "额定电压 24 V");
+        assert_eq!(result.identity.as_ref().unwrap().len(), 64);
+    }
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_crash_after_vector_persist_never_exposes_an_uncommitted_document() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump.md", "额定电压 24 V");
+    h.service.crash_at(CommitPoint::AfterVectorPersist);
+    assert_ne!(h.index(id).await.status, JobStatus::Succeeded);
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    let before = h.embeddings.calls.load(Ordering::SeqCst);
+    assert!(matches!(
+        h.search("额定电压").await,
+        Err(AppError::Intelligence(
+            IntelligenceErrorCode::IndexRepairRequired
+        ))
+    ));
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), before);
+    h.h.jobs.start().unwrap();
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_empty_library_and_broken_readiness_never_embed() {
+    let h = KnowledgeHarness::new().await;
+    let result = h.search("query").await.unwrap();
+    assert!(result.hits.is_empty());
+    assert_eq!(result.identity, None);
+    h.import_text("pump", "24V");
+    assert!(matches!(
+        h.search("query").await,
+        Err(AppError::Intelligence(
+            IntelligenceErrorCode::IndexRepairRequired
+        ))
+    ));
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
+    h.close().await;
+}
+
+#[tokio::test]
+async fn knowledge_reindex_replacement_persists_a_new_vector_id() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let before: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(before, "0");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let after: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(after, "1");
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_delete_survives_clean_reopen_and_never_embeds() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    let j = h.service.delete_owned(id, Arc::new(())).await.unwrap();
+    assert_eq!(h.wait(j.id).await.status, JobStatus::Succeeded);
+    assert!(
+        h.h.store
+            .transaction(|tx| Ok(!tx.query_row(
+                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
+                [id.to_string()],
+                |r| r.get::<_, bool>(0)
+            )?))
+            .unwrap()
+    );
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    assert!(h.search("24V").await.unwrap().hits.is_empty());
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.index.shutdown().await.unwrap();
+}
+
+#[tokio::test]
+async fn knowledge_reconcile_marks_stale_authoritative_hash_without_embedding() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE document_chunks SET content_hash=?1 WHERE document_id=?2",
+                rusqlite::params!["b".repeat(64), id.to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(status, "repair_required");
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_rebuild_reclaims_tombstone_ids_and_atomically_repoints_ready_mappings() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    let old: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                    [],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    h.service.rebuild_owned(Arc::new(())).await.unwrap();
+    let new: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                    [],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_ne!(old, new);
+    let vector: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(vector, "0");
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_preserves_healthy_hits_and_repairs_the_inverse_owner_of_malformed_metadata()
+ {
+    let h = KnowledgeHarness::new().await;
+    let broken = h.import_text("broken", "24V");
+    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
+    let good = h.import_text("healthy", "48V");
+    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let binding = super::service::state::binding(&h.h.store, &profile)
+        .unwrap()
+        .unwrap();
+    let key = binding.generation.key().unwrap();
+    let generation = binding.generation.generation;
+    let ticket = h
+        .index
+        .open_existing(binding.generation, Arc::new(()))
+        .unwrap();
+    h.index
+        .enqueue_open(&ticket)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    let mut vector = vec![0.0; 128];
+    vector[0] = 1.0;
+    let new = h
+        .index
+        .corrupt_metadata(&ticket, 0, vector, r#"{"schema_version":1}"#.into())
+        .await
+        .unwrap();
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE document_chunks SET vector_id=?1 WHERE document_id=?2",
+                rusqlite::params![new.to_string(), broken.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE knowledge_generation_mappings SET \
+                    vector_id=?1 WHERE document_id=?2 AND \
+                    identity=?3 AND generation=?4",
+                rusqlite::params![
+                    new.to_string(),
+                    broken.to_string(),
+                    key,
+                    generation.to_string()
+                ],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let receipt = h.index.persist(&ticket).await.unwrap();
+    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
+    h.index.acknowledge(&ticket, ack).await.unwrap();
+    let result = h.search("voltage").await.unwrap();
+    assert_eq!(result.hits.len(), 1);
+    assert_eq!(result.hits[0].document_id, good);
+    assert_eq!(result.unavailable_documents, 1);
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [broken.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(status, "repair_required");
+    h.close().await;
+}
+
+mod search;
+
+struct FixtureParser;
+#[async_trait::async_trait]
+impl super::parser::DocumentParser for FixtureParser {
+    async fn parse(
+        &self,
+        path: std::path::PathBuf,
+        _kind: DocumentKind,
+        _cancel: tokio::sync::watch::Receiver<bool>,
+    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
+        let text = std::fs::read_to_string(path).map_err(|_| IntelligenceErrorCode::ParseFailed)?;
+        let end = text.chars().count() as u32;
+        Ok(vec![TextBlock {
+            text,
+            location: TextLocation {
+                page: None,
+                paragraph: 1,
+                heading: None,
+                start_char: 0,
+                end_char: end,
+            },
+        }])
+    }
+}
+mod binding;
+mod continuation;
+
+mod recovery;
+
+mod publication;
diff --git a/src-tauri/src/knowledge/index_tests/binding.rs b/src-tauri/src/knowledge/index_tests/binding.rs
new file mode 100644
index 0000000..670b43c
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests/binding.rs
@@ -0,0 +1,187 @@
+use super::*;
+#[tokio::test]
+async fn knowledge_binding_none_to_same_dimension_and_profile_endpoint_model_edits() {
+    let h = KnowledgeHarness::new().await;
+    let mut profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    profile.dimensions = None;
+    h.h.config
+        .save(SaveModelProfile {
+            profile: profile.clone(),
+            credential: CredentialChange::Keep,
+        })
+        .await
+        .unwrap();
+    let id = h.import_text("binding", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let first = h.search("24V").await.unwrap().identity;
+    profile.dimensions = Some(128);
+    h.h.config
+        .save(SaveModelProfile {
+            profile: profile.clone(),
+            credential: CredentialChange::Keep,
+        })
+        .await
+        .unwrap();
+    assert_eq!(h.search("24V").await.unwrap().identity, first);
+    profile.name = "Renamed".into();
+    profile.timeout_ms = 2000;
+    h.h.config
+        .save(SaveModelProfile {
+            profile: profile.clone(),
+            credential: CredentialChange::Keep,
+        })
+        .await
+        .unwrap();
+    assert_eq!(h.search("24V").await.unwrap().identity, first);
+    for change in ["endpoint", "model", "profile"] {
+        match change {
+            "endpoint" => profile.base_url = "http://127.0.0.1:12346/v1".into(),
+            "model" => profile.model = Some("changed-model".into()),
+            "profile" => profile.id = Uuid::new_v4(),
+            _ => unreachable!(),
+        }
+        h.h.config
+            .save(SaveModelProfile {
+                profile: profile.clone(),
+                credential: if change == "profile" {
+                    CredentialChange::Replace {
+                        api_key: "synthetic".into(),
+                    }
+                } else {
+                    CredentialChange::Keep
+                },
+            })
+            .await
+            .unwrap();
+        let mut settings = h.h.config.settings().await.unwrap();
+        settings.embedding_profile_id = Some(profile.id);
+        h.h.config.save_settings(settings).await.unwrap();
+        let calls = h.embeddings.calls.load(Ordering::SeqCst);
+        assert!(h.search("24V").await.is_err(), "{change}");
+        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        assert_ne!(h.search("24V").await.unwrap().identity, first, "{change}");
+    }
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_binding_concurrent_first_index_conflicting_actual_dimensions_has_one_winner() {
+    let h = IntelligenceHarness::new();
+    let profile = ModelProfile {
+        kind: ServiceKind::Embedding,
+        max_context_tokens: None,
+        max_output_tokens: None,
+        dimensions: None,
+        ..test_llm_profile()
+    };
+    h.config
+        .save(SaveModelProfile {
+            profile: profile.clone(),
+            credential: CredentialChange::Replace {
+                api_key: "synthetic".into(),
+            },
+        })
+        .await
+        .unwrap();
+    let mut settings = h.config.settings().await.unwrap();
+    settings.embedding_profile_id = Some(profile.id);
+    h.config.save_settings(settings).await.unwrap();
+    let index = crate::knowledge::index::IndexOwner::new(
+        h.temp_dir.path().join("indexes"),
+        h.store.clone(),
+    );
+    index.start_owned(Arc::new(())).await.unwrap();
+    let second_jobs = Arc::new(crate::intelligence::jobs::JobScheduler::new(
+        h.store.clone(),
+    ));
+    let first_vectors = Arc::new(Axes::default());
+    let second_vectors = Arc::new(Axes::default());
+    let mut v = vec![0.; 256];
+    v[0] = 1.;
+    second_vectors.vectors.lock().unwrap().push_back(v);
+    first_vectors.block.store(true, Ordering::SeqCst);
+    second_vectors.block.store(true, Ordering::SeqCst);
+    let first = KnowledgeService::new(
+        h.store.clone(),
+        h.config.clone(),
+        h.jobs.clone(),
+        first_vectors.clone(),
+        index.clone(),
+        h.temp_dir.path().into(),
+    );
+    let second = KnowledgeService::new(
+        h.store.clone(),
+        h.config.clone(),
+        second_jobs.clone(),
+        second_vectors.clone(),
+        index.clone(),
+        h.temp_dir.path().into(),
+    );
+    first.register_handlers().unwrap();
+    second.register_handlers().unwrap();
+    h.jobs.start().unwrap();
+    second_jobs.start().unwrap();
+    let fixture = KnowledgeHarness {
+        h,
+        service: first.clone(),
+        index: index.clone(),
+        embeddings: first_vectors.clone(),
+    };
+    let one = fixture.import_text("one", "24V");
+    let two = fixture.import_text("two", "48V");
+    let job1 = first.reindex_owned(one, Arc::new(())).await.unwrap();
+    let job2 = second.reindex_owned(two, Arc::new(())).await.unwrap();
+    let entered = tokio::time::timeout(std::time::Duration::from_secs(3), async {
+        first_vectors.entered.notified().await;
+        second_vectors.entered.notified().await;
+    })
+    .await;
+    first_vectors.block.store(false, Ordering::SeqCst);
+    second_vectors.block.store(false, Ordering::SeqCst);
+    first_vectors.release.notify_waiters();
+    second_vectors.release.notify_waiters();
+    assert!(
+        entered.is_ok(),
+        "both independent cores must enter before either publishes"
+    );
+    let statuses = [
+        fixture.wait(job1.id).await.status,
+        fixture.wait(job2.id).await.status,
+    ];
+    assert_eq!(
+        statuses
+            .iter()
+            .filter(|s| **s == JobStatus::Succeeded)
+            .count(),
+        1,
+        "conflicting actual dimensions must not both bind"
+    );
+    let (active, ready): (u32, u32) = fixture
+        .h
+        .store
+        .transaction(|tx| {
+            Ok((
+                tx.query_row(
+                    "SELECT count(*) FROM \
+                    knowledge_index_generations WHERE \
+                    state='active' AND semantic_key IS NOT NULL",
+                    [],
+                    |r| r.get(0),
+                )?,
+                tx.query_row(
+                    "SELECT count(*) FROM documents WHERE status='ready'",
+                    [],
+                    |r| r.get(0),
+                )?,
+            ))
+        })
+        .unwrap();
+    assert_eq!((active, ready), (1, 1));
+    second_jobs.shutdown().await.unwrap();
+    fixture.close().await;
+}
diff --git a/src-tauri/src/knowledge/index_tests/continuation.rs b/src-tauri/src/knowledge/index_tests/continuation.rs
new file mode 100644
index 0000000..473f531
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests/continuation.rs
@@ -0,0 +1,136 @@
+use super::*;
+async fn import(h: &KnowledgeHarness, name: &str) -> JobView {
+    let path = h.h.temp_dir.path().join(name);
+    std::fs::write(&path, "额定电压24V").unwrap();
+    let documents = super::super::documents::DocumentService::new(
+        h.h.store.clone(),
+        h.h.config.clone(),
+        h.h.jobs.clone(),
+        Arc::new(FixtureParser),
+        h.h.temp_dir.path().to_path_buf(),
+    );
+    let job = documents.import(path).await.unwrap();
+    let parent = h.wait(job.id).await;
+    assert_eq!(parent.status, JobStatus::Succeeded);
+    parent
+}
+async fn wait_state(h: &KnowledgeHarness, id: Uuid, want: &str) -> bool {
+    tokio::time::timeout(std::time::Duration::from_secs(3), async {
+        loop {
+            let status: String =
+                h.h.store
+                    .transaction(|tx| {
+                        Ok(tx.query_row(
+                            "SELECT status FROM documents WHERE id=?1",
+                            [id.to_string()],
+                            |r| r.get(0),
+                        )?)
+                    })
+                    .unwrap();
+            if status == want {
+                return;
+            }
+            tokio::task::yield_now().await;
+        }
+    })
+    .await
+    .is_ok()
+}
+#[tokio::test]
+async fn knowledge_import_continues_only_committed_success_into_reindex() {
+    let h = KnowledgeHarness::with_auto(true).await;
+    let parent = import(&h, "pump.md").await;
+    let id = parent.object_id.unwrap();
+    let ready = wait_state(&h, id, "ready").await;
+    h.close().await;
+    assert!(
+        ready,
+        "committed successful Import did not continue into indexing"
+    );
+    let count: u32 =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT count(*) FROM intelligence_jobs WHERE \
+                    kind='reindex' AND object_id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(count, 1);
+}
+#[tokio::test]
+async fn knowledge_import_without_selected_embedding_stays_parsed_with_deferred_reason() {
+    let h = KnowledgeHarness::with_auto(true).await;
+    let mut settings = h.h.config.settings().await.unwrap();
+    settings.embedding_profile_id = None;
+    h.h.config.save_settings(settings).await.unwrap();
+    let parent = import(&h, "pump.md").await;
+    let id = parent.object_id.unwrap();
+    let deferred = tokio::time::timeout(std::time::Duration::from_secs(3), async {
+        loop {
+            let exists: bool =
+                h.h.store
+                    .transaction(|tx| {
+                        Ok(tx.query_row(
+                            "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_repairs WHERE document_id=?1 AND \
+                    reason='auto_index_deferred')",
+                            [id.to_string()],
+                            |r| r.get(0),
+                        )?)
+                    })
+                    .unwrap();
+            if exists {
+                return;
+            }
+            tokio::task::yield_now().await;
+        }
+    })
+    .await
+    .is_ok();
+    h.close().await;
+    assert!(deferred);
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(status, "parsed");
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
+}
+#[tokio::test]
+async fn knowledge_manual_reindex_observer_loss_still_wakes_committed_admission() {
+    let h = KnowledgeHarness::new().await;
+    let first = h.import_text("first", "24V");
+    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
+    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
+    let id = h.import_text("second", "48V");
+    let (entered, wait) = tokio::sync::oneshot::channel();
+    let (release, receive) = std::sync::mpsc::channel();
+    h.service
+        .at_checkpoint(CommitPoint::AdmissionCallback, move || {
+            let _ = entered.send(());
+            let _ = receive.recv_timeout(std::time::Duration::from_secs(5));
+        });
+    let observer = {
+        let service = h.service.clone();
+        tokio::spawn(async move { service.reindex_owned(id, Arc::new(())).await })
+    };
+    wait.await.unwrap();
+    observer.abort();
+    assert!(observer.await.unwrap_err().is_cancelled());
+    release.send(()).unwrap();
+    let ready = wait_state(&h, id, "ready").await;
+    h.close().await;
+    assert!(
+        ready,
+        "entered configuration admission committed after observer loss but did not wake the scheduler"
+    );
+}
diff --git a/src-tauri/src/knowledge/index_tests/publication.rs b/src-tauri/src/knowledge/index_tests/publication.rs
new file mode 100644
index 0000000..567a8b7
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests/publication.rs
@@ -0,0 +1,529 @@
+use super::*;
+fn document_chunks(h: &KnowledgeHarness, count: u32) -> Uuid {
+    let id = h.import_text("paged", "first");
+    h.h.store
+        .transaction(|tx| {
+            for ordinal in 1..count {
+                let text = format!("chunk {ordinal}");
+                let location = TextLocation {
+                    page: None,
+                    paragraph: ordinal + 1,
+                    heading: Some(format!("heading {ordinal}")),
+                    start_char: 0,
+                    end_char: text.chars().count() as u32,
+                };
+                tx.execute(
+                    "INSERT INTO document_chunks(id,document_id,\
+                    chunk_index,text,location_json,content_hash,\
+                    document_version,index_state) VALUES (?1,?2,\
+                    ?3,?4,?5,?6,1,'parsed')",
+                    rusqlite::params![
+                        Uuid::new_v4().to_string(),
+                        id.to_string(),
+                        ordinal,
+                        text,
+                        serde_json::to_string(&location).unwrap(),
+                        crate::knowledge::chunk_hash(&text)
+                    ],
+                )?;
+            }
+            Ok(())
+        })
+        .unwrap();
+    id
+}
+#[tokio::test]
+async fn knowledge_publication_real_multi_page_ready_and_dense_rebuild() {
+    let h = KnowledgeHarness::new().await;
+    let id = document_chunks(&h, 241);
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let ready: u32 =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT count(*) FROM document_chunks WHERE \
+                    document_id=?1 AND index_state='ready'",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(ready, 241);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.service.rebuild_owned(Arc::new(())).await.unwrap();
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    let (count, last): (u32, String) =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT count(*),max(CAST(vector_id AS \
+                    INTEGER)) FROM document_chunks WHERE \
+                    document_id=?1 AND index_state='ready'",
+                    [id.to_string()],
+                    |r| Ok((r.get(0)?, r.get::<_, u32>(1)?.to_string())),
+                )?)
+            })
+            .unwrap();
+    assert_eq!((count, last), (241, "240".into()));
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_publication_rolls_back_every_page_on_document_location_pointer_or_evidence_change()
+ {
+    for mutation in [
+        "version",
+        "sha",
+        "parser",
+        "import",
+        "location",
+        "pointer",
+        "evidence",
+        "missing_evidence",
+        "binding",
+    ] {
+        let h = KnowledgeHarness::new().await;
+        let id = document_chunks(&h, 241);
+        let store = h.h.store.clone();
+        h.service.at_checkpoint(CommitPoint::BeforeReady, move || {
+            store
+                .transaction(|tx| {
+                    match mutation {
+                        "version" => {
+                            tx.execute(
+                                "UPDATE documents SET version=2 WHERE id=?1",
+                                [id.to_string()],
+                            )?;
+                        }
+                        "sha" => {
+                            tx.execute(
+                                "UPDATE documents SET sha256=?1 WHERE id=?2",
+                                rusqlite::params!["b".repeat(64), id.to_string()],
+                            )?;
+                        }
+                        "parser" => {
+                            tx.execute(
+                                "UPDATE documents SET parser_identity='different' WHERE id=?1",
+                                [id.to_string()],
+                            )?;
+                        }
+                        "import" => {
+                            tx.execute(
+                                "UPDATE documents SET import_job_id=(SELECT \
+                    id FROM intelligence_jobs WHERE object_id=?1 \
+                    AND kind='reindex' ORDER BY created_at_ms \
+                    DESC LIMIT 1) WHERE id=?1",
+                                [id.to_string()],
+                            )?;
+                        }
+                        "location" => {
+                            tx.execute(
+                                "UPDATE document_chunks SET \
+                    location_json=json_set(location_json,\
+                    '$.heading','changed') WHERE document_id=?1 \
+                    AND chunk_index=240",
+                                [id.to_string()],
+                            )?;
+                        }
+                        "pointer" => {
+                            tx.execute(
+                                "UPDATE knowledge_index_generations SET active_generation=?1",
+                                [Uuid::new_v4().to_string()],
+                            )?;
+                        }
+                        "evidence" => {
+                            tx.execute(
+                                "UPDATE knowledge_index_work_items SET \
+                    document_fence_json='{}' WHERE ordinal=240",
+                                [],
+                            )?;
+                        }
+                        "missing_evidence" => {
+                            tx.execute(
+                                "UPDATE knowledge_index_work_items SET \
+                    document_fence_json=NULL WHERE ordinal=240",
+                                [],
+                            )?;
+                        }
+                        "binding" => {
+                            tx.execute(
+                                "UPDATE knowledge_index_generations SET semantic_key=?1",
+                                ["c".repeat(64)],
+                            )?;
+                        }
+                        _ => unreachable!(),
+                    }
+                    Ok(())
+                })
+                .unwrap();
+        });
+        assert_ne!(h.index(id).await.status, JobStatus::Succeeded, "{mutation}");
+        let (published, ready, dirty): (u32, u32, u32) =
+            h.h.store
+                .transaction(|tx| {
+                    Ok((
+                        tx.query_row(
+                            "SELECT count(*) FROM knowledge_generation_mappings WHERE published=1",
+                            [],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT count(*) FROM document_chunks WHERE index_state='ready'",
+                            [],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT count(*) FROM knowledge_index_generation_seals WHERE dirty=1",
+                            [],
+                            |r| r.get(0),
+                        )?,
+                    ))
+                })
+                .unwrap();
+        assert_eq!((published, ready, dirty), (0, 0, 1), "{mutation}");
+        h.close().await;
+    }
+}
+#[tokio::test]
+async fn knowledge_reindex_every_commit_boundary_excludes_unpublished_and_explicit_retry_succeeds()
+{
+    for point in [
+        CommitPoint::AfterSqliteIntent,
+        CommitPoint::AfterVectorMutation,
+        CommitPoint::AfterVectorPersist,
+        CommitPoint::BeforeReady,
+    ] {
+        let h = KnowledgeHarness::new().await;
+        let id = h.import_text("crash", "24V");
+        h.service.crash_at(point);
+        assert_ne!(h.index(id).await.status, JobStatus::Succeeded);
+        h.close().await;
+        h.index.start_owned(Arc::new(())).await.unwrap();
+        let before = h.embeddings.calls.load(Ordering::SeqCst);
+        assert!(h.search("24V").await.is_err());
+        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), before);
+        h.h.jobs.start().unwrap();
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
+        h.close().await;
+    }
+}
+#[tokio::test]
+async fn knowledge_reconcile_tombstone_pressure_rebuilds_locally_without_explicit_flag() {
+    let h = KnowledgeHarness::new().await;
+    let deleted = h.import_text("delete", "24V");
+    let survivor = h.import_text("survive", "48V");
+    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
+    let job = h.service.delete_owned(deleted, Arc::new(())).await.unwrap();
+    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    let old: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                    [],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    let (new, vector): (String, String) =
+        h.h.store
+            .transaction(|tx| {
+                Ok((
+                    tx.query_row(
+                        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                        [],
+                        |r| r.get(0),
+                    )?,
+                    tx.query_row(
+                        "SELECT vector_id FROM document_chunks WHERE document_id=?1",
+                        [survivor.to_string()],
+                        |r| r.get(0),
+                    )?,
+                ))
+            })
+            .unwrap();
+    assert_ne!(
+        new, old,
+        "capacity consumed by retired/tombstoned records needs local replacement"
+    );
+    assert_eq!(vector, "0");
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, survivor);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_reconcile_clean_last_document_deletion_rebuilds_to_empty_generation() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("last", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let job = h.service.delete_owned(id, Arc::new(())).await.unwrap();
+    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    let count: u64 =
+        h.h.store
+            .transaction(|tx| {
+                let mut q = tx.prepare(
+                    "SELECT s.seal_json FROM \
+                    knowledge_index_generation_seals s JOIN \
+                    knowledge_index_generations g ON \
+                    g.identity=s.identity AND \
+                    g.active_generation=s.generation WHERE \
+                    s.dirty=0",
+                )?;
+                let mut rows = q.query([])?;
+                let row = rows.next()?.unwrap();
+                Ok(
+                    crate::knowledge::index::Seal::decode(crate::knowledge::service::state::text(
+                        row, 0, 65536,
+                    )?)?
+                    .stored_count,
+                )
+            })
+            .unwrap();
+    assert_eq!(
+        count, 0,
+        "empty library should reclaim fully tombstoned capacity"
+    );
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_rebuild_rejects_late_uncovered_survivor_chunk_without_switching_pointer() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("survivor", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let old: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                    [],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    let store = h.h.store.clone();
+    h.service
+        .at_checkpoint(CommitPoint::AfterVectorPersist, move || {
+            store
+                .transaction(|tx| {
+                    let (identity, generation): (String, String) = tx.query_row(
+                        "SELECT identity,active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                        [],
+                        |r| Ok((r.get(0)?, r.get(1)?)),
+                    )?;
+                    let chunk = Uuid::new_v4();
+                    let location = TextLocation {
+                        page: None,
+                        paragraph: 2,
+                        heading: None,
+                        start_char: 0,
+                        end_char: 3,
+                    };
+                    let hash = crate::knowledge::chunk_hash("48V");
+                    tx.execute(
+                        "INSERT INTO document_chunks(id,document_id,\
+                    chunk_index,text,location_json,content_hash,\
+                    document_version,index_state,\
+                    embedding_identity,vector_id) VALUES (?1,?2,\
+                    1,'48V',?3,?4,1,'ready',?5,'1')",
+                        rusqlite::params![
+                            chunk.to_string(),
+                            id.to_string(),
+                            serde_json::to_string(&location).unwrap(),
+                            hash,
+                            identity
+                        ],
+                    )?;
+                    tx.execute(
+                        "INSERT INTO \
+                    knowledge_generation_mappings(identity,\
+                    generation,chunk_id,document_id,\
+                    document_version,vector_id,content_hash,\
+                    published) VALUES (?1,?2,?3,?4,1,'1',?5,1)",
+                        rusqlite::params![
+                            identity,
+                            generation,
+                            chunk.to_string(),
+                            id.to_string(),
+                            hash
+                        ],
+                    )?;
+                    Ok(())
+                })
+                .unwrap();
+        });
+    assert!(
+        h.service.rebuild_owned(Arc::new(())).await.is_err(),
+        "every current survivor chunk must be covered before swap"
+    );
+    let current: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                    [],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(current, old);
+    h.close().await;
+}
+
+pub(super) fn reassociate(store: &crate::storage::LocalStore, id: Uuid, mutation: &str) {
+    store
+        .transaction(|tx| {
+            match mutation {
+                "sha" => {
+                    tx.execute(
+                        "UPDATE documents SET sha256=?1 WHERE id=?2",
+                        rusqlite::params!["b".repeat(64), id.to_string()],
+                    )?;
+                }
+                "parser" => {
+                    tx.execute(
+                        "UPDATE documents SET parser_identity='changed' WHERE id=?1",
+                        [id.to_string()],
+                    )?;
+                }
+                "import" => {
+                    tx.execute(
+                        "UPDATE documents SET import_job_id=(SELECT \
+                    id FROM intelligence_jobs WHERE object_id=?1 \
+                    AND kind='reindex' ORDER BY created_at_ms \
+                    DESC LIMIT 1) WHERE id=?1",
+                        [id.to_string()],
+                    )?;
+                }
+                "location" => {
+                    tx.execute(
+                        "UPDATE document_chunks SET \
+                    location_json=json_set(location_json,\
+                    '$.heading','changed') WHERE document_id=?1",
+                        [id.to_string()],
+                    )?;
+                }
+                "mapping" => {
+                    tx.execute(
+                        "UPDATE document_chunks SET vector_id='17' WHERE document_id=?1",
+                        [id.to_string()],
+                    )?;
+                    tx.execute(
+                        "UPDATE knowledge_generation_mappings SET \
+                    vector_id='17' WHERE document_id=?1 AND \
+                    published=1",
+                        [id.to_string()],
+                    )?;
+                }
+                _ => unreachable!(),
+            }
+            Ok(())
+        })
+        .unwrap();
+}
+#[tokio::test]
+async fn knowledge_local_copy_freezes_rebuild_associations_before_vector_copy() {
+    for mutation in ["sha", "parser", "import", "location", "mapping"] {
+        let h = KnowledgeHarness::new().await;
+        let id = h.import_text("survivor", "24V");
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        let old: String =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                        [],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        let store = h.h.store.clone();
+        h.service
+            .at_checkpoint(CommitPoint::BeforeLocalCopy, move || {
+                reassociate(&store, id, mutation)
+            });
+        assert!(
+            h.service.rebuild_owned(Arc::new(())).await.is_err(),
+            "pre-copy {mutation} association must stay frozen"
+        );
+        let current: String =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                        [],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        assert_eq!(current, old, "{mutation}");
+        h.close().await;
+    }
+}
+
+#[tokio::test]
+async fn knowledge_rebuild_failed_target_keeps_source_clean_searchable_and_reopenable() {
+    for point in [
+        CommitPoint::BeforeLocalCopy,
+        CommitPoint::AfterVectorPersist,
+    ] {
+        let h = KnowledgeHarness::new().await;
+        let id = h.import_text("source", "24V");
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        let old: String =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    state='active'",
+                        [],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        h.service.crash_at(point);
+        assert!(h.service.rebuild_owned(Arc::new(())).await.is_err());
+        let clean: bool =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT dirty=0 AND seal_json IS NOT NULL \
+                    FROM knowledge_index_generation_seals WHERE \
+                    generation=?1",
+                        [&old],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        assert!(
+            clean,
+            "failed target must complete the source refresh after failure"
+        );
+        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
+        h.close().await;
+        h.index.start_owned(Arc::new(())).await.unwrap();
+        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
+        h.index.shutdown().await.unwrap();
+    }
+}
diff --git a/src-tauri/src/knowledge/index_tests/recovery.rs b/src-tauri/src/knowledge/index_tests/recovery.rs
new file mode 100644
index 0000000..04d7b89
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests/recovery.rs
@@ -0,0 +1,415 @@
+use super::*;
+
+async fn interrupted_delete(h: &KnowledgeHarness, id: Uuid) -> Uuid {
+    h.service.crash_at(CommitPoint::AfterVectorPersist);
+    let job = h.service.delete_owned(id, Arc::new(())).await.unwrap();
+    assert_ne!(h.wait(job.id).await.status, JobStatus::Succeeded);
+    job.id
+}
+fn pointer(h: &KnowledgeHarness) -> (String, String) {
+    h.h.store
+        .transaction(|tx| {
+            Ok(tx.query_row(
+                "SELECT identity,active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    active_generation IS NOT NULL",
+                [],
+                |r| Ok((r.get(0)?, r.get(1)?)),
+            )?)
+        })
+        .unwrap()
+}
+fn exists(h: &KnowledgeHarness, id: Uuid) -> bool {
+    h.h.store
+        .transaction(|tx| {
+            Ok(tx.query_row(
+                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
+                [id.to_string()],
+                |r| r.get(0),
+            )?)
+        })
+        .unwrap()
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_recovers_survivor_bits_locally_and_reopen_is_idempotent() {
+    let h = KnowledgeHarness::new().await;
+    let deleted = h.import_text("delete", "24V");
+    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
+    let survivor = h.import_text("survive", "48V");
+    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
+    let (key, old) = pointer(&h);
+    let source = h.h.temp_dir.path().join("indexes").join(&key).join(&old);
+    let before = std::fs::read(source.join("vectors.bin")).unwrap();
+    let survivor_bits = before[24 + 128 * 4..24 + 2 * 128 * 4].to_vec();
+    let job = interrupted_delete(&h, deleted).await;
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(
+        !exists(&h, deleted),
+        "witnessed deletion must finish locally"
+    );
+    let (_, new) = pointer(&h);
+    assert_ne!(new, old);
+    assert!(!source.exists());
+    let after = std::fs::read(
+        h.h.temp_dir
+            .path()
+            .join("indexes")
+            .join(&key)
+            .join(new)
+            .join("vectors.bin"),
+    )
+    .unwrap();
+    assert_eq!(&after[24..24 + 128 * 4], &survivor_bits);
+    let work: u32 =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
+                    [job.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(work, 0);
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, survivor);
+    h.index.shutdown().await.unwrap();
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(!exists(&h, deleted));
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_last_document_publishes_empty_destination() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("last", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let (_, old) = pointer(&h);
+    interrupted_delete(&h, id).await;
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(!exists(&h, id));
+    assert_ne!(pointer(&h).1, old);
+    assert!(h.search("24V").await.unwrap().hits.is_empty());
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_changed_vector_witness_preserves_bytes_and_deleting_work() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("tamper", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let (key, old) = pointer(&h);
+    let path =
+        h.h.temp_dir
+            .path()
+            .join("indexes")
+            .join(key)
+            .join(&old)
+            .join("vectors.bin");
+    let job = interrupted_delete(&h, id).await;
+    h.close().await;
+    let mut bytes = std::fs::read(&path).unwrap();
+    bytes[24] ^= 1;
+    std::fs::write(&path, &bytes).unwrap();
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(exists(&h, id));
+    assert_eq!(pointer(&h).1, old);
+    assert_eq!(std::fs::read(path).unwrap(), bytes);
+    let (status, work): (String, u32) =
+        h.h.store
+            .transaction(|tx| {
+                Ok((
+                    tx.query_row(
+                        "SELECT status FROM documents WHERE id=?1",
+                        [id.to_string()],
+                        |r| r.get(0),
+                    )?,
+                    tx.query_row(
+                        "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
+                        [job.to_string()],
+                        |r| r.get(0),
+                    )?,
+                ))
+            })
+            .unwrap();
+    assert_eq!(status, "deleting");
+    assert_eq!(work, 1);
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_destination_persist_crash_keeps_source_and_retry_finishes() {
+    let h = KnowledgeHarness::new().await;
+    let deleted = h.import_text("delete", "24V");
+    let survivor = h.import_text("survive", "48V");
+    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
+    let (key, old) = pointer(&h);
+    let source = h.h.temp_dir.path().join("indexes").join(&key).join(&old);
+    interrupted_delete(&h, deleted).await;
+    h.close().await;
+    let bytes = std::fs::read(source.join("vectors.bin")).unwrap();
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.crash_at(CommitPoint::AfterVectorPersist);
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert_eq!(pointer(&h).1, old);
+    assert!(exists(&h, deleted));
+    assert_eq!(std::fs::read(source.join("vectors.bin")).unwrap(), bytes);
+    h.index.shutdown().await.unwrap();
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(!exists(&h, deleted));
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.index.shutdown().await.unwrap();
+    let count = std::fs::read_dir(h.h.temp_dir.path().join("indexes").join(key))
+        .unwrap()
+        .count();
+    assert_eq!(
+        count, 1,
+        "abandoned destination and retired source are acknowledged locally"
+    );
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_pointer_crash_resumes_retirement_and_work_ack() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("last", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let (key, old) = pointer(&h);
+    let source = h.h.temp_dir.path().join("indexes").join(key).join(&old);
+    let job = interrupted_delete(&h, id).await;
+    h.close().await;
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.crash_at(CommitPoint::AfterPointerPublication);
+    assert!(h.service.reconcile_owned(Arc::new(())).await.is_err());
+    assert!(!exists(&h, id));
+    assert_ne!(pointer(&h).1, old);
+    assert!(source.exists());
+    h.index.shutdown().await.unwrap();
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert!(
+        !source.exists(),
+        "retired source cleanup must resume after pointer publication"
+    );
+    let count: u32 =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
+                    [job.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(count, 0);
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.index.shutdown().await.unwrap();
+}
+#[tokio::test]
+async fn knowledge_dirty_delete_reader_source_lock_is_exclusive_and_release_is_owned() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("lock", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let (key, old) = pointer(&h);
+    interrupted_delete(&h, id).await;
+    let evidence = h
+        .index
+        .recovery_evidence(&key, Uuid::parse_str(&old).unwrap())
+        .unwrap();
+    let target = h
+        .index
+        .create(
+            crate::knowledge::index::Generation {
+                generation: Uuid::new_v4(),
+                ..evidence.source().clone()
+            },
+            Arc::new(()),
+        )
+        .unwrap();
+    assert!(
+        matches!(
+            h.index.begin_recovery(&target, evidence.clone()).await,
+            Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
+        ),
+        "live engine retains source lock"
+    );
+    h.close().await;
+    h.index.start_owned(Arc::new(())).await.unwrap();
+    let target = h
+        .index
+        .create(
+            crate::knowledge::index::Generation {
+                generation: Uuid::new_v4(),
+                ..evidence.source().clone()
+            },
+            Arc::new(()),
+        )
+        .unwrap();
+    let reader = h.index.begin_recovery(&target, evidence).await.unwrap();
+    let path =
+        h.h.temp_dir
+            .path()
+            .join("indexes")
+            .join(key)
+            .join(old)
+            .join("srvdb.lock");
+    let lock = std::fs::OpenOptions::new()
+        .read(true)
+        .write(true)
+        .open(path)
+        .unwrap();
+    assert!(matches!(
+        lock.try_lock(),
+        Err(std::fs::TryLockError::WouldBlock)
+    ));
+    assert!(h.index.copy_recovery(&reader, 0, 201).await.is_err());
+    let copied = h.index.copy_recovery(&reader, 0, 200).await.unwrap();
+    assert_eq!(copied.len(), 1);
+    assert_eq!(copied[0].0, 0);
+    h.index.end_recovery(reader).await.unwrap();
+    assert!(lock.try_lock().is_ok());
+    drop(lock);
+    h.index.shutdown().await.unwrap();
+}
+
+#[tokio::test]
+async fn knowledge_local_copy_freezes_recovery_associations_before_vector_copy() {
+    for mutation in ["sha", "parser", "import", "location", "mapping"] {
+        let h = KnowledgeHarness::new().await;
+        let deleted = h.import_text("delete", "24V");
+        let survivor = h.import_text("survive", "48V");
+        assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
+        assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
+        let (_, old) = pointer(&h);
+        interrupted_delete(&h, deleted).await;
+        h.close().await;
+        h.index.start_owned(Arc::new(())).await.unwrap();
+        let store = h.h.store.clone();
+        let calls = h.embeddings.calls.load(Ordering::SeqCst);
+        h.service
+            .at_checkpoint(CommitPoint::BeforeLocalCopy, move || {
+                super::publication::reassociate(&store, survivor, mutation)
+            });
+        h.service.reconcile_owned(Arc::new(())).await.unwrap();
+        assert!(
+            exists(&h, deleted),
+            "pre-copy {mutation} association must reject deletion publication"
+        );
+        assert_eq!(pointer(&h).1, old, "{mutation}");
+        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+        h.index.shutdown().await.unwrap();
+    }
+}
+
+#[tokio::test]
+async fn knowledge_dirty_delete_rejects_unrecognized_or_symlink_source_entries_without_opening_them()
+ {
+    for entry in ["unexpected", "symlink"] {
+        let h = KnowledgeHarness::new().await;
+        let id = h.import_text("source", "24V");
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        let (key, old) = pointer(&h);
+        interrupted_delete(&h, id).await;
+        h.close().await;
+        let source = h.h.temp_dir.path().join("indexes").join(key).join(&old);
+        if entry == "unexpected" {
+            std::fs::write(source.join("unrecognized"), b"preserve").unwrap();
+        } else {
+            std::fs::remove_file(source.join("metadata.db")).unwrap();
+            #[cfg(unix)]
+            std::os::unix::fs::symlink("vectors.bin", source.join("metadata.db")).unwrap();
+        }
+        let calls = h.embeddings.calls.load(Ordering::SeqCst);
+        h.index.start_owned(Arc::new(())).await.unwrap();
+        h.service.reconcile_owned(Arc::new(())).await.unwrap();
+        assert!(exists(&h, id), "{entry}");
+        assert_eq!(pointer(&h).1, old);
+        assert!(source.exists());
+        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+        h.index.shutdown().await.unwrap();
+    }
+}
+#[tokio::test]
+async fn knowledge_delete_rejects_inconsistent_payload_document_fence() {
+    let h = KnowledgeHarness::new().await;
+    let target = h.import_text("target", "24V");
+    let foreign = h.import_text("foreign", "48V");
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE documents SET status='deleting' WHERE id IN (?1,?2)",
+                rusqlite::params![target.to_string(), foreign.to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let document =
+        h.h.store
+            .transaction(|tx| crate::knowledge::service::state::fence(tx, foreign))
+            .unwrap();
+    let input = crate::knowledge::service::DeleteInput {
+        document_id: target,
+        document_version: 1,
+        document,
+        operation_id: Uuid::new_v4(),
+        identity: None,
+        generation: None,
+    };
+    assert!(
+        h.service
+            .core
+            .delete(&input, Arc::new(()), None)
+            .await
+            .is_err()
+    );
+    assert!(exists(&h, target));
+    assert!(exists(&h, foreign));
+    h.close().await;
+}
+
+#[tokio::test]
+async fn knowledge_delete_resume_advances_past_full_page_of_invalid_payload_associations() {
+    let h = KnowledgeHarness::new().await;
+    for number in 1..=201u128 {
+        let document_id = Uuid::from_u128(number);
+        let job = Uuid::new_v4();
+        let document = crate::knowledge::index::DocumentFence {
+            id: document_id,
+            version: 1,
+            sha256: "a".repeat(64),
+            parser: "fixture".into(),
+            import_job: None,
+        };
+        let input = crate::knowledge::service::DeleteInput {
+            document_id,
+            document_version: if number == 201 { 1 } else { 2 },
+            document,
+            operation_id: job,
+            identity: None,
+            generation: None,
+        };
+        h.h.store.transaction(|tx| {
+            tx.execute("INSERT INTO documents(id,sha256,uri,title,created_at,origin,kind,status,parser_identity) VALUES (?1,?2,'','pending',1,'native','text','deleting','fixture')",rusqlite::params![document_id.to_string(),"a".repeat(64)])?;
+            crate::intelligence::store::admit_job(tx,crate::intelligence::store::JobDraft { id:job,kind:JobKind::DeleteDocument,session_id:None,object_id:Some(document_id),admission:JobAdmission::Manual{exclusion_key:format!("document:{document_id}")},payload:serde_json::json!({"version":1,"profiles":[],"input":input}),initial_failure:None })?;
+            tx.execute("UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",[job.to_string()])?;Ok(())
+        }).unwrap();
+    }
+    h.service.core.resume_deletes(Arc::new(())).await.unwrap();
+    assert!(
+        !exists(&h, Uuid::from_u128(201)),
+        "invalid retained associations must not block later valid deletion pages"
+    );
+    assert!(exists(&h, Uuid::from_u128(1)));
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
+    h.close().await;
+}
diff --git a/src-tauri/src/knowledge/index_tests/search.rs b/src-tauri/src/knowledge/index_tests/search.rs
new file mode 100644
index 0000000..a0e3d11
--- /dev/null
+++ b/src-tauri/src/knowledge/index_tests/search.rs
@@ -0,0 +1,803 @@
+use super::*;
+fn request(query: &str) -> KnowledgeSearch {
+    KnowledgeSearch {
+        query: query.into(),
+        selected_segment_ids: vec![],
+        session_id: None,
+        limit: 5,
+    }
+}
+#[tokio::test]
+async fn knowledge_search_two_slots_retain_capacity_until_cancelled_entered_work_finishes() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    h.embeddings.block.store(true, Ordering::SeqCst);
+    let service = h.service.clone();
+    let first =
+        tokio::spawn(async move { service.search_owned(request("first"), Arc::new(())).await });
+    h.embeddings.entered.notified().await;
+    let second = h.service.search_owned(request("second"), Arc::new(()));
+    tokio::pin!(second);
+    assert!(futures_util::poll!(&mut second).is_pending());
+    let third = tokio::time::timeout(
+        std::time::Duration::from_millis(100),
+        h.service.search_owned(request("third"), Arc::new(())),
+    )
+    .await;
+    // Release entered work before asserting, including the intentional RED.
+    h.embeddings.block.store(false, Ordering::SeqCst);
+    h.embeddings.release.notify_waiters();
+    let first = first.await.unwrap();
+    let second = second.await;
+    h.close().await;
+    assert!(matches!(
+        third,
+        Ok(Err(AppError::Intelligence(IntelligenceErrorCode::Busy)))
+    ));
+    assert!(matches!(
+        first,
+        Err(AppError::Intelligence(IntelligenceErrorCode::Cancelled))
+    ));
+    assert!(second.is_ok());
+}
+#[tokio::test]
+async fn knowledge_search_submitted_partial_and_multi_row_selection_is_authoritative() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let session = h.h.session();
+    let foreign = h.h.session();
+    let mut ids = Vec::new();
+    for (sequence, text) in [
+        (
+            0,
+            "A much longer transcript row containing few selected words and more content",
+        ),
+        (1, "A second longer row"),
+    ] {
+        let id = Uuid::new_v4();
+        h.h.store
+            .append_final_segment(&crate::storage::StoredTranscriptSegment {
+                id,
+                session_id: session,
+                direction: "source_to_target".into(),
+                sequence,
+                occurred_at: 1,
+                language: "en".into(),
+                text: text.into(),
+                is_final: true,
+                details: None,
+                occurred_at_ms: None,
+            })
+            .unwrap();
+        ids.push(id);
+    }
+    let partial = KnowledgeSearch {
+        query: "few selected words".into(),
+        selected_segment_ids: vec![ids[0]],
+        session_id: Some(session),
+        limit: 5,
+    };
+    h.service.search_owned(partial, Arc::new(())).await.unwrap();
+    assert_eq!(
+        h.embeddings.requests.lock().unwrap().last().unwrap(),
+        &["few selected words"]
+    );
+    let multi = KnowledgeSearch {
+        query: "more content\nA second".into(),
+        selected_segment_ids: ids.clone(),
+        session_id: Some(session),
+        limit: 5,
+    };
+    h.service.search_owned(multi, Arc::new(())).await.unwrap();
+    assert_eq!(
+        h.embeddings.requests.lock().unwrap().last().unwrap(),
+        &["more content\nA second"]
+    );
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    let invalid = KnowledgeSearch {
+        query: "few selected words".into(),
+        selected_segment_ids: ids,
+        session_id: Some(foreign),
+        limit: 5,
+    };
+    assert!(matches!(
+        h.service.search_owned(invalid, Arc::new(())).await,
+        Err(AppError::Intelligence(IntelligenceErrorCode::Configuration))
+    ));
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_late_loss_of_last_ready_document_returns_repair() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pump", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let store = h.h.store.clone();
+    *h.embeddings.after.lock().unwrap() = Some(Box::new(move || {
+        store
+            .transaction(|tx| {
+                tx.execute(
+                    "UPDATE documents SET status='repair_required' WHERE id=?1",
+                    [id.to_string()],
+                )?;
+                Ok(())
+            })
+            .unwrap();
+    }));
+    assert!(matches!(
+        h.search("24V").await,
+        Err(AppError::Intelligence(
+            IntelligenceErrorCode::IndexRepairRequired
+        ))
+    ));
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_during_replacement_preserves_parsing_and_parsed_states() {
+    let h = KnowledgeHarness::new().await;
+    let good = h.import_text("good", "24V");
+    let replacing = h.import_text("replace", "48V");
+    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(replacing).await.status, JobStatus::Succeeded);
+    for status in ["parsing", "parsed"] {
+        h.h.store
+            .transaction(|tx| {
+                tx.execute(
+                    "UPDATE documents SET status=?1,version=2,index_identity=NULL WHERE id=?2",
+                    rusqlite::params![status, replacing.to_string()],
+                )?;
+                tx.execute(
+                    "UPDATE document_chunks SET index_state='stale' WHERE document_id=?1",
+                    [replacing.to_string()],
+                )?;
+                Ok(())
+            })
+            .unwrap();
+        let result = h.search("voltage").await.unwrap();
+        assert_eq!(result.hits.len(), 1);
+        assert_eq!(result.hits[0].document_id, good);
+        let actual: String =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT status FROM documents WHERE id=?1",
+                        [replacing.to_string()],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        assert_eq!(actual, status);
+    }
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_removes_earlier_hits_when_later_candidate_repairs_same_document() {
+    let h = KnowledgeHarness::new().await;
+    let broken = h.import_text("broken", "24V");
+    let extra = Uuid::new_v4();
+    let location = TextLocation {
+        page: None,
+        paragraph: 2,
+        heading: None,
+        start_char: 0,
+        end_char: 3,
+    };
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "INSERT INTO document_chunks(id,document_id,\
+                    chunk_index,text,location_json,content_hash,\
+                    document_version,index_state) VALUES (?1,?2,\
+                    1,'48V',?3,?4,1,'parsed')",
+                rusqlite::params![
+                    extra.to_string(),
+                    broken.to_string(),
+                    serde_json::to_string(&location).unwrap(),
+                    crate::knowledge::chunk_hash("48V")
+                ],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
+    let good = h.import_text("good", "96V");
+    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
+        .unwrap()
+        .unwrap();
+    let key = binding.generation.key().unwrap();
+    let generation = binding.generation.generation;
+    let ticket = h
+        .index
+        .open_existing(binding.generation, Arc::new(()))
+        .unwrap();
+    h.index
+        .enqueue_open(&ticket)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    let mut vector = vec![0.0; 128];
+    vector[0] = 0.9;
+    vector[1] = (1.0f32 - 0.9 * 0.9).sqrt();
+    let new = h
+        .index
+        .corrupt_metadata(&ticket, 1, vector, r#"{"schema_version":1}"#.into())
+        .await
+        .unwrap();
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE document_chunks SET vector_id=?1 WHERE id=?2",
+                rusqlite::params![new.to_string(), extra.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE knowledge_generation_mappings SET \
+                    vector_id=?1 WHERE chunk_id=?2 AND \
+                    identity=?3 AND generation=?4",
+                rusqlite::params![
+                    new.to_string(),
+                    extra.to_string(),
+                    key,
+                    generation.to_string()
+                ],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let receipt = h.index.persist(&ticket).await.unwrap();
+    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
+    h.index.acknowledge(&ticket, ack).await.unwrap();
+    let result = h.search("voltage").await.unwrap();
+    assert_eq!(
+        result
+            .hits
+            .iter()
+            .map(|hit| hit.document_id)
+            .collect::<Vec<_>>(),
+        vec![good],
+        "the first good-looking chunk cannot survive repair of its document"
+    );
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_reconcile_malformed_metadata_repairs_only_current_inverse_owner() {
+    let h = KnowledgeHarness::new().await;
+    let broken = h.import_text("broken", "24V");
+    let good = h.import_text("good", "48V");
+    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
+        .unwrap()
+        .unwrap();
+    let key = binding.generation.key().unwrap();
+    let generation = binding.generation.generation;
+    let ticket = h
+        .index
+        .open_existing(binding.generation, Arc::new(()))
+        .unwrap();
+    h.index
+        .enqueue_open(&ticket)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    let mut vector = vec![0.0; 128];
+    vector[0] = 1.0;
+    let new = h
+        .index
+        .corrupt_metadata(&ticket, 0, vector, r#"{"schema_version":1}"#.into())
+        .await
+        .unwrap();
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE document_chunks SET vector_id=?1 WHERE document_id=?2",
+                rusqlite::params![new.to_string(), broken.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE knowledge_generation_mappings SET \
+                    vector_id=?1 WHERE document_id=?2 AND \
+                    identity=?3 AND generation=?4",
+                rusqlite::params![
+                    new.to_string(),
+                    broken.to_string(),
+                    key,
+                    generation.to_string()
+                ],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let receipt = h.index.persist(&ticket).await.unwrap();
+    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
+    h.index.acknowledge(&ticket, ack).await.unwrap();
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [good.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(
+        status, "ready",
+        "one malformed row must not quarantine healthy neighbors"
+    );
+    assert_eq!(h.search("voltage").await.unwrap().hits[0].document_id, good);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_actual_escaped_byte_budget_preserves_prefix_without_filling_lower_hits() {
+    let h = KnowledgeHarness::new().await;
+    let mut ids = Vec::new();
+    for n in 0..4 {
+        let id = h.import_text(&format!("document {n}"), "24V");
+        if n < 3 {
+            // Legal canonical SQLite location remains below16 MiB. Four repeated
+            // headings are under32 MiB raw but require nearly64 MiB escaped JSON.
+            let location = TextLocation {
+                page: None,
+                paragraph: 1,
+                heading: Some("\t".repeat(8 * 1024 * 1024 - 1024)),
+                start_char: 0,
+                end_char: 3,
+            };
+            h.h.store
+                .transaction(|tx| {
+                    tx.execute(
+                        "UPDATE document_chunks SET location_json=?1 WHERE document_id=?2",
+                        rusqlite::params![
+                            serde_json::to_string(&location).unwrap(),
+                            id.to_string()
+                        ],
+                    )?;
+                    Ok(())
+                })
+                .unwrap();
+        }
+        let score = 1.0 - n as f32 * 0.1;
+        let mut vector = vec![0.0; 128];
+        vector[0] = score;
+        vector[1] = (1.0 - score * score).sqrt();
+        h.embeddings.vectors.lock().unwrap().push_back(vector);
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        ids.push(id);
+    }
+    let result = h.search("voltage").await.unwrap();
+    assert_eq!(
+        result
+            .hits
+            .iter()
+            .map(|hit| hit.document_id)
+            .collect::<Vec<_>>(),
+        ids[..2]
+    );
+    assert_eq!(result.unavailable_documents, 0);
+    let actual = serde_json::to_vec(&result).unwrap().len();
+    assert!(actual <= 32 * 1024 * 1024);
+    assert!(actual > 31 * 1024 * 1024);
+    assert_eq!(
+        result.hits[0].location.heading.as_ref().unwrap().len(),
+        8 * 1024 * 1024 - 1024
+    );
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_corruption_repair_does_not_change_indexing_replacement() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("replace", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE documents SET status='indexing',version=2 WHERE id=?1",
+                [id.to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    crate::knowledge::service::state::mark_document_repair(&h.h.store, id, 1, "stale_hash")
+        .unwrap();
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(status, "indexing");
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_search_foreign_document_metadata_repairs_inverse_owner_only() {
+    let h = KnowledgeHarness::new().await;
+    let broken = h.import_text("broken", "24V");
+    let good = h.import_text("good", "48V");
+    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
+        .unwrap()
+        .unwrap();
+    let key = binding.generation.key().unwrap();
+    let generation = binding.generation.generation;
+    let metadata =
+        h.h.store
+            .transaction(|tx| {
+                let (chunk, hash, location): (String, String, String) = tx.query_row(
+                    "SELECT id,content_hash,location_json FROM \
+                    document_chunks WHERE document_id=?1",
+                    [good.to_string()],
+                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
+                )?;
+                Ok(crate::knowledge::index::ChunkMetadata::new(
+                    good,
+                    1,
+                    Uuid::parse_str(&chunk).unwrap(),
+                    hash,
+                    &serde_json::from_str(&location).unwrap(),
+                )?)
+            })
+            .unwrap();
+    let ticket = h
+        .index
+        .open_existing(binding.generation, Arc::new(()))
+        .unwrap();
+    h.index
+        .enqueue_open(&ticket)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    let mut vector = vec![0.; 128];
+    vector[0] = 1.;
+    let new = h
+        .index
+        .corrupt_metadata(
+            &ticket,
+            0,
+            vector,
+            serde_json::to_string(&metadata).unwrap(),
+        )
+        .await
+        .unwrap();
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE document_chunks SET vector_id=?1 WHERE document_id=?2",
+                rusqlite::params![new.to_string(), broken.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE knowledge_generation_mappings SET \
+                    vector_id=?1 WHERE document_id=?2 AND \
+                    identity=?3 AND generation=?4",
+                rusqlite::params![
+                    new.to_string(),
+                    broken.to_string(),
+                    key,
+                    generation.to_string()
+                ],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let receipt = h.index.persist(&ticket).await.unwrap();
+    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
+    h.index.acknowledge(&ticket, ack).await.unwrap();
+    let result = h.search("voltage").await.unwrap();
+    assert_eq!(
+        result
+            .hits
+            .iter()
+            .map(|h| h.document_id)
+            .collect::<Vec<_>>(),
+        vec![good]
+    );
+    assert_eq!(result.unavailable_documents, 1);
+    h.close().await;
+}
+#[tokio::test]
+async fn knowledge_reconcile_half_open_tombstone_and_last_orphan_are_local() {
+    let h = KnowledgeHarness::new().await;
+    let first = h.import_text("first", "24V");
+    let last = h.import_text("last", "48V");
+    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(last).await.status, JobStatus::Succeeded);
+    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
+        .unwrap()
+        .unwrap();
+    let ticket = h
+        .index
+        .open_existing(binding.generation, Arc::new(()))
+        .unwrap();
+    h.index
+        .enqueue_open(&ticket)
+        .unwrap()
+        .observe()
+        .await
+        .unwrap();
+    let metadata = crate::knowledge::index::ChunkMetadata::new(
+        Uuid::new_v4(),
+        1,
+        Uuid::new_v4(),
+        crate::knowledge::chunk_hash("orphan"),
+        &TextLocation {
+            page: None,
+            paragraph: 1,
+            heading: None,
+            start_char: 0,
+            end_char: 6,
+        },
+    )
+    .unwrap();
+    let mut vector = vec![0.; 128];
+    vector[0] = 1.;
+    assert_eq!(
+        h.index
+            .add(&ticket, vec![(vector, metadata)])
+            .await
+            .unwrap(),
+        vec![3]
+    );
+    let receipt = h.index.persist(&ticket).await.unwrap();
+    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
+    h.index.acknowledge(&ticket, ack).await.unwrap();
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.service.reconcile_owned(Arc::new(())).await.unwrap();
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    let result = h.search("voltage").await.unwrap();
+    let mut actual = result
+        .hits
+        .iter()
+        .map(|h| h.document_id)
+        .collect::<Vec<_>>();
+    actual.sort();
+    let mut expected = vec![first, last];
+    expected.sort();
+    assert_eq!(actual, expected);
+    h.close().await;
+}
+
+#[tokio::test]
+async fn knowledge_search_malformed_source_fields_repair_owner_and_preserve_neighbors() {
+    for mutation in ["location", "body", "identity"] {
+        let h = KnowledgeHarness::new().await;
+        let broken = h.import_text("broken", "24V");
+        let good = h.import_text("good", "48V");
+        assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
+        assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
+        h.h.store
+            .transaction(|tx| {
+                match mutation {
+                    "location" => {
+                        tx.execute(
+                            "UPDATE document_chunks SET location_json='{}' WHERE document_id=?1",
+                            [broken.to_string()],
+                        )?;
+                    }
+                    "body" => {
+                        tx.execute(
+                            "UPDATE document_chunks SET text=?1 WHERE document_id=?2",
+                            rusqlite::params!["x".repeat(9601), broken.to_string()],
+                        )?;
+                    }
+                    "identity" => {
+                        tx.execute(
+                            "UPDATE document_chunks SET embedding_identity=?1 WHERE document_id=?2",
+                            rusqlite::params!["x".repeat(65), broken.to_string()],
+                        )?;
+                    }
+                    _ => unreachable!(),
+                }
+                Ok(())
+            })
+            .unwrap();
+        let result = h.search("voltage").await.unwrap();
+        assert_eq!(
+            result
+                .hits
+                .iter()
+                .map(|hit| hit.document_id)
+                .collect::<Vec<_>>(),
+            vec![good],
+            "{mutation}"
+        );
+        let status: String =
+            h.h.store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT status FROM documents WHERE id=?1",
+                        [broken.to_string()],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+        assert_eq!(status, "repair_required", "{mutation}");
+        h.close().await;
+    }
+}
+
+#[tokio::test]
+async fn knowledge_search_final_envelope_recheck_trims_only_trailing_whole_hit() {
+    let h = KnowledgeHarness::new().await;
+    let mut ids = Vec::new();
+    for n in 0..2 {
+        let id = h.import_text(&format!("document{n}"), "24V");
+        let location = TextLocation {
+            page: None,
+            paragraph: 1,
+            heading: Some("\t".repeat(8 * 1024 * 1024 - 1024)),
+            start_char: 0,
+            end_char: 3,
+        };
+        h.h.store
+            .transaction(|tx| {
+                tx.execute(
+                    "UPDATE document_chunks SET location_json=?1 WHERE document_id=?2",
+                    rusqlite::params![serde_json::to_string(&location).unwrap(), id.to_string()],
+                )?;
+                Ok(())
+            })
+            .unwrap();
+        let score = 1.0 - n as f32 * 0.1;
+        let mut vector = vec![0.; 128];
+        vector[0] = score;
+        vector[1] = (1.0 - score * score).sqrt();
+        h.embeddings.vectors.lock().unwrap().push_back(vector);
+        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+        ids.push(id);
+    }
+    let initial = h.search("voltage").await.unwrap();
+    assert_eq!(initial.hits.len(), 2);
+    let gap = 32 * 1024 * 1024 - serde_json::to_vec(&initial).unwrap().len();
+    assert!(gap < 32768 - 9);
+    h.h.store
+        .transaction(|tx| {
+            tx.execute(
+                "UPDATE documents SET title=title||?1 WHERE id=?2",
+                rusqlite::params!["x".repeat(gap), ids[1].to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    assert_eq!(
+        serde_json::to_vec(&h.search("voltage").await.unwrap())
+            .unwrap()
+            .len(),
+        32 * 1024 * 1024
+    );
+    let store = h.h.store.clone();
+    h.service
+        .at_checkpoint(CommitPoint::BeforeSearchRecheck, move || {
+            store
+                .transaction(|tx| {
+                    for _ in 0..10 {
+                        tx.execute(
+                            "INSERT INTO documents(id,sha256,uri,title,\
+                    created_at,origin,kind,status,\
+                    parser_identity) VALUES (?1,?2,'',\
+                    'unavailable',1,'native','text','parsed',\
+                    'fixture')",
+                            rusqlite::params![Uuid::new_v4().to_string(), "a".repeat(64)],
+                        )?;
+                    }
+                    Ok(())
+                })
+                .unwrap();
+        });
+    let result = h.search("voltage").await.unwrap();
+    assert_eq!(
+        result
+            .hits
+            .iter()
+            .map(|hit| hit.document_id)
+            .collect::<Vec<_>>(),
+        vec![ids[0]]
+    );
+    assert_eq!(result.unavailable_documents, 10);
+    assert_eq!(
+        result.hits[0].location.heading,
+        initial.hits[0].location.heading
+    );
+    assert!(serde_json::to_vec(&result).unwrap().len() < 32 * 1024 * 1024);
+    h.close().await;
+}
+
+#[tokio::test]
+async fn knowledge_reconcile_preserves_indexing_with_unfinished_matching_reindex_owner() {
+    let h = KnowledgeHarness::new().await;
+    let id = h.import_text("pending", "24V");
+    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
+    let profile =
+        h.h.config
+            .selected_embedding_snapshot()
+            .await
+            .unwrap()
+            .unwrap();
+    let _serial = h.service.core.serial.lock().await;
+    h.h.store
+        .transaction(|tx| {
+            crate::intelligence::store::admit_job(
+                tx,
+                crate::intelligence::store::JobDraft {
+                    id: Uuid::new_v4(),
+                    kind: JobKind::Reindex,
+                    session_id: None,
+                    object_id: Some(id),
+                    admission: JobAdmission::Manual {
+                        exclusion_key: format!("document:{id}"),
+                    },
+                    payload: serde_json::json!({
+                    "version":1,
+                    "profiles":[profile],
+                    "input":{
+                    "document_id":id,
+                    "document_version":1}
+                    }
+                    ),
+                    initial_failure: None,
+                },
+            )?;
+            tx.execute(
+                "UPDATE documents SET status='indexing' WHERE id=?1",
+                [id.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE document_chunks SET index_state='indexing' WHERE document_id=?1",
+                [id.to_string()],
+            )?;
+            Ok(())
+        })
+        .unwrap();
+    let calls = h.embeddings.calls.load(Ordering::SeqCst);
+    h.service.core.reconcile(Arc::new(()), false).await.unwrap();
+    let status: String =
+        h.h.store
+            .transaction(|tx| {
+                Ok(tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [id.to_string()],
+                    |r| r.get(0),
+                )?)
+            })
+            .unwrap();
+    assert_eq!(status, "indexing");
+    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
+    drop(_serial);
+    h.close().await;
+}
diff --git a/src-tauri/src/knowledge/mod.rs b/src-tauri/src/knowledge/mod.rs
index a035a74..12451b3 100644
--- a/src-tauri/src/knowledge/mod.rs
+++ b/src-tauri/src/knowledge/mod.rs
@@ -1,11 +1,12 @@
 mod chunking;
+pub(crate) mod embedding;
 pub mod identity;
 pub(crate) mod index;
 pub mod parser;
 mod protocol;
 pub mod types;
 pub(crate) use chunking::chunk_blocks_bounded;
 pub use chunking::{chunk_blocks, chunk_hash};
 pub(crate) mod parser_worker;
 
 pub const PARSER_ID: &str = "voxbridge-parser-v1-normalize-v1";
@@ -16,11 +17,20 @@ pub const MAX_PARAGRAPHS: usize = 50_000;
 pub const MAX_PAGES: usize = 2_000;
 pub const MAX_CHUNKS: usize = 10_000;
 
 #[cfg(test)]
 mod parser_tests;
 
 pub(crate) mod copying;
 #[cfg(test)]
 mod document_tests;
 pub mod documents;
+#[cfg(test)]
+mod index_tests;
+pub(crate) mod service;
 mod store;
+
+mod reconcile;
+mod search;
+
+#[cfg(test)]
+mod embedding_tests;
diff --git a/src-tauri/src/knowledge/reconcile.rs b/src-tauri/src/knowledge/reconcile.rs
new file mode 100644
index 0000000..d48873b
--- /dev/null
+++ b/src-tauri/src/knowledge/reconcile.rs
@@ -0,0 +1,545 @@
+//! Local repair and generation replacement never call an embedding provider.
+use super::{
+    embedding::error,
+    index::{Generation, PublicationPlan, Seal, WorkTicket},
+    service::{Core, state},
+};
+use crate::{
+    error::AppResult,
+    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
+};
+use rusqlite::params;
+use uuid::Uuid;
+impl Core {
+    pub(crate) async fn reconcile(&self, owner: OperationLease, rebuild: bool) -> AppResult<()> {
+        self.resume_deletes(owner.clone()).await?;
+        self.store.transaction(|tx| {
+            tx.execute(
+                "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    status='indexing' AND NOT EXISTS(SELECT 1 \
+                    FROM intelligence_jobs j WHERE \
+                    j.kind='reindex' AND \
+                    j.object_id=documents.id AND j.status IN \
+                    ('queued','running') AND \
+                    json_extract(j.payload_json,\
+                    '$.input.document_version')=documents.version)",
+                [],
+            )?;
+            tx.execute(
+                "INSERT INTO knowledge_repairs(document_id,\
+                    reason) SELECT id,'unsealed_generation' FROM \
+                    documents WHERE status='repair_required' ON \
+                    CONFLICT(document_id) DO NOTHING",
+                [],
+            )?;
+            Ok(())
+        })?;
+        let mut after = String::new();
+        loop {
+            let rows = self.store.transaction(|tx| {
+                Ok(tx
+                    .prepare(
+                        "SELECT identity,active_generation FROM \
+                    knowledge_index_generations WHERE \
+                    identity>?1 AND active_generation IS NOT \
+                    NULL ORDER BY identity LIMIT 200",
+                    )?
+                    .query_map([&after], |r| {
+                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
+                    })?
+                    .collect::<Result<Vec<_>, _>>()?)
+            })?;
+            if rows.is_empty() {
+                break;
+            }
+            for (identity, raw) in rows {
+                after = identity.clone();
+                let generation = Uuid::parse_str(&raw).map_err(|_| error(Code::Storage))?;
+                let descriptor = self.store.transaction(|tx| {
+                    let mut q = tx.prepare(
+                        "SELECT seal_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2 AND dirty=0",
+                    )?;
+                    let mut rows = q.query(params![identity, raw])?;
+                    let Some(r) = rows.next()? else {
+                        return Ok(None);
+                    };
+                    let seal = Seal::decode(state::text(r, 0, 65536)?)?;
+                    if seal.descriptor.key()? != identity
+                        || seal.descriptor.generation != generation
+                    {
+                        return Err(error(Code::IndexRepairRequired));
+                    }
+                    Ok(Some(seal.descriptor))
+                });
+                let descriptor = match descriptor {
+                    Ok(Some(g)) => g,
+                    _ => {
+                        state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
+                        continue;
+                    }
+                };
+                let ticket = match self.index.open_existing(descriptor.clone(), owner.clone()) {
+                    Ok(t) => t,
+                    Err(_) => {
+                        state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
+                        continue;
+                    }
+                };
+                let count = match self.index.enqueue_open(&ticket)?.observe().await {
+                    Ok(c) => c,
+                    Err(_) => {
+                        state::mark_identity_repair(&self.store, &identity, "index_missing")?;
+                        continue;
+                    }
+                };
+                self.check_inverse(&identity, generation, count)?;
+                let mut start = 0u64;
+                while start < count {
+                    let page = match self.index.scan_records(&ticket, start, 200).await {
+                        Ok(p) => p,
+                        Err(_) => {
+                            state::mark_identity_repair(&self.store, &identity, "vector_missing")?;
+                            break;
+                        }
+                    };
+                    let mut orphans = Vec::new();
+                    for record in page {
+                        let (id, metadata) = (record.id, record.metadata);
+                        match state::classify(
+                            &self.store,
+                            &identity,
+                            generation,
+                            id,
+                            metadata.as_ref(),
+                        )? {
+                            state::Record::Healthy(_, _) => {}
+                            state::Record::Tombstone if record.deleted => {}
+                            state::Record::Tombstone | state::Record::Orphan => {
+                                self.store.transaction(|tx| {
+                                    tx.execute(
+                                        "INSERT OR IGNORE INTO \
+                    knowledge_index_work_items(operation_id,\
+                    ordinal,identity,generation,vector_id) \
+                    VALUES (?1,?2,?3,?4,?5)",
+                                        params![
+                                            ticket.operation_id().to_string(),
+                                            id,
+                                            identity,
+                                            generation.to_string(),
+                                            id.to_string()
+                                        ],
+                                    )?;
+                                    Ok(())
+                                })?;
+                                orphans.push(id);
+                            }
+                            state::Record::Repair(doc, version, reason) => {
+                                state::mark_document_repair(&self.store, doc, version, reason)?
+                            }
+                        }
+                    }
+                    if !orphans.is_empty() {
+                        self.index.delete(&ticket, orphans).await?;
+                    }
+                    start = start
+                        .checked_add(200)
+                        .ok_or_else(|| error(Code::ResourceLimit))?
+                        .min(count);
+                    tokio::task::yield_now().await;
+                }
+                let receipt = self.index.persist(&ticket).await?;
+                let ack = self
+                    .index
+                    .publish_plan(
+                        &ticket,
+                        &receipt,
+                        PublicationPlan::Cleanup {
+                            operation: ticket.operation_id(),
+                            work: ticket.operation_id(),
+                        },
+                    )
+                    .await?;
+                self.index.acknowledge(&ticket, ack).await?;
+                self.cleanup_generations(&ticket).await?;
+                if rebuild || self.rebuild_needed(&descriptor, 0)? {
+                    self.rebuild_generation(descriptor, owner.clone()).await?;
+                }
+            }
+        }
+        crate::knowledge::documents::cleanup_managed(
+            self.store.clone(),
+            self.data_dir.clone(),
+            owner,
+        )
+        .await
+    }
+    fn check_inverse(&self, identity: &str, generation: Uuid, count: u64) -> AppResult<()> {
+        let mut after = String::new();
+        loop {
+            let rows = self.store.transaction(|tx| {
+                Ok(tx
+                    .prepare(
+                        "SELECT m.chunk_id,m.document_id,m.vector_id,\
+                    m.document_version FROM \
+                    knowledge_generation_mappings m JOIN \
+                    documents d ON d.id=m.document_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.published=1 AND m.chunk_id>?3 AND \
+                    d.status='ready' AND \
+                    d.version=m.document_version ORDER BY \
+                    m.chunk_id LIMIT 200",
+                    )?
+                    .query_map(params![identity, generation.to_string(), after], |r| {
+                        Ok((
+                            r.get::<_, String>(0)?,
+                            r.get::<_, String>(1)?,
+                            r.get::<_, String>(2)?,
+                            r.get::<_, u32>(3)?,
+                        ))
+                    })?
+                    .collect::<Result<Vec<_>, _>>()?)
+            })?;
+            if rows.is_empty() {
+                return Ok(());
+            }
+            for (chunk, doc, raw, version) in rows {
+                after = chunk;
+                let vector = raw.parse::<u64>().map_err(|_| error(Code::Storage))?;
+                if vector >= count {
+                    state::mark_document_repair(
+                        &self.store,
+                        Uuid::parse_str(&doc).map_err(|_| error(Code::Storage))?,
+                        version,
+                        "vector_missing",
+                    )?;
+                }
+            }
+        }
+    }
+    pub(crate) fn rebuild_needed(&self, generation: &Generation, pending: u64) -> AppResult<bool> {
+        self.store.transaction(|tx| {
+            let mut q = tx.prepare(
+                "SELECT seal_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2 AND dirty=0",
+            )?;
+            let key = generation.key()?;
+            let mut rows = q.query(params![key, generation.generation.to_string()])?;
+            let r = rows
+                .next()?
+                .ok_or_else(|| error(Code::IndexRepairRequired))?;
+            let seal = Seal::decode(state::text(r, 0, 65536)?)?;
+            let live: u64 = tx.query_row(
+                "SELECT count(*) FROM \
+                    knowledge_generation_mappings m JOIN \
+                    documents d ON d.id=m.document_id AND \
+                    d.version=m.document_version JOIN \
+                    document_chunks c ON c.id=m.chunk_id AND \
+                    c.document_id=d.id AND \
+                    c.document_version=d.version AND \
+                    c.content_hash=m.content_hash AND \
+                    c.vector_id=m.vector_id WHERE m.identity=?1 \
+                    AND m.generation=?2 AND m.published=1 AND \
+                    d.status='ready' AND d.index_identity=?1 AND \
+                    c.index_state='ready' AND \
+                    c.embedding_identity=?1",
+                params![key, generation.generation.to_string()],
+                |r| r.get(0),
+            )?;
+            seal.needs_rebuild(live, pending)
+        })
+    }
+    pub(crate) fn ensure_capacity(&self, generation: &Generation, pending: u64) -> AppResult<()> {
+        self.store.transaction(|tx| {
+            let mut q = tx.prepare(
+                "SELECT s.seal_json FROM \
+                    knowledge_index_generation_seals s JOIN \
+                    knowledge_index_generations g ON \
+                    g.identity=s.identity AND \
+                    g.active_generation=s.generation WHERE \
+                    s.identity=?1 AND s.generation=?2 AND \
+                    s.dirty=0",
+            )?;
+            let mut rows = q.query(params![
+                generation.key()?,
+                generation.generation.to_string()
+            ])?;
+            let row = rows
+                .next()?
+                .ok_or_else(|| error(Code::IndexRepairRequired))?;
+            let seal = Seal::decode(state::text(row, 0, 65536)?)?;
+            if seal.descriptor != *generation {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            seal.ensure_capacity(pending)
+        })
+    }
+    pub(crate) async fn rebuild_generation(
+        &self,
+        source: Generation,
+        owner: OperationLease,
+    ) -> AppResult<()> {
+        let source_ticket = self.index.open_existing(source.clone(), owner.clone())?;
+        let count = self.index.enqueue_open(&source_ticket)?.observe().await?;
+        let source_receipt = self.index.persist(&source_ticket).await?;
+        let publication = async {
+            let maintenance = self.index.begin_maintenance(&source_ticket).await?;
+            let copied = async {
+                let staging = Generation {
+                    generation: Uuid::new_v4(),
+                    ..source.clone()
+                };
+                let target = self.index.create(staging.clone(), owner.clone())?;
+                self.index.enqueue_open(&target)?.observe().await?;
+                let key = source.key()?;
+                let mut start = 0u64;
+                let mut ordinal = 0u32;
+                while start < count {
+                    let metadata = self.index.scan_records(&source_ticket, start, 200).await?;
+                    let mut selected = Vec::new();
+                    for record in metadata {
+                        if let state::Record::Healthy(meta, fence) = state::classify(
+                            &self.store,
+                            &key,
+                            source.generation,
+                            record.id,
+                            record.metadata.as_ref(),
+                        )? {
+                            selected.push((record.id, meta, fence));
+                        }
+                    }
+                    #[cfg(test)]
+                    self.checkpoint(super::service::CommitPoint::BeforeLocalCopy)?;
+                    for (id, vector) in self.index.copy_vectors(&maintenance, start, 200).await? {
+                        if let Some((_, meta, fence)) =
+                            selected.iter().find(|(old, _, _)| *old == id)
+                        {
+                            let new = self
+                                .index
+                                .add(&target, vec![(vector, meta.clone())])
+                                .await?[0];
+                            self.stage_rebuild(&target, ordinal, meta, fence, id, new)?;
+                            ordinal = ordinal
+                                .checked_add(1)
+                                .ok_or_else(|| error(Code::ResourceLimit))?;
+                        }
+                    }
+                    start = start
+                        .checked_add(200)
+                        .ok_or_else(|| error(Code::ResourceLimit))?
+                        .min(count);
+                    tokio::task::yield_now().await;
+                }
+                Ok::<_, crate::error::AppError>((target, ordinal))
+            }
+            .await;
+            let ended = self.index.end_maintenance(maintenance).await;
+            ended?;
+            let (target, ordinal) = copied?;
+            let receipt = self.index.persist(&target).await?;
+            #[cfg(test)]
+            self.checkpoint(super::service::CommitPoint::AfterVectorPersist)?;
+            let ack = self
+                .index
+                .publish_plan(
+                    &target,
+                    &receipt,
+                    PublicationPlan::Swap {
+                        operation: target.operation_id(),
+                        work: target.operation_id(),
+                        source: source.generation,
+                        count: ordinal,
+                    },
+                )
+                .await?;
+            self.index.acknowledge(&target, ack).await?;
+            Ok::<_, crate::error::AppError>(target)
+        }
+        .await;
+        // The source was read only. Complete this exact refresh even when the
+        // destination fails, before returning its error or retiring the source.
+        let ack = self.index.publish(&source_ticket, &source_receipt).await?;
+        self.index.acknowledge(&source_ticket, ack).await?;
+        let target = publication?;
+        self.index.retire(&source_ticket).await?;
+        self.index
+            .remove_generation(&target, source.clone())
+            .await?;
+        self.remove_generation_rows(&source)
+    }
+    pub(crate) fn stage_rebuild(
+        &self,
+        ticket: &WorkTicket,
+        ordinal: u32,
+        meta: &super::index::ChunkMetadata,
+        fence: &super::index::DocumentFence,
+        previous: u64,
+        vector: u64,
+    ) -> AppResult<()> {
+        let key = ticket.generation().key()?;
+        let generation = ticket.generation().generation.to_string();
+        fence.validate()?;
+        if fence.id != meta.document_id || fence.version != meta.document_version {
+            return Err(error(Code::IndexRepairRequired));
+        }
+        self.store.transaction(|tx| {
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_index_work_items(operation_id,\
+                    ordinal,identity,generation,vector_id,\
+                    document_id,document_version,chunk_id,\
+                    content_hash,document_fence_json,\
+                    location_sha256,previous_vector_id) VALUES \
+                    (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
+                params![
+                    ticket.operation_id().to_string(),
+                    ordinal,
+                    key,
+                    generation,
+                    vector.to_string(),
+                    meta.document_id.to_string(),
+                    meta.document_version,
+                    meta.chunk_id.to_string(),
+                    meta.content_hash,
+                    serde_json::to_string(fence).map_err(|_| error(Code::Storage))?,
+                    meta.location_sha256(),
+                    previous.to_string()
+                ],
+            )?;
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_generation_mappings(identity,\
+                    generation,chunk_id,document_id,\
+                    document_version,vector_id,content_hash,\
+                    published) VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
+                params![
+                    key,
+                    generation,
+                    meta.chunk_id.to_string(),
+                    meta.document_id.to_string(),
+                    meta.document_version,
+                    vector.to_string(),
+                    meta.content_hash
+                ],
+            )?;
+            Ok(())
+        })
+    }
+    async fn cleanup_generations(&self, target: &WorkTicket) -> AppResult<()> {
+        let key = target.generation().key()?;
+        let active = target.generation().generation.to_string();
+        let mut after = String::new();
+        loop {
+            let rows = self.store.transaction(|tx| {
+                let mut q = tx.prepare(
+                    "SELECT generation,seal_json,intent_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation!=?2 AND \
+                    generation>?3 ORDER BY generation LIMIT 200",
+                )?;
+                let mut rows = q.query(params![key, active, after])?;
+                let mut out = Vec::new();
+                while let Some(r) = rows.next()? {
+                    let source = match r.get_ref(1)? {
+                        rusqlite::types::ValueRef::Null => {
+                            crate::knowledge::index::recovery_descriptor(state::text(r, 2, 16384)?)?
+                        }
+                        _ => Seal::decode(state::text(r, 1, 65536)?)?.descriptor,
+                    };
+                    if source.key()? != key
+                        || source.generation.to_string() != state::text(r, 0, 36)?
+                    {
+                        return Err(error(Code::IndexRepairRequired));
+                    }
+                    out.push(source);
+                }
+                Ok(out)
+            })?;
+            if rows.is_empty() {
+                break;
+            }
+            for source in rows {
+                after = source.generation.to_string();
+                if self
+                    .index
+                    .remove_generation(target, source.clone())
+                    .await
+                    .is_ok()
+                {
+                    // The index owner has actually dropped every reader/engine and
+                    // removed the unreferenced managed bytes before acknowledging work.
+                    loop {
+                        let removed = self.store.transaction(|tx| {
+                            let ids = tx
+                                .prepare(
+                                    "SELECT operation_id,ordinal FROM \
+                    knowledge_index_work_items WHERE identity=?1 \
+                    AND generation=?2 LIMIT 200",
+                                )?
+                                .query_map(params![key, source.generation.to_string()], |r| {
+                                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
+                                })?
+                                .collect::<Result<Vec<_>, _>>()?;
+                            for (op, ordinal) in &ids {
+                                tx.execute(
+                                    "DELETE FROM knowledge_index_work_items WHERE \
+                    operation_id=?1 AND ordinal=?2",
+                                    params![op, ordinal],
+                                )?;
+                            }
+                            Ok(ids.len())
+                        })?;
+                        if removed == 0 {
+                            break;
+                        }
+                    }
+                    self.remove_generation_rows(&source)?;
+                }
+            }
+        }
+        Ok(())
+    }
+    pub(crate) fn remove_generation_rows(&self, source: &Generation) -> AppResult<()> {
+        let key = source.key()?;
+        let generation = source.generation.to_string();
+        loop {
+            let removed = self.store.transaction(|tx| {
+                let ids = tx
+                    .prepare(
+                        "SELECT chunk_id FROM \
+                    knowledge_generation_mappings WHERE \
+                    identity=?1 AND generation=?2 LIMIT 200",
+                    )?
+                    .query_map(params![key, generation], |r| r.get::<_, String>(0))?
+                    .collect::<Result<Vec<_>, _>>()?;
+                for id in &ids {
+                    tx.execute(
+                        "DELETE FROM knowledge_generation_mappings \
+                    WHERE identity=?1 AND generation=?2 AND \
+                    chunk_id=?3",
+                        params![key, generation, id],
+                    )?;
+                }
+                Ok(ids.len())
+            })?;
+            if removed == 0 {
+                break;
+            }
+        }
+        self.store.transaction(|tx| {
+            tx.execute(
+                "DELETE FROM knowledge_index_generation_seals \
+                    WHERE identity=?1 AND generation=?2 AND NOT \
+                    EXISTS(SELECT 1 FROM \
+                    knowledge_index_generations WHERE \
+                    identity=?1 AND active_generation=?2)",
+                params![key, generation],
+            )?;
+            Ok(())
+        })
+    }
+}
diff --git a/src-tauri/src/knowledge/search.rs b/src-tauri/src/knowledge/search.rs
new file mode 100644
index 0000000..6012f88
--- /dev/null
+++ b/src-tauri/src/knowledge/search.rs
@@ -0,0 +1,403 @@
+//! Candidates stay compact; SQLite alone authorizes full source hydration.
+use super::{
+    embedding::{EmbeddingExecution, error, json_bytes},
+    identity::EmbeddingIdentity,
+    index::ChunkMetadata,
+    service::{Core, state},
+    types::*,
+};
+use crate::{
+    error::AppResult,
+    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
+};
+use rusqlite::params;
+use serde::Serialize;
+use uuid::Uuid;
+const BUDGET: usize = 32 * 1024 * 1024;
+impl Core {
+    pub(crate) async fn search(
+        &self,
+        request: KnowledgeSearch,
+        owner: OperationLease,
+        mut cancel: tokio::sync::watch::Receiver<bool>,
+    ) -> AppResult<KnowledgeResults> {
+        validate(&self.store, &request)?;
+        let total: u32 = self.store.transaction(|tx| {
+            Ok(tx.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))?)
+        })?;
+        if total == 0 {
+            return Ok(KnowledgeResults {
+                hits: vec![],
+                identity: None,
+                unavailable_documents: 0,
+            });
+        }
+        let _serial = tokio::select! {
+        biased;
+                _=crate::intelligence::http::cancelled(&mut cancel)=>return Err(error(Code::Cancelled)),
+        permit=self.serial.lock()=>permit};
+        let profile = self
+            .config
+            .scoped(owner.clone())
+            .selected_embedding_snapshot()
+            .await?
+            .ok_or_else(|| error(Code::IndexRepairRequired))?;
+        let binding = state::binding(&self.store, &profile)?
+            .ok_or_else(|| error(Code::IndexRepairRequired))?;
+        let identity = binding.generation.key()?;
+        let (healthy, unavailable) =
+            state::health(&self.store, &identity, binding.generation.generation)?;
+        if healthy == 0 {
+            return Err(error(Code::IndexRepairRequired));
+        }
+        // Open validates the local sealed source before the metered request.
+        let ticket = self
+            .index
+            .open_existing(binding.generation.clone(), owner.clone())?;
+        let result = async {
+            let open = self.index.enqueue_open(&ticket)?.observe();
+            tokio::pin!(open);
+            tokio::select! {
+            biased;
+                        _=crate::intelligence::http::cancelled(&mut cancel)=>{
+            ticket.cancel();
+                        let _=open.await;
+                        return Err(error(Code::Cancelled));
+                        }
+            ,
+            result=&mut open=>{
+            result?;
+                        }
+            }
+            let vectors = self
+                .embeddings
+                .embed(
+                    &profile,
+                    &[request.query],
+                    EmbeddingExecution::new(owner, cancel.clone()),
+                )
+                .await?;
+            if *cancel.borrow() {
+                return Err(error(Code::Cancelled));
+            }
+            if vectors.len() != 1 {
+                return Err(error(Code::InvalidResponse));
+            }
+            let vector = vectors.into_iter().next().unwrap();
+            if EmbeddingIdentity::from_profile(
+                &profile,
+                u32::try_from(vector.len()).map_err(|_| error(Code::InvalidResponse))?,
+            )? != binding.generation.identity
+            {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            let work = self
+                .index
+                .candidates(&ticket, vector, (request.limit * 5).min(100));
+            tokio::pin!(work);
+            let candidates = tokio::select! {
+            biased;
+                        _=crate::intelligence::http::cancelled(&mut cancel)=>{
+            ticket.cancel();
+                        let _=work.await;
+                        return Err(error(Code::Cancelled));
+                        }
+            ,
+            value=&mut work=>value?};
+            let mut results = KnowledgeResults {
+                hits: vec![],
+                identity: Some(identity.clone()),
+                unavailable_documents: unavailable,
+            };
+            let mut bytes = json_bytes(&results)?;
+            for candidate in candidates {
+                if *cancel.borrow() {
+                    return Err(error(Code::Cancelled));
+                }
+                let (id, score, metadata) = (candidate.id, candidate.score, candidate.metadata);
+                if !score.is_finite() {
+                    return Err(error(Code::IndexRepairRequired));
+                }
+                match hydrate(
+                    &self.store,
+                    &identity,
+                    binding.generation.generation,
+                    id,
+                    score,
+                    metadata.as_ref(),
+                    bytes,
+                    results.hits.is_empty(),
+                )? {
+                    Hydrated::Hit(hit, count) => {
+                        bytes = bytes
+                            .checked_add(count + usize::from(!results.hits.is_empty()))
+                            .ok_or_else(|| error(Code::ResourceLimit))?;
+                        results.hits.push(hit);
+                        if results.hits.len() == request.limit as usize {
+                            break;
+                        }
+                    }
+                    Hydrated::Overflow => break,
+                    Hydrated::Skip => {}
+                }
+            }
+            Ok(results)
+        }
+        .await;
+        // Read-only search still creates a writable Open intent. Complete its
+        // seal/publish/ack lifecycle on success and on recoverable request errors.
+        let cleanup = ticket.cleanup();
+        if self.index.enqueue_open(&cleanup)?.observe().await.is_err() {
+            state::mark_identity_repair(&self.store, &identity, "index_missing")?;
+            return Err(error(Code::IndexRepairRequired));
+        }
+        let receipt = match self.index.persist(&cleanup).await {
+            Ok(r) => r,
+            Err(e) => {
+                state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
+                return Err(e);
+            }
+        };
+        let ack = self.index.publish(&cleanup, &receipt).await?;
+        self.index.acknowledge(&cleanup, ack).await?;
+        let mut result = result?;
+        #[cfg(test)]
+        self.checkpoint(super::service::CommitPoint::BeforeSearchRecheck)?;
+        if *cancel.borrow() {
+            return Err(error(Code::Cancelled));
+        }
+        self.store.transaction(|tx| {
+            let mut q = tx.prepare(
+                "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
+                    document_chunks c ON c.document_id=d.id JOIN \
+                    knowledge_generation_mappings m ON \
+                    m.chunk_id=c.id JOIN \
+                    knowledge_index_generations g ON \
+                    g.identity=m.identity AND \
+                    g.active_generation=m.generation WHERE \
+                    d.id=?1 AND d.version=?2 AND \
+                    d.status='ready' AND d.index_identity=?3 AND \
+                    c.id=?4 AND c.document_version=d.version AND \
+                    c.index_state='ready' AND \
+                    c.embedding_identity=?3 AND \
+                    c.vector_id=m.vector_id AND \
+                    c.content_hash=?5 AND m.identity=?3 AND \
+                    m.generation=?6 AND m.published=1 AND \
+                    m.document_id=d.id AND \
+                    m.document_version=d.version AND \
+                    m.content_hash=c.content_hash)",
+            )?;
+            let mut keep = Vec::with_capacity(result.hits.len());
+            for hit in &result.hits {
+                let eligible: bool = q.query_row(
+                    params![
+                        hit.document_id.to_string(),
+                        hit.document_version,
+                        identity,
+                        hit.chunk_id.to_string(),
+                        crate::knowledge::chunk_hash(&hit.text),
+                        binding.generation.generation.to_string()
+                    ],
+                    |r| r.get(0),
+                )?;
+                keep.push(eligible);
+            }
+            let mut i = 0;
+            result.hits.retain(|_| {
+                let retained = keep[i];
+                i += 1;
+                retained
+            });
+            Ok(())
+        })?;
+        let (healthy, unavailable) =
+            state::health(&self.store, &identity, binding.generation.generation)?;
+        if healthy == 0 {
+            return Err(error(Code::IndexRepairRequired));
+        }
+        result.unavailable_documents = unavailable;
+        while json_bytes(&result)? > BUDGET {
+            if result.hits.pop().is_none() || result.hits.is_empty() {
+                return Err(error(Code::ResourceLimit));
+            }
+        }
+        Ok(result)
+    }
+}
+pub(crate) fn validate(
+    store: &crate::storage::LocalStore,
+    request: &KnowledgeSearch,
+) -> AppResult<()> {
+    let chars = request.query.chars().count();
+    if !(1..=4096).contains(&chars)
+        || !(1..=20).contains(&request.limit)
+        || request.selected_segment_ids.len() > 200
+    {
+        return Err(error(Code::Configuration));
+    }
+    if !request.selected_segment_ids.is_empty() {
+        let session = request
+            .session_id
+            .ok_or_else(|| error(Code::Configuration))?;
+        store.transaction(|tx| {
+            let mut q = tx.prepare(
+                "SELECT EXISTS(SELECT 1 FROM \
+                    translation_segments WHERE id=?1 AND \
+                    session_id=?2)",
+            )?;
+            for id in &request.selected_segment_ids {
+                let belongs: bool =
+                    q.query_row(params![id.to_string(), session.to_string()], |r| r.get(0))?;
+                if !belongs {
+                    return Err(error(Code::Configuration));
+                }
+            }
+            Ok(())
+        })?;
+    }
+    Ok(())
+}
+enum Hydrated {
+    Hit(KnowledgeHit, usize),
+    Overflow,
+    Skip,
+}
+#[derive(Serialize)]
+struct BorrowedHit<'a> {
+    document_id: Uuid,
+    document_version: u32,
+    chunk_id: Uuid,
+    title: &'a str,
+    location: &'a TextLocation,
+    text: &'a str,
+    score: f32,
+}
+fn hydrate(
+    store: &crate::storage::LocalStore,
+    identity: &str,
+    generation: Uuid,
+    vector: u64,
+    score: f32,
+    metadata: Option<&ChunkMetadata>,
+    used: usize,
+    first: bool,
+) -> AppResult<Hydrated> {
+    match state::classify(store, identity, generation, vector, metadata)? {
+        state::Record::Healthy(_, _) => {}
+        state::Record::Repair(doc, version, reason) => {
+            state::mark_document_repair(store, doc, version, reason)?;
+            return Ok(Hydrated::Skip);
+        }
+        state::Record::Orphan | state::Record::Tombstone => return Ok(Hydrated::Skip),
+    }
+    store.transaction(|tx| {
+        let mut q = tx.prepare(
+            "SELECT d.id,d.version,c.id,COALESCE(d.title,\
+                    ''),c.text,c.location_json,c.content_hash,\
+                    m.document_version,m.content_hash FROM \
+                    knowledge_generation_mappings m JOIN \
+                    knowledge_index_generations g ON \
+                    g.identity=m.identity AND \
+                    g.active_generation=m.generation JOIN \
+                    document_chunks c ON c.id=m.chunk_id JOIN \
+                    documents d ON d.id=c.document_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.vector_id=?3 AND m.published=1 AND \
+                    d.status='ready' AND \
+                    d.index_identity=m.identity AND \
+                    c.index_state='ready' AND \
+                    c.embedding_identity=m.identity AND \
+                    c.vector_id=m.vector_id AND \
+                    c.document_version=d.version AND \
+                    m.document_id=d.id AND \
+                    m.document_version=d.version",
+        )?;
+        let mut rows = q.query(params![
+            identity,
+            generation.to_string(),
+            vector.to_string()
+        ])?;
+        let Some(r) = rows.next()? else {
+            return Ok(Hydrated::Skip);
+        };
+        let doc = Uuid::parse_str(state::text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
+        let version: u32 = r.get(1)?;
+        let chunk = Uuid::parse_str(state::text(r, 2, 36)?).map_err(|_| error(Code::Storage))?;
+        let title = state::text(r, 3, 32768)?;
+        let body = state::text(r, 4, 9600)?;
+        let location_json = state::text(r, 5, 16 * 1024 * 1024)?;
+        if body.chars().count() > 2400 {
+            return Err(error(Code::ResourceLimit));
+        }
+        let location: TextLocation =
+            serde_json::from_str(location_json).map_err(|_| error(Code::IndexRepairRequired))?;
+        if location
+            .heading
+            .as_ref()
+            .is_some_and(|h| h.len() > 16 * 1024 * 1024)
+        {
+            return Err(error(Code::ResourceLimit));
+        }
+        let hash = state::text(r, 6, 64)?;
+        let valid = if let Some(metadata) = metadata {
+            doc == metadata.document_id
+                && version == metadata.document_version
+                && chunk == metadata.chunk_id
+                && hash == metadata.content_hash
+                && state::text(r, 8, 64)? == hash
+                && crate::knowledge::chunk_hash(body) == hash
+                && metadata.matches_location(&location)?
+        } else {
+            false
+        };
+        if !valid {
+            tx.execute(
+                "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    id=?1 AND status='ready' AND version=?2",
+                params![doc.to_string(), version],
+            )?;
+            tx.execute(
+                "INSERT INTO knowledge_repairs(document_id,\
+                    reason) VALUES (?1,'stale_hash') ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+                [doc.to_string()],
+            )?;
+            return Ok(Hydrated::Skip);
+        }
+        let hit = BorrowedHit {
+            document_id: doc,
+            document_version: version,
+            chunk_id: chunk,
+            title,
+            location: &location,
+            text: body,
+            score,
+        };
+        let count = json_bytes(&hit)?;
+        if used
+            .checked_add(count + usize::from(!first))
+            .is_none_or(|n| n > BUDGET)
+        {
+            return if first {
+                Err(error(Code::ResourceLimit))
+            } else {
+                Ok(Hydrated::Overflow)
+            };
+        }
+        Ok(Hydrated::Hit(
+            KnowledgeHit {
+                document_id: doc,
+                document_version: version,
+                chunk_id: chunk,
+                title: title.into(),
+                text: body.into(),
+                location,
+                score,
+            },
+            count,
+        ))
+    })
+}
diff --git a/src-tauri/src/knowledge/service.rs b/src-tauri/src/knowledge/service.rs
new file mode 100644
index 0000000..6d5dc21
--- /dev/null
+++ b/src-tauri/src/knowledge/service.rs
@@ -0,0 +1,295 @@
+//! Facade admissions retain the scheduler; executing collaborators do not.
+use super::{
+    embedding::{EmbeddingExecution, Embeddings, error},
+    index::{IndexOwner, PublicationPlan},
+    types::*,
+};
+use crate::{
+    error::AppResult,
+    intelligence::{
+        config::ServiceConfigStore,
+        jobs::{JobContext, JobHandler, JobScheduler, OperationLease},
+        store::{JobDraft, admit_job, decode_payload},
+        types::{
+            IntelligenceErrorCode as Code, JobAdmission, JobKind, JobStatus, JobView, ModelProfile,
+        },
+    },
+    storage::LocalStore,
+};
+use rusqlite::params;
+use serde::Deserialize;
+#[cfg(test)]
+use std::sync::Mutex;
+use std::{path::PathBuf, sync::Arc};
+use uuid::Uuid;
+mod delete;
+mod reindex;
+pub(crate) mod state;
+#[cfg(test)]
+pub(crate) use delete::DeleteInput;
+mod continuation;
+mod recovery;
+mod searches;
+#[cfg(test)]
+#[derive(Clone, Copy, PartialEq, Eq)]
+pub(crate) enum CommitPoint {
+    AfterSqliteIntent,
+    AfterVectorMutation,
+    AfterVectorPersist,
+    BeforeReady,
+    AfterPointerPublication,
+    AdmissionCallback,
+    BeforeLocalCopy,
+    BeforeSearchRecheck,
+}
+#[derive(Clone)]
+pub(crate) struct KnowledgeService {
+    pub(crate) core: Arc<Core>,
+    jobs: Arc<JobScheduler>,
+    searches: Arc<searches::Searches>,
+}
+pub(crate) struct Core {
+    pub(crate) store: Arc<LocalStore>,
+    pub(crate) config: Arc<ServiceConfigStore>,
+    pub(crate) embeddings: Arc<dyn Embeddings>,
+    pub(crate) index: IndexOwner,
+    pub(crate) data_dir: PathBuf,
+    pub(crate) serial: tokio::sync::Mutex<()>,
+    #[cfg(test)]
+    hook: Mutex<Option<CommitPoint>>,
+    #[cfg(test)]
+    action: Mutex<Option<(CommitPoint, Box<dyn FnOnce() + Send>)>>,
+}
+#[derive(Deserialize)]
+#[serde(deny_unknown_fields)]
+struct IndexInput {
+    document_id: Uuid,
+    document_version: u32,
+}
+impl KnowledgeService {
+    pub(crate) fn new(
+        store: Arc<LocalStore>,
+        config: Arc<ServiceConfigStore>,
+        jobs: Arc<JobScheduler>,
+        embeddings: Arc<dyn Embeddings>,
+        index: IndexOwner,
+        data_dir: PathBuf,
+    ) -> Self {
+        Self {
+            core: Arc::new(Core {
+                store,
+                config,
+                embeddings,
+                index,
+                data_dir,
+                serial: tokio::sync::Mutex::new(()),
+                #[cfg(test)]
+                hook: Mutex::new(None),
+                #[cfg(test)]
+                action: Mutex::new(None),
+            }),
+            jobs,
+            searches: Arc::new(searches::Searches::new()),
+        }
+    }
+    pub(crate) fn register_handlers(&self) -> AppResult<()> {
+        self.jobs.register(
+            JobKind::Reindex,
+            Arc::new(ReindexHandler(self.core.clone())),
+        )?;
+        self.jobs.register(
+            JobKind::DeleteDocument,
+            Arc::new(delete::DeleteHandler(self.core.clone())),
+        )
+    }
+    pub(crate) fn register_continuation(&self) -> AppResult<()> {
+        self.jobs
+            .register_after_terminal(Arc::new(continuation::ImportContinuation {
+                config: self.core.config.clone(),
+                events: self.jobs.event_port(),
+                wake: self.jobs.wake_port(),
+            }))
+    }
+    pub(crate) async fn reindex_owned(
+        &self,
+        id: Uuid,
+        owner: OperationLease,
+    ) -> AppResult<JobView> {
+        let fence = self.jobs.admission_fence(JobKind::Reindex)?;
+        #[cfg(test)]
+        let core = self.core.clone();
+        let events = self.jobs.event_port();
+        let wake = self.jobs.wake_port();
+        let job = self
+            .core
+            .config
+            .scoped(owner)
+            .with_selected_embedding_completion(
+                move |tx, profile| {
+                    fence.run(|| {
+                        let profile = profile.ok_or_else(|| error(Code::Configuration))?;
+                        let f = state::fence(tx, id)?;
+                        let status: String = tx.query_row(
+                            "SELECT status FROM documents WHERE id=?1",
+                            [id.to_string()],
+                            |r| r.get(0),
+                        )?;
+                        if !matches!(
+                            status.as_str(),
+                            "parsed" | "ready" | "repair_required" | "failed"
+                        ) {
+                            return Err(error(Code::Busy));
+                        }
+                        let job = admit_job(
+                            tx,
+                            JobDraft {
+                                id: Uuid::new_v4(),
+                                kind: JobKind::Reindex,
+                                session_id: None,
+                                object_id: Some(id),
+                                admission: JobAdmission::Manual {
+                                    exclusion_key: format!("document:{id}"),
+                                },
+                                payload: serde_json::json!({
+                                "version":1,
+                                "profiles":[profile],
+                                "input":{
+                                "document_id":id,
+                                "document_version":f.version}
+                                }
+                                ),
+                                initial_failure: None,
+                            },
+                        )?
+                        .view()
+                        .clone();
+                        #[cfg(test)]
+                        core.checkpoint(CommitPoint::AdmissionCallback)?;
+                        Ok(job)
+                    })
+                },
+                move |job: &JobView| {
+                    events.publish(job.clone());
+                    wake.notify_one();
+                },
+            )
+            .await?;
+        Ok(job)
+    }
+    pub(crate) async fn delete_owned(
+        &self,
+        id: Uuid,
+        _owner: OperationLease,
+    ) -> AppResult<JobView> {
+        delete::admit(self, id)
+    }
+    pub(crate) async fn reconcile_owned(&self, owner: OperationLease) -> AppResult<()> {
+        let _serial = self.core.serial.lock().await;
+        self.core.reconcile(owner, false).await
+    }
+    #[cfg(test)]
+    pub(crate) async fn rebuild_owned(&self, owner: OperationLease) -> AppResult<()> {
+        let _serial = self.core.serial.lock().await;
+        self.core.reconcile(owner, true).await
+    }
+    pub(crate) async fn search_owned(
+        &self,
+        request: KnowledgeSearch,
+        owner: OperationLease,
+    ) -> AppResult<KnowledgeResults> {
+        self.searches.run(self.core.clone(), request, owner).await
+    }
+    pub(crate) async fn shutdown_searches(&self) -> AppResult<()> {
+        self.searches.shutdown().await
+    }
+    pub(crate) fn reopen_searches(&self) -> AppResult<()> {
+        self.searches.reopen()
+    }
+    #[cfg(test)]
+    pub(crate) fn at_checkpoint(&self, point: CommitPoint, action: impl FnOnce() + Send + 'static) {
+        *self.core.action.lock().unwrap() = Some((point, Box::new(action)));
+    }
+    #[cfg(test)]
+    pub(crate) fn crash_at(&self, point: CommitPoint) {
+        *self.core.hook.lock().unwrap() = Some(point);
+    }
+}
+impl Core {
+    #[cfg(test)]
+    pub(crate) fn checkpoint(&self, point: CommitPoint) -> AppResult<()> {
+        let action = {
+            let mut a = self.action.lock().unwrap();
+            if a.as_ref().is_some_and(|(p, _)| *p == point) {
+                a.take()
+            } else {
+                None
+            }
+        };
+        if let Some((_, action)) = action {
+            action();
+        }
+        let mut h = self.hook.lock().unwrap();
+        if *h == Some(point) {
+            *h = None;
+            return Err(error(Code::IndexRepairRequired));
+        }
+        Ok(())
+    }
+}
+struct ReindexHandler(Arc<Core>);
+fn code(error: crate::error::AppError) -> Code {
+    match error {
+        crate::error::AppError::Intelligence(c) => c,
+        _ => Code::Storage,
+    }
+}
+#[async_trait::async_trait]
+impl JobHandler for ReindexHandler {
+    async fn run(&self, context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
+        let envelope =
+            decode_payload(&serde_json::to_string(&payload).map_err(|_| Code::Configuration)?)
+                .map_err(code)?;
+        if envelope.profiles.len() != 1 {
+            return Err(Code::Configuration);
+        }
+        let input: IndexInput =
+            serde_json::from_value(envelope.input).map_err(|_| Code::Configuration)?;
+        self.0
+            .reindex(context, input, &envelope.profiles[0])
+            .await
+            .map_err(code)
+    }
+    fn terminal_tx(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        id: Uuid,
+        status: JobStatus,
+        _code: Option<Code>,
+        _awaiting: bool,
+    ) -> AppResult<()> {
+        if status != JobStatus::Succeeded {
+            tx.execute(
+                "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    status='indexing' AND id=(SELECT object_id \
+                    FROM intelligence_jobs WHERE id=?1) AND \
+                    version=(SELECT json_extract(payload_json,\
+                    '$.input.document_version') FROM \
+                    intelligence_jobs WHERE id=?1)",
+                [id.to_string()],
+            )?;
+            tx.execute(
+                "INSERT INTO knowledge_repairs(document_id,\
+                    reason) SELECT d.id,'unsealed_generation' \
+                    FROM documents d JOIN intelligence_jobs j ON \
+                    j.object_id=d.id WHERE j.id=?1 AND \
+                    d.status='repair_required' ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+                [id.to_string()],
+            )?;
+        }
+        Ok(())
+    }
+}
diff --git a/src-tauri/src/knowledge/service/continuation.rs b/src-tauri/src/knowledge/service/continuation.rs
new file mode 100644
index 0000000..d4eecc6
--- /dev/null
+++ b/src-tauri/src/knowledge/service/continuation.rs
@@ -0,0 +1,447 @@
+use super::*;
+use crate::intelligence::jobs::{AfterTerminal, AfterTerminalNotice, JobAdmissionFence, JobEvents};
+use tokio::sync::Notify;
+pub(super) struct ImportContinuation {
+    pub(super) config: Arc<ServiceConfigStore>,
+    pub(super) events: JobEvents,
+    pub(super) wake: Arc<Notify>,
+}
+fn matches(tx: &rusqlite::Transaction<'_>, notice: &AfterTerminalNotice) -> AppResult<bool> {
+    Ok(tx.query_row(
+        "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
+                    knowledge_document_imports i ON \
+                    i.document_id=d.id AND \
+                    i.document_version=d.version JOIN \
+                    intelligence_jobs j ON j.id=i.job_id WHERE \
+                    d.id=?1 AND d.version=?2 AND d.sha256=?3 AND \
+                    d.parser_identity=?4 AND d.import_job_id=?5 \
+                    AND d.status='parsed' AND \
+                    i.job_id=d.import_job_id AND \
+                    i.sha256=d.sha256 AND \
+                    i.parser_identity=d.parser_identity AND \
+                    j.status='succeeded')",
+        params![
+            notice.document_id.to_string(),
+            notice.version,
+            notice.sha256,
+            notice.parser,
+            notice.parent_id.to_string()
+        ],
+        |r| r.get(0),
+    )?)
+}
+fn defer(tx: &rusqlite::Transaction<'_>, notice: &AfterTerminalNotice) -> AppResult<()> {
+    if matches(tx, notice)? {
+        tx.execute(
+            "INSERT INTO knowledge_repairs(document_id,\
+                    reason) VALUES (?1,'auto_index_deferred') ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+            [notice.document_id.to_string()],
+        )?;
+    }
+    Ok(())
+}
+#[async_trait::async_trait]
+impl AfterTerminal for ImportContinuation {
+    fn notice(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        job: &JobView,
+    ) -> AppResult<Option<AfterTerminalNotice>> {
+        if job.kind != JobKind::Import {
+            return Ok(None);
+        }
+        let mut q = tx.prepare(
+            "SELECT d.id,d.version,d.sha256,\
+                    d.parser_identity FROM documents d JOIN \
+                    knowledge_document_imports i ON \
+                    i.document_id=d.id AND \
+                    i.document_version=d.version JOIN \
+                    intelligence_jobs j ON j.id=i.job_id WHERE \
+                    d.import_job_id=?1 AND \
+                    i.job_id=d.import_job_id AND \
+                    i.sha256=d.sha256 AND \
+                    i.parser_identity=d.parser_identity AND \
+                    d.status='parsed' AND j.status='running'",
+        )?;
+        let mut rows = q.query([job.id.to_string()])?;
+        let Some(r) = rows.next()? else {
+            return Ok(None);
+        };
+        Ok(Some(AfterTerminalNotice {
+            parent_id: job.id,
+            document_id: Uuid::parse_str(state::text(r, 0, 36)?)
+                .map_err(|_| error(Code::Storage))?,
+            version: r.get(1)?,
+            sha256: state::text(r, 2, 64)?.into(),
+            parser: state::text(r, 3, 128)?.into(),
+        }))
+    }
+    fn deferred(
+        &self,
+        tx: &rusqlite::Transaction<'_>,
+        notice: &AfterTerminalNotice,
+    ) -> AppResult<()> {
+        defer(tx, notice)
+    }
+    async fn succeeded(
+        &self,
+        notice: AfterTerminalNotice,
+        owner: OperationLease,
+        fence: JobAdmissionFence,
+    ) {
+        let events = self.events.clone();
+        let wake = self.wake.clone();
+        let _ = self
+            .config
+            .scoped(owner)
+            .with_selected_embedding_completion(
+                move |tx, profile| {
+                    if !matches(tx, &notice)? {
+                        return Ok(None);
+                    }
+                    let admitted=fence.run(|| {
+                let Some(profile)=profile else{
+defer(tx,
+&notice)?;
+return Ok(None);
+}
+;
+                let job=admit_job(tx,
+JobDraft{
+id:Uuid::new_v4(),
+kind:JobKind::Reindex,
+session_id:None,
+object_id:Some(notice.document_id),
+admission:JobAdmission::Manual{
+exclusion_key:format!("document:{}",
+notice.document_id)}
+,
+payload:serde_json::json!({
+"version":1,
+"profiles":[profile],
+"input":{
+"document_id":notice.document_id,
+"document_version":notice.version}
+}
+),
+initial_failure:None}
+)?.view().clone();
+                tx.execute("UPDATE documents SET status='indexing' WHERE id=?1 AND status='parsed'",
+[notice.document_id.to_string()])?;
+                tx.execute("UPDATE document_chunks SET \
+                    index_state='indexing' WHERE document_id=?1 \
+                    AND document_version=?2",
+params![notice.document_id.to_string(),
+notice.version])?;
+Ok(Some(job))
+            }
+);
+                    match admitted {
+                        Err(crate::error::AppError::Intelligence(Code::Busy)) => {
+                            defer(tx, &notice)?;
+                            Ok(None)
+                        }
+                        other => other,
+                    }
+                },
+                move |job: &Option<JobView>| {
+                    if let Some(job) = job {
+                        events.publish(job.clone());
+                        wake.notify_one();
+                    }
+                },
+            )
+            .await;
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use crate::intelligence::test_support::{IntelligenceHarness, payload, test_llm_profile};
+    struct Park(tokio::sync::Notify);
+    #[async_trait::async_trait]
+    impl JobHandler for Park {
+        async fn run(&self, mut context: JobContext, _: serde_json::Value) -> Result<(), Code> {
+            self.0.notify_one();
+            crate::intelligence::http::cancelled(&mut context.cancel).await;
+            Err(Code::Cancelled)
+        }
+    }
+    fn draft(object: Uuid, kind: JobKind) -> JobDraft {
+        JobDraft {
+            id: Uuid::new_v4(),
+            kind,
+            session_id: None,
+            object_id: Some(object),
+            admission: JobAdmission::Manual {
+                exclusion_key: format!("document:{object}"),
+            },
+            payload: payload(vec![]),
+            initial_failure: None,
+        }
+    }
+    async fn fixture() -> (
+        IntelligenceHarness,
+        ImportContinuation,
+        AfterTerminalNotice,
+        JobAdmissionFence,
+    ) {
+        let h = IntelligenceHarness::new();
+        let profile = ModelProfile {
+            kind: crate::intelligence::types::ServiceKind::Embedding,
+            max_context_tokens: None,
+            max_output_tokens: None,
+            dimensions: Some(128),
+            ..test_llm_profile()
+        };
+        h.config
+            .save(crate::intelligence::types::SaveModelProfile {
+                profile: profile.clone(),
+                credential: crate::app::CredentialChange::Replace {
+                    api_key: "synthetic".into(),
+                },
+            })
+            .await
+            .unwrap();
+        let mut settings = h.config.settings().await.unwrap();
+        settings.embedding_profile_id = Some(profile.id);
+        h.config.save_settings(settings).await.unwrap();
+        let park = Arc::new(Park(tokio::sync::Notify::new()));
+        h.jobs.register(JobKind::Reindex, park.clone()).unwrap();
+        h.jobs.start().unwrap();
+        h.store
+            .transaction(|tx| {
+                admit_job(tx, draft(Uuid::new_v4(), JobKind::Reindex))?;
+                Ok(())
+            })
+            .unwrap();
+        h.jobs.wake();
+        park.0.notified().await;
+        let document = Uuid::new_v4();
+        let parent = Uuid::new_v4();
+        h.store
+            .transaction(|tx| {
+                let mut job = draft(document, JobKind::Import);
+                job.id = parent;
+                admit_job(tx, job)?;
+                tx.execute(
+                    "UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",
+                    [parent.to_string()],
+                )?;
+                tx.execute(
+                    "INSERT INTO documents(id,sha256,uri,title,\
+                    created_at,origin,kind,status,\
+                    parser_identity,import_job_id) VALUES (?1,?2,\
+                    '','fixture',1,'native','text','parsed',\
+                    'fixture',?3)",
+                    params![document.to_string(), "a".repeat(64), parent.to_string()],
+                )?;
+                tx.execute(
+                    "INSERT INTO \
+                    knowledge_document_imports(document_id,\
+                    document_version,job_id,sha256,\
+                    parser_identity,created_at_ms) VALUES (?1,1,\
+                    ?2,?3,'fixture',1)",
+                    params![document.to_string(), parent.to_string(), "a".repeat(64)],
+                )?;
+                Ok(())
+            })
+            .unwrap();
+        let hook = ImportContinuation {
+            config: h.config.clone(),
+            events: h.jobs.event_port(),
+            wake: h.jobs.wake_port(),
+        };
+        let notice = AfterTerminalNotice {
+            parent_id: parent,
+            document_id: document,
+            version: 1,
+            sha256: "a".repeat(64),
+            parser: "fixture".into(),
+        };
+        let fence = h.jobs.admission_fence(JobKind::Reindex).unwrap();
+        (h, hook, notice, fence)
+    }
+    #[tokio::test]
+    async fn knowledge_import_continuation_capacity_exclusion_missing_profile_shutdown_defer_parsed()
+     {
+        for condition in ["capacity", "exclusion", "profile", "shutdown"] {
+            let (h, hook, notice, fence) = fixture().await;
+            match condition {
+                "capacity" => h
+                    .store
+                    .transaction(|tx| {
+                        for _ in 0..31 {
+                            admit_job(tx, draft(Uuid::new_v4(), JobKind::Reindex))?;
+                        }
+                        Ok(())
+                    })
+                    .unwrap(),
+                "exclusion" => h
+                    .store
+                    .transaction(|tx| {
+                        admit_job(tx, draft(notice.document_id, JobKind::Reindex))?;
+                        Ok(())
+                    })
+                    .unwrap(),
+                "profile" => {
+                    let mut settings = h.config.settings().await.unwrap();
+                    settings.embedding_profile_id = None;
+                    h.config.save_settings(settings).await.unwrap();
+                }
+                "shutdown" => h.jobs.shutdown().await.unwrap(),
+                _ => unreachable!(),
+            }
+            let before: u32 = h
+                .store
+                .transaction(|tx| {
+                    Ok(tx.query_row(
+                        "SELECT count(*) FROM intelligence_jobs WHERE kind='reindex'",
+                        [],
+                        |r| r.get(0),
+                    )?)
+                })
+                .unwrap();
+            hook.succeeded(
+                AfterTerminalNotice {
+                    parent_id: notice.parent_id,
+                    document_id: notice.document_id,
+                    version: notice.version,
+                    sha256: notice.sha256.clone(),
+                    parser: notice.parser.clone(),
+                },
+                Arc::new(()),
+                fence,
+            )
+            .await;
+            let (status, reason, after, parent): (String, String, u32, String) = h
+                .store
+                .transaction(|tx| {
+                    Ok((
+                        tx.query_row(
+                            "SELECT status FROM documents WHERE id=?1",
+                            [notice.document_id.to_string()],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT reason FROM knowledge_repairs WHERE document_id=?1",
+                            [notice.document_id.to_string()],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT count(*) FROM intelligence_jobs WHERE kind='reindex'",
+                            [],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT status FROM intelligence_jobs WHERE id=?1",
+                            [notice.parent_id.to_string()],
+                            |r| r.get(0),
+                        )?,
+                    ))
+                })
+                .unwrap();
+            assert_eq!(
+                (status.as_str(), reason.as_str(), after, parent.as_str()),
+                ("parsed", "auto_index_deferred", before, "succeeded"),
+                "{condition}"
+            );
+            assert!(h.http.requests.lock().unwrap().is_empty());
+            h.jobs.shutdown().await.unwrap();
+        }
+    }
+    #[tokio::test]
+    async fn knowledge_import_continuation_stale_or_deleted_notice_never_changes_new_association() {
+        for mutation in [
+            "version", "sha", "parser", "import", "status", "deleted", "parent",
+        ] {
+            let (h, hook, notice, fence) = fixture().await;
+            h.store
+                .transaction(|tx| {
+                    match mutation {
+                        "version" => {
+                            tx.execute(
+                                "UPDATE documents SET version=2 WHERE id=?1",
+                                [notice.document_id.to_string()],
+                            )?;
+                        }
+                        "sha" => {
+                            tx.execute(
+                                "UPDATE documents SET sha256=?1 WHERE id=?2",
+                                params!["b".repeat(64), notice.document_id.to_string()],
+                            )?;
+                        }
+                        "parser" => {
+                            tx.execute(
+                                "UPDATE documents SET parser_identity='new-parser' WHERE id=?1",
+                                [notice.document_id.to_string()],
+                            )?;
+                        }
+                        "import" => {
+                            tx.execute(
+                                "UPDATE documents SET import_job_id=NULL WHERE id=?1",
+                                [notice.document_id.to_string()],
+                            )?;
+                        }
+                        "status" => {
+                            tx.execute(
+                                "UPDATE documents SET status='deleting' WHERE id=?1",
+                                [notice.document_id.to_string()],
+                            )?;
+                        }
+                        "deleted" => {
+                            tx.execute(
+                                "DELETE FROM documents WHERE id=?1",
+                                [notice.document_id.to_string()],
+                            )?;
+                        }
+                        "parent" => {
+                            tx.execute(
+                                "UPDATE intelligence_jobs SET status='cancelled' WHERE id=?1",
+                                [notice.parent_id.to_string()],
+                            )?;
+                        }
+                        _ => unreachable!(),
+                    }
+                    Ok(())
+                })
+                .unwrap();
+            hook.succeeded(
+                AfterTerminalNotice {
+                    parent_id: notice.parent_id,
+                    document_id: notice.document_id,
+                    version: notice.version,
+                    sha256: notice.sha256.clone(),
+                    parser: notice.parser.clone(),
+                },
+                Arc::new(()),
+                fence,
+            )
+            .await;
+            let (jobs, repairs): (u32, u32) = h
+                .store
+                .transaction(|tx| {
+                    Ok((
+                        tx.query_row(
+                            "SELECT count(*) FROM intelligence_jobs WHERE \
+                    kind='reindex' AND object_id=?1",
+                            [notice.document_id.to_string()],
+                            |r| r.get(0),
+                        )?,
+                        tx.query_row(
+                            "SELECT count(*) FROM knowledge_repairs WHERE document_id=?1",
+                            [notice.document_id.to_string()],
+                            |r| r.get(0),
+                        )?,
+                    ))
+                })
+                .unwrap();
+            assert_eq!((jobs, repairs), (0, 0), "{mutation}");
+            assert!(h.http.requests.lock().unwrap().is_empty());
+            h.jobs.shutdown().await.unwrap();
+        }
+    }
+}
diff --git a/src-tauri/src/knowledge/service/delete.rs b/src-tauri/src/knowledge/service/delete.rs
new file mode 100644
index 0000000..20337b5
--- /dev/null
+++ b/src-tauri/src/knowledge/service/delete.rs
@@ -0,0 +1,305 @@
+use super::*;
+use crate::knowledge::{documents::cleanup_managed, index::Seal};
+#[derive(Clone, Deserialize, serde::Serialize)]
+#[serde(deny_unknown_fields)]
+pub(crate) struct DeleteInput {
+    pub(crate) document_id: Uuid,
+    pub(crate) document_version: u32,
+    pub(crate) document: crate::knowledge::index::DocumentFence,
+    pub(crate) operation_id: Uuid,
+    pub(crate) identity: Option<String>,
+    pub(crate) generation: Option<Uuid>,
+}
+impl DeleteInput {
+    pub(super) fn validate(&self) -> AppResult<()> {
+        self.document.validate()?;
+        if self.document.id != self.document_id
+            || self.document.version != self.document_version
+            || self.identity.is_some() != self.generation.is_some()
+            || self.identity.as_ref().is_some_and(|i| {
+                i.len() != 64
+                    || !i
+                        .bytes()
+                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
+            })
+        {
+            return Err(error(Code::Configuration));
+        }
+        Ok(())
+    }
+}
+pub(super) fn admit(service: &KnowledgeService, id: Uuid) -> AppResult<JobView> {
+    let admission = service.jobs.admission_fence(JobKind::DeleteDocument)?;
+    let job = service.core.store.transaction(|tx| {
+        admission.run(|| {
+            let f = state::fence(tx, id)?;
+            let identity: Option<String> = tx.query_row(
+                "SELECT index_identity FROM documents WHERE id=?1",
+                [id.to_string()],
+                |r| r.get(0),
+            )?;
+            let generation = identity
+                .as_ref()
+                .map(|key| {
+                    tx.query_row(
+                        "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE identity=?1",
+                        [key],
+                        |r| r.get::<_, Option<String>>(0),
+                    )
+                })
+                .transpose()?
+                .flatten()
+                .map(|v| Uuid::parse_str(&v).map_err(|_| error(Code::Storage)))
+                .transpose()?;
+            let operation_id = Uuid::new_v4();
+            let input = DeleteInput {
+                document_id: id,
+                document_version: f.version,
+                document: f.clone(),
+                operation_id,
+                identity: identity.clone(),
+                generation,
+            };
+            let job = admit_job(
+                tx,
+                JobDraft {
+                    id: operation_id,
+                    kind: JobKind::DeleteDocument,
+                    session_id: None,
+                    object_id: Some(id),
+                    admission: JobAdmission::Manual {
+                        exclusion_key: format!("document:{id}"),
+                    },
+                    payload: serde_json::json!({
+                    "version":1,
+                    "profiles":[],
+                    "input":input}
+                    ),
+                    initial_failure: None,
+                },
+            )?
+            .view()
+            .clone();
+            if let (Some(identity), Some(generation)) = (identity, generation) {
+                let mut after = String::new();
+                let mut ordinal = 0u32;
+                loop {
+                    let mut q = tx.prepare(
+                        "SELECT vector_id,document_version,chunk_id,\
+                    content_hash FROM \
+                    knowledge_generation_mappings WHERE \
+                    document_id=?1 AND identity=?2 AND \
+                    generation=?3 AND published=1 AND \
+                    chunk_id>?4 ORDER BY chunk_id LIMIT 200",
+                    )?;
+                    let mut rows = q.query(params![
+                        id.to_string(),
+                        identity,
+                        generation.to_string(),
+                        after
+                    ])?;
+                    let mut page = 0;
+                    while let Some(r) = rows.next()? {
+                        if ordinal >= 200000 {
+                            return Err(error(Code::ResourceLimit));
+                        }
+                        tx.execute(
+                            "INSERT INTO \
+                    knowledge_index_work_items(operation_id,\
+                    ordinal,identity,generation,vector_id,\
+                    document_id,document_version,chunk_id,\
+                    content_hash,document_fence_json) VALUES (?1,\
+                    ?2,?3,?4,?5,?6,?7,?8,?9,?10)",
+                            params![
+                                operation_id.to_string(),
+                                ordinal,
+                                identity,
+                                generation.to_string(),
+                                state::text(r, 0, 20)?,
+                                id.to_string(),
+                                r.get::<_, u32>(1)?,
+                                state::text(r, 2, 36)?,
+                                state::text(r, 3, 64)?,
+                                serde_json::to_string(&f).map_err(|_| error(Code::Storage))?
+                            ],
+                        )?;
+                        after = state::text(r, 2, 36)?.into();
+                        ordinal += 1;
+                        page += 1;
+                    }
+                    if page == 0 {
+                        break;
+                    }
+                }
+            }
+            tx.execute(
+                "UPDATE documents SET status='deleting' WHERE id=?1",
+                [id.to_string()],
+            )?;
+            tx.execute(
+                "UPDATE document_chunks SET index_state='deleting' WHERE document_id=?1",
+                [id.to_string()],
+            )?;
+            Ok(job)
+        })
+    })?;
+    service.jobs.event_port().publish(job.clone());
+    service.jobs.wake();
+    Ok(job)
+}
+pub(super) struct DeleteHandler(pub(super) Arc<Core>);
+#[async_trait::async_trait]
+impl JobHandler for DeleteHandler {
+    async fn run(&self, context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
+        let e = decode_payload(&serde_json::to_string(&payload).map_err(|_| Code::Configuration)?)
+            .map_err(code)?;
+        if !e.profiles.is_empty() {
+            return Err(Code::Configuration);
+        }
+        let input: DeleteInput =
+            serde_json::from_value(e.input).map_err(|_| Code::Configuration)?;
+        input.validate().map_err(code)?;
+        if input.operation_id != context.id {
+            return Err(Code::Configuration);
+        }
+        let owner = context.operation_owner().map_err(code)?;
+        let _serial = self.0.serial.lock().await;
+        self.0
+            .delete(&input, owner, Some(&context))
+            .await
+            .map_err(code)
+    }
+}
+impl Core {
+    pub(crate) async fn delete(
+        &self,
+        input: &DeleteInput,
+        owner: OperationLease,
+        context: Option<&JobContext>,
+    ) -> AppResult<()> {
+        input.validate()?;
+        let fence = input.document.clone();
+        if fence.version != input.document_version
+            || !self.store.transaction(|tx| fence.matches(tx, "deleting"))?
+        {
+            return Err(error(Code::Cancelled));
+        }
+        if let (Some(identity), Some(generation)) = (&input.identity, input.generation) {
+            let descriptor = self.store.transaction(|tx| {
+                let mut q = tx.prepare(
+                    "SELECT seal_json FROM \
+                    knowledge_index_generation_seals WHERE \
+                    identity=?1 AND generation=?2 AND dirty=0",
+                )?;
+                let mut rows = q.query(params![identity, generation.to_string()])?;
+                let r = rows
+                    .next()?
+                    .ok_or_else(|| error(Code::IndexRepairRequired))?;
+                let seal = Seal::decode(state::text(r, 0, 65536)?)?;
+                if seal.descriptor.key()? != *identity || seal.descriptor.generation != generation {
+                    return Err(error(Code::IndexRepairRequired));
+                }
+                Ok(seal.descriptor)
+            })?;
+            let ticket = self.index.open_existing(descriptor, owner.clone())?;
+            self.index.enqueue_open(&ticket)?.observe().await?;
+            let mut after = -1i64;
+            loop {
+                let rows = self.store.transaction(|tx| {
+                    Ok(tx
+                        .prepare(
+                            "SELECT ordinal,vector_id FROM \
+                    knowledge_index_work_items WHERE \
+                    operation_id=?1 AND ordinal>?2 ORDER BY \
+                    ordinal LIMIT 200",
+                        )?
+                        .query_map(params![input.operation_id.to_string(), after], |r| {
+                            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
+                        })?
+                        .collect::<Result<Vec<_>, _>>()?)
+                })?;
+                if rows.is_empty() {
+                    break;
+                }
+                let ids = rows
+                    .iter()
+                    .map(|(_, v)| v.parse().map_err(|_| error(Code::Storage)))
+                    .collect::<AppResult<Vec<u64>>>()?;
+                self.index.delete(&ticket, ids).await?;
+                after = rows.last().unwrap().0;
+                tokio::task::yield_now().await;
+            }
+            let receipt = self.index.persist(&ticket).await?;
+            #[cfg(test)]
+            self.checkpoint(CommitPoint::AfterVectorPersist)?;
+            let plan = PublicationPlan::Delete {
+                operation: ticket.operation_id(),
+                work: input.operation_id,
+                document: fence,
+                job: context.map(JobContext::publication_fence),
+            };
+            let ack = self.index.publish_plan(&ticket, &receipt, plan).await?;
+            self.index.acknowledge(&ticket, ack).await?;
+        } else {
+            self.store.transaction(|tx| {
+                let remove = || {
+                    if !fence.matches(tx, "deleting")? {
+                        return Err(error(Code::Cancelled));
+                    }
+                    tx.execute(
+                        "UPDATE knowledge_managed_files SET \
+                    state='delete_pending' WHERE document_id=?1",
+                        [fence.id.to_string()],
+                    )?;
+                    tx.execute("DELETE FROM documents WHERE id=?1", [fence.id.to_string()])?;
+                    Ok(())
+                };
+                match context {
+                    Some(c) => c.commit(tx, remove),
+                    None => remove(),
+                }
+            })?;
+        }
+        cleanup_managed(self.store.clone(), self.data_dir.clone(), owner).await?;
+        self.ack_delete(input.operation_id)
+    }
+    pub(crate) fn ack_delete(&self, operation: Uuid) -> AppResult<()> {
+        loop {
+            let count = self.store.transaction(|tx| {
+                let rows = tx
+                    .prepare(
+                        "SELECT ordinal,identity,generation,vector_id \
+                    FROM knowledge_index_work_items WHERE \
+                    operation_id=?1 ORDER BY ordinal LIMIT 200",
+                    )?
+                    .query_map([operation.to_string()], |r| {
+                        Ok((
+                            r.get::<_, i64>(0)?,
+                            r.get::<_, String>(1)?,
+                            r.get::<_, String>(2)?,
+                            r.get::<_, String>(3)?,
+                        ))
+                    })?
+                    .collect::<Result<Vec<_>, _>>()?;
+                for (ordinal, identity, generation, vector) in &rows {
+                    tx.execute(
+                        "DELETE FROM knowledge_generation_mappings \
+                    WHERE identity=?1 AND generation=?2 AND \
+                    vector_id=?3",
+                        params![identity, generation, vector],
+                    )?;
+                    tx.execute(
+                        "DELETE FROM knowledge_index_work_items WHERE \
+                    operation_id=?1 AND ordinal=?2",
+                        params![operation.to_string(), ordinal],
+                    )?;
+                }
+                Ok(rows.len())
+            })?;
+            if count == 0 {
+                return Ok(());
+            }
+        }
+    }
+}
diff --git a/src-tauri/src/knowledge/service/recovery.rs b/src-tauri/src/knowledge/service/recovery.rs
new file mode 100644
index 0000000..0b4b2b4
--- /dev/null
+++ b/src-tauri/src/knowledge/service/recovery.rs
@@ -0,0 +1,323 @@
+//! Durable local deletion continuation, including witnessed dirty sources.
+use super::delete::DeleteInput;
+use super::*;
+use crate::knowledge::index::{ChunkMetadata, DocumentFence, Generation};
+impl Core {
+    pub(crate) async fn resume_deletes(&self, owner: OperationLease) -> AppResult<()> {
+        let mut after = String::new();
+        loop {
+            let inputs = self.store.transaction(|tx| {
+                let mut q = tx.prepare(
+                    "SELECT d.id,j.payload_json,j.id FROM \
+                    documents d JOIN intelligence_jobs j ON \
+                    j.id=(SELECT id FROM intelligence_jobs WHERE \
+                    kind='delete_document' AND object_id=d.id \
+                    ORDER BY created_at_ms DESC LIMIT 1) WHERE \
+                    d.status='deleting' AND d.id>?1 AND j.status \
+                    NOT IN ('queued','running') ORDER BY d.id \
+                    LIMIT 200",
+                )?;
+                let mut rows = q.query([&after])?;
+                let mut out = Vec::new();
+                while let Some(r) = rows.next()? {
+                    let envelope = decode_payload(state::text(r, 1, 1024 * 1024)?)?;
+                    let input: DeleteInput =
+                        serde_json::from_value(envelope.input).map_err(|_| error(Code::Storage))?;
+                    out.push((
+                        state::text(r, 0, 36)?.to_owned(),
+                        state::text(r, 2, 36)?.to_owned(),
+                        input,
+                    ));
+                }
+                Ok(out)
+            })?;
+            if inputs.is_empty() {
+                break;
+            }
+            for (id, job, input) in inputs {
+                after = id;
+                if input.validate().is_err()
+                    || input.document_id.to_string() != after
+                    || input.operation_id.to_string() != job
+                {
+                    continue;
+                }
+                let result = if let (Some(identity), Some(generation)) =
+                    (&input.identity, input.generation)
+                {
+                    match self.index.recovery_evidence(identity, generation) {
+                        Ok(evidence) => self.recover_delete(&input, evidence, owner.clone()).await,
+                        Err(_) => self.delete(&input, owner.clone(), None).await,
+                    }
+                } else {
+                    self.delete(&input, owner.clone(), None).await
+                };
+                if let Err(failure) = result {
+                    let remains: bool = self.store.transaction(|tx| {
+                        Ok(tx.query_row(
+                            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
+                            [input.document_id.to_string()],
+                            |r| r.get(0),
+                        )?)
+                    })?;
+                    if !remains {
+                        return Err(failure);
+                    }
+                    if let Some(identity) = &input.identity {
+                        state::mark_identity_repair(&self.store, identity, "unsealed_generation")?;
+                    }
+                }
+            }
+        }
+        Ok(())
+    }
+    async fn recover_delete(
+        &self,
+        input: &DeleteInput,
+        evidence: crate::knowledge::index::RecoveryEvidence,
+        owner: OperationLease,
+    ) -> AppResult<()> {
+        input.validate()?;
+        if !self
+            .store
+            .transaction(|tx| input.document.matches(tx, "deleting"))?
+        {
+            return Err(error(Code::IndexRepairRequired));
+        }
+        let source = evidence.source().clone();
+        let count = evidence.count();
+        let destination = Generation {
+            generation: Uuid::new_v4(),
+            ..source.clone()
+        };
+        let target = self.index.create(destination, owner.clone())?;
+        // No source engine is opened. Only the destination uses SrvDB.
+        let reader = self.index.begin_recovery(&target, evidence.clone()).await?;
+        self.validate_survivors(&source, count)?;
+        self.index.enqueue_open(&target)?.observe().await?;
+        let mut start = 0u64;
+        let mut ordinal = 0u32;
+        while start < count {
+            let mut selected = Vec::new();
+            for id in start
+                ..start
+                    .checked_add(200)
+                    .ok_or_else(|| error(Code::ResourceLimit))?
+                    .min(count)
+            {
+                if let Some((metadata, fence)) = self.recovery_chunk(&source, id)? {
+                    selected.push((id, metadata, fence));
+                }
+            }
+            #[cfg(test)]
+            self.checkpoint(CommitPoint::BeforeLocalCopy)?;
+            for (id, vector) in self.index.copy_recovery(&reader, start, 200).await? {
+                if let Some((_, metadata, fence)) = selected.iter().find(|(old, _, _)| *old == id) {
+                    let added = self
+                        .index
+                        .add(&target, vec![(vector, metadata.clone())])
+                        .await?[0];
+                    self.stage_rebuild(&target, ordinal, metadata, fence, id, added)?;
+                    ordinal = ordinal
+                        .checked_add(1)
+                        .ok_or_else(|| error(Code::ResourceLimit))?;
+                }
+            }
+            start = start
+                .checked_add(200)
+                .ok_or_else(|| error(Code::ResourceLimit))?
+                .min(count);
+            tokio::task::yield_now().await;
+        }
+        self.index.verify_recovery(&reader).await?;
+        let receipt = self.index.persist(&target).await?;
+        #[cfg(test)]
+        self.checkpoint(CommitPoint::AfterVectorPersist)?;
+        let plan = PublicationPlan::Recovery {
+            operation: target.operation_id(),
+            work: target.operation_id(),
+            token: reader.token(),
+            fence: evidence.fence.clone(),
+            count: ordinal,
+            delete_work: input.operation_id,
+            document: input.document.clone(),
+        };
+        let ack = self.index.publish_plan(&target, &receipt, plan).await?;
+        self.index.acknowledge(&target, ack).await?;
+        #[cfg(test)]
+        self.checkpoint(CommitPoint::AfterPointerPublication)?;
+        self.index.end_recovery(reader).await?;
+        self.index.retire_recovery(&target, evidence.fence).await?;
+        self.ack_delete(input.operation_id)?;
+        self.remove_generation_rows(&source)?;
+        crate::knowledge::documents::cleanup_managed(
+            self.store.clone(),
+            self.data_dir.clone(),
+            owner,
+        )
+        .await
+    }
+    fn validate_survivors(&self, source: &Generation, count: u64) -> AppResult<()> {
+        let mut start = 0u64;
+        while start < count {
+            for id in start..start.saturating_add(200).min(count) {
+                self.recovery_chunk(source, id)?;
+            }
+            start = start
+                .checked_add(200)
+                .ok_or_else(|| error(Code::ResourceLimit))?
+                .min(count);
+        }
+        // Current Ready mappings outside the certified half-open geometry can
+        // never become survivors. Attribute them through the inverse mapping.
+        let mut after = String::new();
+        loop {
+            let rows = self.store.transaction(|tx| {
+                Ok(tx
+                    .prepare(
+                        "SELECT m.chunk_id,m.document_id,m.vector_id,\
+                    m.document_version FROM \
+                    knowledge_generation_mappings m JOIN \
+                    documents d ON d.id=m.document_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.published=1 AND m.chunk_id>?3 AND \
+                    d.version=m.document_version AND \
+                    d.status='ready' ORDER BY m.chunk_id LIMIT \
+                    200",
+                    )?
+                    .query_map(
+                        params![source.key()?, source.generation.to_string(), after],
+                        |r| {
+                            Ok((
+                                r.get::<_, String>(0)?,
+                                r.get::<_, String>(1)?,
+                                r.get::<_, String>(2)?,
+                                r.get::<_, u32>(3)?,
+                            ))
+                        },
+                    )?
+                    .collect::<Result<Vec<_>, _>>()?)
+            })?;
+            if rows.is_empty() {
+                break;
+            }
+            for (chunk, doc, vector, version) in rows {
+                after = chunk;
+                if vector.parse::<u64>().map_err(|_| error(Code::Storage))? >= count {
+                    state::mark_document_repair(
+                        &self.store,
+                        Uuid::parse_str(&doc).map_err(|_| error(Code::Storage))?,
+                        version,
+                        "vector_missing",
+                    )?;
+                }
+            }
+        }
+        Ok(())
+    }
+    fn recovery_chunk(
+        &self,
+        source: &Generation,
+        vector: u64,
+    ) -> AppResult<Option<(ChunkMetadata, DocumentFence)>> {
+        self.store.transaction(|tx| {
+            let mut q = tx.prepare(
+                "SELECT m.document_id,m.document_version,\
+                    m.chunk_id,m.content_hash,d.version,d.status,\
+                    c.text,c.content_hash,c.location_json,\
+                    c.vector_id,c.embedding_identity,\
+                    d.index_identity,c.index_state,c.document_id,\
+                    c.document_version FROM \
+                    knowledge_generation_mappings m LEFT JOIN \
+                    document_chunks c ON c.id=m.chunk_id LEFT \
+                    JOIN documents d ON d.id=m.document_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.vector_id=?3 AND m.published=1",
+            )?;
+            let key = source.key()?;
+            let mut rows = q.query(params![
+                key,
+                source.generation.to_string(),
+                vector.to_string()
+            ])?;
+            let Some(r) = rows.next()? else {
+                return Ok(None);
+            };
+            let doc = Uuid::parse_str(state::text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
+            let version: u32 = r.get(1)?;
+            let status: Option<String> = r.get(5)?;
+            if r.get::<_, Option<u32>>(4)? != Some(version) {
+                return Ok(None);
+            }
+            let quarantined: bool = tx.query_row(
+                "SELECT EXISTS(SELECT 1 FROM \
+                    knowledge_repairs WHERE document_id=?1 AND \
+                    reason='unsealed_generation')",
+                [doc.to_string()],
+                |r| r.get(0),
+            )?;
+            if status.as_deref() != Some("ready")
+                && (status.as_deref() != Some("repair_required") || !quarantined)
+            {
+                return Ok(None);
+            }
+            let parsed = (|| {
+                let location: TextLocation =
+                    serde_json::from_str(state::text(r, 8, 16 * 1024 * 1024)?)
+                        .map_err(|_| error(Code::IndexRepairRequired))?;
+                let body = state::text(r, 6, 9600)?;
+                let hash = state::text(r, 3, 64)?;
+                if body.chars().count() > 2400
+                    || crate::knowledge::chunk_hash(body) != hash
+                    || state::text(r, 7, 64)? != hash
+                    || r.get::<_, Option<String>>(9)?.as_deref() != Some(&vector.to_string())
+                    || r.get::<_, Option<String>>(10)?.as_deref() != Some(&key)
+                    || r.get::<_, Option<String>>(11)?.as_deref() != Some(&key)
+                    || r.get::<_, Option<String>>(12)?.as_deref() != Some("ready")
+                    || r.get::<_, Option<String>>(13)?.as_deref() != Some(&doc.to_string())
+                    || r.get::<_, Option<u32>>(14)? != Some(version)
+                {
+                    return Err(error(Code::IndexRepairRequired));
+                }
+                Ok((
+                    ChunkMetadata::new(
+                        doc,
+                        version,
+                        Uuid::parse_str(state::text(r, 2, 36)?)
+                            .map_err(|_| error(Code::Storage))?,
+                        hash.into(),
+                        &location,
+                    )?,
+                    state::fence(tx, doc)?,
+                ))
+            })();
+            match parsed {
+                Ok(found) => Ok(Some(found)),
+                Err(_) => {
+                    tx.execute(
+                        "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    id=?1 AND version=?2 AND status IN ('ready',\
+                    'repair_required')",
+                        params![doc.to_string(), version],
+                    )?;
+                    tx.execute(
+                        "UPDATE document_chunks SET \
+                    index_state='repair_required' WHERE \
+                    document_id=?1 AND document_version=?2",
+                        params![doc.to_string(), version],
+                    )?;
+                    tx.execute(
+                        "INSERT INTO knowledge_repairs(document_id,\
+                    reason) VALUES (?1,'stale_hash') ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason='stale_hash'",
+                        [doc.to_string()],
+                    )?;
+                    Ok(None)
+                }
+            }
+        })
+    }
+}
diff --git a/src-tauri/src/knowledge/service/reindex.rs b/src-tauri/src/knowledge/service/reindex.rs
new file mode 100644
index 0000000..b9f508b
--- /dev/null
+++ b/src-tauri/src/knowledge/service/reindex.rs
@@ -0,0 +1,204 @@
+use super::*;
+use crate::knowledge::{
+    embedding::request_body,
+    identity::EmbeddingIdentity,
+    index::{Generation, ReadyPublication},
+};
+impl Core {
+    pub(super) async fn reindex(
+        &self,
+        context: JobContext,
+        input: IndexInput,
+        profile: &ModelProfile,
+    ) -> AppResult<()> {
+        let owner = context.operation_owner()?;
+        let _serial = self.serial.lock().await;
+        let pending: u64 = self.store.transaction(|tx| {
+            Ok(tx.query_row(
+                "SELECT count(*) FROM document_chunks WHERE \
+                    document_id=?1 AND document_version=?2",
+                params![input.document_id.to_string(), input.document_version],
+                |r| r.get(0),
+            )?)
+        })?;
+        if pending > 10000 {
+            return Err(error(Code::ResourceLimit));
+        }
+        if let Some(binding) = state::binding(&self.store, profile)? {
+            if self.rebuild_needed(&binding.generation, pending)? {
+                self.rebuild_generation(binding.generation, owner.clone())
+                    .await?;
+            }
+            let current = state::binding(&self.store, profile)?
+                .ok_or_else(|| error(Code::IndexRepairRequired))?;
+            self.ensure_capacity(&current.generation, pending)?;
+        }
+        let fence = self.store.transaction(|tx| {
+            context.commit(tx, || {
+                let f = state::fence(tx, input.document_id)?;
+                if f.version != input.document_version {
+                    return Err(error(Code::Cancelled));
+                }
+                let status: String = tx.query_row(
+                    "SELECT status FROM documents WHERE id=?1",
+                    [f.id.to_string()],
+                    |r| r.get(0),
+                )?;
+                if matches!(status.as_str(), "parsing" | "deleting") {
+                    return Err(error(Code::Cancelled));
+                }
+                tx.execute(
+                    "UPDATE documents SET status='indexing',\
+                    error_code='index_repair_required' WHERE \
+                    id=?1",
+                    [f.id.to_string()],
+                )?;
+                tx.execute(
+                    "UPDATE document_chunks SET \
+                    index_state='indexing' WHERE document_id=?1 \
+                    AND document_version=?2",
+                    params![f.id.to_string(), f.version],
+                )?;
+                tx.execute(
+                    "INSERT INTO knowledge_repairs(document_id,\
+                    reason) VALUES (?1,'unsealed_generation') ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+                    [f.id.to_string()],
+                )?;
+                Ok(f)
+            })
+        })?;
+        #[cfg(test)]
+        self.checkpoint(CommitPoint::AfterSqliteIntent)?;
+        let existing = state::binding(&self.store, profile)?;
+        let mut ticket = None;
+        let mut expected = None;
+        let mut dimension = None;
+        let mut after = -1i64;
+        let mut staged = 0u32;
+        loop {
+            let page = state::chunks(&self.store, &fence, after)?;
+            if page.is_empty() {
+                break;
+            }
+            let mut cursor = 0;
+            while cursor < page.len() {
+                let mut inputs = Vec::new();
+                let start = cursor;
+                while cursor < page.len() {
+                    inputs.push(page[cursor].text.clone());
+                    if request_body(profile, &inputs).is_err() {
+                        inputs.pop();
+                        break;
+                    }
+                    cursor += 1;
+                }
+                if inputs.is_empty() {
+                    return Err(error(Code::ResourceLimit));
+                }
+                let vectors = self
+                    .embeddings
+                    .embed(
+                        profile,
+                        &inputs,
+                        EmbeddingExecution::new(owner.clone(), context.cancel.clone()),
+                    )
+                    .await?;
+                if vectors.len() != inputs.len() {
+                    return Err(error(Code::InvalidResponse));
+                }
+                let actual = vectors
+                    .first()
+                    .map(Vec::len)
+                    .ok_or_else(|| error(Code::InvalidResponse))?;
+                if dimension.is_some_and(|d| d != actual)
+                    || vectors.iter().any(|v| v.len() != actual)
+                {
+                    return Err(error(Code::InvalidResponse));
+                }
+                dimension = Some(actual);
+                let identity = EmbeddingIdentity::from_profile(
+                    profile,
+                    u32::try_from(actual).map_err(|_| error(Code::InvalidResponse))?,
+                )?;
+                if ticket.is_none() {
+                    let key = identity.key()?;
+                    expected = state::active_pointer(&self.store, &key)?;
+                    let mut chosen = None;
+                    if let Some(binding) = &existing {
+                        if binding.generation.identity != identity {
+                            return Err(error(Code::IndexRepairRequired));
+                        }
+                        if let Ok(t) = self
+                            .index
+                            .open_existing(binding.generation.clone(), owner.clone())
+                        {
+                            if self.index.enqueue_open(&t)?.observe().await.is_ok() {
+                                chosen = Some(t);
+                            }
+                        }
+                    }
+                    if chosen.is_none() {
+                        let t = self.index.create(
+                            Generation {
+                                identity,
+                                requested_dimension: profile.dimensions,
+                                generation: Uuid::new_v4(),
+                            },
+                            owner.clone(),
+                        )?;
+                        self.index.enqueue_open(&t)?.observe().await?;
+                        chosen = Some(t);
+                    }
+                    ticket = chosen;
+                }
+                let t = ticket.as_ref().unwrap();
+                let key = t.generation().key()?;
+                for (chunk, vector) in page[start..cursor].iter().zip(vectors) {
+                    if *context.cancel.borrow() {
+                        return Err(error(Code::Cancelled));
+                    }
+                    let id = if let Some((previous, id)) = &chunk.previous {
+                        if previous == &key && expected == Some(t.generation().generation) {
+                            self.index
+                                .replace(t, *id, vector, chunk.metadata.clone())
+                                .await?
+                        } else {
+                            self.index
+                                .add(t, vec![(vector, chunk.metadata.clone())])
+                                .await?[0]
+                        }
+                    } else {
+                        self.index
+                            .add(t, vec![(vector, chunk.metadata.clone())])
+                            .await?[0]
+                    };
+                    #[cfg(test)]
+                    self.checkpoint(CommitPoint::AfterVectorMutation)?;
+                    state::stage(&self.store, &context, t, staged, chunk, id)?;
+                    staged = staged
+                        .checked_add(1)
+                        .ok_or_else(|| error(Code::ResourceLimit))?;
+                    after = i64::from(chunk.ordinal);
+                }
+            }
+        }
+        let ticket = ticket.ok_or_else(|| error(Code::NoFinalText))?;
+        let receipt = self.index.persist(&ticket).await?;
+        #[cfg(test)]
+        self.checkpoint(CommitPoint::AfterVectorPersist)?;
+        #[cfg(test)]
+        self.checkpoint(CommitPoint::BeforeReady)?;
+        let plan = PublicationPlan::Ready(ReadyPublication {
+            operation: ticket.operation_id(),
+            work: ticket.operation_id(),
+            document: fence,
+            expected_generation: expected,
+            count: staged,
+            job: context.publication_fence(),
+        });
+        let ack = self.index.publish_plan(&ticket, &receipt, plan).await?;
+        self.index.acknowledge(&ticket, ack).await
+    }
+}
diff --git a/src-tauri/src/knowledge/service/searches.rs b/src-tauri/src/knowledge/service/searches.rs
new file mode 100644
index 0000000..982a682
--- /dev/null
+++ b/src-tauri/src/knowledge/service/searches.rs
@@ -0,0 +1,110 @@
+//! Two admitted searches retain handles and actual-completion capacity.
+use super::*;
+use std::sync::{
+    Mutex,
+    atomic::{AtomicBool, Ordering},
+};
+use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch};
+struct SearchOwner {
+    _parent: OperationLease,
+    _permit: OwnedSemaphorePermit,
+}
+struct Slot {
+    cancel: watch::Sender<bool>,
+    task: tokio::task::JoinHandle<()>,
+}
+pub(super) struct Searches {
+    permits: Arc<Semaphore>,
+    slots: Mutex<Vec<Slot>>,
+    closed: AtomicBool,
+}
+struct Observer(watch::Sender<bool>);
+impl Drop for Observer {
+    fn drop(&mut self) {
+        self.0.send_replace(true);
+    }
+}
+impl Searches {
+    pub(super) fn new() -> Self {
+        Self {
+            permits: Arc::new(Semaphore::new(2)),
+            slots: Mutex::new(Vec::new()),
+            closed: AtomicBool::new(false),
+        }
+    }
+    pub(super) async fn run(
+        &self,
+        core: Arc<Core>,
+        request: KnowledgeSearch,
+        parent: OperationLease,
+    ) -> AppResult<KnowledgeResults> {
+        crate::knowledge::search::validate(&core.store, &request)?;
+        let permit = self
+            .permits
+            .clone()
+            .try_acquire_owned()
+            .map_err(|_| error(Code::Busy))?;
+        let (cancel, receiver) = watch::channel(false);
+        let (reply, result) = oneshot::channel();
+        {
+            let mut slots = self.slots.lock().unwrap();
+            slots.retain(|s| !s.task.is_finished());
+            if self.closed.load(Ordering::Acquire) || slots.len() >= 2 {
+                return Err(error(Code::Busy));
+            }
+            for slot in slots.iter() {
+                slot.cancel.send_replace(true);
+            }
+            let owner: OperationLease = Arc::new(SearchOwner {
+                _parent: parent,
+                _permit: permit,
+            });
+            let task = tokio::spawn(async move {
+                let outcome = core.search(request, owner, receiver).await;
+                let _ = reply.send(outcome);
+            });
+            slots.push(Slot {
+                cancel: cancel.clone(),
+                task,
+            });
+        }
+        let _observer = Observer(cancel);
+        result.await.map_err(|_| error(Code::Interrupted))?
+    }
+    pub(super) async fn shutdown(&self) -> AppResult<()> {
+        self.closed.store(true, Ordering::Release);
+        let mut slots = std::mem::take(&mut *self.slots.lock().unwrap());
+        for slot in &slots {
+            slot.cancel.send_replace(true);
+        }
+        while let Some(mut slot) = slots.pop() {
+            match tokio::time::timeout(std::time::Duration::from_secs(10), &mut slot.task).await {
+                Ok(Ok(())) => {}
+                Ok(Err(_)) => {}
+                Err(_) => {
+                    slots.push(slot);
+                    self.slots.lock().unwrap().extend(slots);
+                    return Err(error(Code::Timeout));
+                }
+            }
+        }
+        Ok(())
+    }
+    pub(super) fn reopen(&self) -> AppResult<()> {
+        let mut slots = self.slots.lock().unwrap();
+        if slots.iter().any(|s| !s.task.is_finished()) {
+            return Err(error(Code::Busy));
+        }
+        slots.clear();
+        self.closed.store(false, Ordering::Release);
+        Ok(())
+    }
+}
+
+impl Drop for Searches {
+    fn drop(&mut self) {
+        for slot in self.slots.get_mut().unwrap().iter() {
+            slot.cancel.send_replace(true);
+        }
+    }
+}
diff --git a/src-tauri/src/knowledge/service/state.rs b/src-tauri/src/knowledge/service/state.rs
new file mode 100644
index 0000000..c69b694
--- /dev/null
+++ b/src-tauri/src/knowledge/service/state.rs
@@ -0,0 +1,437 @@
+use super::*;
+use crate::knowledge::{
+    identity::EmbeddingIdentity,
+    index::{ChunkMetadata, DocumentFence, Generation, Seal},
+    types::TextLocation,
+};
+use rusqlite::{OptionalExtension, Transaction, params, types::ValueRef};
+#[derive(Clone)]
+pub(crate) struct Binding {
+    pub(crate) generation: Generation,
+}
+pub(crate) fn text<'a>(
+    row: &'a rusqlite::Row<'_>,
+    column: usize,
+    max: usize,
+) -> AppResult<&'a str> {
+    match row.get_ref(column)? {
+        ValueRef::Text(bytes) if bytes.len() <= max => {
+            std::str::from_utf8(bytes).map_err(|_| error(Code::Storage))
+        }
+        _ => Err(error(Code::ResourceLimit)),
+    }
+}
+pub(crate) fn optional_text<'a>(
+    row: &'a rusqlite::Row<'_>,
+    column: usize,
+    max: usize,
+) -> AppResult<Option<&'a str>> {
+    if matches!(row.get_ref(column)?, ValueRef::Null) {
+        Ok(None)
+    } else {
+        text(row, column, max).map(Some)
+    }
+}
+pub(crate) fn fence(tx: &Transaction<'_>, id: Uuid) -> AppResult<DocumentFence> {
+    let mut q = tx.prepare(
+        "SELECT version,sha256,parser_identity,\
+                    import_job_id FROM documents WHERE id=?1",
+    )?;
+    let mut rows = q.query([id.to_string()])?;
+    let r = rows.next()?.ok_or_else(|| error(Code::Configuration))?;
+    let raw = optional_text(r, 3, 36)?;
+    let f = DocumentFence {
+        id,
+        version: r.get(0)?,
+        sha256: text(r, 1, 64)?.into(),
+        parser: text(r, 2, 128)?.into(),
+        import_job: raw
+            .map(|i| {
+                let id = Uuid::parse_str(i).map_err(|_| error(Code::Storage))?;
+                if id.to_string() != i {
+                    return Err(error(Code::Storage));
+                }
+                Ok(id)
+            })
+            .transpose()?,
+    };
+    f.validate()?;
+    Ok(f)
+}
+pub(crate) fn binding(store: &LocalStore, profile: &ModelProfile) -> AppResult<Option<Binding>> {
+    let semantic = EmbeddingIdentity::from_profile(profile, profile.dimensions.unwrap_or(128))?
+        .semantic_key()?;
+    store.transaction(|tx| {
+        let mut query = tx.prepare(
+            "SELECT g.identity,g.dimension,s.seal_json,\
+                    g.active_generation,g.requested_dimension \
+                    FROM knowledge_index_generations g JOIN \
+                    knowledge_index_generation_seals s ON \
+                    s.identity=g.identity AND \
+                    s.generation=g.active_generation WHERE \
+                    g.semantic_key=?1 AND g.state='active' AND \
+                    s.dirty=0 AND (?2 IS NULL OR g.dimension=?2) \
+                    ORDER BY g.identity LIMIT 2",
+        )?;
+        let mut rows = query.query(params![semantic, profile.dimensions])?;
+        let mut found = None;
+        while let Some(r) = rows.next()? {
+            let seal = Seal::decode(text(r, 2, 65536)?)?;
+            if seal.descriptor.generation.to_string() != text(r, 3, 36)?
+                || seal.descriptor.requested_dimension != r.get::<_, Option<u32>>(4)?
+                || seal.descriptor.key()? != text(r, 0, 64)?
+                || seal.descriptor.identity.dimensions != r.get::<_, u32>(1)?
+                || seal.descriptor.identity.semantic_key()? != semantic
+            {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            if found.is_some() {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            found = Some(Binding {
+                generation: seal.descriptor,
+            });
+        }
+        Ok(found)
+    })
+}
+pub(crate) fn active_pointer(store: &LocalStore, identity: &str) -> AppResult<Option<Uuid>> {
+    store.transaction(|tx| {
+        let raw: Option<Option<String>> = tx
+            .query_row(
+                "SELECT active_generation FROM \
+                    knowledge_index_generations WHERE identity=?1",
+                [identity],
+                |r| r.get(0),
+            )
+            .optional()?;
+        raw.flatten()
+            .map(|s| Uuid::parse_str(&s).map_err(|_| error(Code::Storage)))
+            .transpose()
+    })
+}
+pub(crate) fn health(
+    store: &LocalStore,
+    identity: &str,
+    generation: Uuid,
+) -> AppResult<(u32, u32)> {
+    store.transaction(|tx| {
+        let total: u32 = tx.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))?;
+        let healthy: u32 = tx.query_row(
+            "SELECT count(*) FROM documents d WHERE \
+                    d.status='ready' AND d.index_identity=?1 AND \
+                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
+                    c.document_id=d.id AND \
+                    c.document_version=d.version) AND NOT \
+                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
+                    c.document_id=d.id AND \
+                    c.document_version=d.version AND \
+                    (c.index_state!='ready' OR \
+                    c.embedding_identity IS NOT ?1 OR NOT \
+                    EXISTS(SELECT 1 FROM \
+                    knowledge_generation_mappings m WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.published=1 AND m.chunk_id=c.id AND \
+                    m.document_id=d.id AND \
+                    m.document_version=d.version AND \
+                    m.content_hash=c.content_hash AND \
+                    m.vector_id=c.vector_id)))",
+            params![identity, generation.to_string()],
+            |r| r.get(0),
+        )?;
+        Ok((healthy, total.saturating_sub(healthy)))
+    })
+}
+pub(crate) fn mark_identity_repair(
+    store: &LocalStore,
+    identity: &str,
+    reason: &str,
+) -> AppResult<()> {
+    store.transaction(|tx| {
+        tx.execute(
+            "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    index_identity=?1 AND status='ready'",
+            [identity],
+        )?;
+        tx.execute(
+            "INSERT INTO knowledge_repairs(document_id,\
+                    reason) SELECT id,?2 FROM documents WHERE \
+                    index_identity=?1 AND \
+                    status='repair_required' ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+            params![identity, reason],
+        )?;
+        Ok(())
+    })
+}
+pub(crate) struct Chunk {
+    pub(crate) ordinal: u32,
+    pub(crate) text: String,
+    pub(crate) metadata: ChunkMetadata,
+    pub(crate) previous: Option<(String, u64)>,
+}
+pub(crate) fn chunks(
+    store: &LocalStore,
+    fence: &DocumentFence,
+    after: i64,
+) -> AppResult<Vec<Chunk>> {
+    store.transaction(|tx| {
+        if !fence.matches(tx, "indexing")? {
+            return Err(error(Code::Cancelled));
+        }
+        let mut q = tx.prepare(
+            "SELECT chunk_index,id,text,location_json,\
+                    content_hash,embedding_identity,vector_id \
+                    FROM document_chunks WHERE document_id=?1 \
+                    AND document_version=?2 AND chunk_index>?3 \
+                    ORDER BY chunk_index LIMIT 32",
+        )?;
+        let mut rows = q.query(params![fence.id.to_string(), fence.version, after])?;
+        let mut out = Vec::new();
+        while let Some(r) = rows.next()? {
+            let body = text(r, 2, 9600)?;
+            if body.chars().count() > 2400 {
+                return Err(error(Code::ResourceLimit));
+            }
+            let hash = text(r, 4, 64)?;
+            if crate::knowledge::chunk_hash(body) != hash {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            let location: TextLocation = serde_json::from_str(text(r, 3, 16 * 1024 * 1024)?)
+                .map_err(|_| error(Code::Storage))?;
+            let metadata = ChunkMetadata::new(
+                fence.id,
+                fence.version,
+                Uuid::parse_str(text(r, 1, 36)?).map_err(|_| error(Code::Storage))?,
+                hash.into(),
+                &location,
+            )?;
+            let previous = match (
+                r.get::<_, Option<String>>(5)?,
+                r.get::<_, Option<String>>(6)?,
+            ) {
+                (Some(i), Some(v)) => Some((i, v.parse().map_err(|_| error(Code::Storage))?)),
+                _ => None,
+            };
+            out.push(Chunk {
+                ordinal: r.get(0)?,
+                text: body.into(),
+                metadata,
+                previous,
+            });
+        }
+        Ok(out)
+    })
+}
+pub(crate) fn stage(
+    store: &LocalStore,
+    context: &JobContext,
+    ticket: &crate::knowledge::index::WorkTicket,
+    ordinal: u32,
+    chunk: &Chunk,
+    id: u64,
+) -> AppResult<()> {
+    let identity = ticket.generation().key()?;
+    let generation = ticket.generation().generation.to_string();
+    store.transaction(|tx| {
+        context.commit(tx, || {
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_index_work_items(operation_id,\
+                    ordinal,identity,generation,vector_id,\
+                    document_id,document_version,chunk_id,\
+                    content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,\
+                    ?8,?9)",
+                params![
+                    ticket.operation_id().to_string(),
+                    ordinal,
+                    identity,
+                    generation,
+                    id.to_string(),
+                    chunk.metadata.document_id.to_string(),
+                    chunk.metadata.document_version,
+                    chunk.metadata.chunk_id.to_string(),
+                    chunk.metadata.content_hash
+                ],
+            )?;
+            let f = fence(tx, chunk.metadata.document_id)?;
+            let mut q = tx.prepare("SELECT location_json FROM document_chunks WHERE id=?1")?;
+            let mut rows = q.query([chunk.metadata.chunk_id.to_string()])?;
+            let r = rows.next()?.ok_or_else(|| error(Code::Storage))?;
+            let location: TextLocation = serde_json::from_str(text(r, 0, 16 * 1024 * 1024)?)
+                .map_err(|_| error(Code::Storage))?;
+            if !chunk.metadata.matches_location(&location)? {
+                return Err(error(Code::IndexRepairRequired));
+            }
+            tx.execute(
+                "UPDATE knowledge_index_work_items SET \
+                    document_fence_json=?1,location_sha256=?2 \
+                    WHERE operation_id=?3 AND ordinal=?4",
+                params![
+                    serde_json::to_string(&f).map_err(|_| error(Code::Storage))?,
+                    crate::knowledge::index::location_digest(&location)?,
+                    ticket.operation_id().to_string(),
+                    ordinal
+                ],
+            )?;
+            tx.execute(
+                "INSERT INTO \
+                    knowledge_generation_mappings(identity,\
+                    generation,chunk_id,document_id,\
+                    document_version,vector_id,content_hash,\
+                    published) VALUES (?1,?2,?3,?4,?5,?6,?7,0) \
+                    ON CONFLICT(identity,generation,chunk_id) DO \
+                    UPDATE SET vector_id=excluded.vector_id,\
+                    content_hash=excluded.content_hash,\
+                    published=0",
+                params![
+                    identity,
+                    generation,
+                    chunk.metadata.chunk_id.to_string(),
+                    chunk.metadata.document_id.to_string(),
+                    chunk.metadata.document_version,
+                    id.to_string(),
+                    chunk.metadata.content_hash
+                ],
+            )?;
+            Ok(())
+        })
+    })
+}
+pub(crate) enum Record {
+    Healthy(ChunkMetadata, DocumentFence),
+    Orphan,
+    Repair(Uuid, u32, &'static str),
+    Tombstone,
+}
+pub(crate) fn classify(
+    store: &LocalStore,
+    identity: &str,
+    generation: Uuid,
+    vector: u64,
+    metadata: Option<&ChunkMetadata>,
+) -> AppResult<Record> {
+    store.transaction(|tx| {
+        let mut q = tx.prepare(
+            "SELECT m.document_id,m.document_version,\
+                    m.chunk_id,m.content_hash,d.version,d.status,\
+                    c.text,c.content_hash,c.location_json,\
+                    c.vector_id,c.embedding_identity,\
+                    d.index_identity,c.document_id,\
+                    c.document_version,c.index_state FROM \
+                    knowledge_generation_mappings m JOIN \
+                    knowledge_index_generations g ON \
+                    g.identity=m.identity AND \
+                    g.active_generation=m.generation LEFT JOIN \
+                    document_chunks c ON c.id=m.chunk_id LEFT \
+                    JOIN documents d ON d.id=m.document_id WHERE \
+                    m.identity=?1 AND m.generation=?2 AND \
+                    m.vector_id=?3 AND m.published=1",
+        )?;
+        let mut rows = q.query(params![
+            identity,
+            generation.to_string(),
+            vector.to_string()
+        ])?;
+        let Some(r) = rows.next()? else {
+            return Ok(if metadata.is_some() {
+                Record::Orphan
+            } else {
+                Record::Tombstone
+            });
+        };
+        let current: Option<u32> = r.get(4)?;
+        let status: Option<String> = r.get(5)?;
+        let version: u32 = r.get(1)?;
+        if current != Some(version) || status.as_deref() != Some("ready") {
+            return Ok(if metadata.is_some() {
+                Record::Orphan
+            } else {
+                Record::Tombstone
+            });
+        }
+        let doc = Uuid::parse_str(text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
+        let Some(metadata) = metadata else {
+            return Ok(Record::Repair(doc, version, "vector_missing"));
+        };
+        let location: TextLocation = match text(r, 8, 16 * 1024 * 1024)
+            .and_then(|s| serde_json::from_str(s).map_err(|_| error(Code::Storage)))
+        {
+            Ok(l) => l,
+            Err(_) => return Ok(Record::Repair(doc, version, "vector_missing")),
+        };
+        let valid = (|| {
+            Ok::<_, crate::error::AppError>(
+                metadata.document_id == doc
+                    && metadata.document_version == version
+                    && metadata.chunk_id.to_string() == text(r, 2, 36)?
+                    && metadata.content_hash == text(r, 3, 64)?
+                    && metadata.content_hash == text(r, 7, 64)?
+                    && text(r, 6, 9600)?.chars().count() <= 2400
+                    && crate::knowledge::chunk_hash(text(r, 6, 9600)?) == metadata.content_hash
+                    && optional_text(r, 9, 20)? == Some(vector.to_string().as_str())
+                    && optional_text(r, 10, 64)? == Some(identity)
+                    && optional_text(r, 11, 64)? == Some(identity)
+                    && optional_text(r, 12, 36)? == Some(doc.to_string().as_str())
+                    && r.get::<_, Option<u32>>(13)? == Some(version)
+                    && optional_text(r, 14, 32)? == Some("ready")
+                    && metadata.matches_location(&location)?,
+            )
+        })()
+        .unwrap_or(false);
+        if !valid {
+            return Ok(Record::Repair(doc, version, "stale_hash"));
+        }
+        let frozen = match fence(tx, doc) {
+            Ok(f) => f,
+            Err(_) => return Ok(Record::Repair(doc, version, "stale_hash")),
+        };
+        Ok(Record::Healthy(
+            ChunkMetadata::new(
+                doc,
+                version,
+                metadata.chunk_id,
+                metadata.content_hash.clone(),
+                &location,
+            )?,
+            frozen,
+        ))
+    })
+}
+pub(crate) fn mark_document_repair(
+    store: &LocalStore,
+    id: Uuid,
+    version: u32,
+    reason: &str,
+) -> AppResult<()> {
+    store.transaction(|tx| {
+        if tx.execute(
+            "UPDATE documents SET \
+                    status='repair_required',\
+                    error_code='index_repair_required' WHERE \
+                    id=?1 AND version=?2 AND status='ready'",
+            params![id.to_string(), version],
+        )? != 1
+        {
+            return Ok(());
+        }
+        tx.execute(
+            "UPDATE document_chunks SET \
+                    index_state='repair_required' WHERE \
+                    document_id=?1 AND document_version=?2 AND \
+                    index_state='ready'",
+            params![id.to_string(), version],
+        )?;
+        tx.execute(
+            "INSERT INTO knowledge_repairs(document_id,\
+                    reason) VALUES (?1,?2) ON \
+                    CONFLICT(document_id) DO UPDATE SET \
+                    reason=excluded.reason",
+            params![id.to_string(), reason],
+        )?;
+        Ok(())
+    })
+}
diff --git a/src-tauri/src/knowledge/types.rs b/src-tauri/src/knowledge/types.rs
index 3b6b263..6bf353c 100644
--- a/src-tauri/src/knowledge/types.rs
+++ b/src-tauri/src/knowledge/types.rs
@@ -61,10 +61,42 @@ pub struct DocumentPage {
 }
 #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
 #[serde(deny_unknown_fields)]
 pub struct DocumentDetail {
     pub summary: DocumentSummary,
     pub original_path: Option<std::path::PathBuf>,
     pub managed_path: Option<std::path::PathBuf>,
     pub legacy_uri: Option<String>,
     pub index_identity: Option<String>,
 }
+
+#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
+#[serde(deny_unknown_fields)]
+pub struct KnowledgeSearch {
+    pub query: String,
+    #[serde(default)]
+    pub selected_segment_ids: Vec<uuid::Uuid>,
+    pub session_id: Option<uuid::Uuid>,
+    #[serde(default = "search_limit")]
+    pub limit: u32,
+}
+fn search_limit() -> u32 {
+    5
+}
+#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
+#[serde(deny_unknown_fields)]
+pub struct KnowledgeHit {
+    pub document_id: uuid::Uuid,
+    pub document_version: u32,
+    pub chunk_id: uuid::Uuid,
+    pub title: String,
+    pub location: TextLocation,
+    pub text: String,
+    pub score: f32,
+}
+#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
+#[serde(deny_unknown_fields)]
+pub struct KnowledgeResults {
+    pub hits: Vec<KnowledgeHit>,
+    pub identity: Option<String>,
+    pub unavailable_documents: u32,
+}
diff --git a/src-tauri/src/storage/sqlite.rs b/src-tauri/src/storage/sqlite.rs
index 260c284..22272b3 100644
--- a/src-tauri/src/storage/sqlite.rs
+++ b/src-tauri/src/storage/sqlite.rs
@@ -26,39 +26,43 @@ impl LocalStore {
     pub(crate) fn fail_lifecycle_status_for_tests(&self, status: &str) -> AppResult<()> {
         if !matches!(
             status,
             "running" | "failed" | "starting" | "stopping" | "completed"
         ) {
             return Err(crate::error::AppError::InvalidRequest(
                 "unsupported test lifecycle status".into(),
             ));
         }
         self.connection.lock().unwrap().execute_batch(&format!(
-            "CREATE TRIGGER fail_{status} BEFORE UPDATE ON session_lifecycle WHEN NEW.status = '{status}' BEGIN SELECT RAISE(ABORT, 'injected lifecycle failure'); END;"
+            "CREATE TRIGGER fail_{status} BEFORE UPDATE \
+                    ON session_lifecycle WHEN NEW.status = \
+                    '{status}' BEGIN SELECT RAISE(ABORT, \
+                    'injected lifecycle failure'); END;"
         ))?;
         Ok(())
     }
     pub fn open(data_dir: &Path) -> AppResult<Self> {
         std::fs::create_dir_all(data_dir)
             .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
         Self::initialize(Connection::open(data_dir.join("vox-bridge.sqlite"))?)
     }
     pub fn in_memory() -> AppResult<Self> {
         Self::initialize(Connection::open_in_memory()?)
     }
     fn initialize(mut conn: Connection) -> AppResult<Self> {
         let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
         const MIGRATIONS: &[&str] = &[
             include_str!("../../migrations/0001_session_foundation.sql"),
             include_str!("../../migrations/0002_runtime.sql"),
             include_str!("../../migrations/0003_intelligence.sql"),
             include_str!("../../migrations/0004_knowledge.sql"),
+            include_str!("../../migrations/0005_knowledge_index.sql"),
         ];
         if version > MIGRATIONS.len() as i64 {
             return Err(AppError::InvalidRequest(
                 "database schema is newer than this application".into(),
             ));
         }
         conn.pragma_update(None, "foreign_keys", true)?;
         if version < MIGRATIONS.len() as i64 {
             let tx = conn.transaction()?;
             for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
@@ -83,54 +87,71 @@ impl LocalStore {
             .map_err(|_| AppError::Internal("database lock unavailable".into()))?;
         let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
         let value = operation(&tx)?;
         tx.commit()?;
         Ok(value)
     }
     pub fn create_session(&self, s: &StoredSession) -> AppResult<()> {
         let mut c = self.connection.lock().unwrap();
         let tx = c.transaction()?;
         tx.execute(
-            "INSERT INTO sessions (id,started_at,mode,direction,source_language,target_language) VALUES (?1,?2,?3,?4,?5,?6)",
+            "INSERT INTO sessions (id,started_at,mode,\
+                    direction,source_language,target_language) \
+                    VALUES (?1,?2,?3,?4,?5,?6)",
             params![
                 s.id.to_string(),
                 s.started_at,
                 s.mode,
                 s.direction,
                 s.source_language,
                 s.target_language
             ],
         )?;
         tx.execute(
-            "INSERT INTO session_lifecycle (session_id,status,ended_at) VALUES (?1,'created',NULL)",
+            "INSERT INTO session_lifecycle (session_id,\
+                    status,ended_at) VALUES (?1,'created',NULL)",
             params![s.id.to_string()],
         )?;
         tx.commit()?;
         Ok(())
     }
 
     /// Admission is atomic: no runtime may open until this transaction commits.
     pub fn create_runtime_session(
         &self,
         session: &StoredSession,
         context: &SessionContext,
     ) -> AppResult<()> {
         let context_json = json(context)?;
         let mut connection = self.connection.lock().unwrap();
         let tx = connection.transaction()?;
         tx.execute(
-            "INSERT INTO sessions (id,started_at,mode,direction,source_language,target_language) VALUES (?1,?2,?3,?4,?5,?6)",
-            params![session.id.to_string(), session.started_at, session.mode, session.direction, session.source_language, session.target_language],
+            "INSERT INTO sessions (id,started_at,mode,\
+                    direction,source_language,target_language) \
+                    VALUES (?1,?2,?3,?4,?5,?6)",
+            params![
+                session.id.to_string(),
+                session.started_at,
+                session.mode,
+                session.direction,
+                session.source_language,
+                session.target_language
+            ],
         )?;
-        tx.execute("INSERT INTO session_lifecycle (session_id,status,ended_at) VALUES (?1,'starting',NULL)", params![session.id.to_string()])?;
         tx.execute(
-            "INSERT INTO session_runtime_context (session_id,context_json) VALUES (?1,?2)",
+            "INSERT INTO session_lifecycle (session_id,\
+                    status,ended_at) VALUES (?1,'starting',NULL)",
+            params![session.id.to_string()],
+        )?;
+        tx.execute(
+            "INSERT INTO session_runtime_context \
+                    (session_id,context_json) VALUES (?1,?2)",
             params![session.id.to_string(), context_json],
         )?;
         tx.commit()?;
         Ok(())
     }
 
     pub fn session_context(&self, session_id: Uuid) -> AppResult<Option<SessionContext>> {
         let encoded: Option<String> = self
             .connection
             .lock()
@@ -151,46 +172,62 @@ impl LocalStore {
 
     /// Caller must hold the exclusive data-directory process owner before recovery.
     /// The transaction retains transcript/context and never reopens runtime resources.
     pub fn interrupt_incomplete_sessions(&self, ended_at: i64) -> AppResult<Vec<Uuid>> {
         let occurred_at_ms = ended_at
             .checked_mul(1000)
             .ok_or_else(|| AppError::InvalidRequest("recovery timestamp out of range".into()))?;
         let mut connection = self.connection.lock().unwrap();
         let tx = connection.transaction()?;
         let ids = {
-            let mut query = tx.prepare("SELECT session_id FROM session_lifecycle WHERE status IN ('created','starting','running','stopping','paused') ORDER BY session_id")?;
+            let mut query = tx.prepare(
+                "SELECT session_id FROM session_lifecycle \
+                    WHERE status IN ('created','starting',\
+                    'running','stopping','paused') ORDER BY \
+                    session_id",
+            )?;
             let rows = query.query_map([], |row| row.get::<_, String>(0))?;
             rows.map(|row| {
                 Uuid::parse_str(&row?)
                     .map_err(|_| AppError::Internal("invalid stored session ID".into()))
             })
             .collect::<AppResult<Vec<_>>>()?
         };
         for id in &ids {
             tx.execute(
-                "UPDATE session_lifecycle SET status='failed',ended_at=?2 WHERE session_id=?1",
+                "UPDATE session_lifecycle SET status='failed',\
+                    ended_at=?2 WHERE session_id=?1",
                 params![id.to_string(), ended_at],
             )?;
             let diagnostic = RuntimeDiagnostic {
                 id: Uuid::new_v4(),
                 session_id: *id,
                 direction: None,
                 stream_epoch: None,
                 occurred_at_ms,
                 payload: DiagnosticPayload::Failure {
                     code: RuntimeFailureCode::Interrupted,
                     provider_code: None,
                     log_id: None,
                 },
             };
-            tx.execute("INSERT INTO session_diagnostics (id,session_id,occurred_at_ms,payload_json) VALUES (?1,?2,?3,?4)", params![diagnostic.id.to_string(), id.to_string(), occurred_at_ms, json(&diagnostic)?])?;
+            tx.execute(
+                "INSERT INTO session_diagnostics (id,\
+                    session_id,occurred_at_ms,payload_json) \
+                    VALUES (?1,?2,?3,?4)",
+                params![
+                    diagnostic.id.to_string(),
+                    id.to_string(),
+                    occurred_at_ms,
+                    json(&diagnostic)?
+                ],
+            )?;
         }
         tx.commit()?;
         Ok(ids)
     }
 
     pub fn update_session_lifecycle(&self, lifecycle: &StoredSessionLifecycle) -> AppResult<()> {
         let c = self.connection.lock().unwrap();
         let changed = c.execute(
             "UPDATE session_lifecycle SET status=?2,ended_at=?3 WHERE session_id=?1",
             params![
@@ -203,42 +240,46 @@ impl LocalStore {
             return Err(crate::error::AppError::SessionNotFound(
                 lifecycle.session_id,
             ));
         }
         Ok(())
     }
 
     pub fn get_session_lifecycle(&self, id: Uuid) -> AppResult<Option<StoredSessionLifecycle>> {
         let c = self.connection.lock().unwrap();
         Ok(c.query_row(
-            "SELECT session_id,status,ended_at FROM session_lifecycle WHERE session_id=?1",
+            "SELECT session_id,status,ended_at FROM \
+                    session_lifecycle WHERE session_id=?1",
             params![id.to_string()],
             |r| {
                 Ok(StoredSessionLifecycle {
                     session_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                     status: r.get(1)?,
                     ended_at: r.get(2)?,
                 })
             },
         )
         .optional()?)
     }
     /// Legacy writer: typed provider events must use append_transcript.
     pub fn append_final_segment(&self, s: &StoredTranscriptSegment) -> AppResult<()> {
         if !s.is_final || s.details.is_some() || s.occurred_at_ms.is_some() {
             return Err(crate::error::AppError::InvalidRequest(
                 "legacy writer requires final segments without runtime metadata".into(),
             ));
         }
         let c = self.connection.lock().unwrap();
         c.execute(
-            "INSERT INTO translation_segments (id,session_id,direction,sequence,occurred_at,language,text,is_final) VALUES (?1,?2,?3,?4,?5,?6,?7,1)",
+            "INSERT INTO translation_segments (id,\
+                    session_id,direction,sequence,occurred_at,\
+                    language,text,is_final) VALUES (?1,?2,?3,?4,\
+                    ?5,?6,?7,1)",
             params![
                 s.id.to_string(),
                 s.session_id.to_string(),
                 s.direction,
                 s.sequence,
                 s.occurred_at,
                 s.language,
                 s.text
             ],
         )?;
@@ -248,34 +289,54 @@ impl LocalStore {
         update.validate()?;
         if !update.is_final {
             return Err(AppError::InvalidRequest(
                 "only final transcript events may be persisted".into(),
             ));
         }
         let details = json(update.details.as_ref().expect("validated details"))?;
         let sequence = i64::try_from(update.sequence)
             .map_err(|_| crate::domain::transcript::invalid_transcript())?;
         self.connection.lock().unwrap().execute(
-            "INSERT INTO translation_segments (id,session_id,direction,sequence,occurred_at,language,text,is_final,details_json,occurred_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?9)",
-            params![update.id.to_string(), update.session_id.to_string(), direction_name(update.direction), sequence, update.occurred_at_ms.div_euclid(1000), update.language, update.text, details, update.occurred_at_ms],
+            "INSERT INTO translation_segments (id,\
+                    session_id,direction,sequence,occurred_at,\
+                    language,text,is_final,details_json,\
+                    occurred_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,\
+                    1,?8,?9)",
+            params![
+                update.id.to_string(),
+                update.session_id.to_string(),
+                direction_name(update.direction),
+                sequence,
+                update.occurred_at_ms.div_euclid(1000),
+                update.language,
+                update.text,
+                details,
+                update.occurred_at_ms
+            ],
         )?;
         Ok(())
     }
     pub fn transcript_page(
         &self,
         session_id: Uuid,
         after: Option<i64>,
         limit: u32,
     ) -> AppResult<TranscriptPage> {
         validate_page(after, limit)?;
         let c = self.connection.lock().unwrap();
-        let mut query = c.prepare("SELECT rowid,id,session_id,direction,sequence,occurred_at,language,text,is_final,details_json,occurred_at_ms FROM translation_segments WHERE session_id=?1 AND rowid>?2 ORDER BY rowid LIMIT ?3")?;
+        let mut query = c.prepare(
+            "SELECT rowid,id,session_id,direction,\
+                    sequence,occurred_at,language,text,is_final,\
+                    details_json,occurred_at_ms FROM \
+                    translation_segments WHERE session_id=?1 AND \
+                    rowid>?2 ORDER BY rowid LIMIT ?3",
+        )?;
         let mut rows = query.query(params![
             session_id.to_string(),
             after.unwrap_or(0),
             limit + 1
         ])?;
         let mut found = Vec::new();
         while let Some(row) = rows.next()? {
             let rowid: i64 = row.get(0)?;
             let segment = read_segment_at(row, 1)?;
             let occurred_at_ms = segment.occurred_at_ms.map(Ok).unwrap_or_else(|| {
@@ -306,67 +367,89 @@ impl LocalStore {
     }
     pub fn next_transcript_sequence(
         &self,
         session_id: Uuid,
         direction: SessionDirection,
     ) -> AppResult<u64> {
         if direction == SessionDirection::Bidirectional {
             return Err(crate::domain::transcript::invalid_transcript());
         }
         let max: Option<i64> = self.connection.lock().unwrap().query_row(
-            "SELECT MAX(sequence) FROM translation_segments WHERE session_id=?1 AND direction=?2",
+            "SELECT MAX(sequence) FROM \
+                    translation_segments WHERE session_id=?1 AND \
+                    direction=?2",
             params![session_id.to_string(), direction_name(direction)],
             |r| r.get(0),
         )?;
         max.map(|value| {
             value
                 .checked_add(1)
                 .and_then(|n| u64::try_from(n).ok())
                 .ok_or_else(crate::domain::transcript::invalid_transcript)
         })
         .unwrap_or(Ok(0))
     }
     pub fn append_diagnostic(&self, diagnostic: &RuntimeDiagnostic) -> AppResult<()> {
         diagnostic.validate()?;
-        self.connection.lock().unwrap().execute("INSERT INTO session_diagnostics (id,session_id,occurred_at_ms,payload_json) VALUES (?1,?2,?3,?4)", params![diagnostic.id.to_string(), diagnostic.session_id.to_string(), diagnostic.occurred_at_ms, json(diagnostic)?])?;
+        self.connection.lock().unwrap().execute(
+            "INSERT INTO session_diagnostics (id,\
+                    session_id,occurred_at_ms,payload_json) \
+                    VALUES (?1,?2,?3,?4)",
+            params![
+                diagnostic.id.to_string(),
+                diagnostic.session_id.to_string(),
+                diagnostic.occurred_at_ms,
+                json(diagnostic)?
+            ],
+        )?;
         Ok(())
     }
     pub fn diagnostic_page(
         &self,
         session_id: Uuid,
         after: Option<i64>,
         limit: u32,
     ) -> AppResult<DiagnosticPage> {
         validate_page(after, limit)?;
         let c = self.connection.lock().unwrap();
-        let mut query = c.prepare("SELECT rowid,payload_json FROM session_diagnostics WHERE session_id=?1 AND rowid>?2 ORDER BY rowid LIMIT ?3")?;
+        let mut query = c.prepare(
+            "SELECT rowid,payload_json FROM \
+                    session_diagnostics WHERE session_id=?1 AND \
+                    rowid>?2 ORDER BY rowid LIMIT ?3",
+        )?;
         let mut found = query
             .query_map(
                 params![session_id.to_string(), after.unwrap_or(0), limit + 1],
                 |r| Ok((r.get::<_, i64>(0)?, read_json::<RuntimeDiagnostic>(r, 1)?)),
             )?
             .collect::<Result<Vec<_>, _>>()?;
         let next_cursor = page_cursor(&mut found, limit);
         Ok(DiagnosticPage {
             rows: found.into_iter().map(|(_, row)| row).collect(),
             next_cursor,
         })
     }
     pub fn list_sessions(
         &self,
         limit: u32,
         cursor: Option<String>,
     ) -> AppResult<Vec<SessionSummary>> {
         let p = cursor.map(|v| parse_cursor(&v)).transpose()?;
         let (st, id) = p.map_or((None, None), |(s, i)| (Some(s), Some(i)));
         let c = self.connection.lock().unwrap();
-        let mut q=c.prepare("SELECT id,started_at,mode,direction,source_language,target_language FROM sessions WHERE (?1 IS NULL OR started_at < ?1 OR (started_at = ?1 AND id < ?2)) ORDER BY started_at DESC,id DESC LIMIT ?3")?;
+        let mut q = c.prepare(
+            "SELECT id,started_at,mode,direction,\
+                    source_language,target_language FROM \
+                    sessions WHERE (?1 IS NULL OR started_at < \
+                    ?1 OR (started_at = ?1 AND id < ?2)) ORDER \
+                    BY started_at DESC,id DESC LIMIT ?3",
+        )?;
         let rows = q.query_map(params![st, id, limit], |r| {
             Ok(SessionSummary {
                 id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                 started_at: r.get(1)?,
                 mode: r.get(2)?,
                 direction: r.get(3)?,
                 source_language: r.get(4)?,
                 target_language: r.get(5)?,
             })
         })?;
@@ -375,21 +458,23 @@ impl LocalStore {
     pub fn delete_session(&self, id: Uuid) -> AppResult<()> {
         let c = self.connection.lock().unwrap();
         let tx = c.unchecked_transaction()?;
         tx.execute("DELETE FROM sessions WHERE id=?1", params![id.to_string()])?;
         tx.commit()?;
         Ok(())
     }
     pub fn get_session(&self, id: Uuid) -> AppResult<Option<StoredSession>> {
         let c = self.connection.lock().unwrap();
         Ok(c.query_row(
-            "SELECT id,started_at,mode,direction,source_language,target_language FROM sessions WHERE id=?1",
+            "SELECT id,started_at,mode,direction,\
+                    source_language,target_language FROM \
+                    sessions WHERE id=?1",
             params![id.to_string()],
             |r| {
                 Ok(StoredSession {
                     id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                     started_at: r.get(1)?,
                     mode: r.get(2)?,
                     direction: r.get(3)?,
                     source_language: r.get(4)?,
                     target_language: r.get(5)?,
                 })
@@ -410,37 +495,43 @@ impl LocalStore {
         Ok(c.query_row(
             "SELECT COUNT(*) FROM session_summaries WHERE session_id=?1",
             params![id.to_string()],
             |r| r.get(0),
         )?)
     }
     pub fn create_summary_version(&self, s: &SummaryVersion) -> AppResult<()> {
         let mut c = self.connection.lock().unwrap();
         let tx = c.transaction()?;
         tx.execute(
-            "INSERT INTO session_summaries (id,session_id,version,created_at,status,structured_json) VALUES (?1,?2,?3,?4,?5,?6)",
+            "INSERT INTO session_summaries (id,session_id,\
+                    version,created_at,status,structured_json) \
+                    VALUES (?1,?2,?3,?4,?5,?6)",
             params![
                 s.id.to_string(),
                 s.session_id.to_string(),
                 s.version,
                 s.created_at,
                 s.status,
                 s.structured_json
             ],
         )?;
         tx.commit()?;
         Ok(())
     }
     pub fn list_summary_versions(&self, id: Uuid) -> AppResult<Vec<SummaryVersion>> {
         let c = self.connection.lock().unwrap();
-        let mut q =
-            c.prepare("SELECT id,session_id,version,created_at,status,structured_json FROM session_summaries WHERE session_id=?1 ORDER BY version")?;
+        let mut q = c.prepare(
+            "SELECT id,session_id,version,created_at,\
+                    status,structured_json FROM \
+                    session_summaries WHERE session_id=?1 ORDER \
+                    BY version",
+        )?;
         let rows = q.query_map(params![id.to_string()], |r| {
             Ok(SummaryVersion {
                 id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                 session_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap(),
                 version: r.get(2)?,
                 created_at: r.get(3)?,
                 status: r.get(4)?,
                 structured_json: r.get(5)?,
             })
         })?;
@@ -449,29 +540,36 @@ impl LocalStore {
 
     pub fn get_export_data(&self, id: Uuid) -> AppResult<Option<ExportData>> {
         let Some(session) = self.get_session(id)? else {
             return Ok(None);
         };
         let lifecycle = self.get_session_lifecycle(id)?.ok_or_else(|| {
             crate::error::AppError::Internal("session lifecycle is missing".into())
         })?;
         let c = self.connection.lock().unwrap();
         let mut q = c.prepare(
-            "SELECT id,session_id,direction,sequence,occurred_at,language,text,is_final,details_json,occurred_at_ms
-             FROM translation_segments WHERE session_id=?1 AND is_final=1
-             ORDER BY COALESCE(occurred_at_ms, occurred_at * 1000), sequence, id",
+            "SELECT id,session_id,direction,sequence,\
+                    occurred_at,language,text,is_final,\
+                    details_json,occurred_at_ms
+             \
+                    FROM translation_segments WHERE \
+                    session_id=?1 AND is_final=1
+             \
+                    ORDER BY COALESCE(occurred_at_ms, \
+                    occurred_at * 1000), sequence, id",
         )?;
         let rows = q.query_map(params![id.to_string()], read_segment)?;
         let segments = rows.collect::<Result<Vec<_>, _>>()?;
         drop(q);
         let mut q = c.prepare(
-            "SELECT payload_json FROM session_diagnostics WHERE session_id=?1 ORDER BY rowid",
+            "SELECT payload_json FROM session_diagnostics \
+                    WHERE session_id=?1 ORDER BY rowid",
         )?;
         let diagnostics = q
             .query_map([id.to_string()], |r| read_json(r, 0))?
             .collect::<Result<Vec<RuntimeDiagnostic>, _>>()?;
         let context = c
             .query_row(
                 "SELECT context_json FROM session_runtime_context WHERE session_id=?1",
                 [id.to_string()],
                 |r| read_json(r, 0),
             )
diff --git a/src-tauri/src/storage/sqlite_tests.rs b/src-tauri/src/storage/sqlite_tests.rs
index 1b1bc22..226d100 100644
--- a/src-tauri/src/storage/sqlite_tests.rs
+++ b/src-tauri/src/storage/sqlite_tests.rs
@@ -143,21 +143,21 @@ fn legacy_database_migrates_once_preserving_original_rows() {
         );
         let page = store.transcript_page(s.id, None, 50).unwrap();
         assert_eq!(page.rows[0].id, row.id);
         assert_eq!(page.rows[0].occurred_at_ms, 1_799_000_001_000);
         assert!(page.rows[0].details.is_none());
     }
     assert_eq!(
         connection
             .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
             .unwrap(),
-        4
+        5
     );
 }
 #[test]
 fn failed_migration_rolls_back_columns_version_and_preserves_data() {
     let dir = tempfile::tempdir().unwrap();
     let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
     connection
         .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
         .unwrap();
     connection.execute_batch("CREATE TABLE session_runtime_context (conflict TEXT); INSERT INTO documents (id,sha256,uri,created_at) VALUES ('keep','hash','file',1);").unwrap();
@@ -211,21 +211,21 @@ fn intelligence_migration_from_v2_reopens_twice_and_preserves_rows() {
             "INSERT INTO documents (id,sha256,uri,created_at) VALUES ('keep','hash','file',1)",
             [],
         )
         .unwrap();
     for _ in 0..2 {
         drop(LocalStore::open(dir.path()).unwrap());
         assert_eq!(
             connection
                 .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                 .unwrap(),
-            4
+            5
         );
         assert_eq!(
             connection
                 .query_row("SELECT count(*) FROM intelligence_jobs", [], |r| r
                     .get::<_, i64>(0))
                 .unwrap(),
             0
         );
         assert_eq!(
             connection
diff --git a/src/lib/knowledge-wire.test.ts b/src/lib/knowledge-wire.test.ts
index 1dbccf3..29a6fb8 100644
--- a/src/lib/knowledge-wire.test.ts
+++ b/src/lib/knowledge-wire.test.ts
@@ -1,31 +1,162 @@
-import {expect,it} from 'vitest';
-import fixture from '../../tests/contracts/knowledge-wire.json';
-import * as wire from './knowledge-wire';
-import {WireDecodeError} from './wire';
-it('decodes actual nullable native and legacy document DTOs without display fallbacks',()=>{
-  fixture.kinds.forEach(v=>expect(wire.decodeDocumentKind(v)).toEqual(v));
-  fixture.statuses.forEach(v=>expect(wire.decodeDocumentStatus(v)).toEqual(v));
-  fixture.summaries.forEach(v=>expect(wire.decodeDocumentSummary(v)).toEqual(v));
-  fixture.details.forEach(v=>expect(wire.decodeDocumentDetail(v)).toEqual(v));
+import { expect, it } from "vitest";
+import fixture from "../../tests/contracts/knowledge-wire.json";
+import * as wire from "./knowledge-wire";
+import { WireDecodeError } from "./wire";
+it("decodes actual nullable native and legacy document DTOs without display fallbacks", () => {
+  fixture.kinds.forEach((v) => expect(wire.decodeDocumentKind(v)).toEqual(v));
+  fixture.statuses.forEach((v) =>
+    expect(wire.decodeDocumentStatus(v)).toEqual(v),
+  );
+  fixture.summaries.forEach((v) =>
+    expect(wire.decodeDocumentSummary(v)).toEqual(v),
+  );
+  fixture.details.forEach((v) =>
+    expect(wire.decodeDocumentDetail(v)).toEqual(v),
+  );
   expect(wire.decodeDocumentPage(fixture.page)).toEqual(fixture.page);
   expect(wire.decodeTextLocation(fixture.location)).toEqual(fixture.location);
   expect(wire.decodeTextBlock(fixture.block)).toEqual(fixture.block);
 });
-it('matches exact camelCase command arguments and preserves cursor/null units',()=>{
-  expect(wire.decodeImportDocumentArgs(fixture.commands.import_document)).toEqual(fixture.commands.import_document);
-  expect(wire.decodeReplaceDocumentArgs(fixture.commands.replace_document)).toEqual(fixture.commands.replace_document);
-  expect(wire.decodeListDocumentsArgs(fixture.commands.list_documents)).toEqual(fixture.commands.list_documents);
-  expect(wire.decodeGetDocumentArgs(fixture.commands.get_document)).toEqual(fixture.commands.get_document);
-});
-it('rejects secrets, missing nullable fields, unsafe numbers, wrong tags and unbounded pages',()=>{
-  for (const v of [{...fixture.summaries[0],credential:'secret'},{...fixture.summaries[0],version:0},{...fixture.summaries[0],version:4294967296},{...fixture.summaries[0],created_at_ms:Number.MAX_SAFE_INTEGER+1},{...fixture.summaries[0],updated_at_ms:undefined},{...fixture.summaries[0],kind:'html'},{...fixture.summaries[0],origin:'guessed'}]) expect(()=>wire.decodeDocumentSummary(v)).toThrow(WireDecodeError);
-  expect(()=>wire.decodeDocumentPage({rows:Array(201).fill(fixture.summaries[0]),next_cursor:null})).toThrow(WireDecodeError);
-  expect(()=>wire.decodeReplaceDocumentArgs({document_id:fixture.summaries[0].id,path:'/fixture/source.txt'})).toThrow(WireDecodeError);
-  expect(()=>wire.decodeListDocumentsArgs({after:-1,limit:50})).toThrow(WireDecodeError);
-  expect(()=>wire.decodeListDocumentsArgs({after:null,limit:201})).toThrow(WireDecodeError);
-  expect(()=>wire.decodeGetDocumentArgs({documentId:'invalid'})).toThrow(WireDecodeError);
-});
-it('preserves complete legal long headings rather than truncating to a vector metadata cap',()=>{
-  const v={...fixture.location,heading:'电'.repeat(6000)};
+it("matches exact camelCase command arguments and preserves cursor/null units", () => {
+  expect(
+    wire.decodeImportDocumentArgs(fixture.commands.import_document),
+  ).toEqual(fixture.commands.import_document);
+  expect(
+    wire.decodeReplaceDocumentArgs(fixture.commands.replace_document),
+  ).toEqual(fixture.commands.replace_document);
+  expect(wire.decodeListDocumentsArgs(fixture.commands.list_documents)).toEqual(
+    fixture.commands.list_documents,
+  );
+  expect(wire.decodeGetDocumentArgs(fixture.commands.get_document)).toEqual(
+    fixture.commands.get_document,
+  );
+});
+it("rejects secrets, missing nullable fields, unsafe numbers, wrong tags and unbounded pages", () => {
+  for (const v of [
+    { ...fixture.summaries[0], credential: "secret" },
+    { ...fixture.summaries[0], version: 0 },
+    { ...fixture.summaries[0], version: 4294967296 },
+    { ...fixture.summaries[0], created_at_ms: Number.MAX_SAFE_INTEGER + 1 },
+    { ...fixture.summaries[0], updated_at_ms: undefined },
+    { ...fixture.summaries[0], kind: "html" },
+    { ...fixture.summaries[0], origin: "guessed" },
+  ])
+    expect(() => wire.decodeDocumentSummary(v)).toThrow(WireDecodeError);
+  expect(() =>
+    wire.decodeDocumentPage({
+      rows: Array(201).fill(fixture.summaries[0]),
+      next_cursor: null,
+    }),
+  ).toThrow(WireDecodeError);
+  expect(() =>
+    wire.decodeReplaceDocumentArgs({
+      document_id: fixture.summaries[0].id,
+      path: "/fixture/source.txt",
+    }),
+  ).toThrow(WireDecodeError);
+  expect(() => wire.decodeListDocumentsArgs({ after: -1, limit: 50 })).toThrow(
+    WireDecodeError,
+  );
+  expect(() =>
+    wire.decodeListDocumentsArgs({ after: null, limit: 201 }),
+  ).toThrow(WireDecodeError);
+  expect(() => wire.decodeGetDocumentArgs({ documentId: "invalid" })).toThrow(
+    WireDecodeError,
+  );
+});
+it("preserves complete legal long headings rather than truncating to a vector metadata cap", () => {
+  const v = { ...fixture.location, heading: "电".repeat(6000) };
   expect(wire.decodeTextLocation(v)).toEqual(v);
 });
+it("preserves legal headings exceeding eight MiB and rejects headings beyond sixteen MiB", () => {
+  expect(
+    wire.decodeTextLocation({
+      ...fixture.location,
+      heading: "a".repeat(9 * 1024 * 1024),
+    }).heading?.length,
+  ).toBe(9 * 1024 * 1024);
+  expect(() =>
+    wire.decodeTextLocation({
+      ...fixture.location,
+      heading: "a".repeat(16 * 1024 * 1024 + 1),
+    }),
+  ).toThrow(WireDecodeError);
+});
+it("validates knowledge result identity, finite scores, chunk scalar and aggregate budgets", () => {
+  const hit = {
+    document_id: fixture.summaries[0].id,
+    document_version: 1,
+    chunk_id: fixture.summaries[0].id,
+    title: "pump",
+    location: fixture.location,
+    text: "额定电压",
+    score: 0.5,
+  };
+  const result = {
+    hits: [hit],
+    identity: "a".repeat(64),
+    unavailable_documents: 1,
+  };
+  expect(wire.decodeKnowledgeResults(result)).toEqual(result);
+  expect(
+    wire.decodeKnowledgeResults({
+      hits: [],
+      identity: null,
+      unavailable_documents: 0,
+    }).identity,
+  ).toBeNull();
+  for (const v of [
+    { ...result, identity: "" },
+    { ...result, hits: [{ ...hit, score: NaN }] },
+    { ...result, hits: [{ ...hit, text: "电".repeat(2401) }] },
+    {
+      ...result,
+      hits: Array(4).fill({
+        ...hit,
+        location: { ...fixture.location, heading: "a".repeat(9 * 1024 * 1024) },
+      }),
+    },
+  ])
+    expect(() => wire.decodeKnowledgeResults(v)).toThrow(WireDecodeError);
+});
+it("rejects an empty submitted knowledge query while preserving exact whitespace and Unicode", () => {
+  const request = {
+    query: "",
+    selected_segment_ids: [],
+    session_id: null,
+    limit: 5,
+  };
+  expect(() => wire.decodeKnowledgeSearch(request)).toThrow(WireDecodeError);
+  expect(
+    wire.decodeKnowledgeSearch({ ...request, query: "  电压\n24V  " }).query,
+  ).toBe("  电压\n24V  ");
+});
+
+it("clears exact-selection provenance when the submitted query was edited", () => {
+  const selection = {
+    sessionId: fixture.summaries[0].id,
+    segmentIds: [fixture.summaries[0].id],
+    text: "few selected words",
+  };
+  expect(
+    wire.knowledgeSearchRequest(selection.text, selection).selected_segment_ids,
+  ).toEqual(selection.segmentIds);
+  const edited = wire.knowledgeSearchRequest("edited query", selection);
+  expect(edited.query).toBe("edited query");
+  expect(edited.selected_segment_ids).toEqual([]);
+  expect(edited.session_id).toBe(selection.sessionId);
+});
+
+it("accepts every finite knowledge score allowed by the contract", () => {
+  const hit = {
+    document_id: fixture.summaries[0].id,
+    document_version: 1,
+    chunk_id: fixture.summaries[0].id,
+    title: "pump",
+    location: fixture.location,
+    text: "24V",
+    score: 2,
+  };
+  expect(wire.decodeKnowledgeHit(hit).score).toBe(2);
+  expect(wire.decodeKnowledgeHit({ ...hit, score: -2 }).score).toBe(-2);
+});
diff --git a/src/lib/knowledge-wire.ts b/src/lib/knowledge-wire.ts
index 3690683..87b5055 100644
--- a/src/lib/knowledge-wire.ts
+++ b/src/lib/knowledge-wire.ts
@@ -1,32 +1,157 @@
 /** Actual document serde shapes. Legacy nulls remain null; no fallback labels are persisted. */
-import { decodeUuid, wireSchema } from './wire';
-import { decodeRecordOrigin, decodeIntelligenceErrorCode } from './intelligence-wire';
-const {object,string,integer,nullable,array,enumeration}=wireSchema;
-const u32=integer(0,4294967295);
-const i64=integer(Number.MIN_SAFE_INTEGER);
-const field=string(32768);
-export const decodeDocumentKind=enumeration('pdf','docx','text','markdown','unknown');
-export const decodeDocumentStatus=enumeration('parsing','parsed','indexing','ready','repair_required','failed','deleting');
-export const decodeDocumentSummary=object({
-  id:decodeUuid,title:nullable(field),kind:decodeDocumentKind,version:integer(1,4294967295),
-  origin:decodeRecordOrigin,sha256:field,status:decodeDocumentStatus,chunk_count:u32,
-  created_at_ms:i64,updated_at_ms:nullable(i64),error:nullable(decodeIntelligenceErrorCode),
-});
-export const decodeDocumentDetail=object({
-  summary:decodeDocumentSummary,original_path:nullable(field),managed_path:nullable(field),
-  legacy_uri:nullable(field),index_identity:nullable(field),
-});
-export const decodeDocumentPage=object({rows:array(decodeDocumentSummary,200),next_cursor:nullable(integer(0))});
-export const decodeTextLocation=object({page:nullable(u32),paragraph:u32,heading:nullable(string(8*1024*1024)),start_char:u32,end_char:u32});
-export const decodeTextBlock=object({text:string(8*1024*1024),location:decodeTextLocation});
-export const decodeImportDocumentArgs=object({path:field});
-export const decodeReplaceDocumentArgs=object({documentId:decodeUuid,path:field});
-export const decodeListDocumentsArgs=object({after:nullable(integer(0)),limit:integer(1,200)});
-export const decodeGetDocumentArgs=object({documentId:decodeUuid});
-export type DocumentKind=ReturnType<typeof decodeDocumentKind>;
-export type DocumentStatus=ReturnType<typeof decodeDocumentStatus>;
-export type DocumentSummary=ReturnType<typeof decodeDocumentSummary>;
-export type DocumentDetail=ReturnType<typeof decodeDocumentDetail>;
-export type DocumentPage=ReturnType<typeof decodeDocumentPage>;
-export type TextLocation=ReturnType<typeof decodeTextLocation>;
-export type TextBlock=ReturnType<typeof decodeTextBlock>;
+import { decodeUuid, wireSchema, WireDecodeError, type Decoder } from "./wire";
+import {
+  decodeRecordOrigin,
+  decodeIntelligenceErrorCode,
+} from "./intelligence-wire";
+const { object, string, integer, nullable, array, enumeration } = wireSchema;
+const u32 = integer(0, 4294967295);
+const i64 = integer(Number.MIN_SAFE_INTEGER);
+const field = string(32768);
+export const decodeDocumentKind = enumeration(
+  "pdf",
+  "docx",
+  "text",
+  "markdown",
+  "unknown",
+);
+export const decodeDocumentStatus = enumeration(
+  "parsing",
+  "parsed",
+  "indexing",
+  "ready",
+  "repair_required",
+  "failed",
+  "deleting",
+);
+export const decodeDocumentSummary = object({
+  id: decodeUuid,
+  title: nullable(field),
+  kind: decodeDocumentKind,
+  version: integer(1, 4294967295),
+  origin: decodeRecordOrigin,
+  sha256: field,
+  status: decodeDocumentStatus,
+  chunk_count: u32,
+  created_at_ms: i64,
+  updated_at_ms: nullable(i64),
+  error: nullable(decodeIntelligenceErrorCode),
+});
+export const decodeDocumentDetail = object({
+  summary: decodeDocumentSummary,
+  original_path: nullable(field),
+  managed_path: nullable(field),
+  legacy_uri: nullable(field),
+  index_identity: nullable(field),
+});
+export const decodeDocumentPage = object({
+  rows: array(decodeDocumentSummary, 200),
+  next_cursor: nullable(integer(0)),
+});
+export const decodeTextLocation = object({
+  page: nullable(u32),
+  paragraph: u32,
+  heading: nullable(string(16 * 1024 * 1024)),
+  start_char: u32,
+  end_char: u32,
+});
+export const decodeTextBlock = object({
+  text: string(8 * 1024 * 1024),
+  location: decodeTextLocation,
+});
+export const decodeImportDocumentArgs = object({ path: field });
+export const decodeReplaceDocumentArgs = object({
+  documentId: decodeUuid,
+  path: field,
+});
+export const decodeListDocumentsArgs = object({
+  after: nullable(integer(0)),
+  limit: integer(1, 200),
+});
+export const decodeGetDocumentArgs = object({ documentId: decodeUuid });
+export type DocumentKind = ReturnType<typeof decodeDocumentKind>;
+export type DocumentStatus = ReturnType<typeof decodeDocumentStatus>;
+export type DocumentSummary = ReturnType<typeof decodeDocumentSummary>;
+export type DocumentDetail = ReturnType<typeof decodeDocumentDetail>;
+export type DocumentPage = ReturnType<typeof decodeDocumentPage>;
+export type TextLocation = ReturnType<typeof decodeTextLocation>;
+export type TextBlock = ReturnType<typeof decodeTextBlock>;
+
+const scalarText =
+  (max: number, bytes: number, min = 0): Decoder<string> =>
+  (value) => {
+    const text = string(bytes)(value);
+    let count = 0;
+    for (const _scalar of text) if (++count > max) throw new WireDecodeError();
+    if (count < min) throw new WireDecodeError();
+    return text;
+  };
+const identity: Decoder<string> = (value) => {
+  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
+    throw new WireDecodeError();
+  return value;
+};
+const score: Decoder<number> = (value) => {
+  if (typeof value !== "number" || !Number.isFinite(value))
+    throw new WireDecodeError();
+  return value;
+};
+export const decodeKnowledgeSearch = object({
+  query: scalarText(4096, 16384, 1),
+  selected_segment_ids: array(decodeUuid, 200),
+  session_id: nullable(decodeUuid),
+  limit: integer(1, 20),
+});
+export const decodeKnowledgeHit = object({
+  document_id: decodeUuid,
+  document_version: integer(1, 4294967295),
+  chunk_id: decodeUuid,
+  title: field,
+  location: decodeTextLocation,
+  text: scalarText(2400, 9600),
+  score,
+});
+const results = object({
+  hits: array(decodeKnowledgeHit, 20),
+  identity: nullable(identity),
+  unavailable_documents: u32,
+});
+export const decodeKnowledgeResults: Decoder<ReturnType<typeof results>> = (
+  value,
+) => {
+  const decoded = results(value);
+  // Count escaped UTF-8 fields, plus generous fixed overhead per hit/envelope.
+  // Floating-point encoding differs across Rust/JS, so reserve rather than compare it.
+  const encoder = new TextEncoder();
+  let bytes = 512;
+  for (const hit of decoded.hits) {
+    bytes += 1024;
+    for (const field of [hit.title, hit.text, hit.location.heading]) {
+      if (field !== null) bytes += encoder.encode(JSON.stringify(field)).length;
+    }
+    if (bytes > 32 * 1024 * 1024) throw new WireDecodeError();
+  }
+  return decoded;
+};
+export type KnowledgeSearch = ReturnType<typeof decodeKnowledgeSearch>;
+export type KnowledgeHit = ReturnType<typeof decodeKnowledgeHit>;
+export type KnowledgeResults = ReturnType<typeof decodeKnowledgeResults>;
+
+/** Exact-selection context for the explicit search submission boundary. */
+export type KnowledgeSelection = {
+  sessionId: string;
+  segmentIds: string[];
+  text: string;
+};
+export function knowledgeSearchRequest(
+  query: string,
+  selection: KnowledgeSelection | null,
+  limit = 5,
+): KnowledgeSearch {
+  return decodeKnowledgeSearch({
+    query,
+    selected_segment_ids: selection?.text === query ? selection.segmentIds : [],
+    session_id: selection?.sessionId ?? null,
+    limit,
+  });
+}
