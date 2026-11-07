# Task 5B independent review — round 4

Reviewer: fresh independent Task 5B reviewer (no 5B source authorship).

**Spec: PASS**
**Quality: APPROVED**

## Review basis

The frozen package is internally consistent with the round4 dispatch, verified
directly:

- BASE and HEAD are both `c8f77708ce959f9ab3e454147329fcff3771cc37`.
- `git write-tree` reproduces the cached tree
  `0aff5198ff518179ce165b43b45c5c8cd945701f`.
- The index holds exactly the 24 declared owned paths; every mode is `100644`
  (`git ls-files -s` shows 52 entries across the owned subtrees, all `100644`).
- `git diff --cached --binary` reproduces exactly 186716 bytes with SHA-256
  `f80e6f1e37aa9a54ed89aa03b107d111e27bcdf3565bc5029125dcb74e5c223b`,
  byte-identical to `task-5b-index-owner.diff` and the `task-5b-round4-freeze.json`
  record (manifest SHA-256 `435f116f…` also matches).
- `sha256sum -c task-5b-source-freeze.sha256` passes 24/24; per-file bytes and
  hashes in `task-5b-round4-freeze.json` were checked against the cached blobs.
- No unstaged edits exist in any of the 24 owned paths. The only worktree
  differences are the pre-existing unowned `.gitignore` change (SHA-256
  `1e708ede…`, exactly as ledgered) and the unstaged test-only session
  synchronization fix in `src-tauri/src/session/orchestrator_tests.rs`
  (`wait_status(Stopping)` before the snapshot read), which the round4 refreeze
  ledger explicitly records as outside Task 5B.
- No Cargo command was run. Recorded final evidence was verified from the log
  tails: library 630 passed/0 failed; `--all-targets` 646 passed/0 failed with
  1 intentional paid ignore; desktop binary build exit 0 with no warnings;
  `cargo fmt --check` and both diff checks recorded exit 0.
- Pinned-producer static cross-checks were performed for the named doubts
  against the local offline checkout of SrvDB at
  `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04` and redb 4.4.0 in the local
  registry cache (see Strengths).

## Round 1–3 disposition

Every prior load-bearing finding is repaired in the frozen bytes, with a
deterministic regression that exercises the exact interleaving:

- **Round 1 critical (startup/close admission race):** startup holds the shared
  `intelligence_startup` gate from host-epoch capture through index readiness
  and job publication ([intelligence.rs:8](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/app/intelligence.rs:8));
  close takes the same gate before jobs/index shutdown
  ([lifecycle.rs:142](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/app/lifecycle.rs:142));
  the epoch recheck runs after readiness
  ([intelligence.rs:26](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/app/intelligence.rs:26)).
  The owner-entry readiness gate test
  `intelligence_app_close_waits_for_entered_index_startup_to_join` blocks after
  `start_owned` has entered but before readiness, proves close cannot finish or
  rearm, and then proves the stale epoch is rejected, the owner actually joins,
  and reopen gets a fresh generation.
- **Round 1 important (missing post-durability retry proof):** the fault seam
  fires after real `SrvDB::persist` but before receipt construction
  ([owner.rs:226](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:226));
  `index_persistence_post_durability_failure_retries_on_the_same_live_engine`
  proves the SQLite intent stays dirty, the original `srvdb.lock` and live
  engine remain owned, scans still work, and a same-engine retry publishes and
  acknowledges.
- **Round 2 important (publication/open transition gap):** `persist` installs a
  per-generation transition fence before enqueue
  ([lifecycle.rs:458](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:458)),
  `open_existing` rejects a fenced generation with `Busy`
  ([lifecycle.rs:299](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:299)),
  and `index_lifecycle_typed_work_roundtrip_publishes_and_reopens_after_owned_join`
  proves A publishes, B gets `Busy` in the gap, A acknowledges, and B reopens
  after the owned join.
- **Round 2 important (weak startup interleaving proof):** superseded by the
  owner-entry readiness gate described above, which blocks inside the entered
  owner rather than before admission.
- **Round 3 critical (publish after engine drop; startup bypassing the direct
  barrier):** publication is now an owner-queued command serialized with
  shutdown ([lifecycle.rs:469](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:469),
  [owner.rs:243](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:243)).
  A receipt whose generation stopped before admission returns `Interrupted`
  without touching SQLite — `index_lifecycle_stale_receipt_cannot_publish_after_owner_shutdown`
  proves the row stays dirty=1 with no seal. Startup failure now drains all 16
  direct permits before index shutdown
  ([intelligence.rs:36](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/app/intelligence.rs:36));
  `intelligence_app_startup_failure_waits_for_direct_operation_drain` proves
  shutdown cannot overtake a held direct operation.
