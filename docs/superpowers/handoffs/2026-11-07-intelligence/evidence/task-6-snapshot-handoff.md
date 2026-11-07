# Task 6 preparation — fixed final snapshot, pure planner, export seam

Prepared 2026-11-06 in `.worktrees/vox-bridge-implementation`; observed HEAD `83ddae342137540e4e274b1ed99d5353d3db344e`. This is a read-only source assessment plus this local report, not Task 6 implementation or review completion. Tasks 4–6 remain queued. Active producers may change their own files after these observations. No Cargo command, test, migration, index creation, network/native action, source edit, git-index change or child agent was performed.

Read `task-6-brief.md` and `global-constraints.md`; reused `preflight-current-10-tasks.md` and the relevant ledger rulings. Permanent automatic markers, atomic admission/version counters, manual retries, startup Completed/Failed eligibility and generic task-pair coordination remain the existing decisions (`progress.md`:53,82,100,118,163,175–178; `task-6-brief.md`:49–61,82–90,100). They were not re-reviewed here. Task 3A admission is reviewed; Task 3B scheduler/handler/ownership APIs are unfinished and this report declares none of them.

## Findings and controller decision

1. **Concrete plan mismatch: durable coverage has segment identity only, while the requirement names every split part.** The minimal migration 5 choice is described below. `input_final_count` must remain the number of distinct original final segment IDs, independently of the number of parts or requests.
2. **Current transcript writes are final-only INSERTs, not upserts.** Late correlation promotes a pending identity in memory; it does not update an older SQLite interim row. Capture after successful durable insertion sees the complete final metadata. Existing transcript pages and export loading do not supply the specified fixed read snapshot.
3. A private bounded snapshot and a pure planner can meet the specified limits without a new public API. Use exact rendered UTF-8 byte lengths plus 1024 overhead, complete part partitions, contiguous coverage ranges, strictly decreasing reduction node count and hard counters for all calls/levels. Reject rather than discard input when limits prevent completion.
4. The existing explicit export DTO is the appropriate allowlist seam. New structured versions and hash metadata can be projected there while raw response rows remain outside `ExportData`. The existing legacy JSON sanitizer is not a raw-response exclusion boundary.

### Coverage mismatch and minimal migration 5 choice

Requirement, `task-6-brief.md`:94 (also canonical plan `docs/superpowers/plans/2026-11-04-workbench-intelligence.md`:791):

> Split oversized individual rows at Unicode boundaries with `(segment_id,part_index,part_count)`; every part appears in coverage.

Proposed storage, brief:88 (canonical plan:785):

> `summary_coverage(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, segment_id TEXT NOT NULL, PRIMARY KEY(summary_id,segment_id))`

The existing suggested test at brief:86 compares a set of final snapshot IDs. For a segment split into three parts, recording only part 0 produces exactly the same segment-ID set as recording all three. The two-column primary key cannot distinguish those cases or retain part counts. This is a contract/schema mismatch, not a defect in already implemented summary code.

**Suggested controller ruling, not applied:** replace that planned table definition within migration 5 with the same foreign key plus `part_index INTEGER NOT NULL`, `part_count INTEGER NOT NULL`, `PRIMARY KEY(summary_id,segment_id,part_index)`, and checks `part_count > 0`, `part_index >= 0`, `part_index < part_count`. Use zero-based part indexes; an unsplit row is `(id,0,1)`. Validate consistent `part_count` for every segment against the immutable manifest before insertion/success; a SQL row-local check alone cannot enforce consistency across different parts.

Keep `input_final_count = distinct final snapshot IDs = snapshot.rows.len()` because the transcript table has a primary key on ID. Do not replace it with part count, coverage table row count, or request count. Success requires exact equality of expected and supplied `(segment_id,part_index,part_count)` tuples, with no duplicates, missing parts, or foreign IDs. Row-ID equality remains a separate useful assertion. The migration change is confined to the not-yet-created migration 5; do not alter the reviewed Task 3 admission schema/helper for this issue.

## Current storage and identity facts

