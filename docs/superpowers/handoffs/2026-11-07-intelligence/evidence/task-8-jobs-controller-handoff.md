# Task 8 jobs controller readiness handoff

Status: **preparation only, not Task 8 implementation or approval**. Read task-8-brief.md first. Inspection stayed with reviewed Task 3B intelligence-wire.ts, desktop.ts and commands/intelligence.rs; the shared reviewed wire decoder/ticket contract; narrowly selected frozen 3B subscription/ticket/event-fixture hunks; and reviewed Task 1 session-stream controller/types/ownership tests. No in-progress Task 4B source, Tasks 4–7 producer API invention, UI/design/source change, test execution, Cargo, network, engine, stage, commit or child agent work. Only this ignored report is written.

Task 3B's accepted review authority is task-3b-runtime-review.md, BASE f4b381d to HEAD 5a0d6760417e8f1e5727f01fcd2d9ba5e390e2a0. This handoff does not repeat that review. The controller plan below preserves the Task 8 requirements: **at most 64 visible job states, one-second unfinished-job polling cadence, at most four actual native getter calls concurrently, paged history, independent intelligence subscription cleanup**.

## Actual transport and command shape

DesktopPort is the injected transport: `invoke<T>(command,args?) -> Promise<T>` and `listen<T>(event,handler) -> Promise<() => void>`. The actual desktop adapter passes **event.payload** to the handler; callers must not expect a second `{payload:...}` wrapper. Decode unknown responses/events at this boundary. `withDesktopDeadline` rejects the local observer after **10000 ms**; it does not cancel or acknowledge the original invoke/listen Promise. `invokeDecoded` wraps this observation deadline, so using it alone as semaphore ownership would be incorrect.

Tauri commands take camelCase invocation argument keys for the Rust snake_case parameters; returned DTO fields retain serde snake_case. These commands use no invented `request` wrapper:

| Native command | Invoke arguments | Actual result / decoder |
| --- | --- | --- |
| subscribe_intelligence | `{subscriberId: string}` | `{id: UUID,event_name: string}` / shared `decodeSubscriptionTicket` |
| unsubscribe_intelligence | `{subscriptionId: ticket.id}` | unit / JSON null; completion acknowledges the native registry operation |
| list_jobs | `{after: number|null,limit: number|null}`; initial bounded view `{after:null,limit:64}` | JobPage / `decodeJobPage` |
| get_job | `{jobId: UUID}` | JobView **or null** / `wireSchema.nullable(decodeJobView)` |
| cancel_job | `{jobId: UUID}` | JobView / `decodeJobView`; merge the returned real status, not optimistic cancellation |

Rust list parameters are `after:Option<i64>,limit:Option<u32>`. JobPage has `rows:JobView[]` (wire maximum 200) and `next_cursor:number|null`. Pass this cursor back as an **opaque `after` cursor**; do not derive it from updated_at_ms, created_at_ms or a UUID, and do not invent an unfinished-only filter. The Task 8 visible page may request 64 although the wire accepts larger pages. Backend pages are authoritative history; the controller must not concatenate every page into its 64-row live store.

The subscription ticket is the existing Serialize/Deserialize `SubscriptionTicket {id:Uuid,event_name:String}`. Its exact-key decoder validates UUID and event_name <=512 bytes/code units. Native subscriber tokens contain 1–64 Unicode scalars; one generated UUID is a valid controller-lifetime token. The returned channel currently has the form `voxbridge://intelligence-subscription/{id}`. **Listen on the returned event_name**; do not synthesize a channel from a session, ticket ID or a guessed global event constant.

Native subscriptions use the key `(Uuid::nil(),subscriber_id)` independently of session subscriptions. Reusing that subscriber token replaces its previous forwarder after cancellation/join. Unsubscribe targets the exact ticket UUID, so cleanup of an older ticket cannot unsubscribe a newer replacement. A terminal job does not terminate the global intelligence stream. The frozen forwarder reports lag and continues; its one-second emit deadline is a separate native emission bound, not the frontend polling interval.

## Actual JobView / event / numeric contract

