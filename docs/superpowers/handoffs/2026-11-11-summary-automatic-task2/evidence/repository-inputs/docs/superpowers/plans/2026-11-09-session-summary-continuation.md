# Session Summary Continuation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This is the already-authorized intelligence Task6, with coherent delivery boundaries; source activation waits for independently accepted Task5C repair.

**Goal:** Generate complete-input manual and automatic session summaries with durable versions, strict output, safe export and actual cancellation ownership.

**Architecture:** Deliver the manual summary flow first, including transport, fixed snapshot, full map/reduce coverage, version storage and deletion. Then add one joined automatic delivery owner using a durable lifecycle journal; the existing runtime stop publishes before its nonblocking notification, and window close waits for actual audio ownership to finish. Both use the same admission/handler/store.

**Tech Stack:** Rust, Tauri2, Tokio, existing SQLite owner/ConfigIo/JobScheduler/opaque JsonHttp, configured OpenAI-compatible LLM endpoints.

**Spec:** [Accepted design](../specs/2026-11-04-live-translation-workbench-design.md), [canonical Task6 plan](2026-11-04-workbench-intelligence.md), [archived Task6 source plan](../handoffs/2026-11-07-intelligence/evidence/task-6-source-plan.md), [concrete architecture checkpoint](../handoffs/2026-11-09-knowledge-repair/evidence/task-6-architecture-checkpoint.md), [controller reconciliation](../handoffs/2026-11-09-knowledge-repair/evidence/task-6-controller-preparation.md), and [binding interface amendments](../handoffs/2026-11-09-knowledge-repair/evidence/task-6-interface-amendments.md). The source plan supplies product requirements; the checkpoint/reconciliation supply owned internal ports. The amendments and exact interfaces below supersede only the checkpoint's conflicting proposed reap, manual-recovery, command and audio-close seams. All are required inputs for either task.

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

- Summary metadata belongs to migration6; research belongs to migration7. Existing legacy signed versions/status/seconds remain readable without fabricated native fields.
- Each Native version stores the SHA-256 of its actual admitted bounded raw job payload in a backend-only `job_payload_hash`; Legacy keeps NULL. Fresh domain checks precede snapshot, every model call, every response/root publication and terminal status copy. This witness stays outside DTOs and exports.
- Snapshot≤100000 final rows/16MiB once-normalized JSON; raw metadata parsing separately≤16MiB; acquisition/scan≤5s under an actual owner; release SQLite before HTTP.
- Every request's actual escaped system/user UTF-8 bytes+1024≤context-output. Full manifest before calls; ≤64 calls/≤8 levels; no omitted row/part/child or successful prefix.
- LLM request scopes4; request≤1MiB, HTTP envelope≤8MiB, content≤2MiB, response total≤32MiB/summary, no automatic HTTP retry. Raw and actually supplied map coverage are retained before strict parsing.
- Native content uses the seven required fields, summary≤20000 Unicode scalars, other strings≤2000, arrays≤100; strict bounded deserializers reject unknown/type/limit violations.
- Views≤100/default50 rows, native row≤2MiB+64KiB, legacy content≤2MiB, status≤64KiB, page≤8MiB; cursor canonical signed i64 decimal≤20 bytes. Legacy JSON depth64/nodes100000/decoded string+key bytes≤2MiB.
- Complete summary export projection≤32MiB; rendered artifact≤128MiB. Any completeness/size failure preserves destination. Keys/audio/raw responses stay outside export.
- Automatic delivery worker1/channel32/admission-child1/keyset page32/rescan200ms. Busy/closed/uncertain restart is local retry with no paid replay. Deletion actual-attempt registry16/wait≤10s; delivery close observation≤5s; whole close retains its30s observation fence.
- Window-close audio proof covers two actual stream/open parents and one actual inventory parent, independent of endpoint lease policy. Each Pulse stream owns two stream children and at most two shared association helpers; inventory owns at most three helpers, total OS children≤11. Slots remain held until actual join/reap and same-epoch publication end. Normal terminal notification keeps the existing bounded runtime cleanup/stop order; Failed sessions with retained finals remain eligible.

## Activation and file responsibilities

Root records the independently accepted repaired BASE before dispatch. Existing
worktree/local Cargo environment is reused; one source/Cargo author at a time.
The inactive Task6 checkpoint is never evidence that a method is already
implemented. Implementation consumes these exact current producers:
`JobContext::{operation_owner,publication_fence,commit}`,
`JobAdmissionFence::run` inside SQLite, `LocalStore::transaction` (Immediate),
`ServiceConfigStore::{scoped,resolve_snapshot_credentials}`, and
`JsonHttp::post(JsonRequest,watch::Receiver<bool>)`.

