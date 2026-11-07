# Controller spec coverage — current ten-task plan

Checked against docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md and current docs/superpowers/plans/2026-11-04-workbench-intelligence.md after e73e25d. This maps approved scope to implementation tasks; it does not claim unfinished features or external acceptance passed. The independent pair/self-consistency preflight is preflight-current-10-tasks.md. Final runtime6B wire/API/schema reconciliation is still pending.

| Approved requirement | Owning implementation / verification | Remaining gate |
|---|---|---|
| Windows10 22H2, macOS13, Ubuntu22.04; PulseAudio/PipeWire | Reviewed audio/platform layers; Task2 driver facade/settings; Task10 packages and release checks | Actual minimum-OS installation, permissions, M1/M2 routing and signed packages |
| Live selected input→M1; independent call A→M1 and B/M2→physical output | RuntimeTasks2–5; Task1B bridge; Task2 start/stop/resume and route UI | Runtime6B commands, real workbench tests and hardware acceptance |
| No face-to-face v1; explicit language pair; no silent default-device changes | Binding runtime/workbench constraints and Task2 launch controls | UI/control review, actual OS checks |
| Volc AST real adapter; Aliyun/Palabra honest unvalidated capability state | Reviewed provider layer; Runtime6B capability/config commands; Task2 provider selector | Real Volc credential/quality/latency gate; stub labels and disabled actions in UI |
| Source and translation rows retain direction, sequence, epoch, correlation and supplied timing | RuntimeTask1 durable sink; Task1B exact wire/reconciliation; Task2 stream; Task9 history/export | Frontend contract/ordering/recovery tests; no fabricated sentence pairing |
| Bounded capture/playback/provider queues, explicit recovery and peer independence | Reviewed runtime/audio layers; Task2 actual pause/resume state | Command/UI integration and native failure acceptance |
| Complete final text, context, diagnostics; no audio/key output | RuntimeTask1 storage/export; Task5 lifecycle; Task1B recovery; Task9 history/export | Full wire/export secrecy and complete paging tests |
| Latency segmentation with unavailable stages unknown, no acoustic claim | Runtime measured diagnostics; Runtime6B shipped wire/accounting docs; Tasks2/9 rendering | Exact DTO units/nullable metadata reconciliation after6B |
| Encrypted local credentials, write-only key entry, no plaintext/log leaks | Reviewed6A; active6B policy/compensation; Task3 service config; Tasks2/8 forms | App/command failure matrix and frontend save/clear/reload checks |
| Configurable LLM/Embedding profiles and defaults; explicit bounded probes | Task3 DTO/config/http/jobs/probes; Task8 model/settings UI | Real service-port tests and settings behavior |
| Automatic terminal summaries and manual reruns, strict JSON, complete final input | Task6 atomic job/version admission, full snapshot hierarchy, raw-response retention | Local failed/restart marker tests, complete coverage and stop independence; paid acceptance separate |
| Summary versions view/compare/delete and structured export | Tasks6/9, durable version counters and history projection | No regenerated-on-view output, monotonic versions, complete export |
| PDF/DOCX/text/Markdown managed import; preserve original files; no OCR | Task4 parser worker/resource limits, document versions; Task8 import/replace UI | Offline real document worker + native packaged worker/OS-limit gates |
| Pinned SrvDB, SQLite authority, model identity isolation, recoverable indexing | Task5 actual pinned crate and bounded index owner/reconciliation | Real128-dimensional index/reopen/crash tests and generation mapping |
| Manual selected-text or query search with source parameters | Tasks2/5/8; e73e25d makes submitted query exact and IDs provenance-only | Partial-row/multi-row command tests; no automatic subtitle/keystroke search |
| Confirmed public customer research using Tavily; stored source URL/excerpt/time | Task7 local candidates, confirmed request, source-first storage and cited overview; Task8 UI | No pre-confirmation request; failed overview preserves sources; paid acceptance separate |
| Honest legacy metadata and local data management | Tasks3/4/7/8/9; nullable updated_at and actual data-directory producer added e73e25d | Shared fixtures for missing fields, empty-library directory view, safe delete scopes |
| Approved Taste2.1.5 framed-sheet/shadcn, Chinese UI, responsive sheets | Task1A reviewed complete; Tasks2/8/9 populate it; Task10 visual/interaction gates | Preserve shell; inspect populated desktop/tablet/mobile views with actual states |
| Recoverable jobs outlive session subscriptions; deletion cancels/join owners | Task3 scheduler, Tasks4–7 consumers, Task9 deletion, Task10 smoke | Bounded cancellation, no resurrection, job and result transactions |

No approved feature in this scan lacks an assigned task. This is planning coverage, not proof of implementation. Keep the controller checklist's final interface/self-review item open until final runtime6B DTOs, argument envelopes, subscriptions and schema version are reconciled. Retain all existing task and external gates.
