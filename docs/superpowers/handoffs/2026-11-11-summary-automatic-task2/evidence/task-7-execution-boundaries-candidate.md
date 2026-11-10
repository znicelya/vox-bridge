# Task7 execution boundaries — preparation, no source activation

Binding product requirements remain the approved parent plan Task7, its
archived source task and the accepted workbench design. Read those exact
requirements first. This document records the current producer constraints
found while the complete Task6 review runs at frozen TREE
`d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`. It creates no Rust, migration,
network request or independent acceptance. Activate only after complete Task6
acceptance and a recorded accepted source BASE.

## Current producer interfaces

Use `JobContext::{operation_owner,publication_fence,commit}` and the actual
`JobReapPort::prove(&Transaction, job_id)` producer. A status, process-kill
request, result cache or deleted row does not replace actual release proof.
Entered ConfigIo closures retain the genuine operation owner after observer
drop/timeout. Use `ServiceConfigStore::scoped(owner)` for async admissions
and credential resolution.

Add `with_tavily_selection_completion` using the existing
`with_selected_embedding_completion` signature shape and config→SQLite→
completion order. Read selected settings and the correct-kind profile in
one owned ConfigIo closure, resolve an immutable public ModelProfile and
admit inside that closure's SQLite transaction. Resolve the saved full
profile's opaque credentials for each actual request. The saved Tavily URL
is the full `https://api.tavily.com/search` endpoint; do not append another
`/search`. Use `HttpRetryPolicy::Never` and `JsonErrorPolicy::Generic`.

The accepted LLM uses genuine `LlmExecution::new(owner, cancel)` and
`validate_request_budget`. Its request fields are `system`, `user` and
`max_output_tokens`; response fields are `content` and `response_hash`.
Overview admission selects the explicit/default LLM independently of the
automatic-summary setting. No config/SQLite lock crosses HTTP.

Public Tauri arguments/results retain the exact parent-plan shapes. Internal
mutations are owned variants; the illustrative unowned service signatures
are not authorization to start detached production work. The App's current
cached `SummaryDeliveryReady` proves completed startup, and must be checked
again at actual admission/delete-mark effects. Research's own local recovery
and current epoch remain a separate readiness condition. Closing fences both.
Neither an early delivery Live flag nor only local readiness authorizes work.

## Job identity and version boundaries

The accepted generic admission code maps Research's object ID to its run
and ResearchOverview's object ID to `research-overview:{object_id}`.
Use the run ID for ResearchOverview's object ID, giving one unfinished
overview per run. Its own version row is identified by the admitted job ID
and strict input containing both run and overview IDs. Do not change the
generic exclusion mapping to silently allow concurrent versions of one run.

Every Native search/version has one unique admitted job ID, exact original
bounded payload SHA256 in a backend-only witness, and strict kind/object/
session/profile/input association. Hash the actual helper-stored raw bytes
in the same admission transaction. Claim is anchored by trusted job ID;
fresh checks precede request/model entry, stored raw/parsed results and
terminal status copy. Invalid association does not update a foreign row or
downgrade it to Legacy. A missing result row cannot recreate it.

Allocate a checked u32 overview version and job together with the version
row and counter in one Immediate transaction. Created alone advances the
counter; deletion never resets it. A failed retry preserves successful
sources and all earlier versions. All research/overview jobs remain manual
with no permanent automatic-summary marker and no paid restart replay.

`ResearchRunView.overview` reads the latest successfully parsed version;
version lists retain later failures and their separate error. No usable
stored excerpt returns local InvalidResponse/re-search guidance before a
model request. Citation IDs must be from the actual usable source set
supplied to that request, not merely any source UUID.

## Storage, parsers and projections

Migration7 is research-owned; migration6 and its accepted manual/automatic
proofs remain intact. Preserve original foundation URL/title and NULL
provenance through two reopens. Native title/excerpt/fetched-time derive
from validated Tavily entries and the actual local fetch timestamp. A UI fallback
title/unknown time is not saved evidence. Keep raw failed overview and its
hash backend-only. No source/key/audio/raw model payload enters logs or DTOs.

