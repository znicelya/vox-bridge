# Task6 implementation architecture checkpoint — 2026-11-09

Status: **read-only checkpoint prepared; controller rulings and independent
Task5C acceptance are still required before source or Cargo activation**.

The controller subsequently reported a load-bearing Task5C queued-delete /
old-generation retirement issue. Task6 activation therefore also requires
that repair and independent re-review; the checkpoint's producer citations
remain the observed committed source, not a claim that the pending repair
exists. Root must compare these proposed ports to the repaired accepted BASE
before dispatch. No reviewer internals or repair implementation was read here.

Worktree: `/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.
Observed HEAD: `b72556091fc533dafb0a336907bf077fec377615`.
Producer source commit: `d802613`; the source-only diff from that commit to
observed HEAD is empty. This document proposes interfaces and migration6
changes; none of them exists merely because it is described here.

`task-6-source-plan.md` was read first. The other inputs were
`global-constraints.md`, `task-6-active-brief-draft.md`,
`task-6-current-ownership-note.md`, `task-6-snapshot-handoff.md`,
`task-6-producer-preflight-20261109.md`, and
`task-6-environment-20261109.json`. The controller's
`task-6-query-plan-preflight.json` supplies limited architectural evidence.
The whole canonical plan and Task5C reviewer internals were not read.

This reconciles an approved feature. It does not request product reapproval,
authorize implementation, or declare Task5C reviewed. Root owns rulings,
source/Cargo dispatch, review and commit. The inherited working model route
was used without provider/global configuration changes.

## Controller decisions required

| ID | Concrete recommended ruling | Reason and handoff cost |
| --- | --- | --- |
| A6-1 | Add the lifecycle journal fields and indexes below in migration6. Reserve a channel slot with `try_reserve_owned` before the existing terminal lifecycle write, persist its disposition in that same write, and notify with only the committed UUID after stop completion publication. Use one joined delivery worker, channel32, admission-child1, bounded pages32, and a coalesced rescan signal. Overflow is local `Busy`; close, unavailable delivery and uncertain/stale generation are local `Interrupted`, with no paid dispatch. | A volatile callback/channel cannot simultaneously guarantee bounded capacity, no stop backpressure and durable recovery. Fixed fields on an existing lifecycle row preserve overflow/uncertainty without another callback task or an unbounded in-memory queue. The worker and entered children must join before rearm. |
| A6-2 | Use an opaque owned `LlmExecution`; use owned service methods for production async calls; add the selected-or-explicit LLM completion config gate and an in-transaction summary writer. Preserve `admit_job` unchanged, reuse its factored permanent lookup first in the same transaction, read/check next version without writing, and write counter+summary only on Created. | The draft cancellation-only LLM port and `create_summary_job(&self, ...)` wrapper cannot by themselves compose actual ownership and config→one SQLite transaction→control. The in-transaction companion avoids nested locking; permanent lookup then a read-only counter check reconciles Existing precedence and pre-insert exhaustion. The originally proposed generic callback is withdrawn; see the reconciliation appendix. |
| A6-3 | Replace the unshipped summary DTO with a tagged native/legacy union and an opaque, canonical signed-decimal version cursor, max20 bytes. Keep native u32/JobStatus/actual milliseconds and strict content. Carry legacy signed version, status and seconds as exact decimal/string values, readable sanitized legacy JSON, and absent native metadata as None. Export retains its existing legacy numeric version/seconds/status/structured_json shape. | `complete`, `{title,text}` / `{text}`, negative/large signed versions and seconds cannot truthfully inhabit the proposed mandatory native fields. Task8/9 must consume the union/page and must not normalize `complete` into Succeeded or manufacture milliseconds. |
| A6-4 | Make summary deletion async: persist a deletion fence, cancel the matching job, and wait up to10s for actual attempt ownership and a generation-bound per-job reap predicate before removal. Timeout/error leaves the row and fence for retry. Add a bounded actual-attempt registry16, a narrow nonmutating scheduler reap port, and extend existing session deletion with a session summary fence, bounded cancellation and actual completion before cascade. | A synchronous delete cannot await cancellation. Job terminal status alone is insufficient after an entered ConfigIo timeout or scheduler interruption. Existing session cascade currently can remove summaries/jobs while a summary response is still outliving its observer. |
| A6-5 | A startup/close/uncertain initial failure is `JobStatus::Failed` with `error=Interrupted`, matching the reviewed admission helper. Existing in-flight jobs recovered by scheduler reconciliation remain actual `JobStatus::Interrupted`. Permanent Existing wins before allocation, even if configuration/generation has since become unusable. Clamp negative legacy MAX(version) to0 when seeding; preserve a positive legacy MAX above u32 as an exhausted counter. | The source helper maps every allowed `initial_failure` to Failed. Changing only the summary row to Interrupted would lie about job status. Directly seeding an all-negative MAX contradicts the counter CHECK; casting a large MAX would reuse versions. Exhaustion must leave zero new job/version/marker rows. |
| A6-6 | Adopt the explicit copying/list/export/transport bounds below. Keep the original explicit probe resolver semantics, including its saved profile ID and revision check. For known compatible HTTP context-error codes, allow a small bounded safe-code classifier in `intelligence/http.rs`; unknown provider errors still fail safely. | A page count is not a byte/scan bound. Summary exports must include every version/field or fail as a whole. The existing HTTP producer discards non-2xx bodies as Network, and the explicit probe already resolves its own saved ID atomically; replacing it with a selected-summary resolver would regress reviewed behavior. |

These are proposed internal rulings, not decisions already accepted. In
particular, A6-1, A6-2, A6-3 and A6-4 change unshipped implementation contracts
or add migration authority and must be ledgered before source dispatch.

## Committed producer facts that bind the proposal

| Committed source | Fact used here |
| --- | --- |
| `intelligence/jobs.rs:24–89,152–173` | `JobContext::operation_owner()` issues an actual-completion child only while Running/current. `commit` and `JobPublicationFence::commit` check cancellation and the current Running row under SQLite→generation control. `OperationLease = Arc<dyn Send + Sync>`. |
| `intelligence/jobs.rs:215–235,241–332,378–393` | Capability registration precedes freeze/start. There are two joined workers, a bounded wake queue, a5s shutdown observer and generation identity checks. `admission_fence(Summary)` requires a registered, started, open generation. `event_port` and `wake_port` avoid capturing the scheduler. |
| `intelligence/jobs/child_operations.rs` | Actual child capacity16; factories retain a weak process reference. Shutdown obtains all permits only after workers/cleanup, so observer timeout does not imply actual completion. |
| `intelligence/config.rs:30,288–319,363–384,391–420` | Ephemeral `scoped(owner)` config propagates ownership. Selected embedding completion reads selection/profile and runs supplied SQLite work in one ConfigIo gate, then executes its post-commit callback even after observer loss. Snapshot credential resolution compares the entire `ModelProfile`, checks its saved credential association, then reads only the opaque secret. |
| `config/worker.rs:98–159` | A ConfigIo operation has a10s observer deadline; an entered blocking closure can outlive it. That closure retains its operation owner. Queued cancellation and entered completion differ. |
| `knowledge/embedding.rs:17–46,92–129` | The current Task5C consumer shape is an execution value containing a pre-issued owner and watch receiver; independent acceptance is pending. The long-lived provider owns config/HTTP and a semaphore, not a current request owner. |
| `intelligence/store.rs:35–139` | `admit_job` neither commits nor inserts result versions. Automatic permanent identity is checked before payload/capacity work. Automatic uses `auto-summary:<UUID>` and exclusion `summary:<UUID>`. Manual has no initial failure. Allowed automatic initial failures are Configuration, Busy, NoFinalText and Interrupted; their initial status is Failed. |
| `session/orchestrator.rs:321–420` | One authoritative observer cleans runtime, writes Completed/Failed, closes the session bus, publishes stop completion and releases the active operation. Successful lifecycle persistence, rather than successful diagnostic persistence or a successful stop reply, is the terminal authority. |
| `app/mod.rs:213–292; app/intelligence.rs:8–75` | Construction recovers interrupted sessions, composes handlers, calls scheduler reconciliation, then constructs runtime/orchestrator. Startup holds a gate through index/knowledge readiness and scheduler start. Direct operation capacity16 already retains process ownership. |
| `app/lifecycle.rs:95–115,142–173,181–255` | Session deletion presently lacks summary cancellation. Close immediately signals jobs/probes, runs audio stop concurrently, joins job ownership before index closure, then drains all16 direct permits. Close/rearm must remain fenced on timeout. |
| `storage/sqlite.rs:80–91,175–245,263–317` | One mutex-owned SQLite connection; the transaction helper is Immediate and forbids ConfigIo entry inside it. Final transcript writes are INSERT-only. Current lifecycle update is one durable UPDATE. Recovery marks incomplete lifecycle rows Failed and retains finals. |
| `storage/models.rs:51–68; storage/sqlite.rs:501–586` | Foundation summaries have signed version/seconds, arbitrary TEXT status and structured JSON. Existing list/export loading eagerly collects and is not the fixed summary snapshot. |
| `storage/sqlite_tests.rs:55–78; export/export_tests.rs:52–63` | Concrete legacy fixtures use `complete` and `{text}` / `{title,text}`. Native-only parsing would erase readable output. |
| `export/json.rs:40–46,84–94; export/mod.rs:178–227` | Export has an explicit projection. Its legacy sanitizer is a compatibility filter, not a raw-response exclusion boundary or native JSON validator. |

## Units, exact proposed ports and ownership

Keep Task6 code cohesive rather than putting the whole attempt in AppState:

| Unit | Ownership/responsibility |
| --- | --- |
| `llm/{mod,openai_compatible}.rs` | Opaque owned completion port; compatible chat request, safe envelope parsing, response hashing and request semaphore. No summary store or scheduler reference. |
| `summary/types.rs` | Strict native schema, discriminated public projections, bounded cursor/page and private payload/part identities. |
| `summary/store.rs` | In-transaction admission/counter/version composition, bounded record projections, raw+actual coverage retention, completion/deletion guards. |
| `summary/snapshot.rs` | Private borrowed-ValueRef read on LocalStore's existing connection; deterministic normalization/hash and immutable owned snapshot. This is the focused split of the source plan's summary storage responsibility. |
| `summary/planner.rs` | Pure splitting, immutable manifest, rendered-byte budgeting, map partition and reducing frontier. No config, database, network, key or scheduler access. |
| `summary/terminal.rs` | Channel reservations, durable journal matching/consumption, one joined delivery owner, startup/close local-only reconciliation. |
| `summary/operations.rs` | Actual-attempt completion registry and cancellation-safe bounded deletion scopes. |
| `summary/mod.rs` | Service facade and `SummaryHandler(Arc<SummaryCore>)`. |
| `commands/summary.rs` and AppState composition/lifecycle | Safe command dispatch, owned request admission and runtime/delivery/close readiness. |
| `migrations/0006_summaries.sql` and export modules | Schema6 and explicit complete native/legacy export mapping. Research remains migration7. |

Proposed owned LLM port:

```rust
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    pub max_output_tokens: u32,
}
pub struct LlmResponse {
    pub content: String,
    pub response_hash: String, // lowercase SHA-256 of exact content UTF-8
}
// Opaque backend execution capability: no Serialize, exposed fields or raw Debug.
pub struct LlmExecution {
    owner: OperationLease,
    cancel: watch::Receiver<bool>,
}
impl LlmExecution {
    pub(crate) fn new(owner: OperationLease, cancel: watch::Receiver<bool>) -> Self;
}
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(
        &self,
        profile: &ModelProfile,
        request: LlmRequest,
        execution: LlmExecution,
    ) -> Result<LlmResponse, IntelligenceErrorCode>;
}
```

The service constructor remains
`SummaryService::new(store: Arc<LocalStore>, config: Arc<ServiceConfigStore>,
jobs: Arc<JobScheduler>, llm: Arc<dyn LlmProvider>) -> Self`.
The production async ports are explicitly owned:

```rust
impl SummaryService {
    pub(crate) async fn generate_owned(
        &self, session_id: Uuid, profile_id: Option<Uuid>, owner: OperationLease,
    ) -> AppResult<JobView>;
    pub(crate) async fn after_terminal_owned(
        &self, session_id: Uuid, attempt: AutomaticAttempt, owner: OperationLease,
    ) -> AppResult<()>;
    pub fn list(
        &self, session_id: Uuid, after: Option<SummaryCursor>, limit: u32,
    ) -> AppResult<SummaryPage>;
    pub(crate) async fn delete_owned(
        &self, summary_id: Uuid, owner: OperationLease,
    ) -> AppResult<()>;
    pub(crate) async fn prepare_session_delete_owned(
        &self, session_id: Uuid, owner: OperationLease,
    ) -> AppResult<SummarySessionDeleteGuard>;
}
```

`AutomaticAttempt` is a private discriminated capability:
`Live { delivery_id: Uuid, job_fence: JobAdmissionFence }` or
`LocalRetry { delivery_id: Uuid, reason: LocalRetryReason,
local_fence: SummaryLocalFence }`. `LocalRetryReason` contains Busy and
Interrupted; local admission is incapable of creating a Queued job.
`SummaryLocalFence` validates the current delivery lifecycle ID and stage
under SQLite→delivery control and permits only automatic terminal failure
admission. It is needed before scheduler start and after scheduler admission
is closed; treating an unavailable `JobAdmissionFence` as permission for a
new paid job is forbidden.

The original short generate/after_terminal method names may be offered on a
fixture/request scope that already owns the capability; they must not mint
an implicit `Arc<()>` in production. This is a Rust port reconciliation,
not a new frontend owner parameter. Tauri commands retain
`generate_summary(session_id, profile_id)`, `list_summaries(...)` and
`delete_summary(summary_id)`. AppState obtains its existing direct
`intelligence_lease()` before generate/delete. The fixture harness owns a
real scoped fixture lease and calls these production paths.

Composition is acyclic:

```text
AppState ──> SummaryService ──> JobScheduler
                 │                  │
                 v                  v
            SummaryCore <──── SummaryHandler
                 │
                 ├──> LocalStore
                 ├──> LlmProvider ──> unscoped ServiceConfigStore + JsonHttp
                 └──> bounded AttemptRegistry