| Source | Observed behavior and implication |
|---|---|
| `src-tauri/migrations/0001_session_foundation.sql`:4–5,7 | `translation_segments` is a normal rowid table with text primary-key ID, session FK, direction, sequence, seconds timestamp, language, text, and `CHECK(is_final=1)`. `(session_id,direction,sequence)` is unique. Directional sequences are not a shared ordering clock. |
| `src-tauri/migrations/0002_runtime.sql`:1–2 | Adds nullable `details_json` and `occurred_at_ms`. Native role is inside details, not a dedicated role column. Legacy null metadata is expected, not a source-role default. |
| `src-tauri/src/storage/models.rs`:15–25 | `StoredTranscriptSegment` retains ID/session, direction string, sequence, language/text, seconds, optional milliseconds and optional full `TranscriptDetails`. Preserve this record rather than reconstructing identity from rendered text. |
| `src-tauri/src/domain/transcript.rs`:9–26 | Details contain Source/Translation kind, provider/config ID, epoch, native sentence ID and local correlation ID, plus optional provider/timing data. There is no speaker-name field. |
| `src-tauri/src/storage/sqlite.rs`:224–260 | Legacy writer accepts only finals without runtime metadata; typed writer validates metadata and rejects interims. Both use plain `INSERT`; neither uses `ON CONFLICT`, `REPLACE`, nor a transcript UPDATE. A uniqueness failure does not overwrite an existing final. The legacy writer has no equivalent transcript byte cap. |
| `src-tauri/src/domain/transcript.rs`:150–190; `src-tauri/src/session/transcript_sink.rs`:96–106 | Typed events bound text and selected metadata to 262144 bytes, checking borrowed payload before cloning. This is not a whole-session bound and does not constrain arbitrary legacy rows. |
| `src-tauri/src/session/transcript_sink.rs`:108–141,171–209 | Identity key includes role and native/local/unassociated correlation. A late identified event can reuse the sole same-kind pending unassociated interim's UUID/sequence. Duplicate suppression occurs before persistence/promotion. Final insertion must succeed before identity removal, finalized-key recording, or sequence consumption. Interims remain in memory. |
| `src-tauri/src/session/transcript_sink.rs`:184–217 | SQLite acceptance precedes final publication; bus publication failure does not undo the durable final. A closed session bus is therefore not a source of truth for summary input. |
| `src-tauri/src/storage/sqlite.rs`:262–304 | Public paging orders by rowid and returns only a page; it lacks an upper bound/read transaction spanning pages and does not explicitly filter finals (the current table/writers enforce them). Repeated page calls are not a fixed snapshot. |
| `src-tauri/src/storage/sqlite.rs`:449–487 | Export loads finals in timestamp/sequence/ID order, with separate locks for session/lifecycle and later summaries. It neither exposes rowid nor supplies a consistent summary snapshot. Do not reuse it for summary planning. |
| `src-tauri/src/storage/sqlite.rs`:20–23,48–86 | One mutex-owned connection is authoritative. The reviewed generic transaction helper opens an Immediate transaction and forbids calling ConfigIo inside it. A snapshot can be implemented privately by that same owner, without a second database/connection or holding a transaction across cloud work. Migration list currently ends at 0003. |

Existing focused test source confirms the intended producer cases, but was not run here: `storage/sqlite_tests.rs`:96–118 (duplicate rejection/no interims),122–147 (legacy metadata remains absent),303–325 (cross-direction/session insertion ordering); `session/transcript_sink_tests.rs`:462–518 (late native/local identity reuse),632–698 (failed promoted final keeps identity, successful retry persists once).

### Legacy labels and preservation

For native rows, role comes exclusively from `details.kind`; language must never override it. For legacy rows retain `details=None`, `occurred_at_ms=None`, unknown epoch/correlation, original sequence and original direction. Do not run typed `TranscriptUpdate::validate` on a legacy row or invent native details/timestamps.