```ts
type JobView = {
  id: string; // UUID
  kind: "import" | "reindex" | "delete_document" | "summary" |
        "research" | "research_overview";
  status: "queued" | "running" | "succeeded" | "failed" |
          "cancelled" | "interrupted";
  session_id: string | null; // UUID when present
  object_id: string | null;  // UUID when present
  completed_units: number;  // u32
  total_units: number | null; // u32, unknown remains null
  error: IntelligenceErrorCode | null;
  created_at_ms: number; // epoch milliseconds, safe i64 frontend range
  updated_at_ms: number; // epoch milliseconds, safe i64 frontend range
};
type JobPage = { rows: JobView[]; next_cursor: number | null };
type IntelligenceEvent =
  | { kind: "job_changed"; job: JobView }
  | { kind: "resync_needed"; missed_events: number }; // safe u64 frontend range
```

The frozen Rust-generated 3B contract fixture confirms these **kind-tagged** snake_case events. There is no session-stream `type` tag, event sequence, job version, stream epoch or subscription ID in an intelligence event. Nullable fields must be present as null, not omitted/undefined. `get_job` returning null is missing authoritative data, not a failed/cancelled terminal JobView.

The error enum is exactly configuration, busy, cancelled, interrupted, timeout, network, authentication, rate_limited, response_too_large, invalid_response, context_too_large, unsupported_document, parse_failed, resource_limit, index_repair_required, no_final_text, storage. Commands return sanitized error strings; `intelligenceErrorSubcode` only accepts the fixed `[INTELLIGENCE_ERROR][subcode] ` prefix and validates this enum. Do not parse prose to drive behavior or invent result-payload fields. Desktop's current generic `localDesktopError` contains session-specific text; its ownership/deadline mechanism can be reused without assuming its wording is suitable for jobs UI.

Shared exact-object decoders reject missing/extra keys, credentials, invalid UUIDs, unknown enums and unsafe numbers. u32 is 0..4294967295; the frontend i64/u64 decoders use Number.isSafeInteger, not the entire Rust integer range. Timestamps are epoch milliseconds, not seconds/nanoseconds/durations. total_units=null must not produce an invented percentage; no result body, source data, text or service credential is carried in JobView.

## Subscribe, listen, snapshot: order and races

Recommended controller launch order, matching the reviewed Task 1 ownership pattern:

1. Create/admit one owned run with generation/stopped fences and an empty bounded recovery buffer. Retain the original native subscribe Promise before calling the transport.
2. Invoke subscribe_intelligence with the stable controller subscriberId. Decode and **record the valid ticket inside the original Promise continuation**, before checking whether the run is still current. A valid late ticket belongs to this run and must be unsubscribed if retired.
3. Call listen on ticket.event_name and retain its original Promise. The listener callback decodes unknown payloads and checks this exact run's generation/stopped fence. Record the returned unlisten, or call it immediately if the run retired while listen was pending.
4. Only after listening succeeds, request the initial list_jobs snapshot with after=null,limit=64. Events arriving during that read are coalesced into a bounded per-ID recovery map. A snapshot initiated before the listener would leave a missed-update window; subscribe-before-listen events are repaired by the following SQLite snapshot for the jobs it includes.
5. Merge the bounded snapshot with observed events under per-row local revision/status/progress rules below; reconcile explicitly tracked IDs absent from the page through get_job. Publish the recovered state and run the one-second scheduler. No summary observer subscribes to a session channel that already closed at session completion.

Keep the event recovery map at **at most 64 distinct job IDs**, with one coalesced notification per ID. Overflow clears/coalesces to one resync-needed flag; do not grow an event array or spin a new complete recovery for every event. Use one shared original list request at a time and at most one coalesced/latest pending list intent across recovery/history navigation; a locally timed-out list still owns that original request. Events arriving during recovery can invalidate its captured local revision and require a bounded follow-up; a sustained flood leaves recovery pending rather than spawning unbounded overlapping snapshots.

For resync_needed, treat missed_events as a lag count, not a last-seen sequence. Coalesce one fresh first/requested bounded list page **plus getter reconciliation of already tracked unfinished/uncertain IDs**, including tracked older jobs that no longer appear in that page. Preserve tracked membership according to the 64-state policy; absence from the requested bounded page is not proof that a job disappeared. Do not clear healthy existing rows merely because a recovery request failed. Do not replay service operations or call metered services as part of observation.

### Unsequenced events and same-millisecond updates

The approved lifecycle is **queued -> running -> one terminal outcome**. Terminal means succeeded/failed/cancelled/interrupted. Use terminal-absorbing and non-regressing lifecycle/progress projection for a fixed job UUID: an older queued/running payload cannot downgrade a known running/terminal job; completed_units must not visibly regress for that job. Keep immutable identity/kind/session/object associations coherent. A conflicting terminal outcome, contradictory totals/associations or same-status ambiguous data needs an authoritative tracked getter; do not select truth by timestamp or arrival alone.

