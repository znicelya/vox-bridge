# Task6 binding interface amendments — 2026-11-09

This continues the approved summary feature. It closes the proposed internal
ports in the [architecture checkpoint](task-6-architecture-checkpoint.md)
against its actual producers; it does not claim implementation or test success.
The [active plan](../../../plans/2026-11-09-session-summary-continuation.md)
contains two independently reviewable deliveries: complete manual summaries,
then automatic lifecycle integration. Those deliveries supersede the older
controller preparation's three internal-layer activation proposal. The
independently accepted Task5C repair BASE must still be recorded before source
activation; the observed `b725560` is not that acceptance.

The preserved input reports are [plan closure](task-6-plan-closure-review.md),
[audio producer audit](task-6-audio-close-proof-preflight.md) and
[bounded audio scope](task-6-audio-close-scope-followup.md). Their line
numbers and hashes describe their input snapshots, not the amended plan.

## Native domain authority and admitted payload — A6-7

`summary/job_proof.rs` belongs to the manual delivery. Its private proof
anchors the callback/context's trusted job UUID to exactly one Native
`session_summaries.job_id`; untrusted object/payload IDs never select another
target. The proof freshly checks durable kind, object, session, exact
`summary:<session UUID>` exclusion, NULL or exact same-session automatic
permanent identity, strict `{summary_id,session_id}` input and zero or one
complete valid LLM profile. Profile presence/ID/model must agree with the
Native metadata. Missing profile is a local configuration failure, never
permission to dispatch a model.

Migration6 adds backend-only nullable `job_payload_hash`. Legacy keeps NULL;
Native requires exactly64 lowercase hexadecimal SHA-256 bytes. Only the
Created branch computes this hash from the helper's actual stored
`intelligence_jobs.payload_json` borrowed UTF-8 bytes≤64KiB, and inserts it
with job/profile/model/version in that same Immediate transaction. Existing
changes no witness, counter or version. Do not hash reserialized JSON or put
the witness into a DTO/export.

```text
SummaryJobProof::claim_in_tx(tx, trusted_job_id, claimed_payload: &Value)
    -> AppResult<SummaryJobProof>
SummaryJobProof::require_in_tx(&self, tx) -> AppResult<()>
SummaryJobProof::terminal_in_tx(tx, trusted_job_id, status, error)
    -> AppResult<Option<SummaryJobProof>>
```

At first claim binding, compare complete strictly decoded claimed and durable
payloads and the independent Native raw-byte witness before retaining the
immutable expected digest. At every later authorization, reread durable SQL
and compare that same baseline. This is not cross-transaction cached proof.
Fresh checks are required before snapshot, every `complete`, response/coverage
insertion, root publication and post-transition terminal copy.

The terminal hook uses only its supplied transaction. It validates exact
durable terminal status/error, then updates only the anchored Native row's
status/error. Valid Cancelled/Interrupted deleting rows retain their fence
and receive no raw/content/coverage write. No Native target is a safe no-op;
an existing malformed Native target fails with safe Storage and zero foreign
writes. No Running-only commit, nested control lock, ConfigIo, filesystem,
replacement INSERT or Legacy fallback is allowed. If hook Err rolls the
generic transaction back, report that failure truthfully. A changed durable
kind can prevent this hook from running; its regression requires no foreign
effects rather than a fabricated successful status copy.

The witness detects the stated localized single-record payload/profile drift,
including admission-before-claim drift. It does not claim detection of a
consistent replacement of both job and independent witness. Same-session
manual/automatic provenance beyond the reviewed permanent identity contract
is not a new promise.

## Shared manual recovery and actual reap — A6-9 / A6-10

The manual deliverable owns `summary/recovery.rs`, `SummaryReady` and
`SummaryRecoveryFence`. These private capabilities bind the core/application
epoch and contain no automatic-delivery UUID. The working port is:

```text
SummaryService::reconcile_local_owned(owner: OperationLease,
    epoch: u64, reap: JobReapPort) -> AppResult<SummaryReady>
```

AppState registers SummaryHandler before its existing generic interruption.
Its retained startup owner then performs asynchronous local deletion recovery
before opening manual/job admission. The genuine fresh-process or exact
joined-prior lifecycle authority and the existing application close/startup
gates are required; absence of a generation is not sufficient. The local
recovery path creates no automatic version and makes no LLM call. Failure or
observer timeout leaves readiness closed and actual entered work owned;
startup-failure cleanup waits for it before direct16/index retirement.