The existing export helper `export/mod.rs`:151–175 offers a compatible *display* convention: for known A→B/B→A direction and distinct session languages, match language to that direction's source/target; otherwise role is `unknown`. If reused for summary labels, keep this explicitly a derived legacy label, with origin Legacy and native kind still absent. Bidirectional legacy direction, same-language sessions, and unmatched language remain unknown; arbitrary unrecognized stored direction is retained as unknown rather than turned into A or B. The current public page parser (`storage/sqlite.rs`:551–558) rejects arbitrary unrecognized direction, so it is unsuitable for a preservation-oriented legacy snapshot.

Use labels such as `source_to_target/source/en` and `target_to_source/translation/zh`, with explicit `legacy/unknown` where needed. These label text streams, not people. Do not pair nearby rows into sentences or infer participant names.

## Minimal private snapshot shape and bounded read

Illustrative private shapes only; exact modules/names may change at the Task 6 architecture checkpoint:

```rust
struct FinalSnapshot {
    session_id: Uuid,
    last_rowid: Option<i64>,
    rows: Vec<SnapshotRow>,
    input_final_count: u32, // distinct original IDs, never parts
    normalized_bytes: usize,
    input_hash: String,
}
struct SnapshotRow {
    rowid: i64,
    stored: StoredTranscriptSegment,
    label: SnapshotLabel, // native kind or explicit legacy-derived/unknown label
}
// Private method on the existing SQLite owner, not a command or scheduler API:
// LocalStore::capture_summary_snapshot(session_id) -> AppResult<FinalSnapshot>
```

1. Acquire the existing SQLite owner and open one short read transaction (Deferred is sufficient); first verify the session and obtain `MAX(rowid)` using `WHERE session_id=? AND is_final=1`. That first read pins the SQLite snapshot. `None` means no final rows; preserve it rather than inventing a high-water row. A missing session is distinct from empty transcript.
2. Within that same transaction, iterate `WHERE session_id=? AND is_final=1 AND rowid<=? ORDER BY rowid`, with an overflow sentinel at row 100001. No interim, audio, diagnostic, or session-config body is included as transcript input. Do not fetch pages under separate transactions or query a global table maximum.
3. Enforce checked byte/row accounting **before cloning a row's untrusted text/metadata**. The existing `read_segment_at` (`sqlite.rs`:505–522) eagerly allocates Strings, so do not blindly collect with it and check afterward. Borrow SQLite text via `get_ref`/checked UTF-8, or inspect SQL byte lengths first. Check details/language/direction as well as text; legacy input is not protected by the typed event bound.
4. Define a versioned deterministic normalization/encoding at implementation: retain exact text, null unknown metadata and row boundaries; use fixed field order and length framing or canonical encoding so concatenation cannot collide. Include role/direction/language/ID and preserved identity metadata in the hash. Stream byte counting/hashing into a bounded sink; never construct an unbounded JSON string before counting. Charge retained variable metadata too, or specify a separate bounded retention budget at the architecture checkpoint. A 16MiB normalized-input cap is not a claim that total Rust/SQLite process memory is 16MiB. Encoding/quoting expansion must be counted, not just `text.len()`.
5. Reject the first byte beyond 16MiB or row beyond 100000 with `ContextTooLarge`; do not return a prefix. Persist safe failure on the already admitted version through the reviewed admission/status boundary. Zero final rows yields local `NoFinalText`; preserve empty-text rows in any nonempty snapshot's identity/coverage instead of silently filtering them.
6. Close transaction and release the mutex before rendering requests, parsing model output, ConfigIo, any await or cloud call. Persist hash/final count/high-water metadata from this owned snapshot. Late final insertions after capture belong to a later manual version, not the current snapshot.

The read must not become a long-lived lock on stop's durable writes. Use the reviewed producer's bounded blocking/execution mechanism when available; this report does not invent Task 3B APIs. Current schema indexes are `(session_id,direction,sequence)` and `(session_id,sequence)` (`0001`:5,7); migration 0003 adds no transcript index. `LIMIT 100001` bounds returned rows, not necessarily SQLite sorting/scanning. At implementation, verify the query plan for the ordered session scan/high-water lookup; a migration 5 index on `translation_segments(session_id)` supplies implicit trailing rowid order. This is a concrete performance/bounded-read risk to check, not a query plan measured here or an index action authorized by this report.

