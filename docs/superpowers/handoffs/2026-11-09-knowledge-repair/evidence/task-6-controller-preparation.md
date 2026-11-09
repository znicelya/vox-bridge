# Task6 controller reconciliation — prepared 2026-11-09

Preparation only; no Task6 source/Cargo is activated. Task5C repair and its
independent source gate remain required. Canonical accepted feature text is
the archived task-6-source-plan.md; proposed detailed ports are in the
original intelligence workspace's task-6-architecture-checkpoint.md. This
note records technical reconciliation for later activation, not a new product
design or a claim that those ports already exist.

Root read the checkpoint's producer/ownership, config/admission, migration,
terminal/startup/close, snapshot, hierarchy, legacy projection/export,
active-deletion and verification sections and checked the current unmodified
jobs/store/config/session lifecycle producers. The ongoing repair is limited
to knowledge sources; compare the accepted final repair BASE again before
dispatching Task6.

| Decision | Ruling, reason and cost if wrong |
| --- | --- |
| A6-1 | Ruling: add the fixed pending journal fields/indexes in migration6 and use one joined terminal delivery worker, channel32, child1, keyset pages32 and coalesced rescan. Reserve with try_reserve_owned before the existing lifecycle commit; publish UUID only after runtime cleanup and stop completion. Overflow/close/uncertain startup creates local Busy/Interrupted retry, no cloud replay. — A volatile callback cannot preserve all committed terminations with bounded capacity and no audio stop backpressure. — Cost if wrong: migration/lifecycle/delivery work must be revised; it does not authorize an automatic metered restart. |
| A6-2 | Ruling: use opaque owned LlmExecution, one selected-or-explicit LLM ConfigIo→SQLite→control gate, post-commit event/wake ports and create_summary_job_in_tx. Factor/reuse the permanent lookup, read/check counter before unchanged admit_job, and write counter/version only on Created. No generic scheduler callback. — Existing must win without allocation, exhaustion must precede INSERT, and entered credentials/admission remain actually owned. — Cost if wrong: narrow internal composition changes; job/result/counter atomicity and resolver comparisons remain binding. |
| A6-3 | Ruling: use the unshipped native/legacy tagged SummaryView and exact canonical signed-decimal cursor≤20 bytes. Native keeps u32/JobStatus/actual milliseconds; legacy retains signed version/status/seconds/readable JSON and unknown native metadata. — Existing legacy complete/{title,text}/{text}/signed values cannot honestly inhabit mandatory native fields. — Cost if wrong: Task8/9 decoder and projection work, with stored legacy rows unchanged. |
| A6-4 | Ruling: summary/session deletion is async, fences before cancellation and checks actual-attempt registry16 plus exact-generation nonmutating reap predicate within10s before removal. Timeout keeps row/fence. — Terminal job status cannot prove an entered resolver or claimed-but-unentered handler is finished. — Cost if wrong: the cancellation/deletion ports need revision; no late raw/result INSERT/UPSERT is permitted. |
| A6-5 | Ruling: local initial Interrupted retry is Failed/error=Interrupted, matching admit_job; recovered running jobs preserve actual Interrupted. Seed counters with max(0, legacy MAX), retain >u32 positive MAX as exhausted, and advance the legacy writer's counter in its existing transaction. At exhaustion, persist only a truthful local journal error if possible, with no new job/version/marker. — Casts/status rewriting would reuse versions or invent a persisted result. — Cost if wrong: fixture/projection work; signed legacy storage is preserved. |
| A6-6 | Ruling: adopt the checkpoint's actual copying/transport/list/export bounds; keep explicit probe saved-ID/revision behavior. A bounded64KiB allowlisted context-error classifier may be enabled for compatible LLM requests without changing unrelated generic HTTP callers' existing error policy. — Counts alone do not bound bytes or sorting; legacy/native exports must include every version/field or fail whole. — Cost if wrong: internal bounds/opt-in HTTP policy need revision; no prefix summary/export or raw error output is allowed. |

## Limits to carry verbatim at activation

Snapshot:100000 final rows,16MiB once-normalized versioned JSON, separate16MiB
borrowed metadata parser-work budget, checked row copies and same-session
rowid high-water; short owned read≤5s, released before network. Bundled SQLite
must prove the session/rowid index avoids a global sort/scan.

Planner: actual system/user escaped UTF-8 bytes+1024≤context-output; complete
immutable Unicode-boundary part manifest before the first call, exact expected/
supplied/retained coverage tuples, total≤64 calls/≤8 levels. No pair/no progress/
capacity/context failure is visible ContextTooLarge. No omitted row/part/child.

LLM: four actual request scopes; request≤1MiB, HTTP envelope≤8MiB, extracted
content≤2MiB; no automatic request retry; raw≤2MiB each/32MiB per summary and
actual map coverage atomically retained before strict schema parsing. Full
saved ModelProfile and secret association rechecked in one owned config gate.

Views/export: at most100 rows/default50; native row≤2MiB+64KiB; legacy content
≤2MiB with explicit projection_error, original legacy status≤64KiB; page≤8MiB;
legacy JSON depth64/nodes100000/decoded string+key bytes2MiB. Complete bounded
summary export projection≤32MiB and whole rendered artifact≤128MiB, preserving
destination on any error. Raw response fields/keys/audio never enter export.

Activation should split implementation coherently into core transport/schema/
snapshot/planner, then service/admission/terminal ownership/deletion, then
bounded projection/commands/export; never dispatch concurrent source authors
over shared files. Every split must have its own concrete behavior tests and
independent gate without claiming the complete Task6 before integration.

All decisions need transfer into the canonical intelligence ledger and active
Task6 brief at the accepted repaired BASE. They are currently preparation;
Tasks6–10 remain outside the active repair's source ownership.
