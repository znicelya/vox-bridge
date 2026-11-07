### Task 6: Automatic/manual summaries with complete input coverage

**Files:** Create llm/{mod,openai_compatible,tests}.rs, summary/{mod,types,store,planner,tests}.rs, migrations/0006_summaries.sql and commands/summary.rs. Modify session/orchestrator final-commit notification, AppState/jobs registration and export mapping. Do not change audio stop ordering.

**Interfaces and strict schema:**

```rust
pub struct LlmRequest {
    pub system: String, pub user: String, pub max_output_tokens: u32,
}
pub struct LlmResponse { pub content: String, pub response_hash: String }
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, profile: &ModelProfile, request: LlmRequest,
        cancel: watch::Receiver<bool>) -> Result<LlmResponse, IntelligenceErrorCode>;
}
#[serde(deny_unknown_fields)]
pub struct SummaryContent {
    pub summary: String, pub participants: Vec<String>, pub companies: Vec<String>,
    pub requirements: Vec<String>, pub action_items: Vec<ActionItem>,
    pub risks: Vec<String>, pub terms: Vec<Term>,
}
#[serde(deny_unknown_fields)]
pub struct ActionItem { pub text: String, pub owner: Option<String>, pub due: Option<String> }
#[serde(deny_unknown_fields)]
pub struct Term { pub term: String, pub meaning: String }
pub struct SummaryView {
    pub id: Uuid, pub session_id: Uuid, pub version: u32, pub status: JobStatus,
    pub profile_id: Option<Uuid>, pub model: Option<String>, pub created_at_ms: i64,
    pub content: Option<SummaryContent>, pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>, pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>, pub origin: RecordOrigin,
}
impl SummaryService {
    pub async fn generate(&self, session_id: Uuid, profile_id: Option<Uuid>) -> AppResult<JobView>;
    pub async fn after_terminal(&self, session_id: Uuid) -> AppResult<()>;
    pub fn list(&self, session_id: Uuid, after_version: Option<u32>, limit: u32)
        -> AppResult<Vec<SummaryView>>;
    pub fn delete(&self, summary_id: Uuid) -> AppResult<()>;
}
```

SummaryService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, llm:Arc<dyn LlmProvider>) -> Self; its handler is registered under JobKind::Summary before scheduler start.

Add this crate-private Task 6 store boundary so version allocation cannot race permanent duplicate detection:

```rust
pub(crate) struct SummaryJobRequest {
    pub session_id: Uuid, pub profile: Option<ModelProfile>,
    pub automatic: bool, pub initial_failure: Option<IntelligenceErrorCode>,
}
impl LocalStore {
    pub(crate) fn create_summary_job(&self, request: SummaryJobRequest) -> AppResult<JobView>;
}
```

create_summary_job opens one write transaction, chooses a new summary UUID for the prospective object_id/payload, and calls Task 3's admit_job with the appropriate admission. Existing returns the prior job with no version allocation. Created advances the session's durable summary-version counter and inserts its corresponding Queued/Failed version inside that SAME transaction; any insert failure rolls back job, permanent marker and counter/version together. Deleting a summary cannot rewind that counter or reuse a version number. Only after commit does SummaryService call jobs.wake. Manual requests use Manual { exclusion_key: format!("summary:{}", session_id) } and no initial_failure. Automatic requests use AutomaticSummary; failed automatic admission/preflight may be represented by the terminal failed marker, but a permanent existing job always wins. This shares the tested admission helper rather than duplicating scheduler checks.

Commands generate_summary(session_id,profile_id), list_summaries(session_id,after_version,limit), delete_summary(summary_id). SummaryContent derives Deserialize/Serialize with strict types, max 20000 chars summary, <=100 entries per array, <=2000 chars per string; reject unknown fields/type mismatches, not silently dropping them. Empty arrays are valid. Model returns JSON object directly; optional fenced JSON is not silently repaired—invalid output is a failed version with retry.

- [ ] **1. Write RED tests for full coverage, stop independence and invalid JSON durability.** `SummaryHarness` owns real LocalStore/JobScheduler, RecordingLlm and a fixture runtime/orchestrator; methods completed_session(finals), run_jobs(), responses(), summary_versions(id) are implemented against those collaborators. RecordingLlm queues exact controlled response strings and records request text only in tests.