AppState ──> SummaryDeliveryOwner ──> narrow store/config/event/wake ports
                                      + Weak<JobScheduler> for live fence issuance
SessionOrchestrator ──> generation terminal port (no AppState or service Arc)
```

The handler must not contain SummaryService or JobScheduler. Delivery drops
a temporary scheduler upgrade after issuing the fence and before any await.
Long-lived config/provider/core/factories do not retain a request or process
lease. A worker owns its process lease; factories retain a weak process
reference and owned work receives bounded child leases.

## Config → SQLite → control → post-commit composition

Add this selected-or-explicit gate, following the actual embedding completion
pattern rather than separately calling settings() and profile snapshot:

```rust
pub(crate) enum SummarySelection {
    Manual { profile_id: Option<Uuid> },
    Automatic,
}
pub(crate) struct SummaryConfigSnapshot {
    pub automatic_summary: bool,
    pub profile: Option<ModelProfile>,
    pub selection_error: Option<IntelligenceErrorCode>, // Configuration only
}
impl ServiceConfigStore {
    pub(crate) async fn with_summary_selection_completion<T: Send + 'static>(
        &self,
        selection: SummarySelection,
        operation: impl FnOnce(
            &rusqlite::Transaction<'_>, SummaryConfigSnapshot,
        ) -> AppResult<T> + Send + 'static,
        completed: impl FnOnce(&T) + Send + 'static,
    ) -> AppResult<T>;
}
```

One ConfigIo closure reads automatic_summary and chooses the explicit manual
ID or saved summary_profile_id. It obtains at most one complete LLM
ModelProfile, checks kind and existing profile validation, and represents a
missing/dangling/wrong-kind selection as local Configuration. A config-file
read/IO failure itself cannot be converted into assumed settings. No secret
is read during admission. In the same gate, LocalStore opens exactly one
Immediate transaction, the supplied closure acquires its control fence
inside SQLite, and the completed callback executes after commit. The
callback captures `JobEvents` / `wake_port` only, not the service, AppState or
scheduler Arc. It publishes/wakes a created row even if the caller vanished.

Retain the approved `SummaryJobRequest` fields. Add the `_in_tx` companion:

```rust
pub(crate) struct SummaryJobRequest {
    pub session_id: Uuid,
    pub profile: Option<ModelProfile>,
    pub automatic: bool,
    pub initial_failure: Option<IntelligenceErrorCode>,
}
impl LocalStore {
    pub(crate) fn create_summary_job(
        &self, request: SummaryJobRequest,
    ) -> AppResult<JobView>;
    pub(crate) fn create_summary_job_in_tx(
        tx: &rusqlite::Transaction<'_>, request: SummaryJobRequest,
    ) -> AppResult<JobAdmissionResult>;
}
```

The wrapper opens one transaction and delegates; it is useful for direct
store tests. Production service composition uses `_in_tx` from the config
gate, never the wrapper inside an already-held transaction.

The exact admission sequence is:

1. An automatic replay may perform a bounded nonmutating permanent-job lookup
   first. Factor that lookup out of the reviewed helper and reuse it both
   here and inside `admit_job`. Existing returns the original JobView without
   requiring currently readable config, a live scheduler, a surviving
   summary row or an available counter. Storage failure still fails safely.
2. Otherwise issue an actual admission owner and capture the live/local
   generation capability before entering scoped config. Read selection and
   profile in the one gate above. Inside its SQLite transaction revalidate
   session existence, terminal eligibility for automatic work, pending
   journal identity where applicable, and absence of a session delete fence.
3. Permanent Existing is rechecked by the helper in that transaction. It wins
   over selection/capacity/preflight failures and consumes no counter. For
   automatic disabled with no existing marker, clear the pending notice in
   this transaction and create no job/version. Manual never consults the
   automatic-summary enable switch.
4. Choose prospective summary and job UUIDs. The payload is the reviewed
   version1 envelope, profiles=[] or [the exact selected profile], and a
   small strict input `{summary_id, session_id}`. Both object_id and payload
   name the same summary UUID. There is no raw transcript, key or caller
   supplied ModelProfile in the payload; its existing64KiB cap applies.
5. Reuse the factored permanent lookup first **inside the same Immediate
   transaction**. If missing, read the durable counter or its legacy MAX seed,
   compute checked next u32 read-only, and call unchanged `admit_job`. The
   helper still owns all payload/exclusion/capacity/INSERT policy and keeps
   its permanent recheck. No generic callback is needed; the single SQLite
   owner/Immediate transaction prevents a writer changing the checked counter
   between this read and the helper. Manual error precedence is unspecified;
   safe exhaustion may precede a manual Busy/payload error.
6. `Created` inserts the Native Queued/Failed summary row with that version,
   saved profile/model, exact job creation milliseconds and job_id. Existing
   inserts nothing. The counter, job/permanent marker, version and any
   consumed journal fields commit or roll back together. Only Created writes
   the checked counter value. Counter exhaustion is detected read-only before
   any job/version INSERT. An injected
   summary insert failure rolls everything back, including the counter.
7. For a live automatic capacity/collision Busy, retry the **helper within the
   same transaction** with initial_failure=Busy. Its rejected first call
   inserted nothing and wrote no counter. Allowed automatic
   terminal failures intentionally bypass unfinished capacity/exclusion;
   they never become cloud work. Manual Busy returns without a new version.
8. The journal is consumed only when that transaction commits Existing,
   Created Queued/Failed, or a known automatic-disabled decision. Dequeueing
   a UUID, losing an observer or issuing a config call does not consume it.
   Then the completed callback publishes/wakes, including a late successful
   ConfigIo closure whose observer timed out.

Failure precedence for a new automatic record is: forced local
Interrupted/Busy, then NoFinalText, then missing/invalid LLM Configuration,
then normal admission capacity Busy. An indexed EXISTS-final check suffices
for empty input admission; ContextTooLarge requires the full snapshot and
becomes a terminal failure of an already Queued version, not an unsupported
initial_failure code. Manual missing selection still admits Queued with no
profile and fails locally during handler preflight; manual has no
initial_failure. A malformed explicit ID fails safely without fabricating a
saved profile or secret association.

Local initial_failure=Interrupted means Failed/error=Interrupted because
that is what the helper creates. Recovery of a previously Queued/Running
summary uses scheduler Interrupted and `SummaryHandler::terminal_tx`. Both
retain the permanent automatic job; a Retry is a new manual job/version.

## Migration6 additions, legacy counters and durable terminal authority

Migration6 owns all source-plan summary metadata, response/coverage/counter
tables and the session-id transcript index. Add these further fields rather
than deriving native identity/time from legacy values:

```sql
-- session_summaries: in addition to profile_id, model, input_hash,
-- input_final_count, snapshot_last_rowid, error_code, raw_response_hash:
origin TEXT NOT NULL DEFAULT 'legacy' CHECK(origin IN ('native','legacy'))
job_id TEXT UNIQUE REFERENCES intelligence_jobs(id) DEFERRABLE INITIALLY DEFERRED
created_at_ms INTEGER
raw_response_bytes INTEGER NOT NULL DEFAULT 0
  CHECK(raw_response_bytes >= 0 AND raw_response_bytes <= 33554432)
