# Task7 current owned producer context — preparation only

The approved Task7 requirements are `docs/superpowers/plans/2026-11-04-workbench-intelligence.md` lines812–898 and its exact archived `docs/superpowers/handoffs/2026-11-07-intelligence/evidence/task-7-source-plan.md`. Product requirements, public DTO fields, commands and all exact limits remain those already approved. This context reconciles the existing producer ports; it does not activate a new source author before Task6 independent acceptance.

Current source tree: `d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`; accepted manual LLM/store/config/job producers are already in main at `85eda3a8bca3ddabd72edad0763b4d5776166f9a`. The two-task summary integration review remains pending. No second brainstorming or product approval is needed for this already-authorized subsystem.

## Actual configuration and transport

`ServiceConfigStore::scoped(owner: OperationLease) -> Self` attaches the actual retained operation owner to ConfigIo. Each entered config worker retains its owner after dropped/timed-out observers. Use scoped configuration for every admitted async research effect; no production `Arc<()>` or synthetic process owner.

`with_profile_snapshot(ids: Vec<Uuid>, operation: FnOnce(&Transaction,Vec<ModelProfile>)->AppResult<T>)` performs one config gate then SQLite transaction; ids≤3. `with_summary_selection_completion(SummarySelection::Manual { profile_id }, operation, completed)` atomically chooses the explicit/default LLM profile plus settings. Its `SummaryConfigSnapshot` carries `profile`, `selection_error` and `automatic_summary`. Explicit research overview is independent of the automatic-summary flag.

There is no current selected-Tavily completion port. Add `with_tavily_selection_completion` using the existing selected-embedding completion pattern: select `settings.tavily_profile_id` and its correct-kind ModelProfile in the same owned ConfigIo gate and SQLite transaction, then invoke completion. Do not separately read settings and later admit a stale selected profile. The existing Tavily default endpoint is the COMPLETE URL `https://api.tavily.com/search` (`config.rs:save`); use the selected endpoint directly.

`resolve_snapshot_credentials(expected: ModelProfile)` verifies the complete current profile equality and current encrypted reference before returning opaque SecretString. It accepts no supplied reference and exposes no plaintext getter. Release config/SQLite locks before HTTP.

The actual JSON boundary is:

```rust
pub struct JsonRequest {
    pub endpoint: url::Url,
    pub bearer: SecretString,
    pub body: serde_json::Value,
    pub timeout: std::time::Duration,
    pub max_response_bytes: usize,
    pub retry_policy: HttpRetryPolicy,
    pub error_policy: JsonErrorPolicy,
}
pub struct JsonResponse { pub status: u16, pub body: Vec<u8> }
// JsonHttp::post(request, watch::Receiver<bool>) -> Result<JsonResponse, IntelligenceErrorCode>
```

Tavily uses `HttpRetryPolicy::Never`, `JsonErrorPolicy::Generic`, the configured timeout and≤8MiB envelope. Its body is exactly the approved basic/eight-results/no-answer/no-raw-content shape. SecretString is moved into bearer; never copy a key into JSON/logs/DTOs. Parse raw bytes under explicit depth/node/string/collection bounds; malformed bodies remain distinct from valid empty results.

The actual LLM port is `complete(&ModelProfile,LlmRequest,LlmExecution)->Result<LlmResponse,IntelligenceErrorCode>`. `LlmExecution::new` requires the genuine OperationLease and cancellation receiver. LlmRequest is `{system,user,max_output_tokens}`; LlmResponse is `{content,response_hash}`. The accepted compatible transport applies actual escaped UTF-8 system/user bytes+1024≤context-output,1MiB request/8MiB envelope/2MiB content, no automatic HTTP retry. An overview whose complete supplied evidence will not fit fails locally; do not truncate to a successful evidence prefix.

## Actual job, readiness and deletion ownership

`JobHandler::run(JobContext,payload)` returns the domain IntelligenceErrorCode. `JobContext::{operation_owner,publication_fence,commit}` are current producers; `JobAdmissionFence::run` executes inside SQLite. `terminal_tx(&Transaction,id,status,code,awaiting_reap)` updates only matching domain metadata in the supplied transition transaction, with no filesystem/config/network work. Use the acyclic facade/core/handler structure; job handlers must not strongly own their scheduler or App facade.

App generate/delete summary commands now consume the actual current cached `SummaryDeliveryReady`, not only local SummaryReady. Its publisher completes before runtime create/resume admission. Research prompt mutations must respect the current admitted App epoch and actual completed startup, and recheck authority at the actual config/SQL effects. Use backend-owned service variants carrying real owners/capabilities; the old unowned illustrative async service signatures do not authorize a detached production operation. Public Tauri request/result shapes stay exactly the approved ones.

Native research and overview rows need independent job association and fresh authorization at claim, model/request entry, raw/parsed publication and terminal status copy. Reuse current strict admission/domain witness and actual `JobReapPort` patterns, with migration7 owning research fields. Witnesses/job payloads/raw overview text remain backend-only. Manual overview versions retain a durable monotonic u32 counter after version deletion; failed retries preserve prior successful output and stored source rows.

Existing whole-session deletion in `app/lifecycle.rs:delete_session` retains an actual owner, marks/joins summary scope, closes subscribers, then calls `orchestrator.delete_session_guarded`. Task7 must compose the corresponding research/overview actual cancellation/reap guard before that final DELETE; preserve the elected epoch and retained owner on observer loss/timeout. Research/run/version delete and local restart cleanup must never repopulate deleted rows or replay paid calls. Default fixtures use actual store/config/job collaborators and controlled external HTTP/LLM, not real credentials/keyring/audio/driver operations.

## Product and downstream contract

Suggestion is deterministic/local only: final subtitles, most recent200 rows/128KiB,≤12 deduplicated company/domain candidates; no model or network call. Search requires explicit confirmed=true, trimmed2–256-character query,≤8 real same-session support IDs, optional no-session manual query, http/https only, rejected embedded credentials, no direct website crawl.

Tavily returns≤8 validated/deduplicated sources; original title/excerpt/URL/published date and actual fetched time are saved before optional overview. Preserve foundation Legacy null title/excerpt/fetched time and original URL through migration/reopen; UI fallback text is never LLM evidence. No-result results[] is successful empty data.

Overview contains exactly facts with source IDs plus unknowns,≤30 each,≤2000-character text; every fact cites actual usable stored sources from that run. Reject invented/missing IDs, unknown/schema/type/limit violations. Retain failed raw overview/hash locally and all prior sources/versions. Opening validated http/https sources is an explicit system-browser click; no remote HTML/preview/automatic crawl.

Task8/9 consume the existing independent intelligence JobChanged channel and current desktop bridge. Preserve approved framed-sheet/shadcn design and safe key forms. Confirmed Tavily and overview are explicit actions; selection, typing and candidate extraction never enqueue remote work.

No Rust/source/Cargo/network request is authorized or performed by this preparation file. Activate Task7 only after the current source has independent Task6 integration acceptance and its exact accepted BASE is recorded.