Manual generate/delete/session-delete check the current core-ready epoch.
Generating also requires the independent started/open job admission fence;
SummaryReady alone cannot prove the scheduler is open. Task2 reuses this
local readiness and adds its own automatic journal bootstrap and delivery
UUID/stage fence before publishing delivery-ready.

Reuse the actual-reap producer accepted with Task5C:

```text
JobScheduler::reap_port() -> JobReapPort
JobReapPort::prove(tx, job_id: Uuid) -> AppResult<Option<JobReapProof>>
```

This supersedes the checkpoint's proposed bool `is_reaped` and captured
control-mutex implementation. Issuance only clones the private registry and
is infallible; verification errors belong to prove. The producer reads the supplied SQLite
transaction plus its separate actual-claim registry. It does not reacquire
terminal_tx's held generation control, mutate status or start/stop work.
Every genuine claim is registered before Running publication. Complete
current claim accounting, a safely pruned exact frame, genuine inert lifecycle
and exact fully joined lifecycle are explicit authority; missing frame,
rowid frontier, terminal status or generation=None alone is not authority.
Handler end, permanently closed owner factories and zero actual child scopes
precede pruning; process/slot release precedes child completion.

The private bounded AttemptRegistry16 separately proves all summary child
work ended. Async deletion requires both it and the actual-reap proof, then
rechecks both in the hard-delete transaction under the persistent summary or
session deletion fence. The10s deadline bounds observation and keeps the
row/fence on timeout. Queued claim-before-entry, entered resolver after
observer loss, fresh reopen and joined same-process reopen are real tests.
Automatic output deletion still leaves the permanent job/marker; a later
read-only Existing replay is valid even with no Native row or usable config.
It creates no new job/version/counter/model call.

## Borrowed snapshot seam

The manual deliverable implements this narrow seam inside `storage/sqlite.rs`;
`summary/snapshot.rs` cannot access the private connection or nest the existing
Immediate transaction:

```rust
pub(crate) async fn with_summary_snapshot_owned<T: Send + 'static>(
    self: &Arc<Self>,
    owner: OperationLease,
    cancel: watch::Receiver<bool>,
    read: impl FnOnce(
        &rusqlite::Transaction<'_>, &SummarySnapshotReadScope,
    ) -> AppResult<T> + Send + 'static,
) -> AppResult<T>;
```

`SummarySnapshotReadScope` belongs to the storage seam, has private deadline
and cancellation state and exposes `check() -> AppResult<()>`. Acquisition
and scan observation share5s. A bounded real blocking owner retains its
lease until the connection/transaction actually releases, including timeout
or dropped observers. The seam opens one Deferred transaction; its closure
freshly checks domain proof and the first indexed session read/high-water,
then checks each
borrowed ValueRef/type/UTF-8/charge before copying. It returns only bounded
owned rows; the SQLite transaction ends before ConfigIo, rendering or HTTP.
The existing100000-row,16MiB normalized, separate16MiB metadata and complete
identity/hash requirements remain binding. Query-plan fixtures prove the
session/rowid index avoids global sorting/scanning.

## Actual backend audio close — A6-8

The window-close ack scope is the elected audio backend generation and its runtime /
terminal-notification producers, including failed-open, retired and unattached
inventory work. Retain at most two stream/open parents (the existing two
endpoint pairs/four names) and one inventory parent (the existing native
inventory gate, applied to both CPAL and Pulse). These are actual ownership
slots, independent of endpoint lease/quarantine policy.

Each Pulse stream parent owns two long-lived stream children and one shared
association group with at most two helpers. Open/readiness and monitor reuse
that same group; no next poll/pair starts until both old helpers actually
reap. Inventory owns its fixed sources/sinks/modules group of at most three
helpers. Thus concurrent Pulse OS children are bounded by
`2*(2+2)+3=11`. Failed/cancelled observers cannot release slots to accumulate
more children. Native inventory covers its entire blocking closure and safe
post-read parse/record publication under the same parent.