deleting INTEGER NOT NULL DEFAULT 0 CHECK(deleting IN (0,1))

-- fixed fields on the already-existing session_lifecycle row:
summary_terminal_pending INTEGER NOT NULL DEFAULT 0
  CHECK(summary_terminal_pending IN (0,1))
summary_terminal_generation TEXT
summary_terminal_error TEXT
  CHECK(summary_terminal_error IS NULL OR
        summary_terminal_error IN ('busy','interrupted','resource_limit','storage','configuration'))
summary_session_deleting INTEGER NOT NULL DEFAULT 0
  CHECK(summary_session_deleting IN (0,1))

CREATE INDEX summary_snapshot_session ON translation_segments(session_id);
CREATE INDEX summary_terminal_pending
  ON session_lifecycle(summary_terminal_pending, session_id);
CREATE INDEX summary_terminal_eligible ON session_lifecycle(status, session_id);
```

These are column definitions for ALTER TABLE additions, not executable
migration code in this checkpoint. Native writers explicitly set origin and
job_id; origin defaults Legacy for old rows and the foundation legacy writer.
A Native read requires valid matching native metadata; it must not silently
fall back to Legacy to hide a malformed newly produced row. Queued/failed
content uses JSON `null` in the existing NOT NULL structured_json column.
No foundation table rewrite, status rewrite or seconds conversion is needed.
The deferred job FK allows whole-session cascades to remove job and summary
rows together; deleting one summary never deletes its job/permanent marker.

The source-plan `summary_responses` and `summary_coverage` definitions remain,
including zero-based part identity and per-row coverage checks. Add response
ordinal0..63, phase map/reduce and byte-length≤2MiB checks as defense in
depth. Use checked `raw_response_bytes` in the same response insertion
transaction to enforce total32MiB; do not repeatedly load raw bodies to sum
their lengths. No coverage row or counter references a response body DTO.

Counter seeding must use `max(0, coalesce(MAX(version),0))`, not an unchecked
u32 cast or a negative seed. A positive legacy MAX greater than u32::MAX is
retained as an exhausted INTEGER counter. New allocation uses checked
conversion/addition and produces only1..u32::MAX. The foundation legacy
writer remains a legacy writer; when used after migration6, its already
existing transaction advances the counter to at least
`max(current, max(0, inserted signed version))`. Otherwise inserting then
deleting a later legacy highest version could permit reuse. Legacy rows and
their signed values themselves remain unchanged. Summary deletion leaves
the counter; session deletion cascades it.

At an exhausted counter, the safe result is ResourceLimit before any new
job/version/permanent marker insertion. An automatic pending journal stays
durable with a safe ResourceLimit indication and makes no paid call. It
cannot simultaneously allocate a failed u32 version when no such version
exists; controller A6-5 explicitly recognizes this edge. Repeated attempts
consume neither a version nor a cloud request. A storage failure similarly
cannot be described as a successfully persisted failed version.

## Nonblocking terminal reservation, lossless journal and joined close

The native observer does not call ConfigIo, SummaryService admission or LLM.
Its new work is an O(1) channel reservation/disposition and fixed fields in
its already-required lifecycle commit.

Proposed narrow ports:

```rust
pub(crate) struct TerminalCommitIntent {
    delivery_id: Option<Uuid>,
    local_error: Option<LocalRetryReason>,
}
pub(crate) struct TerminalReservation {
    intent: TerminalCommitIntent,
    permit: Option<tokio::sync::mpsc::OwnedPermit<Uuid>>,
    rescan: Arc<tokio::sync::Notify>,
}
impl SessionTerminalPort {
    pub(crate) fn try_reserve(&self) -> TerminalReservation;
}
impl TerminalReservation {
    pub(crate) fn intent(&self) -> &TerminalCommitIntent;
    pub(crate) fn committed(self, session_id: Uuid);
}
impl LocalStore {
    pub(crate) fn commit_terminal_lifecycle(
        &self, lifecycle: &StoredSessionLifecycle, intent: &TerminalCommitIntent,
    ) -> AppResult<()>;
}
```

Each active runtime has a generation terminal port captured at admission; its
port contains immutable generation identity plus an atomic live/closing flag,
sender and coalesced Notify. It holds no process owner or config collaborator.
`try_reserve` does not lock AppState, SQLite, ConfigIo or scheduler control,
and never awaits. When live it calls `Sender::try_reserve_owned()`:

* Slot available: a real channel permit is retained across the existing
  terminal write; intent is current generation with no local failure.
* Full: no task/waiter is created. Intent is local Busy for that generation.
* Closing, unavailable, receiver closed, or no live generation: intent is
  local Interrupted. No attempt is made to reopen a sender or worker.

The journal is written in the **same single terminal lifecycle UPDATE**:
Completed/Failed, ended_at, pending=1, delivery UUID, and local error. Replay
of an already terminal transition preserves its settled/pending journal;
only the authoritative new transition writes a new intent. A failed UPDATE
creates no terminal notice and drops the permit. Interrupted-runtime recovery
sets its corresponding pending local Interrupted intent in its existing
recovery transaction. Neither path reads settings, allocates summary versions
nor creates intelligence jobs during audio cleanup.

The observer retains the current cleanup ordering: runtime stop/drain and
network cleanup, terminal lifecycle commit, final event/session bus close,
stop completion publication, active operation release. After dropping the
active mutex it invokes `reservation.committed(record.id)` **only if the
terminal lifecycle commit succeeded**. This method receives only UUID,
sends using the already-owned permit or coalesces a rescan wake, and returns
without awaiting. A later diagnostic write failure/failed stop reply does not
erase a successfully committed terminal notice. There is no additional
config/admission wait on stop. The existing required SQLite lifecycle write
can still contend with other short SQLite work; this proposal does not claim
to remove that baseline persistence dependency.

The delivery owner has exactly one worker, channel32, at most one currently
entered config/admission child, and a retained completion watch/JoinHandle.
Its worker owns an actual process lease. The private child factory has one
semaphore permit and a weak process reference, following the committed
actual-completion pattern. Queue entries are only UUIDs; with one active
worker there are at most33 delivery UUIDs resident outside a bounded scan
page. Reserved permits count against the32 channel slots. No callback creates
a per-session task, waiter list, detached JoinHandle or retry timer.

Overflow and missed hints are recovered from the indexed fixed lifecycle
fields, in keyset pages32 with one extra sentinel. A periodic coalesced
rescan (one200ms interval for the single owner) covers commit-before-notify
observer loss. Dequeueing never clears the journal. Live eligibility requires
all of: current live delivery ID, current job admission fence, matching
pending journal, terminal Completed/Failed session, and no session delete
fence. A native/full/closed intent instead goes through local-only admission;
after the settings gate, enabled automatic summary produces one Failed
Busy/Interrupted job/version/permanent marker and consumes the journal in
that same transaction. Disabled clears the journal without cloud work.

The durable retry indications are fixed fields in existing session history,
not another bounded-memory task queue pretending to hold unlimited notices.
There is at most one indication per persisted lifecycle row. Their total is
bounded by persisted history, not by32, and no complete history vector is
loaded. A strict finite live queue is achieved by moving overflow to this
local retry disposition, never to later paid work. The worker processes
bounded indexed pages and records each decision; on IO/resource failure it
leaves the exact pending indication and fails its readiness/close barrier.
It cannot claim all retries are materialized when config/storage could not
persist them. This is why a plain `try_send` plus dropped Full/Closed error
is insufficient.

An entered ConfigIo closure may outlive its10s observer. Its child owner
remains captured by scoped config and the transaction/completed closure.
The worker must not issue another child/admission after timeout until the
actual child permit is released; it then rereads the journal/permanent job
to determine whether the late closure committed. It never reissues a paid
call on ambiguous observer completion. Post-commit publication/wake is in
the entered closure, not in the abandoned observer.

Close is an owned sequence, not an early receiver drop:

1. With the existing close election, atomically change delivery to Closing,
   signal jobs/probes as today, and promptly run the audio stop branch. The
   terminal sender/owned reservations remain valid. From this instant any
   not-yet-admitted notice can only produce a local Interrupted retry.
2. Intelligence shutdown joins job workers/children and probes/subscriptions
   while the delivery worker finishes local-only work. Local terminal failure
   admission uses `SummaryLocalFence`, never a fake open scheduler fence.
   A live transaction already fenced before close can commit Queued; the
   scheduler shutdown transaction sees/cancels that job before completion.
3. The audio branch performs its existing early stop and admission-fenced
   final stop, then acknowledges that runtime lifecycle work is finished.
   Delivery must not seal before that acknowledgment. Stop completion can
   precede the observer's final synchronous notification, so this proof also
   includes outstanding channel reservations: after the audio acknowledgment,
   receiver.close() disallows new reservations, and recv-to-EOF drains both
   queued UUIDs and every pre-issued OwnedPermit. Do not equate an empty
   buffer or has_active_operation()==false with permit completion.
4. Perform a final indexed pending-journal tail scan, consume local retry or
   disabled/existing decisions, and drain the child semaphore's all-permits
   barrier. Only then drop the worker process lease, publish delivery done
   for that exact UUID, and permit lifecycle retirement.
5. Only after both jobs and delivery actually joined may the existing close
   path drain all16 direct permits and close the index. A5s delivery/join
   observer timeout and the existing30s whole-close timeout leave admission
   and rearm fenced; the actual worker/child retains process ownership. Close
   must still signal/observe delivery if another intelligence result failed,
   rather than returning early and abandoning it. A later close attempt joins
   the same identity; it cannot retire a newer generation.

Stale callbacks cannot repopulate anything. They have only an old terminal
port and UUID. Old-generation live admission fails its SQLite→delivery→job
control checks; a new generation adopts remaining uncertainty only as local
Interrupted. The worker checks session existence and pending journal identity
again inside its transaction. Summary/session deleting flags fence admission
and publication. A UUID queued after whole-session deletion is a no-op;
neither a journal row nor a session is INSERTed from a callback. Deleting
an automatic version leaves its permanent job, so an old callback returns
that Existing job and never allocates another version.

## Startup reconciliation and readiness

Register `SummaryHandler(Arc<SummaryCore>)` under Summary before
`jobs.reconcile()`/start. Runtime/orchestrator receive only the terminal port.
The source constructor order can add the service/handler/delivery owner at
composition without starting cloud work or capturing AppState.

During `start_intelligence`, hold the existing startup gate and a process-owned
delivery stage `Bootstrapping` through these steps:

1. Existing interrupted-runtime recovery has already committed Failed plus
   retained finals and local journal intent. Scheduler reconciliation marks
   existing Queued/Running summaries Interrupted through terminal_tx, retaining
   their raw/hash/coverage/counters and automatic permanent identities.
2. Run existing index/knowledge startup. Run summary local reconciliation
   before scheduler/delivery paid admission becomes available. Scan all
   eligible Completed and Failed lifecycle rows, including migration-era rows
   with pending=0 and no automatic marker; never depend solely on notifications
   produced by the current binary. Query each fixed status with the
   `(status,session_id)` index and a signed/string UUID keyset, page32; avoid an
   IN-clause global sort masquerading as a bounded page.
3. In each owned selected-LLM config→SQLite→local-control transaction, recheck
   eligibility/existence/delete fence/permanent marker. Automatic enabled and
   missing marker means local Failed/error=Interrupted, not a Queued replay.
   Automatic disabled makes no request. Existing wins even if its summary was
   deleted. Never interpret an old `live_pending` generation as new cloud
   authorization. These rules cover completed/failed terminal-commit crash
   windows and sessions marked Failed by interrupted runtime recovery.
4. Finish/resume locally persisted summary deletion fences only after actual
   prior process ownership is known gone. These operations make zero LLM
   calls. A second reopen finds the same permanent markers and no new version.
5. Only after successful local reconciliation and epoch checks, start jobs,
   activate the one delivery generation, and publish ready. Summary commands
   and new runtime admission receive that readiness capability; while startup
   is incomplete they return bounded Busy/closing, rather than racing a
   half-open delivery generation. Stop remains available during startup/close.

Readiness failure does not set Ready or rearm. Entered config work remains
owned and is joined before startup failure cleanup closes shared collaborators.
If history is too large to finish during an observation deadline, keep paged
progress/uncertainty durable and readiness fenced; never load the history or
pay for a subset. Every page and owner count is bounded, and each local
decision makes keyset progress without clearing unresolved records.

## Owned LLM execution and strict response durability

At handler entry, decode the existing64KiB JobPayload and verify its one
optional admitted profile, summary UUID, session UUID and matching native
job_id. Register one actual attempt lease while the job is current Running;
that lease includes a pre-issued `context.operation_owner()` child. No child
is acquired from a terminal context. Reuse that actual attempt lease for
short owned snapshot work, ephemeral resolver scopes and sequential LLM calls.

`OpenAiCompatibleLlm::new(config, http)` stores only unscoped ports and a
request semaphore4. A call scope combines the pre-issued attempt owner and
its request permit; scoped config retains this combined scope if the resolver
outlives its observer. Resolve with
`resolve_snapshot_credentials(admitted_profile.clone())`: every profile field
and the saved secret association must still match before secret access. Do
not put SecretString in LlmRequest, profile, payload, result DTO, diagnostics,
fixtures or Debug. Its plaintext remains inaccessible to complete; only the
opaque JsonRequest bearer crosses the HTTP boundary.

The actual complete future is owned by the scheduler's joined worker, not by
the generate/stop caller. Await HTTP directly inside that owned worker; do
not select an external timeout that drops entered completion while declaring
it reaped. Existing shutdown observation waits on generation completion,
leaving the worker/child alive on timeout. Reqwest's actual operation is
cancel-aware and bounded by the admitted1–120s timeout/body cap; an injected
noncooperative future remains within the owned worker until it returns.
No per-response detached task is needed. An entered ConfigIo operation that
times out holds its actual attempt scope independently; deletion and close
wait for that scope as well as handler completion.

For each planned call, validate the full profile and exact request budget and
send nonstreaming `/chat/completions`: model, system/user messages,
temperature0, max_tokens equal to the admitted output limit, no tools,
proprietary response_format or automatic HTTP retry. Use
`HttpRetryPolicy::Never`, body≤1MiB, HTTP envelope≤8MiB (existing transport
maximum), extracted content≤2MiB. The transport envelope cap is an independent
safe bound: an unusually escaping/verbose envelope can fail ResponseTooLarge
even when a decoded content field would have fit the local raw cap.

Parse only a bounded choices[0].message.content string; absent/empty choices,
wrong types, null/missing content or a refusal fail InvalidResponse. The
system prompt requests the exact schema, treats transcript text as quoted
data, and requests no invented participants/actions/dates. Model output
cannot trigger tools or network. Compatible provider context errors must
fail, never shorten input or retry a prefix. The current HTTP producer
classifies non-2xx as Network before returning a body. A6-6 allows exact
allowlisted context error codes (for example context_length_exceeded) in a
non-2xx error body bounded64KiB to map to ContextTooLarge; no error-body prose
or raw content is returned/logged. Unrecognized errors retain safe generic
failure behavior.

Every returned map/reduce content string is retained **before native schema
parsing**. In one SQLite transaction, `context.commit` / publication fence
checks current Running job, cancel, matching summary job_id and deleting=0;
then writes raw_text/hash/ordinal/phase, actual map coverage (maps only),
checked total bytes and latest summary raw_response_hash. Any retention
failure rolls back that response and its coverage together. Exceeding2MiB or
32MiB fails the version visibly and keeps earlier retained responses. A
supplied response that was too large/unavailable/unretained does not claim
durable retained map coverage.

Now deserialize/validate `SummaryContent`, ActionItem and Term with
deny_unknown_fields and exact string/array types. All seven root fields are
required. Summary prose≤20000 Unicode scalar values; every other string,
including action text/owner/due and term/meaning,≤2000; each array≤100. Empty
arrays are valid, nullable owner/due use their approved Option semantics.
Use bounded deserializers/validation; never strip fences, repair JSON, drop
unknown fields, stringify wrong types, truncate strings or take100 entries.
Failed strict parsing retains raw/hash/actual supplied coverage, while
structured_json remains null. Generic diagnostics include safe codes only.

Final success stores the strict root content under the same publication
guards only after full manifest/retained coverage checks. Return from run;
the scheduler then performs its actual terminal transition and calls
terminal_tx in that transaction. `SummaryHandler::terminal_tx` copies exact
JobStatus/error into the corresponding native version, including Queued
cancellation and scheduler interruption, without overwriting an older
version. A late result/raw callback is an UPDATE/INSERT guarded by an existing
nondeleting native row plus current Running job; it cannot recreate a deleted
summary/session. No automatic request-level LLM retry is performed.

The original explicit probe remains `probe_snapshot(saved_id)` plus revision/
full-profile-fenced `persist_probe`; it already resolves the requested saved
ID and owned opaque bearer in one gate. Do not redirect it through the
currently selected summary profile or remove its revision check. Task7
ResearchService must reuse the owned LlmExecution and admitted full-profile
resolver for its own saved ID.

## Fixed borrowed SQLite snapshot and deterministic input hash

Proposed private snapshot shapes:

```rust
struct FinalSnapshot {
    session_id: Uuid,
    last_rowid: Option<i64>,
    rows: Vec<SnapshotRow>,
    input_final_count: u32,
    normalized_bytes: usize,
    input_hash: String,
}
struct SnapshotRow {
    rowid: i64,
    id: Uuid,
    direction: String, // exact stored value, including unknown legacy strings
    sequence: i64,
    occurred_at_seconds: i64,
    occurred_at_ms: Option<i64>,
    language: String,
    text: String,
    kind: Option<TranscriptKind>,
    identity: SnapshotIdentity,   // allowlisted actual native correlation metadata
}
struct SnapshotIdentity {
    provider: Option<String>,
    provider_config_id: Option<Uuid>,
    stream_epoch: Option<u64>,
    sentence_id: Option<String>,
    local_correlation_id: Option<u64>,
    provider_timestamp_ms: Option<u64>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
}
```

Use a short private Deferred transaction on the existing mutex connection.
First verify session existence; the first indexed read pins the snapshot:

```sql
SELECT MAX(rowid) FROM translation_segments
WHERE session_id=?1 AND is_final=1;

