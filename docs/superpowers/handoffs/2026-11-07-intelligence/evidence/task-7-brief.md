# Task brief — workbench-intelligence — Task 7

Prepared, NOT dispatched. Actual BASE, reviewed producer handoffs and ownership are supplied at dispatch. Read global-constraints.md and ledgered task-specific rulings. Do not invent unfinished earlier producer APIs or redo completed tasks.

### Task 7: Explicitly confirmed Tavily research and cited overview

**Files:** Create research/{mod,types,entities,tavily,store,tests}.rs, migrations/0006_research.sql and commands/research.rs. Extend AppState/jobs registration and LLM integration; preserve stored sources independently of overview success.

**Interfaces:**

```rust
pub struct EntityCandidate {
    pub id: Uuid, pub text: String, pub session_id: Uuid,
    pub supporting_segment_ids: Vec<Uuid>, pub kind: String,
}
pub struct ResearchRequest {
    pub query: String, pub session_id: Option<Uuid>,
    pub supporting_segment_ids: Vec<Uuid>, pub confirmed: bool,
}
pub struct ResearchSource {
    pub id: Uuid, pub run_id: Uuid, pub title: Option<String>, pub url: String,
    pub excerpt: Option<String>, pub published_at: Option<String>,
    pub fetched_at_ms: Option<i64>, pub origin: RecordOrigin,
}
pub struct ResearchClaim { pub text: String, pub source_ids: Vec<Uuid> }
pub struct ResearchOverview { pub facts: Vec<ResearchClaim>, pub unknowns: Vec<String> }
pub struct ResearchListItem {
    pub id: Uuid, pub session_id: Option<Uuid>, pub query: String,
    pub status: JobStatus, pub created_at_ms: i64, pub source_count: u32,
}
pub struct ResearchPage { pub rows: Vec<ResearchListItem>, pub next_cursor: Option<i64> }
pub struct ResearchOverviewVersion {
    pub id: Uuid, pub run_id: Uuid, pub version: u32, pub status: JobStatus,
    pub profile_id: Option<Uuid>, pub model: Option<String>, pub created_at_ms: i64,
    pub content: Option<ResearchOverview>, pub error: Option<IntelligenceErrorCode>,
}
pub struct ResearchRunView {
    pub id: Uuid, pub session_id: Option<Uuid>, pub query: String,
    pub status: JobStatus, pub created_at_ms: i64,
    pub sources: Vec<ResearchSource>, pub overview: Option<ResearchOverview>,
    pub error: Option<IntelligenceErrorCode>,
}
impl ResearchService {
    pub fn suggest(&self, session_id: Uuid) -> AppResult<Vec<EntityCandidate>>;
    pub async fn search(&self, request: ResearchRequest) -> AppResult<JobView>;
    pub fn get(&self, run_id: Uuid) -> AppResult<Option<ResearchRunView>>;
    pub async fn overview(&self, run_id: Uuid, profile_id: Option<Uuid>) -> AppResult<JobView>;
}
```

ResearchService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, http:Arc<dyn JsonHttp>, llm:Arc<dyn LlmProvider>) -> Self; register Research and ResearchOverview handlers before scheduler start.

Commands suggest_research_entities(session_id), start_research(request), get_research(run_id), generate_research_overview(run_id,profile_id), list_research(session_id,after,limit) and delete_research(run_id). list_research returns ResearchPage with 1–100 rows. list_research_overviews(run_id,after_version,limit) returns Vec<ResearchOverviewVersion> with 1–50 rows; delete_research_overview(overview_id) returns (). List returns rows without nested sources plus rowid cursor; get returns at most eight sources and the current overview. Overview versions use the same durable version pattern as summaries, with separate research overview table and list/delete commands; do not overwrite a successful version with a failed retry.

- [ ] **1. Write RED tests proving confirmation and attribution.**

```rust
#[tokio::test]
async fn unconfirmed_candidate_never_queues_a_tavily_request() {
    let h = ResearchHarness::new().unwrap();
    let result = h.service.search(ResearchRequest {
        query: "Northstar Fluid Systems".into(), session_id: None,
        supporting_segment_ids: vec![], confirmed: false,
    }).await;
    assert!(result.is_err());
    assert_eq!(h.http.request_count(), 0);
    assert_eq!(h.store.unfinished_job_count().unwrap(), 0);
}
```