| Unit | Responsibility |
| --- | --- |
| llm/{mod,openai_compatible,tests}.rs | Owned compatible transport, full-profile resolution, bounded response/hash. |
| summary/{types,content}.rs | Strict native schema; native/legacy view and exact cursor. |
| summary/{snapshot,planner}.rs | Owned borrowed fixed input; immutable Unicode part manifest and bounded hierarchy. |
| summary/{store,handler,operations,mod}.rs | Admission/counters, raw+coverage, job handler, actual attempt registry and deletion. |
| summary/job_proof.rs | Trusted job-ID anchoring, strict Native association and independent admitted-payload witness; fresh SQL authorization at every effect. |
| summary/recovery.rs | Shared owned local deletion recovery and manual-ready epoch, independent of automatic delivery UUID. |
| summary/terminal.rs | Durable journal, bounded joined automatic delivery, startup/close. |
| migrations/0006_summaries.sql | Metadata/response/coverage/counter and lifecycle journal indexes/fields. |
| commands/summary.rs; app/{mod,intelligence,lifecycle}.rs | Command ownership, handler registration, local startup and close composition. |
| storage/{sqlite,models}.rs; export/{mod,json,markdown,plain_text}.rs | Private borrowed Deferred snapshot seam, legacy counter writer and explicit complete exports. |
| intelligence/{store,config,http,jobs}.rs; intelligence/jobs/reap.rs | Reused permanent lookup, owned selection gate, opt-in context error and the accepted nonmutating actual-reap producer. |
| intelligence/probes.rs; knowledge/embedding.rs; intelligence/http_tests.rs | Existing JsonRequest literals explicitly keep Generic error policy; existing probe/embedding behavior is retained. |
| session/orchestrator.rs | O(1) terminal reservation and same lifecycle commit, notification after stop publication. |
| audio/join.rs; audio/platform/process_io/audio_owned.rs | Fixed actual-parent/leaf registry and narrow cancel-safe audio spawn/helper/reap, independent of cached operation results. |
| audio/runtime.rs; audio/platform/{native_io,pulse_io,inventory}.rs; session/{runtime,resources,direction}.rs | Actual native/runtime join sources, failed-open/retired retention, safe replacement, Closing entry and inventory publication fences. |

### Task 1: Manual complete-input summary, durable versions and safe export

**Files:** Create llm/{mod,openai_compatible,tests}.rs,
summary/{mod,types,content,snapshot,planner,store,handler,job_proof,recovery,operations,tests}.rs,
commands/summary.rs and migrations/0006_summaries.sql under src-tauri/src
(the migration is under src-tauri/migrations). Modify lib.rs, app composition/
lifecycle/command registration, storage/sqlite.rs/models.rs, intelligence
store/config/http/jobs and their focused tests, intelligence/probes.rs and
knowledge/embedding.rs for explicit Generic request wiring, and
export/{mod,json,markdown,plain_text}.rs and their covering tests.
Keep files together by responsibility; split large tests into child modules.
Automatic terminal notification/delivery is Task2.

**Interfaces:** Own `LlmExecution { owner: OperationLease, cancel: watch::Receiver<bool> }`
with private fields/no Serialize/raw Debug and a crate-private constructor.
`LlmProvider::complete(&ModelProfile,LlmRequest,LlmExecution)->Result<LlmResponse,IntelligenceErrorCode>`.
LlmRequest fields system/user/max_output_tokens; LlmResponse content/response_hash.
`SummaryService::new(store,config,jobs,llm)` constructs an acyclic facade plus
`SummaryHandler(Arc<SummaryCore>)`. Exact public native/legacy DTO/cursor/page,
`generate_owned(session_id,profile_id,owner)`, `list(session_id,after,limit)`,
`delete_owned(summary_id,owner)` and `prepare_session_delete_owned(session_id,owner)`
are defined in checkpoint sections66–181 and886–1083. `SummaryJobRequest` and
`LocalStore::{create_summary_job,create_summary_job_in_tx}` follow sections182–305.
Task1 produces these working ports for Task2; only Task2 produces
`after_terminal_owned(session_id,AutomaticAttempt,owner)` and delivery ports.