## Pure planner shape and algorithm

Keep snapshot/plan/request provenance separate from model content. The planner reads no database, config, network, key or scheduler state. Its state is owned by one summary attempt; no public planner DTO is needed.

```rust
struct PartRef {
    snapshot_row: usize,
    byte_range: Range<usize>, // a UTF-8 boundary range into owned snapshot text
    segment_id: Uuid,
    part_index: u32,
    part_count: u32,
}
struct PlanState { requests_reserved: u32, level: u32 }
struct SummaryNode {
    content: SummaryContent, // already parsed and strictly validated
    coverage: Range<usize>, // contiguous range in the immutable PartRef manifest
}
struct RequestPlan {
    ordinal: u32,
    level: u32,
    phase: Phase, // Map or Reduce
    input_range: Range<usize>, // all parts or all complete child nodes in this range
    coverage: Range<usize>,
}
```

The manifest is ordered by snapshot rowid then part index. Store byte ranges into snapshot text rather than cloning text for every part or materializing all request strings simultaneously. Because map packing and reductions use contiguous groups, coverage can be represented as adjacent ranges rather than repeatedly copying large ID sets. This is an optimization only: tests expand ranges to exact tuples, and persistence writes per-part identities under the suggested schema.

### Request budget and map partition

- For an LLM profile validate present context/output values and checked `max_context_tokens - max_output_tokens`. No zero/unbounded defaults. Charge the actual rendered system and user strings as UTF-8 bytes, **plus 1024** reserved prompt/schema/protocol overhead; include labels, JSON escaping, delimiters, IDs and part indexes/counts. Counting system/schema explicitly plus the reserve is conservative and avoids assuming an expanding system instruction fits within the reserve. Every emitted request must satisfy `system.len() + user.len() + 1024 <= context - output` with checked arithmetic.
- This deliberately uses bytes as a conservative token estimate; it is not exact accounting for every compatible tokenizer. A provider context rejection still fails safely with `ContextTooLarge`, rather than dropping input or retrying a shorter prefix.
- Render transcript as quoted data in a fixed envelope. Test the rendered request's length, not only an estimate of unescaped text. A delimiter/role/language/ID envelope that cannot fit even one character yields `ContextTooLarge`.
- Greedily pack consecutive whole rows when they fit. Split an individually oversized row at UTF-8 character boundaries into the largest fitting prefixes; count serialized/escaped prefix size. Do not use arbitrary byte slicing, trim content, split only the first part, or lose an empty row. Reserve worst-case checked u32 digit widths for part index/count while finding splits, then render with actual values and recheck. This prevents a larger `part_count` from invalidating already chosen boundaries. Every row's parts reconstruct the exact original text and use indexes `0..part_count`.
- Build the complete manifest/map partition before a first LLM call. It must cover every part exactly once in order. A count-only `take(64)` or early return after a limit silently loses input. Reject a plan that cannot finish even under the minimum remaining calls: one map request may be final, but multiple map requests require at least one further reduce request. Thus 64 map requests already exceed the total 64-call budget and fail locally before cloud work.
- Render one planned request at a time, retaining its immutable coverage provenance. No coverage/participant/action declaration emitted by the model is trusted as proof of input supply.

### Bounded hierarchy without silent drops

