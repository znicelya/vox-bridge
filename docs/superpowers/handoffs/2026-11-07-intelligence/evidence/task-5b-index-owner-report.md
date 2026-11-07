# Task 5B index owner implementation report

Status: **implemented, independently reviewed, and committed as `99cc3b3`.**

BASE: `c8f77708ce959f9ab3e454147329fcff3771cc37`

Branch/worktree: `codex/vox-bridge-implementation` at
`/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation`.

This report covers only Task 5B: the owned SrvDB generation facade, strict
metadata/reference codec, durable SQLite intent/receipt/ack fence, trusted
preflight and close protocol, bounded owner queue and Rayon pool, generation
maintenance copy, and AppState startup/close/rearm integration. Task 5C
embedding transport, public document search/reindex/delete commands, automatic
import indexing, reconciliation handlers and user-facing results remain open.
Whole Task 5 is therefore not complete.

No real paid endpoint, native keyring, physical audio device, system repair,
merge, push or publication was performed. The paid Volcengine integration test
remained ignored. The existing unowned `.gitignore` edit remains outside the
Task 5B source package.

## Implemented behavior

- Added the exact approved SrvDB git revision
  `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`; the selected transitive redb is
  `4.4.0`. Added direct bounded Rayon, CRC32 and XXH3 support for the private
  writer/pool/header policy. No path dependency or upstream source edit is used.
- Added canonical `EmbeddingIdentity` validation and semantic keying. A local
  actual-dimension discovery remains representable with
  `requested_dimension: None`; profile execution-only fields do not change a
  vector-space identity.
- Added schema2 `chunk_location_ref_v1` with strict canonical UUID/hash/codec
  decoding, nullable inline page distinct from missing fields, and a streamed
  SHA-256 over the complete canonical SQLite `TextLocation`. The 6000-CJK
  heading remains exact in SQLite while the vector metadata stays below
  16384 bytes.
- Added version1 private generation intents and seals with the fixed writer,
  policy, identity, generation, expected count, file lengths and streamed file
  hashes. SQLite rows are authoritative. `DurableIntent`, `PersistedReceipt` and
  `PublicationAck` cannot be fabricated through public fields; create/open/
  publish/recover paths validate operation, owner epoch, generation and prior
  seal hash.
- Added pre-constructor allowlist, regular-file/symlink, dimension, count,
  vector-header, metadata-size, graph-envelope/CRC and bounded streaming-hash
  checks. Existing unpersisted/dirty bytes are rejected before eager redb
  opening; rejected files remain unchanged.
- Added real pinned-SrvDB record operations. Tests cover first ID 0, count as an
  excluded half-open boundary, tombstones distinct from missing metadata,
  replacement returning a new appended ID, whole-batch validation before
  insertion, search serialization and a 200-record command limit.
- Added one dedicated owner thread, one owner-scoped two-worker Rayon pool, a
  queue capacity of 16, pre-issued operation-token retention, queued
  cancellation and entered-work completion. Observer loss cannot discard an
  accepted mutation or move SrvDB destruction to the async caller.
- Added a single retained join coordinator per generation. The independent base
  process lease is held through the actual owner join, then dropped before
  completion publication. Final runtime teardown uses a synchronous fallback
  after forcing a failed close retry waiter awake.
- Added normal fenced close: close intent before Drop, bounded redb4.4
  header/checksum/geometry/newer-transaction/user-root checks, file and
  directory sync, post-Drop streaming hashes, immutable close receipt and exact
  lost-ack recovery. Clean Live seals survive an actual subprocess exit without
  destructors; corrupt/dirty/unsealed bytes fail closed.
- Added two-engine capacity: at most two embedded engines, clean-only LRU
  eviction, and Busy when both slots are dirty or otherwise ineligible.
- Added a bounded generation maintenance lease. Writes/remap/close of the
  leased source return Busy, unrelated work may proceed, each copy step is at
  most 200 stored IDs, and `VectorStorage` borrowed slices are copied and
  validated before the owner yields. The non-cloneable maintenance ticket
  releases through a cancellation-protected fallback even if the caller or
  observer disappears or the normal end envelope cannot enter a full queue.
  The staging engine retains f32 bits and allocates new destination IDs.