Task1 additionally freezes these backend-private seams; their types have
private capability fields and cannot be constructed from a status/bool:

```text
SummaryJobProof::claim_in_tx(tx, trusted_job_id, claimed_payload: &Value)
    -> AppResult<SummaryJobProof>
SummaryJobProof::require_in_tx(&self, tx) -> AppResult<()>
SummaryJobProof::terminal_in_tx(tx, trusted_job_id, status, error)
    -> AppResult<Option<SummaryJobProof>>
SummaryService::reconcile_local_owned(owner: OperationLease,
    epoch: u64, reap: JobReapPort) -> AppResult<SummaryReady>
JobScheduler::reap_port() -> JobReapPort
JobReapPort::prove(tx, job_id: Uuid) -> AppResult<Option<JobReapProof>>
```

`SummaryReady` and `SummaryRecoveryFence` belong to core local recovery, bind
the admitted application/core epoch and are checked by manual generate,
summary delete and session delete. They contain no delivery UUID. Automatic
delivery extends them in Task2. Recovery success does not substitute for the
separate started/open `JobAdmissionFence`. Reuse the actual-reap producer
accepted with Task5C; the older checkpoint's bool/control-mutex `is_reaped`
proposal does not authorize a second implementation.

The command boundary is exact and keeps application-owned leases:

```text
generate_summary(session_id: Uuid, profile_id: Option<Uuid>) -> JobView
list_summaries(session_id: Uuid, after_version: Option<SummaryCursor>,
               limit: Option<u32>) -> SummaryPage
delete_summary(summary_id: Uuid) -> ()
```

Wire cursor/next_cursor are canonical signed-i64 decimal strings≤20 bytes;
`after_version=None` is unbounded below, not zero. Omitted limit means50;
explicit limits must be1..=100. Service uses `after` and a required limit.
`SummaryPage` contains `rows` and `next_cursor`; Native/Legacy remain tagged.

- [ ] Write real SQLite/job/config/HTTP behavioral RED fixtures. SummaryHarness
  composes real LocalStore/ServiceConfigStore/JobScheduler, a recording LLM
  double at the external port and controlled final transcript insertion.
  Its completed_session/final insertion, run_jobs, versions, raw_responses,
  coverage and delete methods call production collaborators. Its owner is
  a real harness process/job lease, not a production Arc<()> fallback.
  Native-only `native_versions` returns Vec<NativeSummaryView>; public lists
  still return the tagged SummaryView union.

```rust
#[tokio::test]
async fn manual_retry_preserves_prior_output_and_monotonic_versions() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["客户询问24V泵"]).await;
    h.llm.push(r#"{"summary":"24V需求","participants":[],"companies":[],"requirements":["24V"],"action_items":[],"risks":[],"terms":[]}"#);
    let first = h.generate(session, None).await.unwrap();
    assert_eq!(h.wait(first.id).await.status, JobStatus::Succeeded);
    h.llm.push("{invalid");
    let second = h.generate(session, None).await.unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(h.wait(second.id).await.status, JobStatus::Failed);
    assert_eq!(h.native_versions(session).unwrap()[0].version, 1);
    assert_eq!(h.native_versions(session).unwrap()[1].version, 2);
    assert_eq!(h.raw_responses(second.object_id.unwrap()).unwrap()[0], "{invalid");
}
```

- [ ] Observe runtime RED for manual Succeeded/Failed/Cancelled/Interrupted
  retries, concurrent manual exclusion, model switch, no model/no finals,
  deletion during actual entered work and strict JSON/raw durability. Preserve
  exact command/actual assertion/exit. New API compile wiring is recorded
  separately from behavioral RED.
- [ ] Add migration6 metadata origin/job/actual-ms/deleting/raw-byte fields,
  backend-only job_payload_hash, response/coverage/counter tables and fixed
  journal columns/indexes from the checkpoint. Native witness is exactly64
  lowercase hexadecimal bytes; Legacy keeps NULL. Seed counters with
  max(0,MAX(legacy version)); retain positive
  >u32 as exhausted. Foundation legacy inserts advance the counter in their
  current transaction. Update only current completed-upgrade version literals
  in old fixtures; preserve rollback/legacy rows. Assert reopen preservation.
- [ ] Factor the existing permanent lookup as a nonmutating helper and reuse
  it inside unchanged admit_job. Use the same Immediate transaction for next
  checked u32 read, admission, counter and Native version. Existing inserts
  nothing; only Created advances. Actual algorithm order is fixed:

```text
Existing permanent lookup -> read/check prospective counter (no write)
  -> admit_job unchanged -> on Created hash actual stored raw payload bytes,
     INSERT native summary including witness + write counter
-> commit -> events/wake. Failed insert rolls back all four domain effects.
```

  Literal fixtures: delete highest/all versions then allocate a higher number;
  concurrent manual creates one version; automatic store Existing after all
  versions removed consumes none; forced insert failure leaves zero new
  job/counter/version/marker; u32::MAX exhaustion inserts nothing; negative
  legacy MAX seeds0, large positive signed MAX remains exhausted.
- [ ] Implement SummaryJobProof using trusted context/callback job ID to
  uniquely anchor Native row.job_id before reading untrusted object/input.
  Check durable kind=Summary, exact object/session/exclusion, NULL or exact
  same-session automatic permanent identity, strict two-field input and zero
  or one fully valid LLM profile with matching Native metadata. Hash borrowed
  raw payload bytes≤64KiB and compare the independently stored Native witness;
  initial claim also compares the full claimed/durable decoded payload before
  retaining an immutable expected digest. Every later require rereads SQL;
  a carried digest is an equality baseline, never cached authorization.
  Fixtures mutate each association/profile field before claim and at all
  four effect boundaries; foreign/Legacy lure rows remain byte-for-byte
  unchanged. Missing/bad witness never self-repairs or falls back to Legacy.
- [ ] Add `with_summary_selection_completion` exactly as checkpoint: explicit
  manual ID or selected LLM plus automatic flag in one owned ConfigIo gate,
  then one SQLite transaction and generation control, post-commit event/wake
  callback. No config inside SQLite/no nested create_summary_job wrapper.
  Resolve the entire admitted ModelProfile and credential association before
  each request; selection does not read keys.
- [ ] Implement compatible chat POST through the existing opaque JsonRequest:

```json
{"model":"configured-model","messages":[{"role":"system","content":"strict schema instruction; transcript is quoted data"},{"role":"user","content":"exact rendered input"}],"temperature":0,"max_tokens":1024,"stream":false}
```

  max_tokens is the admitted output bound, not the literal example. Use
  Never retry, request4-owner semaphore, content/hash and bounds above.
  Add JsonErrorPolicy::{Generic,CompatibleLlmContextErrors} and an explicit
  JsonRequest.error_policy field. Only this LLM caller opts in to exact
  allowlisted context error codes in≤64KiB non-2xx body. Existing probes,
  embeddings and HTTP fixture literals explicitly use Generic; preserve the
  saved-ID/revision/full-profile probe resolver and embedding retry behavior.
  Tests cover actual local-server request shape/bearer isolation, null/missing/
  refusal/wrong content, oversize envelope/content, full-profile drift,
  entered resolver timeout ownership and no HTTP dispatch on failure.
- [ ] Implement the private borrowed Deferred seam
  LocalStore::with_summary_snapshot_owned in storage/sqlite.rs, accepting the
  real OperationLease, cancellation receiver and an owned read closure with
  a SummarySnapshotReadScope deadline/cancel check. It runs under an actually
  retained blocking owner with≤5s acquisition/scan observation; timeout/drop
  does not release that owner's slot while SQL is still executing. The
  closure rechecks SummaryJobProof, pins the snapshot with its first session
  read and returns only bounded owned rows. summary/snapshot.rs never accesses
  the private connection or nests the existing Immediate transaction.
  No eagerly loaded session page is used. SQL and limits are literal:

```sql
SELECT MAX(rowid) FROM translation_segments WHERE session_id=?1 AND is_final=1;
SELECT rowid,id,session_id,direction,sequence,occurred_at,language,text,
       is_final,details_json,occurred_at_ms
FROM translation_segments
WHERE session_id=?1 AND is_final=1 AND rowid<=?2 ORDER BY rowid LIMIT 100001;
```

  Borrow ValueRef/type/UTF-8/byte lengths before copies. Normalize the exact
  versioned positional JSON and safe identity allowlist defined in checkpoint
  sections652–783 through a count+SHA writer, once; raw details remain local.
  Missing legacy kind/actual ms is None. Deadline/cancel checks precede every
  row/charge. Verify bundled SQLite plan uses the session/rowid index without
  global scan/sort. Fixtures: late final absent/present in successive snapshots,
  cross-direction insertion order,100001/16MiB overflow before calls, escaped
  body/metadata bounds and complete identity hash.
