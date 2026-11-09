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