Freeze the narrow owned-audio process seam rather than changing unrelated
limited-parser or privileged driver behavior:

```text
ProcessSpawner::spawn_audio_owned(spec: ProcessSpec,
    execution: AudioLeafExecution) -> AppResult<AudioOwnedProcess>
output_audio_owned(spawner: &dyn ProcessSpawner, args: &[&str],
    execution: AudioLeafExecution) -> AppResult<Vec<u8>>
AudioOwnedProcess -> private ProcessPipes + AudioProcessJoin parts
AudioProcessJoin::wait() -> AppResult<AudioProcessAck>
AudioControl::actual_join_port() -> AppResult<AudioOwnerJoin>
AudioOwnerJoin::wait() -> AppResult<AudioOwnerAck>
AudioBackend::begin_close(key: AudioCloseKey) -> AppResult<AudioBackendCloseJoin>
AudioBackendCloseJoin::wait() -> AppResult<AudioBackendCloseAck>
AudioBackend::reopen_after_close(ack: &AudioBackendCloseAck) -> AppResult<()>
SessionOrchestrator::capture_close_join(key: AudioCloseKey)
    -> AppResult<SessionCloseJoin>
AudioCloseJoin::wait() -> AppResult<AudioCloseAck>
```

The spawn/output functions are async. `AudioLeafExecution` is an opaque
pre-issued capability for the original parent/group and actual leaf slot.
Register actual ownership before OS spawn/native entry, including a spawn
observer that fails without returning pipes. The actual child owner retains
its handle after kill/wait observation errors and only publishes positive
exit/reap after actual wait and descriptor/owner release. Nonzero exit can
prove child exit while the ordinary operation remains failed. Cached error,
EOF, kill_on_drop or monitor/task return is not that proof. Defaults without
an actual producer fail closed; product code has no Arc<()> success fallback.
Prefer `process_io/audio_owned.rs` for this narrow implementation and keep
unrelated `spawn`/`spawn_limited` behavior and their existing callers intact.

`audio/join.rs` owns the opaque parent/leaf/join types and fixed slot registry.
Types returned by existing public Rust traits have private fields and
crate-private producer constructors, no Serialize/raw Debug. Completion
receivers must not strongly retain native command senders whose disconnect
cleanup they await. CPAL retains and actually joins its outer thread after
native destructor and capture/playback worker teardown. Pulse retains and
joins workers, monitor and every owned stream/helper child's actual source.

Replacement suppresses/closes the old stream, then waits for its actual
parent join before reusing the pair and entering a new open. An old close
reply alone is insufficient. No third stream parent or borrowed inventory
slot is allowed. No-ports failure and aborted acquisition still retain their
independently registered backend parent.

At backend entry, validate Ready/same epoch when taking a slot and again
after any wait before native entry. Closing atomically rejects new external
inventory/open, including endpoint/driver-status/validation paths that bypass
session admission. Existing teardown only uses original cancel/discard/drain/
join capabilities; it cannot create resume/open/inventory/poll work. Late
inventory parse/record must remain under its owner and recheck the same epoch
before publishing; a completed native child must not let an old observer
rewrite a newer inventory snapshot after ack.

AppState's existing close election constructs a key from the elected close
epoch, exact delivery UUID and backend epoch. Before jobs/probes waits and
early stop, synchronously signal audio/delivery Closing, capture runtime /
backend sources and retain one same-key AudioCloseJoin plus its actual owner
in CloseState. Run early stop, admission-fenced final stop, then await real
actor/native/lifecycle/notification producer joins. Only that actual audio
close owner combining SessionCloseJoin and AudioBackendCloseJoin constructs
AudioCloseAck. Outcome/status/stop result/active=None cannot construct it.

The actual-join receipt remains independent of diagnostic/storage/result
errors and sticky quarantine/NeedsRestart. A genuine joined receipt can
coexist with a Failed result; unknown native completion cannot. Observation
timeout/drop keeps the same elected key/UUID and actual owner available for
retry, and admission/rearm stays fenced.

This stronger ack applies to window close and delivery drain. Normal session
termination retains the existing bounded runtime cleanup, durable lifecycle,
bus/stop/active-release and O(1) UUID notification order. It does not wait for
unrelated inventory or LLM, and Failed sessions with retained final text keep
their approved automatic eligibility. Adding a native backend join wait to
the ordinary stop result is not part of this amendment.