- [ ] Implement pure planner and strict bounded content visitors. Immutable
  PartKey/PartRef/Manifest/RequestPlan/SummaryNode shapes and normalization are
  checkpoint sections784–885; every tuple/range is fixed before calls. Pack
  complete adjacent input, Unicode-split individual rows, account actual
  escaped envelopes, reserve complete levels against64/8, carry singleton
  nodes, require strict frontier decrease. Literal tests reconstruct Chinese,
  emoji, combining/quoted/newline text; middle-only part omission fails even
  with equal distinct segment IDs; prospective call65/level9/64 maps/no-pair
  all fail before illegal dispatch. Parsed output never sets its own coverage.
- [ ] Run the owned handler sequence, retaining raw and actual map tuples in
  one fenced transaction before parsing each result:

```text
fresh domain proof + Running/nondeleting check -> mark this Native Running
-> capture whole snapshot -> persist snapshot hash/count/high-water
-> freeze complete manifest/map partition -> for every map call:
   fresh domain proof -> owned complete
   -> fresh domain proof + retain exact raw/hash/actual coverage -> strict parse
-> reduce all adjacent complete children with reserved levels/calls
-> compare streamed persisted tuples to the full immutable manifest
-> fresh domain proof + store strict complete root with Running/nondeleting fence
-> scheduler terminal_tx copies exact job status/error to this version.
```

  Response ordinal0..63/phase map or reduce/raw≤2MiB,total≤32MiB; storage,
  cancellation or bound failure retains earlier rows and cannot publish root.
  Test exact supplied full coverage, invalid map/reduce raw/hash, total breach,
  cancellation/deletion/storage fence, growing reductions and delayed LLM.
- [ ] In terminal_tx, stay inside the supplied post-transition transaction;
  validate exact durable status/error and fresh trusted-ID Native proof, then
  copy only status/error to that same row. Queued cancellation and startup
  interruption use the independent witness without ConfigIo/Running commit.
  Valid deleting rows permit status copy without content/raw/coverage writes.
  No Native target is a safe no-op; an existing malformed Native target is a
  safe Storage failure with no foreign write. Do not re-lock held control,
  INSERT a replacement or claim persistence when Err rolls back transition.
  Exercise actual scheduler queued/startup/normal terminal paths, not direct
  callback invocation alone. A changed kind may bypass Summary hook; its test
  requires zero foreign effects without inventing successful status copy.
- [ ] Implement actual-attempt registry16 and reuse the independently accepted
  exact-generation JobReapPort::prove. Proof checks its separate actual-claim
  registry within the supplied SQLite transaction and does not reacquire the
  terminal control mutex. Missing frame/generation alone proves nothing;
  inert/current-complete-ledger/exact-joined lifecycle authority remains
  distinct. Async summary/session deletion persists fences,
  cancels outside SQLite and waits≤10s for both actual children and claimed
  handler reap before hard DELETE. Timeout retains row/fence; startup resumes
  locally under process proof. Tests include queued claim race, terminal but
  owned resolver, close/rearm, late result never repopulating and counter /
  permanent marker survival.
- [ ] Implement shared SummaryReady/SummaryRecoveryFence and
  reconcile_local_owned in summary/recovery.rs. Before worker/manual admission,
  AppState registers SummaryHandler, performs existing generic interruption
  under genuine fresh-process or actually joined prior ownership, and awaits
  local summary deletion recovery with its real process/startup lease and
  reap port. No automatic admission, config guess or LLM call occurs here.
  Failed/observed-timeout recovery keeps readiness closed and its actual
  owner alive; startup-failure close waits before direct16/index retirement.
  Task1 fresh-process and same-process reopen fixtures recover deletion and
  reject claimed-unentered/entered-old work. Task2 adds its journal bootstrap
  before publishing delivery-ready; it is not a prerequisite for this manual
  deliverable.
- [ ] Implement native/legacy SummaryView/page and explicit bounded export
  projection from checkpoint. Preserve original signed decimal cursor/status/
  seconds/JSON; Native never falls back to Legacy. Render every native field,
  action owner/due and term meaning in JSON/Markdown/text. Exports query no raw
  response body and fail whole on reserved raw fields/limits. Fixtures cover
  both readable legacy shapes, negative/>u32/>JS-exact cursors, partial invalid
  legacy projection, native limits, all fields and raw/key/audio sentinels.
  Real command/service fixtures include omitted limit, rejected noncanonical
  cursors, signed large exact cursors and tagged Native/Legacy wire projection.