`updated_at_ms` is not a version: transitions can share the same millisecond, and wall time is not an event ordering proof. Avoid `incoming.updated_at_ms >= stored.updated_at_ms` as the sole replacement rule. No speculative wire sequence/version field is required by this preparation.

Store a bounded **local row revision** with each tracked row and capture it at getter/list admission. A relevant event, operation result, newer accepted read or local retirement changes that revision. A late snapshot/read that crosses the revision fence cannot overwrite newer observed status/progress; merge safe non-regressing facts and mark ambiguity for reconciliation. The revision is a frontend fence, not a producer sequence. After an event-conflicting read is ignored, a fresh getter admitted after that event resolves uncertainty; coalesce this request rather than recursively launching a new call from every delivery.

A final one-off getter may resolve an uncertain terminal notification. It is reconciliation, not indefinite terminal polling. Once an authoritative coherent terminal JobView is accepted, remove the ID from the periodic unfinished set and queued poll work. Terminal/terminal contradictions are resolved by the authoritative read rather than combining incompatible error/outcome fields. Do not reset a terminal row to running because a delayed pre-completion event arrived.

## Bounded tracked state, history and polling

Use one stable application/workbench-owned controller and external-store snapshot/subscribe interface; views are consumers. The following are controller-internal proposals, not invented Tasks 4–7 producer APIs:

- Live/visible JobView map: <=64 IDs. Derive unfinished membership from tracked rows whose status is queued/running. Store local revisions, due timestamps and selection/pinning alongside these same bounded entries; remove associated bookkeeping on eviction.
- Admission favors explicitly observed/current operations and unfinished visible work, evicting terminal/unpinned rows first under a deterministic policy. If the view is full, further untracked events do not create a 65th retained state; expose bounded overflow/history availability. Explicitly navigating history can change membership without cancelling the underlying job. Do not retain an unbounded hidden unfinished-ID set behind the 64-row display.
- History is cursor-paged on explicit navigation, with a bounded page/window. Never walk all list_jobs pages on each poll or resync and concatenate their rows. Older jobs remain durable in SQLite.
- One recovery event map <=64, one due-ID set/queue <=64, one fair rotation cursor, and one coalesced resync flag. Reuse queue membership instead of appending a new 64-ID batch each second.

**Discovery limit:** a first 64-row snapshot and the event stream do not guarantee discovery of every old unfinished durable job, especially if its only notification occurred before listen. The wire supplies no status-filtered list command. This is compatible with polling **tracked unfinished jobs**, not a claim of complete global pending-job enumeration. Explicit history/operation-owned IDs can be tracked; this handoff does not invent a filter or whole-history crawl to claim stronger coverage.

One-second scheduler plan:

1. A single controller timer ticks every 1000 ms. Mark currently tracked unfinished IDs due, coalescing with any existing due item. If no unfinished IDs remain, periodic getter work is idle; the global listener remains active for new jobs.
2. A fair FIFO/round-robin pump admits due jobs through **one controller-global four-slot native read gate**, shared with initial/resync/history list reads and across current **and retired** runs. This conservative plan keeps all observation getters within four, not just get_job polls. Deduplicate each job's original in-flight get_job Promise across generations so a timeout or new run cannot create a second native call for that same ID.
3. When a slot is available, dequeue a still-current, still-tracked ID and issue get_job. A periodic due item requires queued/running status; a separately coalesced one-off reconciliation reason may verify uncertain terminal data. Store both reasons with the same bounded ID rather than duplicate queue entries. Four workers can drain one bounded round; do not artificially restrict to only four IDs per second. Per-ID due times prevent periodic launches faster than the one-second cadence. If native work is slower than the interval, coalesce ticks and continue fairly; do not enqueue overlapping full rounds or promise a rate the native backend cannot sustain.
4. Own the slot from admission until the **original native Promise actually settles**, on either native success or rejection. `withDesktopDeadline` may end the local wait but does not release the gate, in-flight ID, retired run or cleanup responsibility. A timed-out get stays shared/pending; later ticks skip it and use remaining slots. If four originals never settle, no fifth native getter is admitted.
5. Decode/validate non-null results and matching requested job ID. Apply only through the current generation/row revision merge. A late old-run result cannot publish into a newer run. Real null removes the row from unfinished polling and marks the local unavailable/missing state; do not synthesize terminal success or endlessly poll missing data. Real rejection reports a local observation error and makes an unfinished ID eligible on a later scheduled tick, with no immediate tight retry loop.
6. Accepted coherent terminal outcomes remove periodic due work immediately; retain only a specifically required one-off reconciliation reason if an outcome remains uncertain. A stale queued/running response cannot re-add an already terminal row. Recovery and periodic reads share admission/ownership, so repeated resync events cannot bypass the four-slot bound. No all_metadata, result-body fetch, network, embedding or session-channel polling is part of this controller.