1. Map is level 1. Every map response is durably retained (size/hash caps) before parsing, then independently deserialized/validated against the same strict SummaryContent schema. Unknown fields, fenced JSON, wrong types or array/string limit violations fail; never repair JSON or truncate arrays to 100 entries.
2. If map produces one validated node, it is the final structured result. Otherwise render complete child SummaryContent objects and greedily partition **all** current nodes into contiguous groups satisfying the same request byte budget. No array concatenation followed by `take(100)`, child omission, beginning/end sampling or string slicing of intermediate JSON is allowed.
3. Reduce groups containing two or more nodes. Carry a singleton forward unchanged and retain its coverage; carrying is not an LLM request. If no group can combine at least two nodes, fail `ContextTooLarge` before dispatching that level. A child whose complete serialized representation cannot fit a legal reduce request cannot be silently cut down; fail. This gives explicit progress: the next frontier has strictly fewer nodes. A reducer's verbosity need not shrink bytes, so recompute the next frontier's actual byte sizes rather than assuming `max_output_tokens` guarantees the observed response size.
4. Before dispatching a reduction level, reserve all its calls with checked `used + level_calls <= 64`; include maps and the final reduce in that counter. Never dispatch request 65. Maps count as level 1 and the final reduction may use level 8; reject a prospective level 9. If more than one node remains when the next level/call cannot be reserved, the version fails visibly; it does not return one convenient child as success. Provider errors/cancellation do not reset either counter and do not cause automatic LLM retries.
5. A reduced node's coverage is the exact union of all its children's adjacent ranges, set by the planner. Validate no gaps, overlaps or foreign manifest entries. Preserve singleton ranges unchanged. Root success requires exactly `[0..manifest.len())`; that check is independent of what the model elects to mention. This promises complete supplied input, not semantic completeness of the model's prose or entities.
6. Keep expected, attempted and response-retained coverage distinct. A useful durable-table meaning is “map inputs for which the raw response was retained,” inserted together with that raw response before parse; an invalid JSON response still retains its input provenance. Cancelled/failed attempts can have partial coverage. On success the retained map coverage must equal the complete expected part manifest. Do not mark an uncalled planned chunk as actually supplied.

Strict response/cap handling stays the brief's requirement (`task-6-brief.md`:63,90,94): each raw response ≤2MiB, all raw responses ≤32MiB, preserve prior retained responses on a later cap failure. Keep raw text/hashes out of ordinary error messages/logs and planner/public views. An oversized later response is a visible failure, not permission to skip its child and continue.

## Structured export and hash-only metadata seam

Current storage is deliberately separate from rendering: `storage/models.rs`:51–68 has SummaryVersion with signed version, seconds created_at, status String and structured_json String; ExportData has a vector of those versions. `sqlite.rs`:415–446 inserts/lists legacy summaries in ascending version order. New versions need the Task 6 store mapping; this report does not reuse that legacy writer as atomic job admission.

`export/json.rs`:40–47,84–94 builds an explicit JsonSummary with ID/version/time/status/sanitized structured data. Extend that *allowlist*, and a corresponding export-safe model, with `profile_id`, saved `model`, optional checked `created_at_ms`, input hash/count, safe error code, origin and optional `raw_response_hash`. Do not serialize a whole internal summary/store record or an entire ModelProfile/credential resolver. Preserve legacy seconds/version/status compatibility; conversions to milliseconds/u32 must be checked, never fabricated or silently cast.

For new summaries, export the strictly typed SummaryContent including all seven fields, all action text/owner/due, and all term/meaning values. Failed/Cancelled/Interrupted/Queued versions still export identity/status/safe nullable metadata, with no invented content. For older structured_json/status rows, retain the existing sanitized legacy structured value and mark legacy/missing new metadata None; trying to deserialize old `{title,text}` data only as the new strict schema would erase readable legacy content. A separate legacy variant/fallback is appropriate inside the export projection, not a relaxation of the new model parser.

Keep `summary_responses.raw_text` out of ExportData entirely. `safe_summary_json` (`export/mod.rs`:178–227) recursively removes many key/audio-path fields, but an arbitrary `raw_response` property would pass its current filter. It is therefore not a boundary for raw response storage, and it must not be used to repair/validate new SummaryContent. It is the existing compatibility filter for legacy structured JSON. Hash metadata can be exported explicitly without reading response bodies. Define the summary-level response hash consistently (for example, latest retained response hash, including the failing response); individual retained response hashes/ordinals remain local response-detail records. The report does not introduce a raw-detail command.