- [ ] Observe covering GREEN with real fixtures; format owned source and
  report exact commands/results/logs. Root freezes/reviews this complete
  manual deliverable and commits task paths after independent acceptance.

```bash
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib summary
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib llm
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib export
```

### Task 2: Exactly-once automatic delivery and joined audio close

**Files:** Create summary/terminal.rs, audio/join.rs,
audio/platform/process_io/audio_owned.rs and focused actual-join /
terminal/startup/close tests; modify summary/{mod,store,recovery}.rs,
session/{orchestrator,runtime,resources,direction}.rs, storage/sqlite.rs,
audio/runtime.rs, audio/platform/{native_io,pulse_io,process_io,inventory}.rs,
app/{mod,intelligence,lifecycle}.rs and the current audio/session/process
fixtures affected by these internal ports. Reuse Task1 transport/planner/
handler/version/deletion/export and the existing capture/playback native
drop-before-worker-join behavior.

**Interfaces:** Consumes Task1 SummaryService/store/owned LlmProvider and
JobAdmissionFence/event/wake ports. Produces the exact checkpoint
`AutomaticAttempt::{Live,LocalRetry}`, `SummaryLocalFence`,
`SessionTerminalPort::try_reserve()->TerminalReservation`,
`TerminalReservation::{intent,committed}`,
`LocalStore::commit_terminal_lifecycle(&StoredSessionLifecycle,&TerminalCommitIntent)`,
`SummaryService::after_terminal_owned(session_id,attempt,owner)` and the one
retained SummaryDeliveryOwner. All capabilities are backend-private; frontend
commands keep their Task1 shapes. Field/order/ownership definitions in
checkpoint sections66–181/374–563 and the interface amendments are binding.
Exact lifecycle composition also produces these opaque ports:

```text
SummaryDeliveryOwner::start_owned(owner: OperationLease, epoch: u64,
    local_ready: SummaryReady) -> AppResult<SummaryDeliveryReady>
SummaryDeliveryOwner::signal_closing(key: &AudioCloseKey) -> AppResult<()>
SummaryDeliveryOwner::close_join_owned(owner: OperationLease,
    key: AudioCloseKey, audio: AudioCloseJoin) -> AppResult<()>
AudioBackend::begin_close(key: AudioCloseKey) -> AppResult<AudioBackendCloseJoin>
AudioBackendCloseJoin::wait() -> AppResult<AudioBackendCloseAck>
AudioBackend::reopen_after_close(ack: &AudioBackendCloseAck) -> AppResult<()>
AudioControl::actual_join_port() -> AppResult<AudioOwnerJoin>
AudioOwnerJoin::wait() -> AppResult<AudioOwnerAck>
ProcessSpawner::spawn_audio_owned(spec: ProcessSpec,
    execution: AudioLeafExecution) -> AppResult<AudioOwnedProcess>
output_audio_owned(spawner: &dyn ProcessSpawner, args: &[&str],
    execution: AudioLeafExecution) -> AppResult<Vec<u8>>
AudioProcessJoin::wait() -> AppResult<AudioProcessAck>
SessionOrchestrator::capture_close_join(key: AudioCloseKey)
    -> AppResult<SessionCloseJoin>
AudioCloseJoin::wait() -> AppResult<AudioCloseAck>
```

Rust types crossing the existing public audio traits expose only private
capability fields and crate-private producer constructors, with no Serialize
or raw Debug. Unsupported trait defaults fail closed. AudioCloseKey binds
the elected application close epoch, exact delivery UUID and audio backend
epoch. AppState retains one AudioCloseJoin and elected actual close owner;
retry observes that same key/UUID. SessionCloseJoin covers real actor join,
terminal lifecycle/bus/stop/active-release and final notification producer.
AudioBackendCloseJoin independently covers its entered native owners. Only
the actual audio close owner combining both sources constructs AudioCloseAck;
the delivery branch cannot construct it from status, stop result or no active.
Audio stop/result/diagnostic/health failures stay distinct from actual join.
This actual ack is the window-close barrier. Normal terminal notification
remains O(1) after existing bounded runtime cleanup, durable lifecycle commit
and stop publication; it does not wait for unrelated inventory or LLM.