SELECT rowid,id,session_id,direction,sequence,occurred_at,language,text,
       is_final,details_json,occurred_at_ms
FROM translation_segments
WHERE session_id=?1 AND is_final=1 AND rowid<=?2
ORDER BY rowid LIMIT 100001;
```

The upper bound is that session's MAX, not a global MAX or a timestamp/sequence
surrogate. None is genuine empty input. Rows are storage insertion order
across directions. No interim/audio/config/context/diagnostic bodies enter the
snapshot. Late promoted finals carry their actual final UUID/sequence/details
because final persistence precedes publication; no interim upsert assumption
or sentence pairing is introduced.

Read every untrusted string/metadata field with `Row::get_ref`/ValueRef and
checked UTF-8; check byte/type/escaping charges before to_owned or legacy
metadata parsing. Do not call the eager `read_segment_at`, public page reader
or get_export_data. Use fixed scalar types for sequence/times and bounded UUID
parsing. Decode actual allowlisted identity fields from bounded metadata;
missing native kind is None, never guessed from language. Present malformed
native types fail safely. Unknown legacy direction/language and absent
epoch/correlation/milliseconds remain unknown/absent. No typed native
TranscriptUpdate validation is imposed on a legacy row.

Define normalization `voxbridge.summary-input.v1` as a fixed-field-order JSON
document with format tag, session UUID and ordered rows containing rowid/ID,
stored direction/sequence/seconds, nullable actual milliseconds/kind,
language, exact text and the nullable safe identity allowlist. Count/hash
that representation once, including framing/quoting/escaping/null distinctions.
Use compact positional row arrays in the versioned JSON document:
`[rowid,id,direction,sequence,seconds,milliseconds,kind,language,text,identity]`,
with identity fields in the SnapshotIdentity declaration order and nulls for
missing values. Document keys are format, session_id, rows; UUIDs use checked
canonical text, numbers exact decimal and JSON strings standard escaped UTF-8.
The positional schema/version fixes meaning without repeating verbose keys
100000 times; empty legacy rows can exercise the row-count boundary without
being rejected merely by unnecessary field-name overhead. Request envelopes
still use explicit labelled objects and their actual rendered-byte budget.
Raw details_json is not another quoted field in normalized input or prompt;
it remains unchanged in the existing local transcript table and is not
cloned into SnapshotRow. Requests use that explicit safe identity/role/
direction/language envelope plus exact quoted text. The allowlist retains
actual provider/config UUID, epoch, native sentence ID, local correlation,
provider timestamp/start/end where present; it excludes unneeded raw
telemetry, log/event envelopes and arbitrary legacy properties. Missing kind
remains None. No key/audio/config body is fetched or inferred. Stream into
a checked counting SHA-256 writer; do not first construct an unbounded
canonical String. Row boundaries cannot hash-collide through concatenation.

Borrowed raw metadata inspection has a separate16MiB aggregate parser-work
budget, checked before safe field decoding/allocation. It is not added to
normalized input bytes. This satisfies the handoff's separate metadata-bound
option without counting raw and decoded identity twice. Exceeding that
separate budget is a visible local ResourceLimit, not a claim that normalized
input exceeded16MiB. Omitted raw properties stay stored locally; none is
forwarded into a request. If a future local-detail reader copies them, its
own aggregate16MiB retention bound must be enforced independently.

Snapshot limits are100000 finals and16MiB of that normalized encoding, checked
before row copies. Row100001/byte16777217 yields ContextTooLarge, never a
successful prefix. A nonempty snapshot retains empty-text rows and their
IDs. Distinct final count equals rows.len(), not split parts/request count.
Only a wholly successful capture writes input_hash/count/last_rowid on the
admitted version. Native metadata missing due to early local failure remains
None; empty admission may truthfully record count0/high-water None.

Run the short read on a bounded owned blocking scope (at most the two summary
workers), with a5s acquisition/scan budget and cancellation checks. A bounded
try-lock acquisition avoids creating an indefinite new snapshot waiter;
scanning checks that same deadline before every row/encoding charge. Retain
the actual attempt lease through the JoinHandle and never abandon an entered SQLite
operation by dropping an observer and releasing ownership. Finish/drop the
read transaction, statements and mutex before request rendering, ConfigIo,
schema parsing or any network await. The snapshot owns bounded strings and
fixed row headers;16MiB is an input encoding cap, not a16MiB total process
memory claim. Total retained text/safe identity is bounded by that encoding;
decoded metadata copies are bounded by the admitted safe identity, plus at most100000
fixed row headers. No whole-session Vec of eagerly allocated legacy rows is
created and then checked.

The controller's disposable Python SQLite3.53.2 experiment found the existing
ordered query using session_sequence plus TEMP B-TREE; the proposed
`summary_snapshot_session(session_id)` removed the sort and supplied a
session_id/rowid range scan. This is not bundled-Rust SQLite test evidence.
Task6 must verify EXPLAIN QUERY PLAN with the actual bundled SQLite: ordered
session/high-water queries use the session index, no unrelated table scan or
temporary ORDER BY tree. A LIMIT alone does not satisfy that contract.

## Immutable part manifest, bounded hierarchy and exact supplied coverage

```rust
struct PartKey { segment_id: Uuid, part_index: u32, part_count: u32 }
struct PartRef {
    snapshot_row: usize,
    byte_range: Range<usize>,
    key: PartKey,
}
struct Manifest { parts: Vec<PartRef> } // frozen before first request
struct RequestPlan {
    ordinal: u32, level: u32, phase: Phase,
    input_range: Range<usize>, coverage: Range<usize>,
}
struct SummaryNode {
    content: SummaryContent,
    coverage: Range<usize>, // adjacent manifest part range, independent of prose
}
struct PlannerCounters { calls_reserved: u32, level: u32 }
```

The immutable manifest orders rowid then zero-based part index. An unsplit
row is `(id,0,1)`. Byte ranges refer to the owned snapshot, not cloned text.
Greedily pack consecutive whole rows; if an individual rendered row cannot
fit, split exact text at Unicode scalar UTF-8 boundaries. Find maximal
fitting escaped prefixes with worst-case10 decimal digits for u32 part
index/count, assign actual counts, then re-render/recheck every request.
Concatenating each row's part ranges reconstructs the exact original bytes,
including Chinese/emoji/combining characters/newlines/quotes. An empty row
still contributes one part. An identity envelope that cannot fit yields
ContextTooLarge rather than omission.

For every request, with checked arithmetic:

```text
actual_system_UTF8_bytes + actual_user_UTF8_bytes + 1024
    <= admitted_max_context_tokens - admitted_max_output_tokens