Tavily raw envelope is at most8MiB and native usable results at most8.
Its parser must impose depth/node/string/collection bounds while parsing;
an already-unbounded `serde_json::Value` followed by a size check is not the
bound. Tolerate bounded documented top-level metadata, distinguish malformed
results from valid `results:[]`, and skip bounded invalid entries without
inventing/truncating a valid title/excerpt. Enforce parent-plan URL/title/
excerpt limits. Canonical URLs are dedup keys; save the original validated
source URL separately. Reject embedded credentials and unsafe schemes before
any external opening. A source click uses the system browser, never HTML,
remote previews or a direct crawler.

Overview uses its own two-field schema. Its content is exactly
`{facts:[{text,source_ids}],unknowns:[...]}` with the parent-plan strict
limits, no unknown fields, and every fact citing supplied usable sources.
Measure the actual complete escaped evidence request before calling the LLM.
A complete input that exceeds budget fails visibly; no successful prefix.

Bound all local readers before owned copies. Candidate scanning reads only
the most recent200 final rows/128KiB, with at most12 deduplicated candidates
and actual supporting segment IDs. A source/translation pair is not invented.
History/research/version pages retain their exact parent-plan count limits,
borrowed bounded fields and safe serialized budgets. Get returns at most8
sources. Legacy missing metadata remains absent, not fabricated Native data.

## Session deletion and local recovery

The current `SummarySessionDeleteGuard::delete_owned` performs the actual
final session DELETE inside summary `wait_scope`. Merely waiting for research
once before handing over that guard leaves a later authorization gap.
Compose research's persistent session/run/version deletion fence, actual
attempt-idle proof and fresh JobReapPort proof in that final same SQLite
transaction. Cover the summary local-recovery hard DELETE too, including a
persisted version6 deletion upgraded to version7. An optional missing
research dependency cannot authorize deleting Native research work.

Mark all affected domains before cancellation, keep the genuine original
owner/epoch/deadline until actual completion, and recheck fences in every
admission/publication. No joined-before-mark snapshot or row absence is a
cleanup receipt. Research run/version deletion joins its actual owned work
before DELETE; cancellation and failure retain prior successful evidence.
Use bounded local-only restart cleanup; interruption never calls Tavily/LLM.
Startup and close compose research's retained owners with the existing jobs,
summary delivery and direct16/index barriers. A failed branch must not
abandon another owned close branch or publish application readiness.

## Meaningful RED/GREEN and handback

Use actual LocalStore/ConfigIo/JobScheduler/reap/capabilities and controlled
external HTTP/LLM ports. Read TDD and writing-good-tests before tests. For a
new API, distinguish compile wiring from a behavioral assertion failure.
Preserve the exact command, complete raw result, real failure and source
association; do not retrospectively label successful tests RED.

Cover confirmation=false (zero jobs/calls), local candidates, bad URL/query/
foreign support IDs, atomic selected-profile admission, missing credentials,
malformed/oversize/empty/duplicate responses, 429/timeout/no retry, source
durability, complete overview budget, fabricated/missing citations, failed
raw retention, monotonic version/delete/retry, entered config/SQL observer
loss, startup/Closing fences, genuine session/run/version deletion and local
restart without calls. Native/Legacy wire cases must mirror all real fields.

One source/Cargo owner uses the already-accepted local environment. Run the
parent-plan research/storage/command checks plus the changed config/lifecycle/
summary collaborators, then required production check/build/fmt/diff gates.
No paid services, hardware, keyring or drivers in default tests. An unchanged
passing broad gate is not repeated absent an amended source or material gap.
Provide a cohesive full task report, exact owned-path manifest, source/evidence
handback and independent Spec+Quality review before accepting parent Task7.