- [ ] Write and observe actual RED using a fixture runtime/orchestrator plus
  real store/service: closed session bus, durable Completed/Failed callback,
  diagnostics failure after durable lifecycle, delayed/noncooperative LLM,
  duplicate/reopened automatic marker, overflow/close/uncertain startup and
  whole-session deletion. Stop returns before model release:

```rust
#[tokio::test]
async fn audio_stop_completion_precedes_delayed_automatic_llm() {
    let h = SummaryHarness::runtime().await;
    let session = h.running_session(&["客户询问24V泵"]).await;
    h.llm.hold_next();
    let stopped = h.stop(session).await.unwrap();
    assert_eq!(stopped.status, SessionStatus::Completed);
    h.llm.wait_entered().await;
    assert_eq!(h.versions(session).unwrap().len(), 1);
    h.llm.release();
    h.wait_automatic(session).await;
    assert_eq!(h.llm.request_count(), 1);
}
```

- [ ] Implement O(1) reservation: current immutable delivery UUID/live flag,
  sender.clone().try_reserve_owned, coalesced Notify; Full is local Busy,
  Closed/closing/no live generation is local Interrupted. No callback await,
  config read or spawned per-session task. A retained permit counts inside32.
- [ ] Change only the existing terminal lifecycle write to set Completed/Failed,
  ended_at, pending, delivery ID and local error in that same durable UPDATE;
  replay of terminal status preserves its existing journal. Interrupted runtime
  recovery records local Interrupted in its current transaction. Notification
  is sent only after the existing bus close/stop completion/active release,
  and only on successful lifecycle commit. Later diagnostic failure does not
  erase that committed intent. Reservation code has these fixed branches:

```text
live + owned slot -> intent(current UUID,None)
live + Full -> intent(current UUID,Busy)
closing/closed/no generation -> intent(local UUID or None,Interrupted)
failed lifecycle write -> drop permit, send nothing
successful lifecycle write -> after stop publication, permit.send(UUID)
```

- [ ] Implement actual backend ownership before native entry: two stream/open
  parent slots plus one inventory slot, preserving the existing pair/native
  inventory capacities. Separate actual slots from lease/quarantine policy.
  CPAL retains the outer-thread handle and joins it after native drop/workers;
  Pulse retains both stream children, workers and monitor plus a single shared
  two-helper association group across open and monitor. Inventory uses one
  CPAL/Pulse root gate and its fixed three-helper Pulse group. No new group,
  poll or replacement open enters while the old actual group/parent is pending.
  Old parent join precedes replacement; no third stream or borrowed inventory
  slot. Unknown cleanup retains its original actual capacity.
- [ ] Implement spawn_audio_owned/output_audio_owned with a pre-issued opaque
  AudioLeafExecution, leaf registration before OS spawn and an independent
  positive exit/reap port. An observer returning Err/no pipes keeps the actual
  entered child/group/parent owned until wait acknowledges exit. Nonzero exit
  proves exit while result remains failed. Preserve the existing helper byte/
  observation bounds and unrelated ordinary/limited/driver call behavior.
  Fixture implementations explicitly provide genuine controlled sources;
  missing source defaults fail closed. Test spawn observer loss, killed/wait
  error followed by late exit, one held helper across repeated ticks and one
  held inventory helper across a new inventory request.
- [ ] Fence backend inventory/open Ready+epoch before slot/native entry and
  recheck after waiting; Closing rejects direct endpoint/status/validation
  reentry as well as new resume/open/poll. Original scopes retain teardown
  capabilities. Inventory parse/record stays owned and checks the same epoch
  before publication, so late native results cannot overwrite a new snapshot.
- [ ] Implement one process-owned joined delivery worker/channel32/child1 and
  keyset scans32+sentinel using pending/status indexes, rescan200ms. Dequeueing
  is a hint; journal/permanent marker/session/delete/current generation are
  rechecked inside owned config→SQLite→delivery→job control before admission.
  Native live starts Queued only after successful ready; local retry cannot
  produce paid work. A10s ConfigIo observer timeout does not release its child
  or dispatch the next admission before actual release/late commit check.