- Added AppState composition with an inert index owner. Startup captures the
  host-owned process lease, waits for index readiness without holding lifecycle
  locks, rechecks the host epoch, and starts jobs before publishing readiness.
  Close waits for job workers/child slots first, acquires all 16 direct
  operation permits, then closes/drains/joins the index before successful rearm.
  A failed job close leaves the index cleanup available for explicit retry.

## Controller-resumed failures and fixes

The previous implementation agent stopped with
`task-5b-typed-commands-red.log` showing four passes and two
`Intelligence(Interrupted)` failures. Controller resumed from that exact source.

- Replaced the remaining typed-command stubs with one bounded owner command
  envelope for open/add/scan/delete/replace/search/persist/maintenance/ack.
- Reproduced the next meaningful failure: persistence retired the active dirty
  intent, so the acknowledgment path incorrectly tried to validate that same
  intent again. Root cause was confirmed at the repository publication
  boundary. Acknowledgment now validates the committed `PublicationAck` before
  admission and again inside the owner; it retains the original generation
  control fence without weakening publication validation.
- Added semantic RED/GREEN for typed work roundtrip and failed-close retry.
  Controller's fresh lifecycle group then passed 6/6.
- Added AppState RED tests before integration and passed close-order/fresh-
  generation behavior after implementation.
- Added typed scan/mutation and maintenance-copy RED tests. The only test
  adjustments were fixture geometry/order assumptions, not production changes.
  The completed lifecycle group passes 12/12, including dropped-maintenance
  release and forced join-coordinator spawn failure.
- Added clean-LRU and dirty-capacity tests; both pass with the real crate.
- Added an AppState observer-timeout regression. The first close observation
  times out while job cleanup is blocked; the index remains owned and a later
  observation succeeds only after the jobs drain.
- Independent review round1 found a startup/close admission race. AppState now
  holds one `intelligence_startup` generation gate from host-epoch capture
  through index readiness, epoch recheck and jobs publication; close takes the
  same gate before jobs/index shutdown. A deterministic blocked-startup test
  proves close cannot succeed or rearm around a stale index owner.
- Added a controlled persistence fault after actual SrvDB durability but before
  receipt construction. The regression proves SQLite intent stays dirty, the
  original engine and directory lock remain owned, scans continue on that live
  engine, and retry on the same engine publishes and acknowledges successfully.
- Independent review round2 found that a new `open_existing` could overwrite the
  clean seal after publication but before its exact acknowledgment. Persistence
  now installs a per-generation transition fence before enqueue; intent open is
  rejected until the exact owner acknowledgment succeeds (or shutdown clears
  the retired generation). The typed roundtrip regression publishes A, proves
  B receives Busy during the gap, acknowledges A, then reopens after shutdown.
- Round2 also required a stronger startup interleaving proof. A test-only
  readiness gate now blocks inside the entered index owner before readiness;
  the regression proves close cannot complete or rearm, releases readiness,
  observes stale-epoch rejection and actual join shutdown, then verifies a
  fresh generation reopen.
- Independent review round3 found that a persisted receipt could still publish
  after owner shutdown had dropped its engine. Publication now enters the owner
  queue and is serialized with shutdown; a receipt whose generation stopped
  before exact publication returns `Interrupted` and cannot change SQLite from
  dirty to clean. A standalone regression retains the receipt, shuts the owner
  down, attempts publication and verifies the state remains unchanged.
- Round3 found that startup failure called index shutdown without first draining
  all 16 direct-operation permits. Startup failure now takes the same direct
  barrier as normal close, and the controlled failure test proves shutdown
  cannot overtake an entered direct operation.
- Round3 found that a delayed shutdown observer from generation A could clear a
  transition fence belonging to generation B. Stop transitions now contain both
  operation ID and owner epoch, and an old observer clears only its own epoch.
  The two-observer A/B regression proves B's publication gap remains fenced.

## Verification evidence

All Cargo commands used `login:false`, `--offline --locked`, the writable local
CARGO_HOME and the existing bounded ALSA sysroot prefix. The fresh controller
TMPDIR was `/dev/shm/voxbridge-task5b-resume.UF4InG`.