```

The bytes are those actually rendered with quoted labels, IDs, parts,
delimiters, schema/system instruction and all escaping. Profile validation
already bounds context2048..131072/output128..16384 and output≤context/2.
The extra1024 reserve is in addition to counting system text. This is a
conservative UTF-8-byte token estimate for arbitrary compatible tokenizers,
not a tokenizer-exact guarantee. Provider rejection is a failed version.

Before any LLM request, finish the full manifest/map partition or fail with
zero calls. Keep at most64 RequestPlans and one rendered request at a time.
A manifest allocation cap100000 parts is a safe feasibility bound, not
permission to truncate: the mandatory part JSON subset
`segment_id/part_index/part_count/direction/kind/language/text` is at least134
bytes even with empty labels/text;64 ×131072 total request bytes permits fewer
than62602 such records. Thus a >100000-part manifest cannot finish under the
approved request/context limits. The implementation must verify the declared
mandatory envelope lower bound; use checked actual aggregate counting, and
fail before allocation/calls when it cannot fit. Do not collect millions of
one-character PartRefs before noticing the64-call limit.

One map may itself be the final result. Multiple maps reserve at least one
additional reduce, so64 maps already fail before the first call. Maps are
level1. Reserve a whole prospective level's actual calls before dispatch;
maps and every reduce including the root share the64 counter. No call65,
level9, counter reset on failure/cancellation or automatic request retry.

Each map parses the same strict schema after raw retention. For reductions,
render each complete child SummaryContent; greedily group all adjacent nodes
within the same actual byte budget. Groups of≥2 issue a reduce; a singleton
is carried unchanged with its coverage and consumes no call. At least one
pair must combine and the next frontier must have strictly fewer nodes.
No pair fit/no progress/unsliceable oversized child/exhausted calls or levels
is ContextTooLarge. Recompute sizes after every response; output token limits
do not prove that returned JSON is smaller. Never trim an intermediate
object, concatenate arrays then take100, skip a child or choose a convenient
frontier node as success.

Keep three distinct provenance concepts:

* Expected: the complete immutable manifest created from the fixed snapshot.
* Supplied/attempted: the exact contiguous parts in a map request that was
  actually dispatched. Planned uncalled work is not supplied coverage.
* Retained: map inputs whose raw response/hash was committed. Insert these
  exact tuples atomically with that response before parsing. Invalid strict
  JSON can have retained coverage; unavailable/oversize/uncommitted responses
  cannot. Failed/Cancelled/Interrupted versions may be partial.

A reduced node gets the exact union of its complete adjacent child ranges,
set by the planner, not extracted from generated prose. Validate no gaps,
overlaps or foreign ranges. On success root range is exactly
`0..manifest.parts.len()`, and the actual persisted map coverage is the exact
expected `(segment_id,part_index,part_count)` set. Stream a bounded coverage
read under the publication transaction, compare every tuple/count against
the manifest, check consistent per-segment part_count, uniqueness and count;
the row-local SQL CHECK is not a whole-manifest proof. In particular,
`(id,0,3)` and `(id,2,3)` cannot succeed by matching distinct segment IDs
while omitting `(id,1,3)`. `input_final_count` remains distinct original rows.

The guarantee is complete supplied input/provenance. It does not promise that
model prose or entities semantically mention every input fact. Intermediate
frontier parsed strings are bounded by strict per-response2MiB and retained
total32MiB, with≤64 nodes and bounded array/string headers. No full set of
rendered request strings or repeated large coverage HashSets is retained.

## Native/legacy view, cursor and complete export reconciliation

Recommended unshipped wire shape:

```rust
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum SummaryView {
    Native(NativeSummaryView),
    Legacy(LegacySummaryView),
}
pub struct NativeSummaryView {
    pub id: Uuid, pub session_id: Uuid, pub version: u32,
    pub status: JobStatus, pub created_at_ms: i64,
    pub content: Option<SummaryContent>, pub deleting: bool,
    pub profile_id: Option<Uuid>, pub model: Option<String>,
    pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>, pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>,
}
pub struct LegacySummaryView {
    pub id: Uuid, pub session_id: Uuid,
    pub version: String,             // canonical exact signed i64 decimal
    pub status: String,              // original TEXT, e.g. "complete"
    pub created_at_seconds: String,  // canonical exact signed i64 decimal
    pub created_at_ms: Option<i64>,   // None; no seconds multiplication
    pub content: Option<serde_json::Value>, // bounded legacy sanitizer result
    pub projection_error: Option<IntelligenceErrorCode>,
    pub profile_id: Option<Uuid>, pub model: Option<String>,
    pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>, pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>,
}
pub struct SummaryCursor(String); // private field, canonical signed decimal <=20 bytes
pub struct SummaryPage {
    pub rows: Vec<SummaryView>,
    pub next_cursor: Option<SummaryCursor>,
}
```

Legacy native metadata is None, and invalid/oversize legacy structured data
has an explicit projection_error rather than a fabricated native failure
status. Storage is never rewritten. Existing readable `{title,text}` / `{text}`
survive the compatibility sanitizer; new native parsing never uses that
sanitizer. Origins come from stored migration/writer metadata, not guessed
from status spelling, field names or successful strict parsing.

The cursor parses canonically to i64, rejects >20 bytes, whitespace, leading
plus/zeroes/noncanonical negative zero and out-of-range values, and compares
to signed stored version. None means no lower bound, not0. Page by
`WHERE session_id=? AND version>? ORDER BY version` using the existing unique
session/version index. The cursor is a string so React does not round a
legacy value beyond JavaScript's exact integer range. Deleted cursor/highest
versions still make progress. Return at most100 rows, default50,8MiB total
projection bytes. A native row allows2MiB+64KiB for its canonical content,
nullable-field expansion and safe metadata; a legacy content field allows
2MiB and otherwise reports explicit projection_error. Original legacy status
has a64KiB borrowed byte bound; an overlarge status fails the page safely
rather than being clipped or mapped to another status. Native model names
retain the admitted200-character profile bound, hashes are64 ASCII hex bytes,
and UUID/cursor encodings have their fixed bounds. Read borrowed lengths
before allocation; depth64, at most100000 JSON nodes and at most2MiB decoded
string/key bytes constrain legacy Value/sanitizer allocation and recursion.
If a next row exceeds the page byte budget,
return the complete earlier rows and their exact next cursor; if the row
itself is invalid/oversize, return its identity/original values plus explicit
projection error, never a successful content prefix. Do not advance past an
unreturned row or force legacy values through `after_version: Option<u32>`.

Export consumes an explicit `SummaryExportRecord` projection rather than
serializing storage rows, ModelProfile, JobPayload or response tables. Native
records contain all SummaryContent fields, each action text/owner/due and
term/meaning, version/status/actual creation time, origin, saved model/profile
and optional safe hash/count/error. Failed/Queued/Cancelled/Interrupted versions
retain identity/status/nullable metadata without invented content. Legacy
exports preserve numeric signed version, original numeric seconds, original
status and readable sanitized structured_json; new native metadata remains
null. New native created_at seconds may be deliberately written from the
actual native creation timestamp for export compatibility; legacy milliseconds
are never manufactured by multiplying seconds.

Use bounded borrowed summary projections/pages and a total32MiB summary
projection budget for the existing in-memory ExportData/rendering path.
Bound the whole rendered artifact at128MiB with a checked writer; an
unrepresentable/overlarge/invalid legacy record or total-cap breach makes the
export fail as a whole and preserves the destination, rather than silently
dropping a version or its content. A future streaming export can extend
capacity without changing this completeness contract. Keep the existing
complete transcript/export behavior and implement only the necessary bounded
summary projection/rendering seam here; do not call the new summary snapshot
reader as a substitute for export's legacy display/order semantics.

Markdown and plain text render the complete native structure plus safe
model/version/status metadata and keep readable legacy content, not just the
summary prose. JSON extends its existing allowlist. No export query loads
summary_responses.raw_text; raw content and its unique sentinels never enter
ExportData. `safe_summary_json` remains the legacy filter, not a raw-response
boundary; reject reserved raw-response fields if found in arbitrary legacy
structured input rather than blindly adding them to output. Hashes are
allowlisted explicitly. Internal local response access is one bounded2MiB
row (or at most64 hash/ordinal records), not an unbounded raw Vec; Task6 does
not add a public raw-detail command.

Task8/9 handoff must ledger the discriminated union, exact cursor/page,
deleting flag, local Interrupted retry meaning and Retry→manual semantics.
No history screen or product layout is implemented at this checkpoint.

## Async active deletion and actual completion

Expose this narrow **nonmutating** reap port from scheduler composition:

```rust
pub(crate) struct JobReapPort {
    // Private exact-generation control and completion receiver; no scheduler Arc.
}
impl JobScheduler {
    pub(crate) fn reap_port(&self) -> AppResult<JobReapPort>;
}
impl JobReapPort {
    pub(crate) fn is_reaped(
        &self, tx: &rusqlite::Transaction<'_>, job_id: Uuid,
    ) -> AppResult<bool>;
}
```

The port is issued before cancel/delete and releases scheduler-state locks
before SQLite entry. Its predicate runs only SQLite→captured generation
control: the matching job is terminal, is absent from that generation's
`Control.active`, and no claim/handler entry is outstanding. In the observed
producer, active removal follows handler.run completion in the terminal
transaction, while claim installs active before commit. Thus a recovered
Interrupted status with an old claimed handler not yet entered is not reaped.
The separate AttemptRegistry then proves all entered child work completed.
Generation retirement is accepted only from the matching completion
acknowledgment, never inferred from a missing/different current generation.
A no-generation startup port is issued only under the process/startup proof
that no prior generation exists or that the prior owner actually joined.
Any repaired Task5C port that supplies these same exact proofs should be
reused; do not create a competing interpretation of its retirement state.

Deletion may check this small predicate on a bounded20ms interval within its
10s deadline; events are hints, never reap proof. The hard-delete transaction
rechecks the predicate and registry idle state under the persisted deletion
fence. A Queued cancellation wins before claim or waits for the actual claimed
handler; a terminal-status poll alone cannot decide which won.

The private AttemptRegistry is bounded16, matching the generation's actual
child cap. It stores weak actual-attempt owners and completion watches, not
strong owners that prevent their own completion. A registered owner wraps
the pre-issued job child; every entered snapshot/resolver/LLM scope clones
it. The final clone dropping after real work releases its process/child
permit before publishing actual done. Handler run completion and scheduler
terminal status are separate observations, both checked by deletion. A
blocking resolver that timed out can therefore keep an attempt active even
after its job became Failed/Interrupted.

`delete_summary` is async, admitted with one of the existing16 direct leases.
Its retained task owns only service/store/registry/job/epoch ports and the
lease/completion response; no AppState capture is needed. Cancelled callers
do not detach accepted deletion. The task count is bounded by those permits;
close's all-permits barrier actually joins the accepted work.

1. In a short SQLite→current mutation/admission-control transaction, find the
   summary, verify native job/session identity, and set deleting=1. That
   persistent fence blocks all future raw/coverage/content publication and
   retry generation for that object. A legacy inactive row needs no job
   cancellation. Do not call jobs.cancel while SQLite is held.
2. Outside SQLite, cancel the matching queued/running job with the existing
   scheduler method. Queued cancellation runs terminal_tx; a Running worker
   receives its watch signal. A worker racing registration must check the
   already-persisted deleting flag/current job before it can enter LLM work.
3. Within a10s observation budget, await the registered actual owner(s) and
   the `JobReapPort` predicate. Do not poll just JobStatus or just weak-count
   while a claim→handler-entry race can create work. Handler cleanup may
   notify a waiter, but does not replace the generation-bound active/reap
   predicate or actual-child completion. Scheduler interruption with
   awaiting_reap does not fabricate either proof. Owned cleanup watches/
   entries remain≤16.
4. After both proofs, recheck existence/deletion fence/current lifecycle in
   SQLite and DELETE the summary. Its raw/coverage cascade; counter and
   automatic job/permanent marker stay. No model callback can UPSERT it.
5. Timeout, close cancellation or storage error leaves the row/deleting fence
   and reports Timeout/Interrupted/Storage; hard deletion does not occur
   later through an abandoned waiter. Retry resumes that same fenced delete.
   Close/reopen remains protected by scheduler/attempt/direct ownership.

For whole-session deletion, retain the existing no-active-audio eligibility
and subscription cleanup. Before cascade, an owned summary guard atomically
sets summary_session_deleting=1 and fences that session's native versions,
collects at most32 unfinished matching summary jobs, cancels them outside
SQLite and waits for matching actual owners/reaped proofs (≤16) within10s.
Terminal delivery/admission rechecks this flag; a late entered ConfigIo
admission either won before the guard and is included, or sees the fence and
inserts nothing. Finally the guarded existing session DELETE cascades journal,
summary rows/responses/coverage/counter and session jobs. Failure keeps the
session and fence available for local retry. Reconciliation resumes deletion
fences locally before opening admission, without LLM calls. This is required
backend ownership work, not implementation of the future history UI.

## Verification to require after activation; no commands run here

The source plan's real SummaryHarness owns LocalStore, JobScheduler,
RecordingLlm and fixture runtime/orchestrator. Its completed_session,
run_jobs, responses and summary_versions must exercise those collaborators,
including delivery ownership/readiness, rather than a planner-only fake.
RecordingLlm holds controlled strings/requests only in tests; defaults never
use paid/network/native keyring/audio/driver resources.

| Contract | Required substantive behavioral evidence |
| --- | --- |
| Terminal authority / stop | Real terminal commit with a closed session bus; repeat callback before/after job terminal and storage reopen; a delayed LLM shows stop completion already returned. Failed diagnostic after a successful terminal commit still delivers; failed lifecycle commit does not. |
| Delivery capacity / loss / join | Hold the single admission child, exhaust32 reserved/buffered UUID slots, and prove overflow returns stop promptly and produces one local Busy retry with zero overflow LLM calls. Drop commit-before-notify observer, close before/after reservation/commit/completion, hold an OwnedPermit after active release, and prove drain waits for its notification and actual ConfigIo child. No new generation before actual join. |
| Recovery | Completed and Failed commit-before-admission crash windows and interrupted-runtime Failed with retained finals produce one local Interrupted retry, zero LLM calls, same marker/version after second reopen. Old live generation journal/hint becomes local only. |
| Admission / counter | Real helper transaction; permanent Existing beats later queue/config/counter changes; manual rerun/retry after each terminal status has distinct job/version; concurrent manual requests admit one new version. Trigger summary INSERT rollback leaves no job/marker/counter advance. Delete highest/all then reopen produces larger versions. Negative/zero/out-of-u32 legacy seeds, post-migration legacy insertion and u32 exhaustion have explicit checks. |
| Owned config / HTTP | Hold entered ConfigIo beyond observer timeout, change any admitted ModelProfile field/delete association before resolution, cancel/close and attempt rearm/delete. No stale HTTP dispatch or raw/content repopulation. Per-call owner and semaphore remain until actual completion. Existing explicit probes retain saved-ID/revision semantics. |
| Fixed snapshot | Real SQLite insertion order across directions/unrelated sessions; same-session high water; late final absent from one snapshot/present next; final promotion preserves actual identity. Borrowed oversized legacy text/details and escaping expansion fail before eager copies;100000/16MiB boundary capture and overflow; transaction released before delayed LLM. Bundled SQLite query plan has session-id/rowid range access and no temp sort. |
| Manifest / budget | Chinese/emoji/combining/newline/quote split reconstruction; empty rows retained; every request's exact rendered bytes+1024 satisfies budget. Full snapshot-ID and full part-tuple equality, including a middle-only omission that still has the same distinct IDs. Planned uncalled parts never become retained coverage. |
| Hierarchy | Multiple levels, singleton carry, complete children, strict node-count progress and recomputed growing outputs. Finish on call64/level8; prospective call65/level9 and64 maps fail before illegal dispatch. No prefix/node success when capacity fails. |
| JSON / durability | Unknown fields, fences, mismatched types, overlong strings/arrays fail without repair; invalid map/reduce raw/hash and actual map coverage persist first. Later2MiB/32MiB breaches retain prior rows. Storage failure/cancellation/deletion fence block terminal publication. |
| Delete | Delete Queued/Running/internally Failed-but-owned attempt, entered resolver timeout and noncooperative response; bounded timeout leaves fenced row, completion retry removes it, raw/coverage cascade, counter/permanent marker remain. Whole-session delete race with terminal/config admission cannot repopulate. |
| View / cursor / export | Native and complete legacy fixtures, signed negative/zero/>u32/>JS-exact versions/timestamps, exact cursor after deletion, byte/depth limits and explicit invalid projection. JSON/Markdown/text include every native field/action/term and legacy readable content/status/seconds; raw-response/key/audio sentinels are absent. Whole export cap/error preserves destination and never succeeds with a missing version. |

Only after the controller activates Task6 with reviewed Task5C BASE should
the implementer run the approved focused summary/LLM/session/export commands
and required full all-targets/desktop/fmt/diff/frontend gates. Save behavioral
RED/GREEN evidence; compilation-only failure is not runtime RED. The future
environment remains the recorded task-local TMPDIR, Cargo home/target and
ALSA prefix; root confirms them at activation. No global configuration,
mount/permissions or editor repair is implied.

## Checkpoint-only verification and limitations

Source/index inspection before writing found only the pre-existing staged
`.gitignore`, whose SHA256 was
`1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`.
No source diff was present. This report is the only intended output path.
No Cargo/test, implementation/migration/index creation, production database,
paid/network/native/config-secret action, git-index mutation, stage/commit,
agent spawn or global/provider configuration change was performed.

The query-plan artifact is architectural evidence on disposable Python
SQLite3.53.2, not bundled-SQLite verification. Every new interface, cap,
transaction/lifecycle proof and behavioral test above is proposed work;
independent Task5C review and controller rulings remain external to this
checkpoint. No implementation completion, Spec PASS or Quality APPROVED is
claimed.

Checkpoint self-review checked the source-plan requirement mapping and
placeholder/type/ownership/transaction contradictions. A read-only arithmetic
check confirmed the mandatory part JSON subset is134 bytes and its64-call /
131072-byte upper-bound quotient is62601 parts; this is a document-bound
calculation, not a Task6 runtime test. Final read-only inspection confirmed
the report exists, HEAD remains b725560, the source diff including staged
source is empty, and the staged path list remains only .gitignore. Worktree
and index .gitignore SHA256 both match the protected hash quoted above.

## Reconciliation appendix — controller questions

**A6-2: the generic pre-create callback is unnecessary and withdrawn.**
There is no counterexample to the smaller seam under the existing single
SQLite owner and one Immediate transaction: reuse the factored permanent
lookup first; Existing returns without any allocation; otherwise read the
counter/legacy seed and check next u32 without writing; call unchanged
admit_job; only Created writes counter and version. The helper still owns
all admission policy and its permanent recheck. A failed summary/counter
write rolls back the job/marker as before. Exhaustion precedes job/version
INSERT. Manual error precedence is not specified and need not justify a
generic scheduler API. The main proposal above is aligned with this smaller
choice; the _in_tx companion and owned config completion gate remain needed.

**Snapshot charging is safe identity once, raw metadata only locally.**
Normalized input/hash and request rendering contain actual row identity,
role/direction/language/text, signed original sequence/seconds, nullable
actual milliseconds, provider/config UUID, epoch/native/local correlation
and optional stored provider/start/end times. No language-based kind guess,
invented timestamp or copied key/audio/config body. Raw details_json stays
in the existing local table, is not a quoted normalization field or prompt,
and is not cloned into the snapshot. A separate16MiB aggregate borrowed
metadata parser-work bound precedes safe decoding; its breach is ResourceLimit,
whereas normalized16MiB/100000-row breach remains ContextTooLarge. This avoids
double charging and makes unneeded telemetry's separate bound explicit.

**The strict schema's logical maxima are not the native read-size bound.**
All maximum strings/arrays can total1,820,000 Unicode scalars (up to7,280,000
UTF-8 bytes before JSON syntax), so strict validation alone cannot justify
a2MiB list row. Every generated native root also came from a retained raw
response≤2MiB. Canonical serialization can add missing nullable action fields
(at most a few KiB) and the view adds bounded model/UUID/hash/status metadata.
Thus the native row allowance is2MiB+64KiB, measured by a checked writer,
inside the8MiB whole-page budget. A native producer cannot bypass the raw
cap merely because its typed content satisfies string/array limits. Four
near-maximum native rows may require another page; no row is clipped.

Legacy content remains independently bounded at2MiB, with explicit
projection_error for invalid/overlarge JSON while carrying the original
signed version/seconds and status unchanged. An overlarge legacy status
itself fails the page safely rather than substituting a native status.
Exports preserve original readable legacy data/status/seconds or fail as
a whole on a bound/error; they never turn complete into Succeeded or claim
missing native metadata was known. These are implementation bounds and
truthful projections within the approved constraints, not new product scope.
