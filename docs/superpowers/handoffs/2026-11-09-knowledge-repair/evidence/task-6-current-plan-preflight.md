# Task6 current plan preflight — preparation only

Plan: `docs/superpowers/plans/2026-11-09-session-summary-continuation.md`
(548-line amended plan). Root read its complete current text, canonical product
requirements and binding interface amendments after recovering the Task5C
repair review. This is an interface/source preparation record, not source
activation, a behavioral test result or an independent verdict.

Current HEAD is `b725560`; the preceding knowledge repair tree is `da237389`.
Neither is an independently accepted repaired BASE: N1–N3 remain in fix round2.
No Task6 Rust/Cargo author has been dispatched. Recheck the exact accepted
source/commit and producer signatures before activating this plan.

## Shared-task interface scan

| Pair / shared producer and consumer | Current plan reconciliation |
| --- | --- |
| Manual1 → Automatic2: service/admission/counter | Manual owns transport, planner, Native proof, version store and create_summary_job_in_tx. Automatic reuses these; permanent Existing precedes counter/config failure and changes no version. Manual retries keep NULL permanent identity. |
| Manual1 → Automatic2: recovery readiness | Manual produces core/application-epoch SummaryReady and SummaryRecoveryFence without delivery UUID. Automatic consumes that local readiness and adds its journal/bootstrap and exact delivery-generation gate. JobAdmissionFence remains independently required. |
| Manual1 → Automatic2: ConfigIo and HTTP | Manual produces selected-or-explicit LLM completion gate, actual owned request4 and opt-in bounded context-error policy. Automatic performs admission under its retained child1; it does not duplicate credential resolution or replay metered calls during startup. |
| Manual1 → Automatic2: migration6 and legacy data | Manual creates the complete migration6 metadata, response/coverage/counter and fixed journal columns. Automatic writes the reserved journal fields; it does not allocate another migration. Legacy signed values, seconds/status/JSON remain separate from Native metadata. |
| Manual1 → Automatic2: terminal and actual reap | Manual anchors trusted job UUID plus independent raw payload witness and uses the already accepted separate JobReapPort registry. Automatic registers no second reap implementation and preserves the supplied terminal transaction's control-lock composition. |
| Manual1 → Automatic2: deletion/startup/close | Both share persistent deletion fences and actual-attempt16/reap checks. Manual local recovery precedes admission; automatic adds local-only journal bootstrap. Window close adds actual audio acknowledgment before delivery EOF/jobs/direct/index teardown, while ordinary stop retains its approved order. |
| Manual1 → Automatic2: view, command and export contract | Manual owns tagged Native/Legacy SummaryPage and signed decimal SummaryCursor, omitted limit50/range1–100, and complete bounded exports. Automatic leaves command shapes unchanged and only adds versions through the same store. |

## Internal task scan

| Task | Files, code responsibilities and specified tests |
| --- | --- |
| Manual1 | New llm/summary modules, job_proof/recovery, migration6, command, borrowed snapshot and export responsibilities align with its tests. Real producer fixtures cover admission/counter rollback, complete Unicode parts, strict/raw durability, phase-by-phase Native proof drift, entered cancellation and legacy exact paging. Automatic notification/audio ports are reserved for Task2. |
| Automatic2 | Terminal journal/reservation, one joined delivery owner, actual backend/process parents and close composition are assigned to explicit files. Tests cover positive native/process join independently of cached results, retained observers/helpers/permits, exactly-once restart, disabled/config-unreadable/exhausted outcomes and same-key close retry. No paid/hardware tests or ordinary-stop join expansion are authorized. |

## Current producer check and next handoff

- The source currently has JobContext operation_owner/publication_fence/commit,
  admitted payload storage, selected model settings and opaque JsonHttp; the
  amended plan names their narrow extensions and explicit existing Generic
  request literals. These proposed extensions are not yet implemented.
- Current TypeScript intelligence wire contains model settings and job DTOs,
  with no existing SummaryView/list_summaries decoder to preserve. Task1 must
  provide real backend command/serde projection fixtures; Task8/9 consume the
  newly frozen tagged view/cursor rather than the old mandatory Native draft.
- Current committed migration dispatcher is at version5. Accepted summary
  owns6 and research owns7. Only completed-upgrade fixture version literals
  advance; rollback and legacy preservation assertions keep their meaning.
- The binding amendments supersede the old three-internal-unit activation
  suggestion, bool/control-mutex is_reaped proposal and generation-absence
  inference. The active deliveries are complete manual1 then automatic2.
- Task7's existing interface preflight is preserved separately. Its Tavily
  selection, research-specific Native proof/async deletion and legacy cursor
  seams are consumers to reconcile at the accepted Task6 BASE.

No new product ruling or design approval is needed from this scan. The pending
knowledge repair review remains the activation gate. Root will create this
plan's own ledger/task brief at accepted BASE and dispatch one source/Cargo
author, with the complete constraints and persistent reports.