ResearchHarness uses IntelligenceHarness/RecordingHttp and real source storage. Add suggestions only local, unsupported URL schemes, foreign-session segment IDs, empty/no-result response, 429/timeout, oversized/malformed body, duplicate source URLs, invalid citation ID, overview failure leaving sources intact, prompt-injection text rendered as text and no tool calls, cancel/retry and source deletion cascades.

- [ ] **2. Add source/provenance migration.** Version 6 adds query, profile_id, error_code, response_hash and confirmed_at_ms to research_runs; nullable excerpt, nullable published_at, nullable fetched_at_ms, origin (legacy `legacy`) and ordinal to research_sources. Retain the foundation source title column as nullable. Add `research_support(run_id,segment_id)` and versioned `research_overviews` with parsed_json, raw_text/hash, profile/model/status/error, created time and unique(run_id,version). Preserve legacy sources with title=None/excerpt=None/fetched_at_ms=None and origin=Legacy when those fields were absent; never invent an empty excerpt, page title or fetch time. Native Tavily writes Some validated title/excerpt/fetched time and origin=Native; optional published_at remains None if not supplied. Source metadata is saved before optional overview begins. Add a foundation fixture with title=NULL and no provenance columns, migrate/reopen twice, and assert nullable values and original URL survive unchanged. Wire fixture/UI tests must cover this legacy source beside a complete native source.

- [ ] **3. Implement local candidates and confirmation.** Read only final subtitles, cap the most recent 200 rows/128KiB. Extract HTTPS/HTTP domain references and bounded company-like spans, including Chinese suffixes 公司/集团/科技/有限公司 and English company suffixes. Return <=12 deduplicated candidates with supporting IDs. Candidate generation uses deterministic rules, not an automatic LLM/network call; confidence is not a fabricated percentage. Empty result keeps manual company/URL input available.

`start_research` requires confirmed=true, query trimmed 2–256 characters, <=8 supporting IDs belonging to the supplied session. Manual query with no session is supported. Confirmation is the user's explicit button action; selecting a subtitle or typing cannot queue the job. Normalize URL queries for display and allow http/https only; discard URL credentials/fragments and reject embedded credentials rather than leaking them. Never crawl the entered website directly; Tavily owns search.

- [ ] **4. Implement Tavily and citations.** POST selected endpoint with `Authorization: Bearer <backend key>` and body `{query, search_depth:"basic", max_results:8, include_answer:false, include_raw_content:false}`. This release uses basic depth with eight-result cap; no unsupported depth setting is exposed. Use Task 3 HTTP limits/cancellation; no secret in JSON/logs. Accept documented `results` entries with url/title/content and optional published_date. Canonicalize valid http/https URLs for deduplication, cap URL 2048 bytes/title 300 chars/excerpt 8000 chars, skip invalid entries with bounded local counts. Store request hash and response hash, actual fetched time and original source URL. No results is successful empty data, not a network failure.

Optional overview sends only stored source IDs/title/url/excerpt to the selected LLM; exclude legacy rows with absent/empty excerpts, never send UI fallback text as evidence. If no source has usable stored evidence, return a local InvalidResponse with re-search guidance before any LLM request. Strict schema `{facts:[{text,source_ids}],unknowns:[...]}` with <=30 facts/unknowns, text <=2000 chars. Every fact requires at least one source ID from this run; reject invented IDs and missing citations, and show links from stored URLs. Unknowns may contain explicit unanswered questions but cannot be promoted to facts. Persist a failed raw overview response with hash and retain all source rows. Opening a result uses the system browser only on user click and revalidates http/https; never render remote HTML or auto-load remote previews.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib research
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
```

Commit `feat: research confirmed customer entities with stored citations`.

Read task-5-job-credential-ownership-handoff.md and reviewedTask5 finalowner/resolverhandoff before model credentials. Reuse the actual counted JobContext native-operation owner and same-config-gate immutable-profile resolver implemented/reviewed5; do not assume an unscoped observer timeout ends native work or invent a preexisting resolve_snapshot_credentials API. Long-lived handlers keep unscoped config; each entered credential operation retains the issued owner token through actual completion.