- [ ] Share Task1 counter/admit helper: automatic Existing always wins; enabled
  missing marker creates Queued on normal live finals or Failed local Busy /
  Interrupted/NoFinalText/Configuration atomically with version/counter/marker.
  Automatic disabled clears the pending journal and creates nothing. Manual
  retry always creates a new job/version after terminal, permanent_key=NULL.
  Replay/deleting automatic output never creates another automatic model call.
  When config is unreadable, a trusted session/permanent identity may perform
  only read-only Existing replay, including after Native output deletion;
  otherwise retain pending and fail readiness rather than guess settings.
  Counter exhaustion records a truthful local journal error when possible and
  creates no job/version/marker. Normal live preflight precedence is
  NoFinalText -> missing/invalid LLM Configuration -> unfinished/exclusion Busy.
- [ ] Bootstrap local-only summaries before opening delivery/job/runtime
  admission. Scan both Completed and Failed, including migration-era pending0
  and interrupted runtime finals; enabled missing automatic marker creates
  one visible Failed/errorInterrupted retry and zero calls across second
  reopen. Forced local Interrupted/Busy takes precedence over NoFinalText and
  Configuration; uncertain history always keeps Interrupted rather than
  replacing it with a new preflight diagnosis. Existing is checked first;
  unreadable config/storage keeps the exact pending journal/readiness fenced.
  start_owned consumes Task1's local-ready capability and retained process
  owner, performs this bootstrap in Bootstrapping and publishes the exact
  SummaryDeliveryReady only with current application/job/delivery readiness.
- [ ] Integrate close in its existing election: Closing delivery first, prompt
  audio stop in parallel; capture same-key runtime/native completion sources
  before stop and retain the AudioCloseJoin in CloseState. Complete the
  admission-fenced final audio pass and actual source joins; finish accepted
  jobs through scheduler shutdown. Backend native producers are required,
  including failed-open/no-ports, retired directions, blocked native drop and
  positive process reap after observer errors; cached close results and
  kill_on_drop are not join proof. Only the actual owner publishes the ack;
  after actual audio acknowledgment close receiver and drain every queued /
  pre-issued OwnedPermit to EOF; final local journal tail and child1 barrier;
  join delivery plus jobs before direct16 barrier/index close. Observe≤5s /
  whole30s without discarding handles/leases. Retry joins the same UUID;
  late old callbacks cannot retire or repopulate a new generation.
  jobs/probes/audio errors are retained after progressing both owned close
  branches; no early jobs? return abandons delivery. Preserve native quarantine
  and NeedsRestart independently of any later actual-join receipt.
- [ ] Observe GREEN for all above and current session/intelligence/summary
  interactions. Cases include channel32/33 overflow, stale generation, close
  before/after reservation and commit, late entered config, summary/session
  deletion, permanent Existing under unusable config, failed automatic manual
  retry and empty terminal transcript. No paid/native tests.
  Hold native factory/drop/ordinary process exit after actor completion and
  active release; ack stays pending until actual source release. Exercise
  failed-open no-ports, nonzero exit/EOF/unacknowledged worker, cancelled
  monitor/helper and inventory observer, diagnostic failure with actual join,
  jobs shutdown failure, same-key close retry and late pre-issued permit send.
  Default tests use controlled real producer collaborators and synthetic
  native/process ports; they do not open actual hardware or paid services.
- [ ] Freeze final complete Task6 source; Root runs all-targets/check/desktop/
  fmt and affected frontend contract/build checks, then one independent
  Task6 integration review. Archive source/log/verdict, transfer DTO/cursor/
  Retry→manual handoff to Task8/9, and commit only independently accepted paths.

```bash
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib summary
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib session
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib intelligence
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --offline --locked --manifest-path src-tauri/Cargo.toml --lib
cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --bin vox-bridge
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

## Execution environment and self-review

All Cargo commands use the same task-local CARGO_HOME/CARGO_TARGET_DIR/TMPDIR/
ALSA overrides recorded at accepted BASE; default examples above are command
suffixes, never permission to bypass that environment. Root confirms writable
TMPDIR and no other Cargo owner. Each task report includes actual covering
RED/GREEN and frozen handback; Root owns final gates/review/integration.

Coverage: canonical Task6 step1 fixtures map to both manual Task1 and automatic
Task2; step2 persistence/raw/coverage/counters/deletion and step3 snapshot /
hierarchy / transport map to Task1; step4 terminal/exactly-once/startup/audio
independence maps to Task2; step5 final integration verification maps to Task2.
Legacy truthful projection/export and actual ownership reconciliation map to
Task1; joined terminal ownership maps to Task2. No approved requirement is
removed. All shared ports are named in the checkpoint, binding interface
amendments and consuming Interface sections above. This plan is queued until
the Task5C repair gate.
