# Task6 summary UI producer handoff — current round1 candidate

This candidate reconciles all twelve public producer paths against frozen
TREE `d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`. Eleven paths remain byte-identical to accepted
manual Task1; `app/summary.rs` now adds prompt combined-readiness guards
and passes the actual capability into owned mutation paths. Public command
arguments, response DTO fields, cursor and export shapes are unchanged.
Exact source hashes are in `task-6-summary-ui-handoff-round1-inputs.json`.

The seven current Root gates pass, including909 Rust cases and15 affected
frontend contract tests. Independent scoped Task2 re-review and complete
Task6 integration remain pending; this candidate does not activate Task8/9.

## Commands and ownership

| Command | Tauri JSON arguments | Result |
| --- | --- | --- |
| `generate_summary` | `sessionId`, optional `profileId` | `JobView` |
| `list_summaries` | `sessionId`, optional `afterVersion`, optional `limit` | `SummaryPage` |
| `delete_summary` | `summaryId` | null/void |

The Rust public names are `session_id`, `profile_id`, `after_version`,
`summary_id`. Existing real Tauri command tests prove camelCase invocation
arguments; response DTO fields remain the actual snake_case serde fields.
Use the single desktop bridge and mirror the real wire DTOs.

Generate and delete require the admitted application epoch and the actual
completed, current same-facade SummaryDeliveryReady cached by App publication.
Local recovery alone cannot authorize either mutation. The actual owned
config/SQLite admission and delete mark also validate that same capability. Listing uses the safe local reader without that ready gate.
`Retry` invokes `generate_summary` and creates a new manual job/version;
it does not replay or mutate the failed automatic version. The prior
automatic permanent marker survives output deletion and manual retry.

`generate_summary` returns a job, not completed content. Its `object_id`
identifies the created Native summary when present. Track completion using
the independent `subscribe_intelligence` ticket and `JobChanged` stream,
then refresh the summary page. The session channel may already be closed.
Existing job commands are `get_job`, `list_jobs`, `cancel_job`; do not invent
`get_intelligence_job` or `list_intelligence_jobs` aliases.

## Exact view union

`SummaryView` is a flat serde union tagged by `origin: "native" | "legacy"`.
The tag represents storage/projection origin. It does not encode automatic
versus manual generation; version1 is not evidence of an automatic source.

Native fields are `id`, `session_id`, numeric u32 `version`, `status`,
i64 `created_at_ms`, nullable `content`, boolean `deleting`, nullable
`profile_id`, `model`, `error`, `input_final_count`, `input_hash` and
`raw_response_hash`. Native status/error mirror the shared snake_case enums.
There is no exposed `job_id`, raw response body or `job_payload_hash`.

Legacy fields are `id`, `session_id`, exact decimal-string `version`, original
string `status`, exact decimal-string `created_at_seconds`, nullable
`created_at_ms`, nullable general-JSON `content`, and nullable
`projection_error`. The struct also serializes nullable `profile_id`, `model`,
`error`, `input_final_count`, `input_hash`, `raw_response_hash`; its Legacy
projection supplies None rather than fabricating Native metadata. Mirror
those serialized nulls rather than inventing populated values or a Native
`deleting` field. An invalid Legacy projection remains visibly Legacy with
its error; it is not parsed as strict Native content.

## Content and paging

Native content has exactly seven fields: `summary`, `participants`,
`companies`, `requirements`, `action_items`, `risks`, `terms`.
`ActionItem` is `{text, owner: string|null, due: string|null}`;
`Term` is `{term, meaning}`. `owner` and `due` may be omitted in model input
but serialize as nullable output fields. Display all fields; do not rename
`text` to `task` or `meaning` to `definition` at the wire boundary.

`SummaryPage` is `{rows, next_cursor}`. Both `afterVersion` and `next_cursor`
use a canonical signed-i64 decimal string of at most20 bytes. None/no
argument has no lower bound and includes negative Legacy versions. Omitted
limit means50; explicit limits are1..100. The production SQL orders by
version ascending, uses a `limit+1` sentinel and may end a bounded wire page
earlier at its byte ceiling. Always follow the returned cursor until null
when a view needs the complete local list.

Do not pass cursors through Number, parseInt or JSON numeric conversion.
Exact comparisons may use BigInt locally while invocation and cached cursor
keys keep the original canonical string. Native u32 versions are exactly
representable by a JavaScript number. Legacy versions and original seconds
can exceed the JavaScript exact-integer range.

## Export and presentation limits

Export is generated completely by the backend and written through its safe
destination-preserving flow. Its Legacy compatibility shape retains numeric
signed-i64 versions and original seconds exactly in the produced bytes.
Do not rebuild an export by parsing/restringifying the display DTO in React.
The complete projection and rendered artifact have their own accepted bounds.

Display/compare versions is read-only. A failure/retry keeps the earlier
version and metadata visible. Raw model responses, admitted raw job payloads,
the backend-only witness, credentials and audio never enter frontend output
or export. Hash metadata is not a raw response body.

Retain the approved framed-sheet/shadcn interface and the existing bounded
job controller contract. Task8/9 consume these actual Rust producers with shared wire fixtures for
large signed cursors, Legacy errors, every Native content field, manual
Retry and completion after the session channel closes.

## Automatic lifecycle and readiness

Automatic delivery uses one worker, channel32, one owned admission child,
32-row keyset scans and a200ms rescan. Successful terminal lifecycle and
pending intent commit together; notification follows stop publication and
active release. Ordinary audio stop completes independently of LLM work.
Startup, overflow and closing disposition produce local retries without
paid replay. The permanent automatic marker survives output deletion.

Window close uses the elected key and delivery UUID through timeout/retry,
with actual runtime/native audio join before receiver EOF, outstanding permit/
child/delivery joins and then final direct/index retirement. An error result
is separate from actual resource completion; failed durable effects require
real same-key repair. A final narrow facade join does not publish complete-
application success or readiness.

CV-1's sole200000 coherent Native-record recovery measurement returned
Timeout at10.000312414s, readiness closed and zero provider calls. Successful
recovery at that size is unproven. If a captured executor is unavailable or
cleanup completion is unknown, the original ownership fence remains held.
Three-platform real audio, paid-service quality and signed installation
packages remain external acceptance gates.


Current same-key terminal repair finishes before the fresh local delivery
tail. A genuine successful repair receipt may survive a later tail failure
under the original key, while successful reopen still requires the current
tail and all actual barriers. Zero-entry Pulse rejection releases only its
claimed lease; actual entered cleanup and unrelated quarantine remain
independent. Runtime create/resume consume actual App readiness publication.