```rust
#[tokio::test]
async fn closed_session_channel_does_not_prevent_one_automatic_summary() {
    let h = SummaryHarness::new().unwrap();
    let id = h.completed_session(vec!["customer asks for 24 V pump"]).await;
    h.bus.close(id);
    h.service.after_terminal(id).await.unwrap();
    h.service.after_terminal(id).await.unwrap();
    h.run_jobs().await;
    assert_eq!(h.llm.request_count(), 1);
    assert_eq!(h.summary_versions(id).unwrap().len(), 1);
    assert_eq!(h.summary_versions(id).unwrap()[0].status, JobStatus::Succeeded);
}
```

Add tests for successful manual rerun after a Succeeded version and manual retry after each Failed/Cancelled/Interrupted version, with strictly increasing versions and distinct job UUIDs; two concurrent manual summary requests must create only one new version. Replay after_terminal after an automatic job finishes and after reopening storage; it must keep the original job/version. A failed automatic job followed by manual retry must retain one automatic permanent marker and add exactly one manual version. Deleting the successful automatic summary version must not allow after_terminal to launch another automatic request. Inject a summary-version insert failure and assert neither a job nor a permanent marker remains after rollback.

Delete the highest version, reopen storage, then manually generate again: the new version must exceed the deleted version. Also delete every version and verify the next version still advances; a rolled-back admission consumes no version. Cover checked u32 exhaustion with a safe local failure before any job/version insertion.

Add delayed LLM proving stop already returned, failed JSON raw/hash persisted, selected-model switch creates a new version, cancellation, deletion during an active summary, storage failure, no configured LLM, empty transcript, and a transcript larger than one request whose middle sentinel appears in the recorded map requests. Assert coverage set equals the complete final snapshot IDs; never test only the first/last lines.

- [ ] **2. Extend version storage safely.** Version 6 adds profile_id, model, input_hash, input_final_count, snapshot_last_rowid, error_code and raw_response_hash to existing session_summaries. Add `summary_responses(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, ordinal INTEGER, phase TEXT, raw_text TEXT NOT NULL, response_hash TEXT NOT NULL, PRIMARY KEY(summary_id,ordinal))` and `summary_coverage(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, segment_id TEXT NOT NULL, part_index INTEGER NOT NULL, part_count INTEGER NOT NULL, PRIMARY KEY(summary_id,segment_id,part_index), CHECK(part_count > 0), CHECK(part_index >= 0 AND part_index < part_count))`. Add `summary_version_counters(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, last_version INTEGER NOT NULL CHECK(last_version >= 0))`, seeded from each existing session's MAX(version); new sessions allocate from zero on first admission. Counter increment uses checked u32 range inside the admission transaction. Summary deletion leaves the counter; session deletion cascades it. Keep old structured_json/status rows readable as legacy versions; missing metadata remains None.

`summary_coverage` part indexes are zero based; an unsplit final uses `(segment_id,0,1)`. Validate each segment's consistent part count against the immutable manifest; success requires the exact complete set of expected part tuples, while `input_final_count` remains the number of distinct final segment IDs. Persist supplied map-input coverage with its retained raw response before parsing, so a failed attempt cannot claim uncalled planned parts. Add a middle-part omission regression that fails even when the distinct segment-ID set matches.

Use create_summary_job to allocate the next durable counter value together with job admission, create a Queued version before cloud work, and update its status/JSON on success/failure. Raw response is written before parsing. Store each map/reduce response <=2MiB and <=32MiB total per summary; exceeding the cap fails visibly while preserving prior responses. Return hashes/status/parsed data to UI; do not put raw model content into generic diagnostics/logs. Summary deletion cascades responses/coverage and rejects or cancels an in-flight version with bounded completion before removal. Export includes structured summaries and safe model/version metadata; raw responses remain local detail data, outside default export.