Markdown/text currently iterate all versions (`export/markdown.rs`:28–45; `export/plain_text.rs`:18–24); preserve that behavior and render the same structured fields plus safe version/model/status metadata. Do not reduce new exports to the summary prose alone. Existing key/audio exclusion tests in `export/export_tests.rs`:95–123 remain useful, but do not exercise a future response table or new structured schema. The explicit projections must retain those exclusions.

## Focused behavior tests for implementation

These are proposed RED tests/fixtures, not tests run or source added in this preparation.

| Area | Focused observable behavior |
|---|---|
| Fixed snapshot | Interleave A/B finals with reversed timestamps/sequences and another session's larger rowid; snapshot contains only the selected session in insertion order and its own high-water rowid. Pause capture after its first read, commit/attempt a later final with a fixture-controlled connection, then verify it is absent from this snapshot and present in the next. Demonstrate the transaction is released before a delayed provider call. |
| Late promotion | Accept unassociated source interim, then native/local identified final; snapshot has the same UUID/sequence, final text and actual final details, never the interim. Inject an insertion failure: capture sees no final until the successful retry, then one final. Reuse the producer tests' controlled trigger pattern; do not change sink dedup policy. |
| Legacy preservation | Native same-language Source/Translation retain explicit roles. Legacy reverse rows use only direction-aware derived labels; bidirectional, same-language, unmatched/unrecognized directions remain unknown. No native epoch/correlation/milliseconds appears where absent. Empty text rows retain IDs. |
| Local limits | Exactly 100000 rows and exactly 16MiB of the chosen normalized encoding are accepted; one extra row/byte gives durable ContextTooLarge and zero provider calls. Include an oversized legacy text/details row to prove bounds are checked before eager collection; include JSON escape expansion and checked arithmetic overflow. Verify no successful prefix version or fabricated metadata. |
| Part identity | One row needing ≥3 parts with Chinese/emoji/combining marks/newlines/quotes reconstructs byte-for-byte. Expected tuple set equals every rendered map request tuple set. A fixture that omits only the middle part must fail even though distinct segment IDs/count still match. Persist/reopen all part tuples, enforce consistent count and uniqueness; input_final_count stays 1 for this row. |
| Request budget | For every RecordingLlm request compute actual system/user UTF-8 lengths +1024 and compare to context-output. Exercise exact boundary, one byte over, insufficient envelope budget, varying decimal part_count widths and multibyte boundary. No chars-as-bytes or unescaped-text-only assertion. |
| Coverage | Include a middle sentinel in a transcript requiring multiple maps, and assert equality of the entire snapshot-ID set **and** manifest part tuple set. Require every reduce request to include each complete child object assigned to its group; root range is the complete manifest even when fixture output does not repeat the sentinel. |
| Hierarchy | Controlled large map outputs require ≥2 reduce levels. Singleton carry retains coverage; all-singleton/no-pair-fit and oversized unsliceable child fail ContextTooLarge without omission. A response that grows rather than shrinks forces recomputed packing; no node-count progress fails visibly. |
| Hard counters | 64 maps fail before a first call; a fixture schedule that completes on call 64/level 8 succeeds, while a required call 65/level 9 fails before that call. Include a final reduce in the count, with partial prior raw responses retained on late failure. |
| Strict response durability | Valid strict map/reduce JSON succeeds; unknown fields, fenced JSON, wrong types and array/string excess fail without truncation. Raw/hash is stored before parse, with no raw value in public error/status/export. A later 2MiB/32MiB cap breach preserves prior response rows. |
| Export compatibility/exclusion | Populate native structured version plus legacy `{title,text}`, failed version, and several retained raw response rows with unique synthetic sentinel strings. JSON/Markdown/text include every version, complete structured fields, safe model/version/hash metadata, legacy nulls and readable legacy data; none contains raw sentinels, response bodies, audio paths or configured synthetic credentials. Unknown failed model JSON never becomes structured export content. |

Implementation must still run the full brief's appropriate summary/LLM/session/export gates and the reviewed real harness lifecycle/admission tests. Those checks, migration 4-before-5 integration, Task 3B final API binding and controller acceptance of the coverage-schema ruling are pending; this preparation claims none of them complete.