| Gate | Command | Result |
|---|---|---|
| Full library | `cargo test --manifest-path src-tauri/Cargo.toml --lib` | exit 0; **630 passed**, 0 failed |
| All targets | `cargo test --manifest-path src-tauri/Cargo.toml --all-targets` | exit 0; library 630, main 5, real document worker 6, Volcengine offline 2 and 1 paid ignored, loopback example 3; **646 passed**, 0 failed, 1 intentional ignore |
| Desktop binary | `cargo build --manifest-path src-tauri/Cargo.toml --bin vox-bridge` | exit 0; no warnings |
| Format | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | exit 0 |
| Patch whitespace | `git diff --check` and `git diff --cached --check` | exit 0 |
| Dependency pin | `cargo tree -i redb@4.4.0 --depth 3` | redb4.4.0 is reached through pinned SrvDB and the app only |
| Frozen source hashes | `task-5b-source-freeze.sha256` | exact per-file SHA-256 manifest |

The prior all-target flake was reproduced again before finalizing: the focused
test alternated pass/fail because it waited for Provider `finish`, which can run
before the lifecycle observer commits durable `Stopping`. The test now waits on
the actual state condition before reading the stop snapshot. It passed 30
consecutive focused runs and the complete 17-test orchestrator group before the
fresh full gates. This is a deterministic synchronization correction to an
unowned session test, not a Task5B production change.

Retained focused historical RED/GREEN logs remain in this directory:

- `task-5b-codec-red.log`, `task-5b-codec-green.log`
- `task-5b-persistence-red.log`, `task-5b-persistence-second.log`
- `task-5b-close-red.log`, `task-5b-close-first.log`
- `task-5b-close-failures-red.log`, `task-5b-close-failures-green.log`
- `task-5b-records-red.log`, `task-5b-records-green.log`
- `task-5b-envelope-red.log`, `task-5b-envelope-green.log`
- `task-5b-stop-attempt-red.log`, `task-5b-stop-attempt-green.log`
- `task-5b-typed-commands-red.log`

The fresh all-target output is the superseding final evidence; authoritative
hash manifests and complete diff packages are generated from the frozen source
after the report is written.

Freeze artifacts:

- Cached source tree: `0aff5198ff518179ce165b43b45c5c8cd945701f`
- Complete staged Task5B patch: `task-5b-index-owner.diff`
- Patch command: `git diff --cached --binary`
- Patch bytes: `186716`
- Patch SHA-256:
  `f80e6f1e37aa9a54ed89aa03b107d111e27bcdf3565bc5029125dcb74e5c223b`
- Owned cached paths: 24
- `git diff --cached --check`: exit 0
- `sha256sum -c task-5b-source-freeze.sha256`: all 24 entries OK
- Round4 tree/path/mode/byte/hash metadata:
  `task-5b-round4-freeze.json`
- Independent round4 verdict: SpecPASS / QualityAPPROVED, zero
  Critical/Important and four assigned non-load-bearing Minor observations
- Source commit: `99cc3b3` (`feat: add owned SrvDB index persistence`)
- Session synchronization test commit: `1ea0bdd`

## Exact source ownership

Task-owned paths:

- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `src-tauri/src/knowledge/mod.rs`
- `src-tauri/src/knowledge/identity.rs`
- `src-tauri/src/knowledge/index.rs`
- `src-tauri/src/knowledge/index/**`
- `src-tauri/src/app/mod.rs`
- `src-tauri/src/app/intelligence.rs`
- `src-tauri/src/app/intelligence/tests.rs`
- `src-tauri/src/app/lifecycle.rs`

Explicitly excluded: `.gitignore` and every frontend, migration, parser/documents
handler, provider, summary/research and release artifact outside this list.

## Open boundaries

- Reviews round1–4 are retained as corresponding files; all seven load-bearing
  findings are repaired and round4 is clean. Task5B is complete at `99cc3b3`.
- Task 5C still owns embedding transport, document/ready reconciliation,
  authoritative hydration, explicit search commands, automatic import
  continuation, orchestration of maintenance-copy mapping publication and
  public DTO budgets.
- File caps and integer record limits are operational bounds. They are not an
  RSS, throughput or small-memory guarantee.
- Closed-header proof is intentionally limited to the pinned known redb/SrvDB
  writer and exact persisted bytes; it is not a general arbitrary-file decoder.
- Windows/macOS compile acceptance, real native audio, minimum-OS routes,
  keyring, paid services and signed/package release gates remain external.