- [ ] **3. Implement complete final snapshot and bounded hierarchy.** Capture a SQLite read snapshot of final rows with session-filtered rowid upper bound, each row retaining ID, direction, role/language and text. Max 16MiB normalized input and 100000 rows; exceeding limit creates ContextTooLarge, never truncates. No interim or audio. Order by storage insertion order; use direction-specific labels, not inferred speaker names. Check borrowed SQLite text/metadata lengths before eagerly allocating legacy rows and charge the bounded normalized encoding, including escaping and retained identity metadata. Release the read transaction before any network await. Add a migration-6 index on `translation_segments(session_id)` and verify the ordered session/high-water query plan, rather than treating a page limit as a bound on an unrelated table scan or sort. Native kind comes from stored details; legacy missing kind remains explicitly unknown in the new snapshot and is never inferred from language.

Budget each request with a conservative UTF-8-byte estimate plus 1024 prompt/schema overhead against `max_context_tokens - max_output_tokens`; document this as an estimate for arbitrary compatible tokenizers, not exact token accounting. Provider context-limit errors fail safely. Split oversized individual rows at Unicode boundaries with `(segment_id,part_index,part_count)`; every part appears in coverage. Map chunks use the same strict schema. Hierarchically reduce bounded intermediate summaries until one fits, max 64 requests and 8 levels; if no reduction progress/context limit, fail ContextTooLarge. Never omit a chunk to meet a limit. Preserve request coverage independently of what the model chooses to mention; do not promise semantic completeness beyond the supplied input.

OpenAiCompatibleLlm::new(config:Arc<ServiceConfigStore>, http:Arc<dyn JsonHttp>) -> Self uses the same crate-private resolver and opaque JsonRequest bearer; its complete method cannot read a plaintext key. ResearchService and explicit probes use that resolver for their own saved profile ID as well.

Chat POST `/chat/completions`: model, system/user messages, temperature 0, max_tokens, non-streaming, no proprietary tools/schema capability requirement; the system instruction requests the strict JSON and the local parser verifies it. System instruction treats transcript text as quoted data and requests no invented participants/actions/dates. LLM output never triggers tools or network. Parse `choices[0].message.content` as a bounded string; missing choices/refusal/wrong types fail InvalidResponse. Secrets resolve backend-only at execution.

- [ ] **4. Wire exactly-once automatic scheduling after durable termination.** Add an injected terminal observer or callback on the lifecycle owner's successful Completed/Failed commit; it receives only session ID and cannot delay audio cleanup/network stop. Schedule with JobAdmission::AutomaticSummary and its permanent key `auto-summary:<session UUID>` using create_summary_job. Repeated observer calls return the same job/version even after that job is terminal; a duplicate does not reserve another version. Existing session bus closes normally; intelligence events/polling carry progress. At startup, local reconciliation uses the same eligible Completed-or-Failed terminal predicate, including sessions marked Failed by interrupted runtime recovery with retained final text. With automatic-summary enabled and no permanent automatic marker/job/version, it creates a visible local Interrupted/retry record rather than silently reissuing a paid call after uncertain crash history. Add completed and failed terminal-commit crash-window tests plus a restart-interrupted session with retained finals; all must create one local retry record and make zero LLM calls, including after a second reopen. New normal terminal sessions start automatically when final text exists, including a failed session with retained text; an empty terminal session records a local NoFinalText result without calling a model. Missing model/queue-full records a failed summary version with safe code and retry control. Manual regeneration or retry uses the same in-flight exclusion key `summary:<session UUID>` with permanent_key=NULL, creates a NEW job UUID/version after the prior job is terminal, and never overwrites prior output. A failed/interrupted automatic job retains its permanent marker; clicking Retry creates a manual job, not a second automatic job. Removing a summary version does not remove the automatic marker. Missing model, queue-full or NoFinalText creates the failed automatic marker and failed version atomically so repeated terminal callbacks cannot keep creating failures. Stop result is independent of LLM success.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib summary
cargo test --manifest-path src-tauri/Cargo.toml --lib llm
cargo test --manifest-path src-tauri/Cargo.toml --lib session
cargo test --manifest-path src-tauri/Cargo.toml --lib export
```

Commit `feat: summarize complete session transcripts with durable versions`.