```text
SummaryDeliveryOwner::start_owned(owner: OperationLease, epoch: u64,
    local_ready: SummaryReady) -> AppResult<SummaryDeliveryReady>
SummaryDeliveryOwner::signal_closing(key: &AudioCloseKey) -> AppResult<()>
SummaryDeliveryOwner::close_join_owned(owner: OperationLease,
    key: AudioCloseKey, audio: AudioCloseJoin) -> AppResult<()>
```

start/close are async; signal is synchronous O(1). The retained delivery
owner consumes core-ready, bootstraps local-only history before live-ready
and keeps its single child through actual ConfigIo release. close waits for
the same-key actual audio ack, then closes the receiver and drains every
queued/pre-issued OwnedPermit to EOF, performs indexed local journal tail
and child1 barrier, and publishes exact-UUID delivery completion. Jobs/probes
errors cannot abandon either close owner. Both jobs and delivery join before
direct16/index retirement. Delivery5s and whole-close30s remain observation
limits; no timeout releases actual work or authorizes a new generation.

Future Task2 fixtures must hold old association helpers across repeated ticks,
one inventory helper across another inventory request, failed-open no-ports,
native close reply before outer-thread join and a late old inventory record.
They also exercise Closing queued entry, genuine joined/diagnostic failure,
nonzero/EOF/unacknowledged worker, jobs shutdown error, same-key close retry
and a pre-issued permit sent after active release. These use controlled real
producer collaborators, no hardware/native keyring/paid endpoint. Existing
capture/playback internals are reused; this amendment does not claim their
real platform acceptance.

## Compatible HTTP and commands

Freeze `JsonErrorPolicy::{Generic,CompatibleLlmContextErrors}` and the explicit
`JsonRequest.error_policy` field. Only compatible LLM requests opt in to a
bounded64KiB non-2xx exact-code context-error classifier; no raw body enters
errors. Never retry remains independent. The old literals in
`intelligence/probes.rs`, `knowledge/embedding.rs` and
`intelligence/http_tests.rs` explicitly use Generic. Preserve the probe's
saved-ID/revision/full-profile resolver and existing embedding behavior.

```text
generate_summary(session_id: Uuid, profile_id: Option<Uuid>) -> JobView
list_summaries(session_id: Uuid, after_version: Option<SummaryCursor>,
               limit: Option<u32>) -> SummaryPage
delete_summary(summary_id: Uuid) -> ()
```

The command wrapper retains existing AppState error/lease conventions.
`SummaryPage { rows, next_cursor }` uses the tagged Native/Legacy view. Both
cursor fields are canonical signed-i64 decimal strings≤20 bytes. None means
no lower bound, not zero; omitted limit is50 and explicit limits are1..=100.
Real command/service fixtures exercise these fields and exact large/signed
values. Export retains its existing legacy numeric version/seconds/status
shape; frontend cursor strings do not rewrite exported legacy numbers.
Use the real `export/plain_text.rs` path. In Native-only test cases,
`native_versions` returns `Vec<NativeSummaryView>`; the public list still
returns the tagged union.

## Local failure precedence and permanent replay

Forced LocalRetry or uncertain startup uses Interrupted/Busy before
NoFinalText/Configuration. Enabled old Completed/Failed history with no
automatic marker gets one visible Failed/error=Interrupted record, zero LLM
calls and permanent Existing on second reopen. An unreadable config file
cannot establish whether automatic is enabled; retain pending/readiness.

Normal live admission uses NoFinalText, then missing/invalid-model
Configuration, then unfinished-capacity/exclusion Busy. ContextTooLarge
belongs to the admitted Queued version's complete snapshot/handler; do not
extend the generic initial-failure allowlist. Permanent Existing precedes
configuration selection failure and counter allocation. If configuration is
unreadable, only the trusted session/permanent identity's read-only Existing
lookup may succeed; never guess a new configuration.

An exhausted counter creates no job/version/marker. Preserve a truthful
local journal ResourceLimit indication when storage permits, rather than
inventing a failed summary version. Automatic disabled consumes pending and
creates nothing. Manual Retry always creates a new manual version and keeps
the original automatic marker.