## Strict lifetime and cleanup ownership

Carry over these reviewed Task 1 mechanics rather than copying only its interval:

- A run has generation, stopped flag, ticket, unlisten, owned original requests and local waiters. `alive` requires not stopped, exact current run and matching generation. Consumer unsubscribe only detaches its frontend change listener; native subscription ownership belongs to the controller lifecycle owner.
- Keep retired generations whose original native work or cleanup has not settled. Reuse the reviewed **MAX_OWNED_RUNS=4** admission pattern and exactly one latest-only queued start; this four-run bound is separate from the global four-getter gate. Repeated mounts/restarts must not instantiate fresh owners that reset either pending count.
- For the stable same subscriber key, do not admit a newer subscribe until an older original native subscribe settles. Otherwise the older subscribe may take effect late and replace the new ticket. Superseded local starts settle promptly; they do not add unbounded native subscriptions or queued waiters. A pending unsubscribe of an already known old ticket can coexist with a new replacement because it is UUID-targeted.
- Stop/retire first fences publication, cancels interval/admission timers and queued local starts, clears bounded recovery/poll work and calls the recorded unlisten once. It cannot cancel an entered getter/invoke by dropping its local observer.
- A late valid subscribe result is still recorded and retired by unsubscribe_intelligence for **that ticket.id**. A late listen result calls its returned unlisten immediately. An old callback/result is ignored by the run fence; ignoring it does not release its native ownership early.
- Share one original unsubscribe operation per ticket. A real native rejection may permit the reviewed Task 1 bounded retry (one immediate retry); a **local deadline never permits a second concurrent attempt**. Retain the ticket/retired run if cleanup is still pending or actually failed. Clear ticket ownership only on real native success. Do not invent unsubscribe-all commands or release another controller's subscriber ID.
- A run leaves the owned set only when stopped, all original requests settled, no ticket remains and no unlisten remains. Local stop/start deadlines provide an observer outcome, not proof that every native operation/forwarder ended. Cleanup obligations remain with the stable owner until native acknowledgement; admission stays bounded while they remain unresolved.
- Poll/native read admission is also global to this stable owner. Old-generation getters count toward four until actual settlement even if stop returned after its local deadline. Restart neither wipes the gate nor admits duplicate old-ID reads. Event subscriptions persist independently of any recording session and are not closed when a single job reaches terminal state.

## Concrete later verification targets (not executed here)

Task 8 implementation should reuse the desktop protocol fixture and fake-clock ownership tests to cover: subscribe/listen/snapshot interleavings; same-ms queued/running/terminal payloads and terminal conflicts; lag and bounded event overflow; tracked older IDs absent from the requested bounded page; real null getters; 64-state/due/history bounds; fair one-second polling with four actual native calls; timeout followed by late native effects across stop/restart; late ticket/listener cleanup; same-key subscribe serialization; old-ticket unsubscribe not cancelling a replacement; and no periodic getter for terminal jobs. Defer Tasks 4–7 command/result flows until their reviewed dispatch handoffs exist.

Verification during preparation: scoped static reads and report write/readback only. No UI/design implementation, source edit, test/Cargo/network/engine execution, git mutation or Task 4B review.

## Controller ordering correction

A later named contract check in frozen3B intelligence/store.rs:154–166 confirms `list_jobs` uses `WHERE rowid > after ORDER BY rowid LIMIT limit+1`, ascending insertion order. With after=null the result is the first bounded history page, not the newest64jobs. The commands/wire-only preparation could not establish ordering and its earlier “latest” wording was incorrect; corrected above. Keep cursor opaque and explicit history paged. Any initial/resync design consumes the requested bounded page plus already tracked IDs; it must not claim discovery of all active jobs or recent history without an actual producer API. No backend/API/source change or automatic whole-history scan is made by this correction.