- **Round 3 important (stale observer clearing a newer generation's fence):**
  transitions carry operation id and owner epoch; successful shutdown clears
  only its own epoch
  ([lifecycle.rs:630](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:630));
  `index_lifecycle_stale_stop_return_does_not_clear_new_generation_transition`
  blocks an A observer after completion, starts B, and proves B's fence
  survives the delayed A return.

## Strengths

- **Dependency provenance.** `srvdb` is pinned to the exact approved git
  revision with no path override; the lock records redb 4.4.0 reached through
  that pin; direct `rayon`, `crc32fast`, and pinned `xxhash-rust 0.8.19` serve
  the owner pool, graph CRC, and redb slot checksum. The Cargo.lock diff adds
  only the new dependency tree and re-qualifies duplicate-version references
  (`cpufeatures`, `lock_api`, `parking_lot`); no existing pinned version moved.
- **Header formats match the pinned writers exactly.** Static comparison
  against the local redb 4.4.0 source confirms the 9-byte magic, god-byte
  flags at offset 9 (bit 1 = recovery required), 4096 page size at 12, region
  geometry at 16–28, the 128-byte commit slots at 64/192 (version 3, root at
  8..40, zeroed 72..104, transaction at 104..112, xxh3-128 seed-0 checksum at
  112..128), and the file-layout formula. The SrvDB vectors header
  (`0x53764442`, version 4, u16 dimension, u64 count, 8 zero bytes) and the
  `SRVD`+`HNSW` version-1 graph envelope (payload length at 12, CRC32 of the
  payload at 20) match `preflight.rs` and `redb_header.rs` field for field.
- **Durable fence types.** `DurableIntent`, `PersistedReceipt`, and
  `PublicationAck` have private fields and repository/owner-only construction.
  `validate_intent` binds the exact stored intent JSON, operation id, identity,
  generation, owner epoch, prior seal, and prior-seal digest
  ([repository.rs:141](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/repository.rs:141)).
  `publish_seal` revalidates operation and descriptor before committing clean;
  `validate_ack` only accepts the exact committed seal, so lost acks are
  recoverable from committed repository state and never inferred from persist
  success. The 4B migration schema (identity/generation PK, dirty, intent and
  seal bounds) matches every repository SQL statement.
- **Trusted open.** Pre-constructor checks enforce the literal file allowlist,
  regular-file/symlink rejection, directory entry and file caps, vector header
  geometry, metadata non-emptiness, graph envelope/CRC, streamed hashes, and
  exact sealed-byte equality. Unsealed or dirty bytes are never passed to the
  constructor; rejection preserves bytes. Clean reopen only accepts a
  clean-sealed current-writer source whose Live header snapshot is byte-equal;
  the real subprocess `process::exit` fixture proves a published Live seal
  survives actual crash without destructors, while changed final-Drop bytes are
  never blessed by an old seal.
- **Bounded owner execution.** One owner thread, exactly two retained
  Rayon workers with joined handles ([owner.rs:706](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:706)),
  a 16-slot queue, closed command enum with no closures in envelopes, 200-record
  pages, first-id-0/half-open/tombstone/replace-append semantics proven on the
  real crate, whole-batch validation before mutation, capacity reservation
  including tombstones, and clean-only LRU with `Busy` for dirty capacity.
- **Maintenance copy.** A retained ticket fences source mutation/remap/close,
  reads only the persisted source through a private `VectorStorage` (which the
  pinned source shows opens only `vectors.bin`), copies f32 bits in bounded
  pages, and releases through a cancellation-protected fallback even when the
  caller or observer disappears or the queue is full.
- **Lifecycle ownership.** The worker owns receiver, engines, pool, and the
  independent base lease; no facade Arc or sender cycle exists, so the last
  facade drop drains to EOF under the base lease. `JoinPayload` retains the
  base lease through the actual owner join, drops it before completion
  publication, and has a synchronous force-then-join Drop fallback for
  coordinator spawn failure, all outside facade locks
  ([lifecycle.rs:1006](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:1006)).
  The real `app.lock` probes prove idle engines hold the process lease until
  actual shutdown and that final facade drop releases it without a Tokio
  runtime.
- **AppState integration.** Close drains jobs/child work first, keeps index
  cleanup admission available when job cleanup fails or a close observation
  times out, then acquires all 16 direct permits and retains that barrier
  through index shutdown; successful reopen starts a fresh index generation
  before jobs; no lock is held across waits (the blocked-SQLite capture test
  proves prompt stop signaling).
- **No fabricated readiness.** The only durable state written by 5B is
  `building` for generations; no document `Ready` state is touched. The
  repository publication is one SQLite transaction with a documented insertion
  point for 5C's atomic seal/mapping/pointer/Ready composition
  ([repository.rs:90](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/repository.rs:90)).

## Findings

### Critical

None.

### Important

None.

### Minor

1. **`open_existing` holds the in-memory transition-fence mutex across the
   SQLite intent transaction** —
   [lifecycle.rs:299](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/lifecycle.rs:299).
   This is the deliberate round-2 design that makes fence installation atomic
   with intent creation. The lock order is one-way (`publications` → SQLite;
   nothing holds SQLite while acquiring `publications`), no config or
   generation-control lock participates, and no owner wait occurs under the
   lock, so the binding config→SQLite→control ordering is preserved and there
   is no deadlock. Impact: a long SQLite write transaction elsewhere can delay
   the fence lookup for persist/acknowledge on this owner instance. Required
   fix: none for 5B; optionally document the deliberate exception before 5C
   adds further publication paths.

2. **Blanket `allow(dead_code)` in production builds** —
   [index.rs:1](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index.rs:1).
   Until 5C consumes the ports, unused-API mistakes in the owner surface would
   not surface as warnings. Impact: review-time, not runtime. Required fix:
   none for 5B; Task 5C should remove the blanket allow once real consumers
   land.

3. **Same-process clean-entry reuse skips preflight byte re-verification** —
   [owner.rs:117](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/owner.rs:117).
   Acknowledged-clean reuse upgrades the live engine to Dirty from the prior
   seal hash without re-hashing on-disk bytes. Impact: none under the trust
   model — all mutations flow through the owned live engine, and the close
   protocol re-verifies sealed bytes before any Closed publication, so dirty
   bytes can never gain trusted provenance. Required fix: none; a clarifying
   comment would help future maintainers.

4. **Writer temporaries are rejected rather than capped** —
   [preflight.rs:41](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/preflight.rs:41).
   A crashed write leaving `.vectors.bin.tmp`/`.metadata.db.tmp` makes the
   generation fail preflight (repair required) instead of tolerating the
   declared temporary caps. Impact: conservative and consistent with the
   dirty-crash → `IndexRepairRequired` policy; originals are always preserved.
   Required fix: none for 5B; keep this behavior unless the controller rules
   otherwise.

None of the Minor findings is load-bearing; none blocks the verdict.

## Cannot verify — assigned future owners

- **Task 5C:** embedding transport, credential/binding resolution,
  authoritative hydration, public search/reindex/delete commands, automatic
  import continuation, and the actual atomic seal + mapping + generation
  pointer + document-ready publication transaction. The transaction insertion
  point and typed plan seam exist
  ([repository.rs:90](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/repository.rs:90));
  the integrated transaction does not yet exist and must keep the same
  operation/generation/epoch fence.
- **Tasks 6–10:** summaries, research, knowledge UI, history, packaging, and
  end-to-end release behavior.
- **External owners:** Windows/macOS compilation and filesystem semantics
  (including Windows lock-file retention during owner join), minimum-OS audio
  routes, keyring, paid-service acceptance, signing, and packaging.
- **Independent execution:** per the review contract I did not rerun Cargo.
  This verdict relies on the frozen bytes, static inspection, pinned-producer
  source comparison in the local offline caches, and the recorded RED/GREEN
  and final logs (library 630/630, all-targets 646/0 failed/1 intentional
  paid ignore, desktop build, fmt, diff checks).

## Conclusion

The frozen round4 package satisfies the Task 5B brief, checkpoint, global
constraints, and all ledgered rulings relevant to the owner/codec/recovery
slice. All seven findings from rounds 1–3 are repaired with deterministic
regressions, and no new Critical, Important, or load-bearing Minor finding was
identified. Task 5C and Tasks 6–10 remain open as recorded.
