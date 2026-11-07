# SDD ledger — plan: docs/superpowers/plans/2026-11-04-workbench-intelligence.md

## Pre-flight (prepared while provider work runs; no UI implementation dispatched)

| Tasks | Shared boundary | Finding / ruling |
|---|---|---|
| 1 → 2/3/4/5/6/7 | component registry, tokens, test harness | Initialize official shadcn primitives and tests once; add later required primitives when consumed. |
| 2 ↔ 3 | App navigation, active session and history | Both modify App; execute sequentially, preserve stable active-session ownership. |
| 2 ↔ 4 | selected subtitle knowledge search | Pass selected text/segment ID through typed callback to search command; no automatic retrieval. |
| 2 ↔ 6 | entity candidates and confirmation | Candidate suggestion stays local; explicit confirmation is required before Tavily network request. |
| 3 ↔ 5 | session detail and summary versions | Final text source must distinguish original/translation and direction; summary version controls consume persisted versions. |
| 4 ↔ 7 | configured embeddings/secrets | Backend-only config/keys; index partition by model/dimension and SQLite authority. |
| 5 ↔ 7 | configured LLM/secrets | Full configurable endpoint/model/timeouts/context contract must exist, not UI-only fields. |
| 6 ↔ 7 | Tavily endpoint/secrets | Backend performs authenticated request after confirmation, saves sources before overview. |
| 4 ↔ 6 | shared commands/app state | Sequential changes; preserve command registration and resource ownership. |
| 1/2/3/4/5/6/7 → 8 | build/fixture/UI smoke | Mock boundaries only in tests; product cannot present fixture data as a live connected session. |
| 1 self | commands/list of files | shadcn generated aliases/components and Vite/TS configuration changes are necessary despite omitted file entries. |
| 2 self | tests omit required callbacks | Adapt tests to real typed props; active mode/role/status claims verified through behavior. |
| 3 self | metadata requested vs foundation SessionRecord | Add backend fields/commands/migrations as necessary, not placeholder provider/title/duration. |
| 4 self | parse fixture document.chunks vs parser/chunker layers | Test resulting import pipeline and source metadata without forcing duplicate chunk models. |
| 5 self | cargo test llm summary command | Cargo accepts one test filter; run separate focused filters or pass libtest arguments correctly. |
| 6 self | search signature ConfirmedEntity vs unconfirmed test | Enforce confirmation in API command conversion and typed domain boundary; test rejected unconfirmed requests at that boundary. |
| 7 self | five UI sections vs missing backend CRUD/tests | Wire real secure commands and service tests; no success-only placeholders. |
| 8 self | required release build vs absent signed drivers | Run production frontend/Rust and local fixture UI checks; packaging/real acceptance remains gated and reported. |

## Rulings

- Ruling: pin Taste to v2.1.5 (`12af471f627f866411b4bd785b19646d3672784c`), retaining the approved app-screen/framed-sheet pipeline. Upstream 2.5.0 released 2026-11-05 explicitly replaced that pipeline with marketing-site rules. Cost if wrong: future skill migration/revalidation, not redesign of the approved UI.
- Ruling: plan file lists are ownership guidance, not a ban on necessary module/command/test/alias wiring; keep additions minimal and reviewed. Cost if wrong: additional touched files require reviewer attention.
- Ruling: use real state/metadata and backend actions; extend foundation interfaces where the UI requirements expose a gap. Cost if wrong: migrations or type changes may need rework.
- Ruling: the external driver/OS/credential gates remain pending until actually exercised; local fixture code development proceeds within the existing user authorization. Cost if wrong: hardware integration may require rework.

## Tasks

Tasks 1–8 queued; provider adapters and runtime composition must complete first.

2026-11-06 scope update: the current written plan has10tasks after prior refinement; the older preflight table above is historical and must be reconciled before full execution. Dependency-independent Task1 shell/tooling subset1A is now executed under docs/superpowers/plans/2026-11-06-frontend-shell.md with its own ledger/review. Real runtime bridge/wire recovery remains1B and requires reviewed runtime6; no workbench task is complete solely from the shell. Source plan updated byc61ac69; backend/runtime APIs and intelligence scope unchanged.

## SrvDB pinned-source preflight

- Exact revision checked out read-only under `.superpowers/knowledge-source/srvdb`: `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`. Rust crate is workspace member `crates/srvdb`, version0.3.0, MSRV1.99, Apache-2.0.
- Public API: `SrvDB::new_with_config(path: &str, DatabaseConfig)`, trait `VectorEngine::{add,add_batch,search,get_metadata,persist,count}`, plus `SrvDB::{delete,replace,is_deleted,live_count,all_metadata}`.
- `DatabaseConfig::new` permits dimension128–4096. Knowledge fixtures must use a valid dimension; user-configured incompatible dimensions must return an actionable error, not silently pad/truncate vectors or change dependency revision.
- `replace(old_id, vector, metadata)` tombstones old record and appends a NEW u64 ID; SQLite must save the returned ID. `count()` includes tombstones; `live_count()` is searchable count. Deletes become durable after persist().
- Search returns cosine similarity (higher is better), not probability; preserve score semantics in UI. Metadata is an optional JSON STRING and hydration must verify identity/hash against authoritative SQLite rows.
- Ruling: implement wrapper upsert through actual add/replace/delete APIs and update SQLite vector IDs from returned values; respect documented constructor dimension range. Cost if wrong: index reconciliation or allowed embedding-model choices require adjustment.

## Current ten-task preflight, 2026-11-06

Independent read-only report: preflight-current-10-tasks.md (workbench_preflight_refresh, gpt-6-astra/high). Controller read the exact query/summary/DTO producer and consumer text and confirmed four bounded corrections. This supersedes the older eight-task dependency table for dispatch; no feature completion is implied. Runtime6B final API reconciliation remains pending.

- Ruling: N1 uses submitted KnowledgeSearch.query as the exact explicit embedding text; selected IDs validate session provenance without expanding it to whole rows — this matches approved partial selection and editable search — cost if wrong: selection contract/fixtures need revision, with no wider text transmitted silently. Add single-row partial and multi-row command tests.
- Ruling: N2 startup summary reconciliation includes both Completed and Failed, including interrupted recovery with final text, and creates only local Interrupted/retry records under the existing automatic-marker policy — normal and crash paths must cover the same terminal sessions — cost if wrong: eligibility tests/policy need adjustment; recovery still performs no paid replay.
- Ruling: Apply the existing real-metadata rule to M1 by projecting nullable durable DocumentSummary.updated_at_ms and updating it on committed document/version/status changes — UI last update cannot be reconstructed from creation time or visible jobs — cost if wrong: timestamp semantics/fixtures need migration-compatible revision. Unknown legacy times remain None.
- Ruling: Apply the existing real-metadata rule to M2 by assigning Task3 a read-only get_managed_data_location view of actual AppState.data_dir, consumed by Task8 — the empty-library settings page otherwise has no producer — cost if wrong: reuse/rename after final runtime API reconciliation; no new file-access permission.

## Refreshed pair and self-consistency tables

Each unordered task pair appears once. The command registry/AppState composition and shared wire/fixture infrastructure make even otherwise separate backend services share an integration boundary. Rows record concrete coordination, not permission to edit another active task's files. Numeric task labels here are exclusively the current ten-task plan.

| Pair | Shared file/interface and plan references | Required handoff / disposition |
|---|---|---|
| 1 ↔ 2 | `App`/`AppShell`, desktop port, stream state, runtime command contracts (P:77–107,149–159,173–175) | Reuse completed 1A content slots; 1B supplies real stream/fixtures after runtime 6; Task 2 owns active workbench state. |
| 1 ↔ 3 | Command-module registration, bridge wire conventions, application ticket/error contracts (P:77–79,237–303,366,437) | Preserve runtime fixtures while adding intelligence namespace; stable subcodes survive the actual command envelope. |
| 1 ↔ 4 | Command registration and shared document wire/fixture infrastructure (P:77–79,486–524,894) | Task 4 produces DTOs; Task 8 extends the same bridge/contracts, including M1, without inventing native paths. |
| 1 ↔ 5 | Shared desktop command contract/fixtures (P:79,604–635,894) | Search/reindex/delete use real Task 5 DTOs and command envelopes; N1 needs one agreed selected-text meaning. |
| 1 ↔ 6 | Session identity, command registration, summary wire fixtures (P:79,107,697–753,894) | Summary jobs use application events; live stream cleanup must not own or cancel automatic summary lifetime. |
| 1 ↔ 7 | Command registry, desktop port and research contracts (P:79,805–851,894) | Preserve nullable legacy provenance and explicit confirmation in the shared simulator/decoder. |
| 1 ↔ 8 | `wire.ts`, fixtures, desktop port, shell/routes and test setup (P:77–79,157–159,892–894) | Extend the existing bridge with intelligence DTOs and an independently cleaned-up job controller. Do not reinstall shell tooling. |
| 1 ↔ 9 | `App` routing, transcript paging and wire fixtures (P:149–159,940–979) | History reads durable pages independently of the live 500-row window; add history DTOs at the same boundary. |
| 1 ↔ 10 | Package/build files, fixture desktop, production App/bridge (P:77,995–1003) | Add explicit test-ui entry/build separation; preserve production empty/browser behavior and completed shell assets. |
| 2 ↔ 3 | AppState/command registration, provider settings/probe command (P:173,223,237,298,472) | Task 2 initially labels local config checks truthfully; Task 3 produces bounded actual probes; Task 8 completes settings consumption. |
| 2 ↔ 4 | AppState/registry and native process/platform boundary (P:173–191,486,554–562) | Integrate sequentially; preserve driver owner semantics while parser adds a distinct limited-process mode. No driver action at startup. |
| 2 ↔ 5 | SelectionContext → KnowledgeSearch; AppState/registry (P:175,211,219,599,612–635,684) | N1: full-row ID derivation cannot silently replace a partial selection. Membership validation remains required. |
| 2 ↔ 6 | Active session identity, AppState/registry and terminal lifecycle (P:173,215,697,790) | Stop result remains independent of LLM completion; the terminal hook belongs to Task 6 after reviewed runtime ownership. |
| 2 ↔ 7 | Selection/session/supporting IDs and AppState/registry (P:175,219,805,810–816,873–875) | Local selection/candidate generation never admits Tavily; explicit confirmed request validates supporting IDs. |
| 2 ↔ 8 | `ToolRail`, `App`, settings routes and selection (P:173–175,219–223,892,920–926) | Task 8 replaces tool empty states with real tools and completes five groups; preserve reachable stop, no automatic search, and N1. |
| 2 ↔ 9 | App navigation/active session; session deletion (P:173,215,940,973–977) | Keep one active-session owner across route changes; reject active deletion; history navigation never stops audio. |
| 2 ↔ 10 | Workbench/settings controls, responsive sheets, driver capability/release configuration (P:191,215–223,995,1019,1031) | Smoke exercises actual controls through DesktopPort; browser evidence cannot assert real audio or privileged driver success. |
| 3 ↔ 4 | AppState/registry, migration dispatcher, config/jobs/RecordOrigin (P:237–321,429,486–524,548,585) | Migration 3 precedes 4; register one Import handler before start. Document admission uses its shared exclusion key. |
| 3 ↔ 5 | AppState/jobs, config snapshots, HTTP/secret ports, knowledge mutation admission (P:321–370,429–437,599,633,658,680) | Index consumers use opaque credentials and immutable identity; deletion cleanup intent survives document cascade. |
| 3 ↔ 6 | AppState/migrations/jobs, `admit_job`, automatic markers and atomic version allocation (P:372–437,697,739–751,778–790) | Reuse the transaction helper; automatic failures bypass unfinished capacity only as specified. Global migration order still requires Task 4 before 6. |
| 3 ↔ 7 | AppState/migrations/jobs, config/HTTP, RecordOrigin (P:237–303,321–437,805–879) | Research and overview handlers register before start; source/overview rows and admissions use durable version/cancel conventions. |
| 3 ↔ 8 | Config/probe/job DTOs and application subscriptions (P:244–303,433–472,892–926) | Consume actual commands, bounded job recovery and write-only keys; M2 needs an explicit location producer. |
| 3 ↔ 9 | LocalStore/jobs and session-delete cascade (P:413,427–437,940,973–979) | Cancel/wait for owned jobs before cascade; terminal automatic markers live until session deletion, not summary deletion. |
| 3 ↔ 10 | Package/registry/CSP and configuration/job fixture boundaries (P:237,439–472,995–1043) | Default tests inject credentials/network; production contains no fixture secret selector or frontend service network path. |
| 4 ↔ 5 | `knowledge` store/documents/commands, parsed chunks and Import continuation (P:486–524,548–585,599–680) | Attach indexing continuation during composition, before scheduler start. Continue under the admitted document owner rather than enqueueing a colliding mutation while Import holds its key; no second Import handler. |
| 4 ↔ 6 | Migration dispatcher, LocalStore, AppState/job composition (P:486,548,697,778,1051) | Integrate migration 4 before 5 despite independent parser/LLM service logic; retain separate modules/handlers. |
| 4 ↔ 7 | Migration dispatcher, LocalStore, AppState/registry and RecordOrigin (P:486–548,805,871,1051) | Preserve document legacy migration while research adds version 6; both consume shared RecordOrigin from Task 3. |
| 4 ↔ 8 | Document DTOs/commands, native dialog capability (P:486,498–524,892,920) | M1 adds real last-update projection. Dialog cancel is no-op; legacy paths remain absent and require explicit replacement. |
| 4 ↔ 9 | LocalStore/registry integration and ownership of managed documents (P:486,524,548,940,977) | Session/history deletion must preserve knowledge documents; source files and managed-copy lifecycle remain with document services. |
| 4 ↔ 10 | `main.rs`/`lib.rs`, Cargo/package config, worker entry and release gates (P:486,554–595,995,1031–1043) | Worker path must precede Tauri initialization; preserve package/native acceptance gates and pure-Rust decompressor verification. |
| 5 ↔ 6 | AppState/jobs and LocalStore/config composition (P:599,633,697,737) | Separate Embeddings and LLM ports; sequential wiring registers distinct handlers with no scheduler ownership cycle. |
| 5 ↔ 7 | AppState/jobs/registry, shared config/HTTP limits (P:599,633,658,805,849,877) | Knowledge search and Tavily are separate explicit operations; service limits/cancellation share the common transport boundary. |
| 5 ↔ 8 | Knowledge DTOs/commands, index identity/status/repair semantics (P:604–635,678–684,918–922) | N1 governs query/provenance; mixed collections show unavailable count and actual repair actions; model change does not reinterpret old vectors. |
| 5 ↔ 9 | LocalStore/registry and deletion scopes (P:599,680,940,973–977) | Document deletion retains its own durable cleanup; deleting a session cascades session intelligence only. |
| 5 ↔ 10 | Search/reindex command fixtures and index recovery documentation (P:635–684,1019,1043) | Smoke uses actual command transitions; recovery/identity/capacity assertions remain in real-index tests, not fabricated UI state. |
| 6 ↔ 7 | `LlmProvider`, summary-style version storage, AppState/migrations/registry (P:697–751,778–788,805,849–879) | Task 6 produces the LLM port before research overview; research versions preserve stored sources and independently validate citations. |
| 6 ↔ 8 | Summary settings/DTOs, application job controller (P:721–753,790,892–926) | Automatic/manual progress survives session closure; configured defaults and prior successful output remain visible. N2 changes local recovery eligibility only. |
| 6 ↔ 9 | Summary storage/list/delete, exports, history projection and cancellation (P:721–780,940–979) | Version counter/automatic marker survive version deletion; complete structured versions export; no raw responses or audio. |
| 6 ↔ 10 | Summary fixture flows, lifecycle/export acceptance and docs (P:755–798,1019,1043) | Smoke covers rerun/compare/delete; backend coverage/stop-independence tests and native service gates remain distinct evidence. |
| 7 ↔ 8 | Research command DTOs, confirmation/source/overview UI and external links (P:810–879,892–924) | Preserve nullable provenance, require explicit confirmation, retain sources after overview failure, and open only validated user-clicked links. |
| 7 ↔ 9 | Research storage/jobs and session-delete coordination (P:815,851,871,940,973–977) | Session-associated research jobs are cancelled/joined before cascade; independent manual research remains outside another session's deletion. |
| 7 ↔ 10 | Research command simulator, external-browser capability/CSP and documentation (P:875–879,995,1019,1031–1043) | Test candidate editing/confirmation and failed-overview source retention; no remote HTML/previews in WebView. |
| 8 ↔ 9 | `App` routing, wire fixtures/jobs, native dialog setup and exports (P:892–894,926,940–979) | Task 8 owns dialog registration/capability; Task 9 consumes save, handles cancel/error accurately and adds history contracts at the same bridge. |
| 8 ↔ 10 | Package/capabilities, tool/settings fixtures and responsive App (P:892,918–934,995–1043) | Smoke uses real settings/tools through fixture commands; CSP stays local, and test-ui modules remain excluded from production. |
| 9 ↔ 10 | History/export fixtures, routing, package/capabilities and docs (P:940–989,995–1043) | Cover history after stop, version operations and export cancel/failure; production reports actual successful paths only. |

## Task self-consistency table

| Task | Contract/spec coverage checked | Result / minimum follow-through |
|---|---|---|
| 1 | P:29,77–169; S:30,97–101 | 1A is complete. 1B still needs actual final runtime fixtures, bounded recovery and cleanup; no contradiction justifies repeating the shell. Unknown diagnostics remain opaque/bounded and must not terminate transcript recovery. |
| 2 | P:173–233; S:9–16,26,99–109 | Launch/stop/resume, independent route state, explicit driver actions and five visible settings destinations are covered. `SelectionContext` must retain the exact text required by N1. Necessary hooks/props are already covered by H:29. |
| 3 | P:237–482; S:20,30,101,123 | Config/secret isolation, admission capacity, permanent markers, startup interruption, application events and bounded HTTP/probes are internally specified. Stable subcodes must survive actual command errors; M2 assigns the missing data-location read boundary. |
| 4 | P:486–595; S:18,79–83,110,121 | Bounded parsing, local versions, managed ownership, legacy preservation and packaged-worker limits are specified. M1 supplies last-update projection. Task 5's continuation must integrate with the original Import handler, not duplicate its registration. |
| 5 | P:599–693; S:18,80,83,85–87,110,121 | Identity, embedding validation, actual SrvDB API, SQLite authority/recovery and bounded search are coherent. N1 resolves the only identified query-meaning contradiction; do not reinterpret similarity as probability. |
| 6 | P:697–801; S:17,78,91,111,120 | Full input coverage, failure/raw retention, atomic immutable versions and audio-independent scheduling are specified. N2 aligns startup handling with normal Failed/Completed eligibility. Migration 5 follows 4 under P:1051. |
| 7 | P:805–888; S:19,81,93,111,122 | Confirmation, deterministic local candidates, source-first persistence, legacy provenance and citation validation are covered. No new self-consistency gap found. Prior H:22's confirmation-domain issue is explicitly handled at P:857–865,875. |
| 8 | P:892–936; S:18–20,99–101 | Real tool/settings/job interfaces and local error handling are specified; N1, M1 and M2 are the identified consumer gaps. No generated fallback is to be persisted as real source metadata. |
| 9 | P:940–991; S:16–17,83,91,120 | Durable paging separate from live retention, version comparison/deletion, whole-session job cancellation/cascade and real exports are covered. No new self-consistency gap found; runtime `list_sessions` remains unchanged. |
| 10 | P:995–1045; S:113–124,135–138 | Offline fixture build, production exclusion, visual/interaction checks and release gates are explicit. Existing shell visual approval is retained; populated feature checks still run. Native/paid gates cannot be marked passed from browser fixtures. |


Preflight corrections committed as e73e25d (one approved plan document only). Controller self-review verified query producer/consumer agreement, terminal predicate/no-paid-replay consistency, nullable timestamp/UI fallback and read-only data-location producer. git diff --check and staged diff check clean; no code tests needed for prose/contract planning update. Final runtime6B shape reconciliation remains pending.

Controller current spec-coverage map saved in spec-coverage-current-10-tasks.md:19requirementgroups mapped to runtime/workbenchtasks and concrete remaininggates. No unassignedapprovedfeature identified inthisplanning scan. This is not implementationcompletion; finalruntime6B API/serde/schema reconciliation and requiredplatform/service/visualgates remainpending.

Prepared task-1b-brief.md with full remainingTask1 bridge/recovery requirements and explicit preservation of reviewed1A. NoTask1Bdispatch/sourcework yet; runtime6B review andfinal API/serde/schema reconciliation must precede it. global-constraints.md extracted verbatim forfutureimplementer/reviewer lens.

Carry current runtime6B I2 lesson into finalAPI reconciliation andTask3 ServiceConfigStore handoff: a Clear cleanuporphan mustnotbeconsumedmerelybecauseitsdeterministicIDentryexists. The resolver musthonor thepersistedcredential-reference/state policy beforelookup, matchingtheapproved runtimepolicy whilekeepingpreflightfreeofdecrypt/network. Runtimefixreviewstillpending; do notcopypre-fixconsumersemantics.

Task1B preflight closed: runtime Task6B reviewed at6365830. Controller read final runtime-api-reconciliation.md and verified command wrappers, source DTO/time/diagnostic declarations and fixture gap against code. No producer mismatch requires source repair; command arguments stay camelCase, nested serde unchanged, terminal runtime nullable. Shared contract corpus/filter belongs to1B, driver facade to2, managed location to3. Retain Clear-orphan reference-gate lesson forTask3.
Task1B BASE6365830 recorded. Existing frontend baseline: pnpm test:run src/features/shell, 5passed, exit0. Dispatch bridge/recovery only; preserve reviewed1A shell.

Task1B dispatched frontend_runtime_bridge_impl (gpt-6-astra/high, isolated), BASE6365830; only bridge/reducer/controller/simulator/shared contract corpus and narrow test registration. Initial import RED followed by retained behavioral wire/reducer/controller RED; implementer checkpoint reports28focusedGREEN, remaining late-ticket/error/timeout/overlap andRustcontract work inprogress. Nocompletionclaim.
Task2 prepared task-2-brief.md and read-only task-2-driver-handoff.md. Required integration seams include actual platform lease/quarantine maintenance exclusion, non-Send owner construction on owner thread, bounded runner/operation and acknowledged completion. Existing Linux CommandRunner::output is unbounded and raw-error-bearing; do not connect it directly topublic facade. Preserve packagegate and recheckinventoryafteruninstall. Driver/UIwork has notbeen dispatched.

Task1B implemented in49b568e, report-only d76029e followed by4a6220a restoring ignored/local report convention; net15source/test/fixture/config paths. Controller verified committed ownership and retained35focused/40allfrontend/1Rustcontract passing outputs, productionbuild andPrettier. FullVitest discovery now excludes non-Vitest independent scripts byinclude src/**/*.test.{ts,tsx}. No wire productionruntime changes; no shell redesign. Reviewpackage task-1b-review.diff covers6365830..4a6220a. Independent spec/quality review is next; native/service gates stay pending.

Task1B initial independent review: spec issues / qualityNeedsfixes;0Critical/4Important/1Minor. I1 epochfacts lostafterdisplayeviction;I2 unstablecontroller subscriber+missingfixture replacement;I3 supersededbufferedinterim survivesresync;I4 pending originalsubscribe/unsubscribe requestsoutlive localdeadline withoutbounded accounting. Reviewerread-only probes observedstaleepoch2 afterepoch3+500peerfinals, pre-resyncinterim reappearance,32outstanding subscribes and8unsubscribe requestsforoneticket. Controller verified code paths. M1fixture namespace corrected in samefocusedfix.
Task1B fix1 preparedtask-1b-fix-1-brief.md; reviewedHEAD4a6220a, integrationHEAD92565a9 (independent runtimefixexcludedfromfrontend re-review). Resumeoriginalfrontend_runtime_bridge_impl; do notdispatchTask2 untilreviewclosed.

Ruling: SplitTask2 into2A driverbackend and2B workbench/settingsUI so the backend can proceed after reviewed runtime92565a9 while1B frontend repair continues.2A has no bridge dependency; it produces the already-specified manage_audio_driver envelope/DriverStatus.2B still waits1B review. Ownership and Cargo are explicit; Task2 completes only afterbothreviews+integration. Cost ifwrong: cross-boundary integration/review rework, with no native acceptance or safety waiver.
Task2A BASE88bc5b4 recorded; scopedtask-2a-driver-brief.md + task-2-driver-handoff.md + globalconstraints. NotyetTask2complete.

Task1B fix1 checkpoint: six behavioralRED regressions retained. Controller accepts bounded4native-working/ticket-owningruns+1latest-onlywaitingstart, stablecontroller token andper-sessionoriginalsubscribe serialization; originalrequest accountingends onlyatactualsettle, retryunsubscribeonlyafteractualrejection. Pendinglatestwaitmusthavebounded/cancellationexit; workerwillrecord/test.
Task2A dispatched workbench_driver_impl (gpt-6-astra/high), BASE88bc5b4. It ownsbackenddriver/app/orchestrator/commands.rs/lib.rs, while1B retainsRustwirefixture/constructor. Cargo firstgoesto1Bnamespacegate,then2Aafterhandback. Await2Aarchitecturecheckpoint and1BfixGREEN.

Task1B fix1 implemented f989355,9ownedpaths. Controller verified complete appendedreport, actual48focused/53fullfrontend pass, build832ms, formatting and scoped checks; Rustnamespacecontract1pass wasverifiedbeforedriverCargo handback. All4Important+MinorM1 addressed byimplementer; awaitindependentscopedre-review. task-1b-fix-1-review.diff covers4a6220a..f989355 frontend-ownedpaths, excludesruntime92565a9/docs88bc5b4.

Task1B fix round 1/5: all I1–I4 and M1 addressed, 0 open; independent frontend_bridge_fix1_review read frozen nine-path diff and recorded no new breakage in task-1b-fix-1-review.md. Controller inspected review and retained evidence. Task1B: complete (commits 6365830..f989355; review clean). Task1: complete (reviewed 1A shell plus 1B bridge, without claiming native acceptance).
Task2A implemented 4a6f89c, 19 owned source/script/test paths. Controller checked HEAD/source list and full retained report: 26 focused, 436 library + 2 offline integration passing; 1 paid gate ignored; package/fmt/diff checks recorded. Independent workbench_driver_review dispatched on frozen task-2a-driver-review.diff (88bc5b4..4a6f89c owned paths only, excludes parallel f989355). Review pending, Task2 not complete. Cargo returned to controller.
Recovery note: only pre-existing .gitignore modification appends .taste-skill with missing final newline; leave it untouched and do not stage as source ownership.
Task2B BASE4a6f89c; fresh controller frontend baseline pnpm test:run passed 53/53 in seven files, exit0. Original workbench_ui_impl encountered an upstream Responses resource-routing error before any source changes; one retry had the same infrastructure error. Reassigned fresh workbench_ui_impl_retry (gpt-6.1-sol/high) with identical full brief and ownership; no completed work duplicated. Active driver review is read-only; Cargo remains controller-owned. Task2B implementation and review pending.
Task3 read-only handoff intelligence_foundation_handoff dispatched against existing Task3 brief and actual runtime ConfigIo/LocalStore/AppState/subscription APIs. It produces no source code and does not complete Task3. Task3 must preserve runtime safe command categories while exposing stable intelligence subcodes and reuse AppState::data_dir for the managed-data projection.
Ruling: Extend backend-only JsonRequest with HttpRetryPolicy { Never, OnceTransient }, since the approved transport signature otherwise lacks a way to distinguish chat from embedding/Tavily retry semantics without unsafe URL inference. Never applies to chat; OnceTransient permits only one 429/503 retry with validated Retry-After <=5 seconds inside the original total deadline and cancellation. Adapter retains private header handling. Cost if wrong: downstream request constructors and retry-policy tests require adjustment; no frontend DTO change or broadened retries.
Ruling: Intelligence command failures use [INTELLIGENCE_ERROR][snake_case_subcode] followed by a fixed local message; AppError Display remains fixed local text and all existing runtime error envelopes stay unchanged. This makes the approved stable-code retry/setup contract survive the real string command boundary. Cost if wrong: later frontend error decoder grammar must be revised; no service body, credential or URL is exposed.
Task2A: complete (commits 88bc5b4..4a6f89c driver-owned paths; independent workbench_driver_review spec PASS / quality APPROVE, no findings). Controller read full review/evidence; remaining cannot-verify items are explicit native OS/audio/package/paid gates and2B combined integration, not an unassigned backend gap. Full Task2 still waits2B.
Ruling: Task3 backend may execute after reviewed2A while2B frontend continues, preserving the plan's sequential shared Rust-module integration and explicit source ownership. Split3 into3A DTO/config/store/admission/opaque transport contract and3B scheduler/HTTP/probes/application composition, each independently reviewed; fullTask3 completes only after combined gates. Cost if wrong: cross-boundary integration and contract review rework; no premature Task2/3 completion.
Ruling: JobScheduler repeated start while running is idempotent; freeze handlers permanently after first start; permit a fresh worker generation only after acknowledged shutdown. Uncertain cleanup retains owner/process lock and fences new starts/admission. Host explicitly rearms startup in an async-runtime context after successful window reuse; Interrupted jobs never replay paid requests. This preserves existing AppState window reuse without releasing unfinished intelligence ownership. Cost if wrong: scheduler generation and host lifecycle integration/tests need rework.
Task3 runtime handoff closed: controller read task-3-runtime-handoff.md, confirmed current ConfigIo transaction, authoritative data directory, LocalStore migration/connection owner, provider probe cleanup seam and inert sync construction. No additional source prerequisites require redoing reviewed runtime/driver work. Task3A BASE4a6f89c; intelligence_config_store_impl (gpt-6-astra/high) dispatched with task-3a-config-store-brief.md, handoff and global constraints.3A owns only Rust/Cargo contract/config/store paths;2B keeps frontend/package ownership. Cargo handed exclusively to3A. Task3B actual HTTP/scheduler/probes/AppState/command composition remains queued behind3A review; fullTask3 not complete.
Task3A architecture checkpoint accepted: private versioned service document under ConfigIo; one LocalStore transaction owner/admit_job helper; versioned bounded public snapshots captured under config->SQLite lock; callbacks may capture scoped operation leases for3B but long-lived collaborators cannot hold AppState/scheduler cycles. Behavioral RED migration/config/admission sets are planned, not yet claimed green.
Ruling: Require explicit model/context/output for LLM profiles; embedding requires model and omits chat-only context/output; Tavily omits model/context/output/dimension and normalizes its documented default endpoint before persistence. Common Option fields represent service-kind differences rather than unlimited LLM defaults. Cost if wrong: profile validation/default UI fields and fixtures need adjustment; no unbounded request is introduced. Manual admission keys additionally match the exact backend-generated kind/session/object scopes required by the plan, preserving valid summary-version and manual research associations.
Prepared Task3B brief against existing source/handoff; actual reviewed3A interfaces and BASE must be bound before dispatch. Task4 brief extracted with a local Python heading parser because installed skill task-brief invokes a non-executable helper (permission denied); skill files not altered.
Task2B checkpoint: implementer retained initial behavioral RED (seven workbench/four settings failures against shell), then11focusedGREEN and build; expanding accounting/selection/race coverage and production/test-only visual checks. No task completion claimed. App lifetime controller/native request ownership persists across route changes/deadlines; root cleanup retires subscription only, never audio stop. Official@shadcn additions inspect generated files and preserve installed primitives.
Task4 read-only document_parser_preflight dispatched; no Cargo/source/index/native execution. Feature-source checkpoint: Windows CreateJobObjectW additionally requires Win32_Security; lopdf/zip/flate2 selected backend and worker-budget proof remain future locked-feature/native worker implementation gates. Presence of zlib-rs alongside miniz_oxide alone is not selected-backend evidence.
Controller Task2B named-risk spot-check while implementation remains unfinished: useWorkbench.reload used deadline-observed parallel capability reads, permitting repeated outstanding native reads on recheck; asked implementer to add actual-settlement ownership/coalescing/bounds and behavioral deferred-port regressions before changing it. AudioRoutePanel compact unknown-tail notice used any historical diagnostic; asked for current-epoch latest-per-direction projection and unknown→known/resume tests, preserving historical details. Ordinary role/phase strings must use local labels. These are pre-review implementation checkpoints, not independent reviewer fix rounds. Implementer acknowledged; reported71fullfrontendGREEN before amendments, further tests/visuals still pending.
Task3A checkpoint: retained migration2RED/contract6RED/expanded17RED and first19intelligence-relatedGREEN, including config->SQLite callback race and automatic duplicate terminal/reopen/full32/failed-marker capacity behavior. Refining config/storage/no-warning gates; Cargo still owned by3A and no completion claim.
Task4 parser preflight closed: controller read full task-4-parser-handoff.md. Verified source/API notes for lopdf0.38/zip2.4.2/quick-xml0.38, Unix allocation/CPU limits/readiness, Windows job assignment-before-request and required Security feature, replayable process exit ownership limitations. Mandatory final locked feature audit and real worker budget/reap tests remain Task4; minimum-OS installed-package enforcement remains native external gates. No parser source was implemented by preflight.
Task3A debugging checkpoint: final29run failed11config cases after2m12compile/63stest under host load12–21; earlier26GREEN0.97s. Controller read coverage/build logs (26passed and staged unused-contract warnings) and safe resource snapshot; no user processes changed. Exact formerly-failed profile-cap test passed unchanged alone1/1 in0.05s. Implementer added controlled fixture RED occupying both production global ConfigIo permits from distinct temporary directories, proving independent-harness interference. Correction is test-harness-only existing with_runtime_for_tests with same2permits/10s, release/join blockers before assertion; production gate/deadline/worker admission stays unchanged. Covering final tests/no-warning build remain pending; noTask3Acompletion.
Task2B native-language prerequisite corrected to English/中文/日本語/한국어/Français/Deutsch/Español; unknown codes fallback. Implementer reported fresh76fullfrontendGREEN/build with login:false; previous apparent exec stalls largely login-shell startup under load. Narrow disabled Button semantic-token legibility polish and final serial screenshots remain inprogress; finalowned verification/report/commit pending. Root opened three call-ready fold screenshots1440/768/390 and found frame/focus consistent; active-caption-collapse shots and final state images still need review. Existing Button modification must be explicit in reviewer-owned path package.
Task3A implemented f4b381d,20Rust/Cargo/migration paths; controller read full final report and verified cleanRusttree/committed scope. Final30intelligence/46config/20storage/1wire plus build/fmt/diff/no-warning evidence retained; failed29run and fixture gate RED/GREEN explanation explicit, not hidden. Independent intelligence_config_store_review dispatched frozen task-3a-config-store-review.diff (4a6f89c..f4b381d). Task3A review pending; fullTask3 not complete. Cargo/index window returned; UI authorized its owned commit after final gates.
Controller Task2B named driver integration gap before independent review: AudioSettings handled uninstall Ready as 驱动就绪 with cached endpoints still available.2A Ready after uninstall acknowledges operation completion, not installed readiness. Implementer adding behavioral regression, operation-aware display + UI inventory invalidation and explicit recheck, retaining selection/no fallback and late-result handling. Source DTO unchanged. This is implementation checkpoint, not reviewer fix round.
Task3A: complete (commits4a6f89c..f4b381d; independent intelligence_config_store_review specPASS/qualityPASS,0findings). Controller read full review/report; cannot-verify items are explicit3B entered-work/HTTP/scheduler/probe/AppState lifecycle and Task6 atomic version admission dependencies. They remain assigned, not silently completed. Parent fresh combined driver gate after common migration/error/lib changes: cargo test --lib commands::driver,26passed/0failed/440filtered,exit0,0.41s; no native calls.
Task2B driver regression reportedGREEN/full77tests/build after operation-aware inventory invalidation; browser scroll check exposed real jump-control belowfold from42vhscroller. Implementer fixing bounded workbench/slot height chain with failed browser evidence before final screenshots/commit. This necessary layout wiring preserves approvedframe, no style redraw. Independent2B review not yet dispatched.
Task3B producer bound to reviewedf4b381d. Exact with_profile_snapshot/job payload64KiB/max3profile, config file128/1MiB and primitive transaction APIs are in3A report. Next3B dispatch must preserve narrow staged expectations/remove when consumed, snapshot→admission transaction and no AppState/scheduler cycles. FullTask3 still requires3B andcombinedgates.
Ruling: Bound version1 job payload to64KiB/max3unique public profile snapshots, service-config document to128profiles/1MiB, and profile-use scans to<=32unfinished rows. Approved services need at most Tavily+optionalLLM; these limits bound immutable admission and native reads without scanning terminalhistory. Cost if wrong: future service count/payload limits need explicit contract expansion; current flows fail locally rather than truncate.
Task3B dispatched intelligence_runtime_impl (gpt-6-astra/high), BASEreviewedf4b381d, task-3b-runtime-brief.md plus3A report/review/runtimehandoff/globalconstraints. ExclusiveCargo handed to3B. ScopeRustscheduler/HTTP/probes/subscriptions/commands/AppState/host/native providercleanup; frontend/wire.ts/sharedruntimecorpus writes await2Bhandback. Source-only commit window requested before staging. No fullTask3 completion.
Task3B architecturecheckpoint accepted: inert frozen registry, one dispatcher/two workers/32unfinished/sharedparserpermit, cycle-free workers, processlease retainedbyuncertaingeneration, conditionalterminalwrites, async hoststart and concurrent promptaudio/intelligenceclose. ScopedServiceConfigStore clones retain enteredConfigI/O leases; basecollaborators remain unscoped. Probes independentglobalowner; proposed ownedconnect leaseintoVolc native lookup plus wait_closed(defaultunsupported;concretejoins) mustprove actualsettlement. Revisionfence includescredentialReplace/Clear whenpublicprofile/refunchanged. Separatetypedapplicationregistry mirrorsboundedticket/generation/lag rules.
Task2B current checkpoint reported80fullfrontendGREEN/build/format/diff; after fixingjump-control height, narrow tooltrigger row-size pushedcaptionsdown. Implementer correctingboundedCSS rows before finalvisual/gates/commit; no independentreview yet. Rustandpreexisting.gitignore untouched.
Canonical recovery status: Task 1: complete; Task 2A: complete; Task 2B: in progress; Task 3A: complete; Task 3B: in progress; Tasks4–10 queued. Full Task2/3 not complete. Current resume snapshot saved to resume.md in this plan workspace; update after each new handback/review, and trust git log + ledger rather than summary memory.

Controller recovery: actual linked worktree verified on codex/vox-bridge-implementation; HEAD83ddae3. Task2B frontend source/report handed back (80tests/build/format/visual evidence retained), independent review pending. Original workbench_ui_review failed at provider resource routing before report; fresh read-only workbench_ui_review_fresh dispatched frozen task-2b-ui-review.diff with same requirements and no duplicate successful review.3B remains exclusiveCargoowner.
Task3B named integration checkpoint: opposite SQLite/control lock acquisition in enqueue_with_profiles versus shutdown/claim. Implementer confirmed independently, retained deterministic actual-entered-SQLite race RED, and is standardizing config→SQLite→control; no controller source edits or independent fix round. Covering GREEN/report/review remain pending.
Task5 read-only srvdb_owner_handoff dispatched for concrete owner/scan/generation-rebuild/capacity risks against pinned source, reusing established API preflight; no Cargo/index/source/native/paid execution. Task4 still queued behind3B review.

Task2B independent review read in full: specFAIL/qualityNeedsFixes,2Important(I1hiddenStopfeedbackonknowledge/history;I2activephonecaptionsbelow80%height,violatingtop60%Taste/subtitlepriority),0Critical/Minor. Controller confirmedI1inAppsourceandacceptedreviewvisualgeometryforI2. Fixround1briefpreparedagainst83ddae3. Cannotverifyitems:2Abackendalreadyreviewed+combined26gate(new3Bcombinedgateassigned);1Bstreamalreadyreviewed;ToolRailreadiness/actionsassignedTask8;native/paidacceptanceremainexternal. None silentlyclaimedcomplete.
3B frontendwirewindowgranted: newintelligence-wire.ts/test/json andnarrowexistingwireprimitiveexports;runtime-wire.jsonunchanged. UIfixwillnotownthesepaths;indexwindowsremainserialized.

Ruling: Extend3B narrowly into RunningSession::request_stop and stop_session after deterministic entered-SQLite early-audio RED showed observe persists Stopping before requesting runtime stop. Send the existing watch intent before database persistence, retaining one observer, shared durable completion and actual cleanup ownership. This is required by prompt cancellation under intelligence DB contention. Cost if wrong: runtime stop-phase ordering and affected lifecycle regressions need rework; no new terminal success or lease release is permitted. Controller read runtime.rs/orchestrator.rs before approval.
Task5 owner handoff read in full; source corrections: SrvDB is Send+Sync but stays serialized; live get_metadata=None is not a tombstone; persist can fail after durable mutation; no implicit Drop persistence. These are exact pinned-source semantics, not dependency changes.
Ruling: Task5 opens Flat full-precision storage with quantization disabled/None, then explicitly set_mode(Auto) on every generation open. The automatic mode does not quantize storage and matches the planned automatic-index behavior. Cost if wrong: automatic migration/performance and reopen tests require policy adjustment; do not claim a fixed HNSW mode.
Ruling: Task5 keeps one published generation per identity but permits one staging destination during a leased rebuild, evicting the other idle identity so total SrvDB engine instances remains<=2. A private read-only facade may use VectorStorage on the existing nonempty source file only while the source SrvDB retains its OS lock, successful persisted count/dimension checks pass, and all source writes/close/remap are fenced; borrowed slices are copied in<=200-record batches. This enables vector-preserving capacity recovery without compact or metered reembedding. Cost if wrong: generation publication/Windows mapped-handle cleanup and SQLite staging mappings require rework.
Ruling: Task5 executes all SrvDB calls inside one owner-scoped Rayon pool of2workers, counting1owner+2internalworkers, while keeping queue16/twoopenengines/200000stored-recordcap/dimensions128–4096. Source shows internal Rayon and large mmap/redb/graph overhead; these are finite supported limits, not a small-RAM or RSS guarantee. Cost if wrong: throughput/internal-resource tests or supported limits need explicit adjustment. Native/stress performance remains a later measured gate.
Task5 dispatch precondition: choose/document fixed managed metadata/graph/header anomaly caps before writableopen, validate source headers/count/dimension/normalgrowthslack without truncating committed files, and fail locally as repair/resource-limit. Actual cap values require the Task5 architecture checkpoint; existing count/dimension/identity limits are not silently lowered. Exact source handoff task-5-srvdb-owner-handoff.md includes tests and failure-safe publication semantics.

Task3B checkpoint (notcompletion): earlyaudioenteredSQLiteREDthenapprovedrequest_stopseamGREEN;combined65passedwithoneheadlesscommandfixtureoriginfailure. FixturecorrectedtoTauriplatformdocumentedorigin, noACLbroadening;remainingcommand/probe/fullgatespending. Wire-onlyintelligence+existingruntimeVitest13GREENreported;existingruntimecorpusunchanged.3BCargoownershipcontinues.
Read-onlysummary_snapshot_handoffpreliminaryfinding: finalstorageisINSERT-onlyandlatecorrelationpromotesmemoryinterimafterdurablefinalinsert;existingpaging/exportisnotafixedfinalsnapshot. Task6coverageformatsegment-onlycannotprovesplitparts;completehandoff/schemaoptionpendingbeforecontrollerruling. NoTask6sourceimplementation.

Task6 snapshot handoff read in full: exact current finalINSERT/latepromotion, fixedsnapshot/page/exportdifferences, byte/identity/read-allocationlimits, completeplanner/strictresponse/exportallowlist reviewed. NoTask6implementation.
Ruling: Refine unimplementedmigration5summary_coverage with zero-basedpart_index/part_count, composite(summary_id,segment_id,part_index)key/rowchecks and manifest-consistencyvalidation;input_final_count staysdistinctfinalIDs. Record actualsuppliedmapinputs with retainedrawresponse, andrequireallparttuplesat success. Segment-onlyschema cannot detect missingmiddleparts requiredbythespec. Cost if wrong: summarycoverage schema/tests and provenance semantics need rework; reviewed3Aadmission unchanged. Canonicalplan andfutureTask6brief updated.
Ruling: Task6 uses a single bounded borrowed-row finalSQLite readsnapshot and a migration5session-only transcriptindex for rowid ordering; verify actualqueryplan, releasebeforecloudcalls and preserve missinglegacykind as unknown rather than inferfromlanguage. Existingpages/export do not supply snapshot or preallocationlimits. Cost if wrong: snapshot/index overhead or legacyexportcompatibility mapping needs adjustment; no latefinals/unknownmetadata silentlyfabricated.

Task2B fixround1 implemented14f0b9f, exactly7UIpaths; fullfixreportreadbycontroller. Evidence23focused/87fullfrontend/build/format/Taste/realgeometryretained, all6call/livefoldsregenerated. Scopedpackage task-2b-fix-1-review.diff generated83ddae3..14f0b9f excludes3B/docs. Independentre-reviewattemptcouldnotinitialize because/home/mint/.codex/thread-writer-locks EROFS; no reviewverdict exists andTask2Bnotcomplete.
Controllerplanrefinementscommittedd6fdbbd afterownedgitdiffcheck:8linecanonicalplanchange, Task5Auto/fixedRayon/rebuildoverlap andTask6partcoverage/fixedsnapshotindex; prepared5/6briefsrefreshed. Docsindexwindowhandedback;3Bsourcewindowmayberequestedaftergates/report.
Environmentblock2026-11-06: rootext4 mountshowsrwtextbutemergency_ro;independent/tmp tinywriteerrno30 EROFS, ignoredworktreewrite/read/unlinksucceeded.3Bfinaltest18pass54fail attempfilecreation afterprior71GREEN; no sourcebuginferred. Useper-commandworktreeTMPDIR, retainfailedlog;CargoHome/homewritesmayfailseparately. No mount/permissions/systemrepair performed. Asyncuserquestionasksrestorehostwritability; no approval or answer assumed. Existingactive3Bvalidationcontinues; new/resumedreviewerscannotinitialize.

Ruling: While rootemergency_ro preventsfresh/resumedreviewerinitialization, use alreadyactive intelligence_runtime_impl for independent read-onlyUI fix1review afteritsbackendfreeze; it authorednoneofthe7UIpaths. Existingreviewseatcouldnotinitializeandproducednoverdict, so no duplicatesuccessfulreview. Cost if wrong: reusedreviewercontext/fatiguecouldmissUIregressions; finalwholebranchreviewmustretainthisexceptionandreviewevidence.3Bitselfisnotindependentlyreviewedbyitsauthor.
3Bfilesystemisolationcheckpoint: workspaceTMPDIRavoidsEROFSbutfuseblkcaused65pass7tiny-deadlinefailures/115s, withproduction/testtimeoutunchanged. Approvedunique/dev/shmfile-onlyfixtures afterwritability/capacity/noexeccheck;executabledriverfixturesstayworkspace. Reportedshmfinal72intelligenceGREEN0.31s/50configGREEN;remaininggates/report/commitpending. Preserveallfailedlogs andcleanup onlyowneduniquepaths.

Task3B sourcecommitted5a0d676,38authorizedpaths, allfinalgroups72/50/20/26/151/84/44/2passed(nooverlapcountsum),libbuild/fmt/diff/frozencorpuspassevidence. Fullreportreadandownedstatuschecked;Carg/indexhandedback. NoTask3completionuntilindependentreview. ActivebackendregionbeganindependentUI fix1read-onlyreview, noneof7UIpathswereitsauthorship.
Ruling: With reviewerinitializationblockedbyconfirmed/homeEROFS and3Bauthorshipseparatefromcontroller, controllerperformsindependent3Btask-scopedreviewofthefrozen5a0d676diff/reportinsteadofself-reviewbytheimplementer. New/resumedreviewerattemptfailedbeforeverdict;source/tmpstillEROFSrechecked. This maintainsindependentproduction-codeauthorshipandallowsauthorizedplanprogresswithoutsystemrepair. Cost if wrong: controllercontextislessisolatedandmaymissfindings;retainfulldiff/report/fixledgerforwholebranchreview. Controllerdoesnotfixproductionfindings;activeoriginalimplementerreceivesanyfixes.


Continuation 2026-11-06: controller recovered actual frozen HEAD5a0d676 and read task-2b-ui-review.md plus complete task-2b-fix-1-review.md. UI independent reviewer was intelligence_runtime_impl, with no authorship of any seven UI fix files. I1/I2 both ADDRESSED, specPASS/qualityAPPROVED, zero remaining findings. Original cannot-verify obligations remain assigned: reviewed2A driver facade with3B integration gates; reviewed1B stream; real tool action/readiness Task8; native/paid/package acceptance external.
Fresh controller frontend integration check atHEAD5a0d676: npm run test:run exit0,11files/87tests passed,23.82s (exec session1961 completed); npm run build exit0,TypeScript/Vite built24.14s (session3245 completed). The build includes only a PLUGIN_TIMINGS performance advisory; no compile error, fixture or native acceptance inferred. No source changed; only original .gitignore remains modified.
Task 2B: fix round 1/5 (2 addressed,0 open; commits83ddae3..14f0b9f; independent scoped review clean).
Task 2B: complete (commits4a6f89c..14f0b9f frontend-owned paths,review clean).
Task 2: complete (reviewed2A4a6f89c plus reviewed2B14f0b9f; combined frontend87tests/build and retained3Bcommands::driver26 integration). Native/paid acceptance remains pending.
New3B reviewer spawn intelligence_runtime_review_resume failed before initialization with /home/mint/.codex/thread-writer-locks EROFS; no review verdict. However followup of existing workbench_ui_impl_retry succeeded and explicitly confirmed reading task3B brief. It is now the unique independent3B task reviewer, no3Bsource authorship, frozen diff/report/brief supplied; report task-3b-runtime-review.md. This supersedes the prior proposed controller-review fallback: controller stopped its alternative review before any verdict and does not duplicate the active reviewer. No system repair performed.
Existing intelligence_runtime_impl concurrently performs only read-onlyTask4 runtime-final-handoff against frozen3B APIs, noTask4source orCargo. Task4 remains undispatched until reviewed3B. Preserve existing active sessions while fresh initialization is blocked.

Ruling: Split Task4 into4A parser/headless-worker/resource-isolation/chunking and4B managed document/version/migration/commands/job lifecycle integration, with independent task-scoped reviews and combined4 gate. The approved behavior/limits remain unchanged;4B consumes reviewed4A and3B. Cost if wrong: additional integration/review boundary requires rework, with no parser/native safety waiver. Prepared task-4a-parser-brief.md; actual dispatch still waits reviewed3B.
Ruling: To continue dependency resolution during root/home emergency_ro, use per-command localCARGO_HOME in ignored .superpowers/toolchain/cargo-writable-20261106, with4322 cached source/archive links to original read-only cache and local sparse-index copy. No global Cargo config/home/system repair; product manifest/lock unchanged by prefetch. Cost if wrong: source-path fingerprint/cache reuse can cause extra recompilation; final product locked graph and offline feature/tests remain mandatory. Independent ignoredparser-prefetch cargofetch exited0(session98802), selectedlopdf0.38.0/zip2.6.1/quick-xml0.38.4. Compatible-version constraints are preserved; newer compatible patches require real compile/source-feature checks.

Task3B independentreview task-3b-runtime-review.md readinfull: specPASS/qualityAPPROVED,0Critical/Important/Minor. Reviewer workbench_ui_impl_retry authorednoneof38reviewedpaths, reviewedall5741lines once and retainedRED/GREEN/fullfinaloutputs. Controller fresh gitdiffcheck+cargofmtcheck exited0 atHEAD5a0d676. Silent-log exitgap is now independently closed byfreshcontrollercommand. Cannotverify3Aalgorithms alreadyclosedbypriorindependent3Areview;future4/6/7handlers/parser/summary/bodyvalidation and8meteredcopy remainassigned;native/paidgatesexternal. Nounassigned3Bgap orreplay/fakesuccess.
Task 3B: complete (commitsf4b381d..5a0d676,38ownedpaths; independentreviewclean).
Task 3: complete (reviewed3Af4b381d plus3B5a0d676; combinedrecorded72intelligence/50config/20storage,26driver/151session/84providers/44app/2wire filters overlap; freshfrontend87/build andfmt/diff verified). Futurehandlers remainseparatetasks; native/paidacceptancepending.
Ruling: While freshsessioninitialization remainsblockedbyroot/homeEROFS, reuse existing active intelligence_runtime_impl forTask4A after3Bindependentreviewclean ratherthan repeat a known-failing freshspawn. Each task retains separatebrief/report/ownedsourcecommit+independentreview bynon-author workbench_ui_impl_retry; source freeze/index/Cargo remainserialized. Costifwrong: largerreusedimplementercontext maycausecross-taskassumptions; architecturecheckpoint andindependentreviewmustcheckthisexplicitly. No systemrepair orindependentreviewwaiver.
Task4A BASE5a0d676 recorded; preparedtask-4a-parser-brief.md plusparser/native/runtimehandoffs. ExclusiveCargoownershipgoestoTask4Aimplementer forparser/resource/worker/chunk subset. Requirearchitecturecheckpointbeforeimplementation andsource-onlyindexwindowafterfinalreport. Task4B/Task5–10stillqueued.

Prepared task-4b-documents-brief.md with frozen3Bactualjob envelope/fence/oneparserpermit/appoperationlease and explicit missing domain/progress/publication/cancel/recovery ports. Actual4Bdispatch waitsreviewed4A; no4Bsource or parallelCargo. Newknowledgewirevariants remaincommonglobalcontract requirement.

Task4A architecturecheckpoint accepted: exactworkerflag;Unix trustedpositiveusize allocationargument,Windows flagonly; malformedworkerargv staysheadless. Version1strictnativepathbytes/UTF16 request/result,8bytereadiness→singleLEu32frame→EOF+successfulrealexit. Fixedparser/normalizationidentity voxbridge-parser-v1-normalize-v1; offsets/reconstruction/preservedWordtabs checked;hashseparatesidentity/text.
Ruling: Task4A DOCX acceptsUTF8withoptionalBOM/utf8declaration and bothstrict/transitionalWordprocessingML namespaceURIs;onlydocument.xml/styles.xml,combinedinflated16MiB,DTD/customentities/depth>64rejected. Missingstyles retains text; malformedstyles failsParseFailed andvalidunsupportedencoding failsUnsupportedDocument ratherthan silentlydiscardingtext. Costifwrong: validlesscommonUTF16DOCXrequireslatercodec support/reimport; preserveoriginalmanagedfile andvisiblelocalfailure.
Ruling: Parserlimitedspawn ownsblockinghandoff/strictchildsupervisor separatelyfromordinaryaudiobehavior; publishesrealexitonlyafterwaitsuccess,keepschild/jobthroughkill/waiterrors and2sobservationexpiry. Assignment/limitfailure sendszero requestbytes andretainsactualcleanupbeforeparse/permitrelease;Unixpre_exec/allocator andWindowsFoundation/JobObjects/Threading/Security+OwnedHandle implementplannedlimits. Costifwrong: supervisor portability/late-spawn cleanup needsrework; uncertainOScleanup stillfencesgeneration/processlease ratherthanclaims success.
Task4A implementationnowcontinuesTDD underexclusiveCargo; no4B/migration/DocumentService/knowledgecommandimplementationyet.

Task5 managed-caps-handoff readinfull: task-5-managed-caps-handoff.md. Staticreport proposesV(d)=max(L+floor(L/2),L+1024*4d),L=24+200000*4d;max4096d4915200036B,metadata16KiB/value+16GiBfile/12GiBwatermark,HNSW512MiB,boundedheader/graph/16entryallowlist. TheseareproposalsuntilTask5architecturecheckpoint/finalTextLocationencoding,notimplementationorRSSclaims. Newload-bearingrisk: redbrecoveryeagerlymaterializestombstones andmetadataStringbeforepost-openchecks,sofilecapsalonecannotcertifyarbitraryunsealedmetadata. CarrytrustedSQLitegenerationseal+dirtyintent/source-provenance decisions intoTask4Bmigration4/Task5checkpoint beforecode;no unsafeunsealedsource-open orimplicitpaidreplay. Reportcoversfreshuniqueempty,cleanreopenwhichitselfmutatesfiles,livepartialpersistretry,dirtycrashrestartfail-local,preservedsource/text,missingderivedgraphlocalrepair andexistingretainedSrvDBlockreaderpremise. NoTask5source/engine/Cargo/test performed.
Task4A firstparserbehaviorRED currentlyawaits initialdependencyrecompileafterlocalCARGO_HOME switch; latestimplementercheckpoint4tests/syntheticfixtures/minimalfailskeleton. NoRED/GREENcompletionclaimed; uniqueverified /dev/shm/voxbridge-task4a.vy4e3_zk file-fixtureTMPDIR ownedby4A. RootdoesnotrunparallelCargo.

Ruling: Task4B migration4 must reserve privateversionedgenerationseal/dirty-intent storage keyedidentity+generation, withFK/clean-state checks, beforeTask5writesengines. Exactschema is4Barchitecturecheckpoint;Task5reservedmigration5 remainsTask6owned. Thisaddresses theestablishedpre-openredb allocation/provenance risk withoutrewritingreviewedmigrationslater. Legacyfiles/rowsremainunchanged/unsealed;noTask5enginebehaviorimplementedin4Bmigration. Costifwrong: auxiliaryschema/namespace mayneedlateradditivemigration orsealformatversion;publicdocumentDTOsandoriginaldataunchanged. Prepared4B/5briefsupdatedwithreport/architecturegate;capsnotyetbindingpendingactualmetadataencoding.
Task4A retained RED/earlyGREEN logs read: parser-red0/4 fail,limits-red0/8 fail,parser-green8/8 pass0warnings includingrealPDF/DOCX/text/chunk/limits; actualIPC/allocator/strictprocessgate stillinprogress. No4Acompletionclaim orrootparallelCargo.

Task4A current evidence read by controller (partial; not completion): limited-child owner3/3 GREEN, supervisor4/4 GREEN with a temporary unusedprotocolmethodwarning later consumed byworker, strictworkerprotocol3/3 GREEN, allocator4/4 assertions GREEN alongside expectedentrygrammarRED, realdocument_worker5/5 GREEN0warnings at21:52. Real worker handles text/PDF/DOCX, actualbefore/after-readinessbudgetexit86 withoutsuccessJSON, inflation→ResourceLimit, waiting-inputtermination+sharedreap, malformedargv/trailingframesheadlessfailure. FirstwiredrealrunfoundstdinEOFnotsentbyTokioshutdown; explicittake/dropfixed it withsame30sdeadline andretainedfailure/coveringtest. Finalfullgates/features/format/build/freeze/commit/reviewstillpending. RootnoCargo/indexrunning; exclusive4A remainswithintelligence_runtime_impl.
PreparedTask4Bbriefwordingclarified; fulloriginalTask4textpreserved. Seal/dirty-intent schemagateandTask5cap/handoffarchitecturegate remainprepared/undispatched.

Task4A finalreportreadinfull: 534lib/5binary/5realworker,normal lib+binarybuild,fmt/diff/frozenAPIchecks,targetfeaturegraphs and10reproduciblefixtures. 30originalsourcehashesverified bycontroller,indexemptybeforegrant. Cachedstageddiffcheck thenfoundintentionaluntrackedfixturewhitespace/CRLF/PDFxref bytes,withouta productiondefect.
Ruling: Addone scoped tests/knowledge/fixtures/.gitattributes to preservebyteexactPDF/DOCX/text/Markdown fixtures acrossautocrlfandmarkappropriatebinarydiffs; only thisfixturefolder, no globalGitconfig orsourcewhitespace suppression. Extend authorizedpathlist30→31; unchanged30hashes and10fixturehashes/reproduction wereverified beforecommit. Costifwrong: Gitfixturebytepresentation/diffsneedadjustment; generatorremainsreadable andruntime/sourcefixturecontentunchanged.
Task4A implementedcommitd72e32a21cc814bb638f70d12d5650983422c66f,exact31paths,postindexempty/original.gitignoreonly. Controllergeneratedtask-4a-parser-review.diff (144422B) afterexactowned-pathdiffmatch5a0d676..d72e32a. Independentworkbench_ui_impl_retry nowownsreadonlyspec+qualityreview/report task-4a-parser-review.md. NoTask4Acomplete untilreview;4Bstillqueued. Cargo/indexreturnedcontroller;originalimplementeractiveforfindings. AllnativeOS/package/signing/realkeyring/audio/paidgatesretainpendingstatus.

Task4A independent review task-4a-parser-review.md readinfull: specPASS/qualityAPPROVED,0Critical/Important/Minor;4068lines/31paths bynon-authorworkbench_ui_impl_retry. Retained534lib/5bin/5realworker0warnings confirmed; namedquickxmldefault/flatebackendrisksresolved byreviewer. Controllerfreshfmt/diffcheckexit0,indexempty/original.gitignoreonly. Cannotverifyactualparserpermit/appLock/documentjob integration assigned4B withexplicitawait-through-reapcontract; nativeinstalledOS limits/packages/signing remainexternal;nofullTask4claim.
Task 4A: complete (commits5a0d676..d72e32a,reviewclean).
Task4B BASEd72e32a recorded. Existingactiveintelligence_runtime_impl now receives remainingdocuments/migration/command/jobintegration brief underalreadyledgeredEROFS reuse policy; separate4Breviewrequired. ExclusiveCargo4B; architecturecheckpointbeforecode/source-onlyindexwindowaftergates/report. Preserve reviewed4Aparser/worker exceptreproducednarrowintegrationdefects. Task5–10stillqueued.
Namedlegacy-schemacheck before4B: 0001_session_foundation.sql alreadyhas documents.sha256 TEXTNOTNULL aswellasuri/title/created_at. Preserveexistingsha256/uri/title verbatim;no needinventnullableSHAorhasholdpaths. Missingguessedsrc/storage/migrations directory isnot a producer; canonicalSQLis src-tauri/migrations. No sourcewasmodified bythisread.

## Task4B controller architecture checkpoint — 2026-11-06
Task4B architecture checkpoint read against its full brief. Accept additive legacy-safe metadata, exact current version/job/parser association, durable managed-file intent, owned pre-admission work, SQL-only terminal reconciliation, retained post-quiescence cleanup, committed chunk progress, narrow event/result ports and one parser permit through real reap. No Task5 engines/paid APIs/UI added.
Ruling: Replace with the same current SHA/parser and an active or successful import is a no-op returning that current job; explicit replacement of a failed/repair-required import advances the same document UUID/version. A hash matching another document does not redirect replacement to that document. — Stable document identity and explicit retry preserve references while the import path retains hash deduplication. — Cost if wrong: future UX may need an explicit force-reparse option or alternate duplicate policy; no existing source/text is discarded.
Ruling: Reject non-UTF8 canonical attribution paths at the public document boundary, while retaining lossless versioned native path storage/worker encoding. — JSON PathBuf projection cannot preserve arbitrary non-UTF8 paths and lossy attribution is false evidence. — Cost if wrong: Linux users must rename an unrepresentable selected path, or a future DTO version must expose native encoded paths.
Ruling: Reserve private knowledge_index_generation_seals with composite identity/generation key, identity FK, bounded seal<=64KiB and intent<=16KiB JSON version1, dirty boolean and clean/dirty invariants, without synthesizing trusted migration rows. — The established eager redb recovery risk requires durable trusted provenance before Task5; detailed format remains Task5-owned. — Cost if wrong: a later additive migration or JSON format version may be needed; legacy files and public DTOs remain unchanged.
Implementation proceeds with current source-only ownership/Cargo exclusivity; staged source window waits for final report/freeze. Independent Task4B review still required. Reviewer may perform only read-only Task5 seal-format compatibility preparation meanwhile, with no source/Cargo/index/native/paid action.

Task4Bpartialmigrationevidenceread:task-4b-migration-red.logexpectedmissingkind0/1;task-4b-migration-seals-red.logexpectedmissingmetadata/tables0/2;task-4b-migration-green.log5/5exit0withoutwarningscoverslegacytwicereopen,seals/intentJSON/version/byte/FKconstraintsandexistingmigrationrollback. Notfull4Bcompletion;implementationprojection/copy/admission/lifecycle/commandgatesremain. IndependentTask5compatprepconfirmedlegal6000-CJKMarkdownheadingalone18000UTF8bytes>proposed16KiBmetadata;actualTask5format/cappolicyawaitsreport/architecture,ratherthanunapprovedparserrestrictionortruncation.

Task5seal-format-handoff.mdreadinfull:boundedseal1141B/intent671B;conservativeidentityescapeenvelope14600Bfitsm4;legal6000-CJKfull-locationmetadata18318Bdisproves16KiBproposal.ReferencecodecproposalmetadataSchema2/382BpreservesfullcanonicalSQLiteTextLocationandvalidatesdigest;rawfull-locationalternative8MiB/valueimplies8TiBphysicalenvelopewithsamegrowthfactor. StillTask5checkpointproposals,nocappolicyorparserrestrictionmade.
Load-bearingpreparedTask5schemaissue:10kpriorIDsnear200000require70001Bbeforeenvelope,exceedjob64KiBandintent16KiB. Pagedcleanupitemsandstaginggenerationmappingstoragecannotbeassumedfromgenerationsealstables;documentcleanupitemsmustsurviveremoval. Rootasked4Bauthorforsmallestadditivem4schema-onlycheckpointbeforefreeze,sothatlaterTask5doesnotrewriteauditedm4orconsumem5/m6. NoTask5implementationauthorizedyet.
Task4BimplementerturnfailedviaAgentRouterHTTP400differentOpenAIresourceaftermigrationGREEN. Rootresumedsameagentbyfollowupwithreport/checkpoint/partialsourcepreservationandnewnarrowm4schemarequest;no sourcecompletionclaimed. Existingworkbenchreviewerread-onlyhandofffinishedandavailable;independent4Breviewstillpending.

Ruling: Add exactly two private empty tables to still-unreviewedmigration4: knowledge_index_work_items keyed(operation_id,ordinal0..199999), and knowledge_generation_mappings keyed(identity,generation,chunk_id), unique generation/vector ID, unpublished default. Both preserve canonical decimal u64 IDs and bounded snapshot metadata; no document/chunk/job cascade, RESTRICT composite generation-seal FK, paged access indexes. — Large durable cleanup/rebuild ID sets exceed job/intent JSON bounds, and successful parse replacement must not erase prior vector mappings before Task5 acknowledges cleanup. This reserves necessary storage without rewriting reviewedm4laterorconsumingreservedsummary/researchmigrations. — Costifwrong: futureTask5codec/mappingpublicationmayneedanadditivemigration;retainedhistoricalrowsrequireexplicitacknowledgedcleanupandmustneverbehydratedwithoutactive-generation/current-Ready/version/hashjoins.4Badds schema/constraint/lifetime/pagingtests only,noTask5engine/networkbehavior. Acceptedtask-4b-work-tables-checkpoint.md.
Task4B resumed successfully afterprovider-resourceerror; newprojectionRED0/1exit101retained. Sole4Bauthorcontinuesprojection/copy/admission/lifecycle/commandgatesandnewreserved-tablestests. Noindexgrant/commit/review/completionyet.

Prepared4Bbriefupdatedwithacceptedwork-tablecheckpoint;Task5briefcarryforwardsseal-format/capcounterexampleanddurablepagingschema. Namedbinding-specreconciliationforfutureTask5referenceoption:per-vectormetadataretainsinlinenullablepageasspecified;fullcanonicalSQLiteTextLocationdigest/hydrationpreservesheading. Digest-onlyproposalnotyetacceptedandmustnotdropthisfield. NoTask5codeccappolicymadeyet.

Task8jobs-controller-handoff.mdreadinfull:actualindependentticket/eventname,snake_caseJobView/camelCaseargs,subscribe→listen→snapshot,revisionsplusnonregressingterminal/progressmergewithoutinventedsequence,global4originalnativegetterboundacrossretiredruns,64tracked/event/duebounds/1000msfairpollingandactualticketcleanup. TrackolderownedIDsabsentfromlatestpageviagetter;doesnotguaranteeallhistoryunfinisheddiscoveryorwalkwholehistory. PreparedTask8briefupdated;noTask8UI/code/testimplementedandno4Breviewperformed. Independentrevieweravailableforfrozen4Bdiff.

Ruling: After2different-resourceHTTP400failures andfailedfreshrecoveryspawnrelationshippersistence,transferremaining4Bimplementationtoexistingworkbench_ui_impl_retry, preservingpartialsource/evidence. Originalintelligenceauthorinterruptedfromerroredstate;rootpsconfirmedno4BCargo/nativebuild/testprocess,indexempty. Sourcebaselinehashesrecorded. Parent,withno4Bproductionauthorship,willperformindependentread-onlyfull4Btaskreviewunderconfirmedfreshsessionfailure;neitherauthor'sself-reviewcounts. — Existingreviewagentcontextinitializationworksandproviderroutehasbeenstable;thismaintainsauthorizedprogressandsource/reviewseparationwithoutsystemrepair. — Costifwrong:reusedimplementer/controllerreviewcontextcanmissintegrationissues;retainfulltaskdiff/report/exceptionsforwholebranchreview,withnoownproductionfixesbycontroller.

Recoveryhandoffcorrectionverifiedbyrootrg/baseline:2reservedwork/mappingtablesanddocument_tests/index_schema.rs3testsarealreadyin8pathSHAhashsnapshot,notstillunimplementedasstalecheckpointnarrativesaid. Newauthorverifiedall8hashesmatch;preserveandcollectmissingfocusedevidence. Rootcorrectedrecoverybrief;source/hashesareauthoritative,notearlierreporttiming. No4Bcompletion/reviewclaim.

Task4BrecoveryindexschemaGREEN3/3exit0retained/logread;temporaryunusedencode_pathwarningremainsuntilcopyintegration,nofinalgateclaim. FutureTask8handoffcorrection:rootnamedorderingcheckintelligence/store.rs154–166confirmslist_jobsascendingrowidafterpage,notlatest64. Correctedreporttofirst/requestedboundedpage;opaque cursor/pagedhistory/trackedgetterrulesstillapply,nobackendAPIchange/newfilter/crawl. Current4Bauthorcontinuescopy/jobintegration.

Task4Bcopyfirstgroupreportandlogsread:behaviorRED0/3exit101(validsource rejected/ResourceLimitandCancelledmissing),GREEN3/3exit0withoutwarnings,13.46scompile/0.03stests. Exactcontent/independenthash/canonicalgenerated-ownedcopy,32MiB+1rejection,signatures/nonUTF8/empty/cancel/symlinkcovered;race/exactboundary/observer-lossnotyetclaimed. encode_pathtemporarywarningconsumed. AdmissionRED0/3exit101againstminimalskeletonConfiguration;version/dedup/Busy/publicImportandterminal/progress/cleanupimplementationremain. No4Bfreeze/indexgrant/commit/review/completion.

Task4Badmissionreport+GREEN-attemptlogread:3/3exit0/0.07s,actualSQLite/copy/parser-portintegrationcoversParsedcanonicalchunks/fullheading,currentSHA/parserdedupactive/successful/32fullqueue,Busyrollback/candidatecleanup,requestedUUIDpreservedonotherdocmatchinghash,priorfullsource/textretentionafterfailureandsameUUIDfailedretrynewversion. Temporaryunusedcleanup_cancelledwarningpendingAppStateconsumer;notfinalgate. Currentauthorcompletesstartup/queuedcancel/observer-loss/realreap/progress/AppState/commands/wire/racecoverage,thenfinalfreeze.

Ruling: Add one explicit public DocumentService::register_import_handler()->AppResult<()> composition entry, internally reusing jobs.register with the existing separate handler; constructor remains inert and AppState uses this entry. — The external Cargo document_worker integration cannot access the crate-private handler factory, but must inject the actual CARGO_BIN_EXE_vox-bridge parser and verify two admitted imports share one real worker permit. This is a meaningful safe registration boundary, not a new command/handler type or Task5 behavior. Existing duplicate/start registry checks remain authoritative and the handler retains no SchedulerArc/DocumentService/AppState cycle. — Costifwrong: public library composition API naming/lifetime may need adjustment; no frontend permission or runtime policy is broadened.

IndependentfutureTask5namedriskprepdispatchedread-onlytooriginalintelligence_runtime_impl(nowexplicitlyno4Bsource/Cargo):unscopedhandlerconfigcredentialobserverTimeoutvsenterednative/configoperationandgenerationprocessleaseretention. Reporttask-5-job-credential-ownership-handoff.md<=1page;existingAPIlineevidenceandminimalfutureportproposalonly,noactualkeys/native/network/test/sourceactionorbroaderreview. Newpurpose/contextaddressesfutureintegrationwhileworkbenchauthorcompletes4B;route400mayrecur,not4Breviewsuccess.

Task5credential-ownerhandoffread:ConfigIo.scopedownerretainedthroughenteredspawn_blocking;unscopedcredentialobserver10stimeoutcanoutlivelateSecretGet/DEKsideeffect. Actualstart_ownedArcFileonlyvsAppStateboundedOperationwith16permitbarrier;JobContextlacksnativeopowner/drain.FutureTask5mustaddcountedgeneration-childoperationportscopedintoConfigIoanddrainbeforegenerationdone/lease-release/rearm,notjuststrongFileArc. Existingresolve_credentialsreadscurrentprofile/secret;resolve_snapshot_credentialsnotimplemented.FuturesnapshotsmustvalidateadmittedpublicprofileinsideexistingconfiggatebeforecredentialHTTP. PreparedTasks5–7briefupdated;no4Bproductiondefectreview/newnativeoperationperformed.
Recoveredoldsession30400metadataUnknownprocess/originalindexschemaGREEN7/7logcomplete;beforetransferpsnoneCargoandnowonlynewauthorcommandsCargo. NooldconcurrentCargo/sourceaction;originalagentnowread-onlyhandoffcomplete.

Ruling: Task5 adds a narrow generation-counted child-operation owner for entered credential/native work, scopes it into ConfigIo per call, and drains issued owners before generation completion/lease release/rearm; immutable admitted profile validation precedes secret resolution under the same config gate. Exact API/bound/model-port carrier is Task5 architecture checkpoint, keeping long-lived collaborators unscoped and no SchedulerArc/AppState handler cycle. — Verified unscoped10sobserver can end while key/DEK work continues; Arc<File> alone does not fence same-AppState rearm. This closes the future model-handler integration gap before cloud work. — Costifwrong: native-operation tracker/provider-call plumbing and shutdown tests may need rework or affect close latency; no actualnativecall/credentialmigration/4Bcodechange made by this ruling.

Ruling: Use AppState::with_document_parser(data_dir,Arc<dyn DocumentParser>)->AppResult<Self> as the one public parser-DI entry for external actual-worker integration; AppState::new delegates with its default ProcessDocumentParser. Keep explicit DocumentService::register_import_handler composition but crate-private, superseding the earlier public visibility choice now the external test enters through AppState. — ConfigIo and ServiceConfigStore::new intentionally remain crate-private; actualAppState integration also proves real process-lock lifetime rather than a unit scheduler lease. Rootnamedconstructor-riskcheckfoundSecretStore::newinert,PlatformAudioBackend::newinert,andDriverController'sboundedidleownercreatesnative manageronlyonrequest;doc-onlyfixturemakesnoaudio/keyring/paidrequest. — Costifwrong: injectedAppState construction/API visibility needsadjustment or additional fixture ports; defaultproductionparser/compositionpolicyandcredentialinternalsunchanged. Existingexternalcompilefailureisinterface/buildattempt,nobehavioralREDclaim.


2026-11-07 resumed root checkpoint: author reports corrected full library 566/566, binary 5/5, real worker 6/6, frontend build, actual commands 2/2, Rust knowledge contract 1/1 and frontend wire 7 tests. Root read report, current implementation and safe amplification probe; no final freeze/commit/review verdict yet. New independent document_bound_review spawn still failed relationship persistence before initialization. Original intelligence author supplied read-only algorithm consultation only, not independent full4B review. Root retains independent full4B review ownership and no production authorship.

Ruling: Accept task-4b-derived-result-checkpoint.md option B, adding only a crate-private bounded chunk entry and shared internal implementation in knowledge/chunking.rs; the reviewed public chunk_blocks retains its exact existing algorithm and unlimited aggregate policy. Only ImportHandler selects the fixed 32MiB logical derived-result budget. Before every heading clone/text collection, checked-sum exact selected UTF-8 text bytes, full heading bytes and size_of<TextBlock>, retaining one aggregate across all groups. Reject overflow/over-budget as ResourceLimit; no truncation/partial results and no RSS guarantee. — The real safe probe's 462242-byte worker result produced 96 chunks and 25MiB of cloned headings; larger legal titles can amplify the bounded worker response. Exact charging avoids conservative estimates rejecting acceptable results or copying the window algorithm. — Cost if wrong: budget may need future explicit product tuning and counts requested logical payload rather than allocator overhead; original input/group/char-buffer bounds still apply. This is separate from Task5 metadata/seal policy. Source ownership extends narrowly to chunking.rs and covering tests; source author must supply meaningful RED/GREEN, accepted 6000CJK/96-chunk probes, single/multiple-group budget failures and actual DocumentService preservation/cleanup, followed by fresh affected gates/hash freeze. No stage/index window yet.


Task 4B: complete — 735284125838e9c1b0fce9a8a4c002482dff281e, feat: manage document imports and versions with fenced recovery. Final37 source paths, 4103 insertions/40 deletions; root independently verified freeze/index/worktree SHA sets and tree8c8c7e15665c8c21ae649c86388c0e8f110aafb0. task-4b-documents-review.md specPASS/qualityAPPROVED,0openCritical/Important/Minor; root no4Bproductionauthorship. Final lib572/bin5/actualworker6, native-free wire7, desktop/frontendbuild/fmt/diff and pureRustflate2 audit pass. Heading amplification repaired via private32MiB exact pre-clone budget; public algorithm/fullheadings preserved. Actual unresolved startupcandidate fixture passed withoutparserreplay. Sourcecommit acknowledged; indexempty/original.gitignore preserved. Freshreviewspawn/oldindependentthread initialization failed duepersist/diskI/O, so ledgered independentcontroller fallback used. No systemrepair/merge/push.
Task 4: complete — combined reviewed4A d72e32a plus reviewed4B7352841. Tasks5–10 remain queued. RealminimumOS/package/signing/audio/keyring/paid gates remain separate; no release-readiness claim.

Task5 read-onlyarchitecturepreflight complete/read: task-5-architecture-preflight.md (~1099words), notimplementation. Preparedtask-5a-ownership-brief.md scopes the required generation-child actualcompletion carrier and immutableadmittedcredentialresolver before actualSrvDB/modelhandlers. ActualTask5 architecture/checkpoint acceptance remains required; no indexengine/source write started yet.


Ruling: Task5 accepts bounded reference metadata schema2/codec chunk_location_ref_v1 with inline nullablepage/paragraph/start/end, exact canonical fullSQLiteTextLocation SHA256, IDs/version/contenthash and authoritative hydration. Keep full headings in SQLite and returnedhits; encoded vector metadata<=16384B. — Legal6000CJK fullheading disproved earlier16KiB inlinefull-location assumption, while reference metadata retains specified inlinepage and fidelity without 8TiB envelope. — Cost if wrong: codeccompatibility requires explicitnewgeneration/reindex, not silently reinterpreting existing mappings. Canonical serdeJSON field order/nulls and equality tests required beforepublication.
Ruling: Accept task-5-architecture-preflight.md fixedfilecaps/provenance: N200000 includingtombstones/d128–4096/f32/FlatthenAuto, vector24Bv4header and V=max(L+floor(L/2),L+1024r), metadata16GiB/watermark12GiB, graph512MiB, directory<=16entries/lock<=4KiB/current-writertemporariesonly; queue16/oneowner+2Rayon/<=2engines includingrebuildoverlap. Trustonlynewuniqueemptyintent creation or validatedcleanSQLite-sealed currentwriter sources, not arbitraryunsealedredb opens. Dirtyintent precedes writableopen/mutation; clean refreshedseal/mappings/readiness publishesonlyafteractualpersist. Dirtyownerloss preservesfiles/text and fails IndexRepairRequired beforeopen, requiringexplicitrepair. Missingderivedgraph mayrebuild only with unchanged verifiedsealedbasevectors+metadata; presentbadgraph ispreserved/rejected. — Eagerredb/postcard allocations happen beforea post-openfacade check; length alone isnot provenance. — Cost if wrong: conservative dirtyrestart repairmayneed explicit reembedding and filecapsareoperationalenvelopes, not a proved redb-growth/RSS/nativeperformance guarantee. No originalsourcefilesdiscard or paidstartuprepair.
Ruling: Vector-space identity remains canonical profile_id/normalizedbase_url/model/actualdimension/normVersion; localdimensionsNone discovery uses semantic profile_id/base_url/model/normVersion and records requestedDimensionoption. Label/timeout/context areexcluded fromvectorsemantics. None requiresexactlyone published/trusted-live actualdimension binding;0/ambiguous=>IndexRepairRequired/noHTTP. Some(d) requiresexactsealedactualdimension; probeNone→Some(sameactuald) retainsoldspace. Credentialexecution separatelycomparesthe entireadmittedModelProfile underoneConfigIogate. — Unknowndimension must nottrigger a metered discoverycall beforelocalreadiness, and non-vectorsettingsmustnotredefine existingvectors. — Cost if wrong: providerdefaultdimensionchanges surfaceInvalidResponse/repair, and ambiguityrequiresexplicitdimensions/reindex; no guessedbinding or silentprofileeditreinterpretation.
Ruling: Healthycurrentdocuments may be searched on an owner-retained trusted-live dirtygeneration, whileaffected documents areIndexing/stale and allsearch/hydration isserialized/revalidated. Durablegeneration.state/readiness and operationdirty have separate meanings; persisted dirty alone neverauthorizessearch. Dirtyrestart/ownerloss invalidatesthe entiregeneration. — This retains mixedhealthy/brokencollection behavior without blessing partial writes. — Cost if wrong: liveness/readiness coordination adds complexity; failclosed ifownertrustcannotbeproved.
Ruling: ExecuteTask5 sequential prerequisite5A generationchildownership+snapshotresolver beforeownercodec/model/search slices. Onegenerationtracker capacity16, token retains processlease/count untilactual finaldrop; acquisition rejectsclosed/stopping/cancelled and releasescontrolbeforeconfig/SQL/index. Drainworkers/ownedcleanup/acceptedchildrequests beforegenerationcompletion/rearm; cleanupusesretainedowner and indexqueueclosesonlyafterjobcleanup. — Required futureConfigIo/index actualwork outlives observerdeadlines, and ArcFilealone cannotfence same-AppState rearm. — Cost if wrong: tracker/ownerrequest APIsmayneedreworkatindexintegration; keepfocusedtests and independent5Areview. Preparedtask-5a-ownership-brief.md is the initialscope; exactAPIcheckpoint still precedesproductioncode. WholeTask5 remainsopen.


Task5A active — intelligence_runtime_impl BASE7352841 soleCargo/sourceowner; task-5a-ownership-brief.md and acceptedtask-5a-ownership-checkpoint.md. Reviewer workbench_ui_impl_retry read-onlytask-5a-review-preflight.md436words read/evaluated byroot. Exactacceptedstructure: lease-freeArcSemaphore16 tracker+Weakprocesslease, child stronglease then slot declaration/drop order, workers+cleanup+drainall16 then dropbase beforedone, noState strongtracker owner. Weakupgrade remains possible fromAppStateroot, so oldgenerationstopping/cancel fences remain authoritative.
Ruling: owneracquisition also requires control.active.contains(context.id) underthesamecontrolguard, missing=>Cancelled, notSQLite-under-control. — Existingworker removesactive afterterminaltransition, so a retained successfulJobContext otherwise couldissuenewnativework whilegenerationstillopen. — Cost if wrong: futurepost-terminalcleanup mustusealreadyacceptedowner, notnewchildadmission. Addterminalretainedcontext behavioraltest.
Ruling: snapshot testscovereveryModelProfilefield includingkind/max_output_tokens/id/absence and invalidreferences. Addonlycfg(test)check_fault("get_secret") atConfigTransaction.secret_get BEFOREensure_secret_lock; invalidsnapshot retains fault, unchangedcontrol consumes it. — Existingfaultseam didnotinterceptsecretreads; genericConfigurationalone cannotprovecompare/readorder, andKeyBackendcountercanbeinfluencedbyDEKcache. — Cost if wrong: a narrowtest-onlyworker pathisinownershipscope; productionConfigIobehavior/deadline/APIunchanged. ContinuecontrolledactualConfigIocallback/app.lock evidence separately. NoTask5 sourcefreeze/indexgrant/reviewverdictyet.


Task5A frozen/index checkpoint — controller independently read complete report, source/index manifests and six final logs at BASE735284125838e9c1b0fce9a8a4c002482dff281e. Fresh Python/git verification exit0: exact10path cached+worktree SHA/bytes, tree9740816f3c5ea5c66573414c410e5a5f10265f1a, full32283Bpatch SHA82697573b37e2ce5dfcee772a8e5f37015f88f07f5429cac9624c02744619bbf, preserved original.gitignore SHA1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a. Final evidence child12/lib588/bin5/realworker6/desktopbuild/fmt/diff all actual exit0/no warning. No source edits since gates. Independent reviewer workbench_ui_impl_retry(no5Aauthorship) preparing formal spec+quality verdict; no commit authorization yet. Four new-file mode headers normalized100755(no-indexpatch) to100644(cachedgit); bytes unchanged, canonical Rust source mode, reviewer evaluates actual cached metadata. EntireTask5 still open.

Ruling: FutureTask5 search retains full heading/location fidelity with canonical locationJSON<=16MiB and raw headingUTF8 wire<=16MiB, existing body8MiB unchanged; compact IDs/scores<=100 and one-row borrowed bounded hydration precede clones. Cap actual compact serde KnowledgeResults JSON at32MiB with checked counting writer, whole-hit eligible score prefix and final envelope/readiness recheck. — Legal9MiB internal-space heading and valid1500-tab/raw-space escaping cross the old8MiB location bound; count-only limits allow180MiB/100candidate amplification. — Cost if wrong: fewer complete hits may fit one reply and a single unfit valid hit returnsResourceLimit; this bounds result body only, not IPC envelope/RSS. Budget omission creates no repair/unavailable count. Wire defense remains post-allocation and does not assume JS/Rust float-byte equivalence. Corrected task-5-search-budget-handoff.md read in full; exact escaped/prefix/long-title/header/late-readiness fixtures required in5C.
Ruling: KnowledgeResults.identity becomes Option<String> before its first production implementation/contract, None for truly empty/unbound local success and Some(lowercase64hex) for resolved indexed vector space. — Empty library may have no embedding profile/actual dimension, so a mandatory String requires fabricated identity. — Cost if wrong: future Task8 consumer/fixtures require nullable handling; no current command is changed.
Ruling: IndexOwner construction is inert; start_owned starts the worker with an independent generation base process lease, not a cached job-child token. Jobs and direct operations must actually drain before index queue closure, then owner drops engines/pool and joins before completion/rearm; final sender loss also drains under base lease. — Idle engine mappings must retain app.lock, while cached child slots would deadlock job shutdown. — Cost if wrong: constructor/lifecycle wiring requires focused AppState close/drop/rearm tests and preserves the existing observer deadlines. Detailed5B durable descriptor/receipt checkpoint is still being finalized before source dispatch.

Task 5A: complete (commits7352841..df5717924dfd26b7f50ee4409f63324e891f5cf6, independentreviewclean). SpecPASS/qualityAPPROVED0open from workbench_ui_impl_retry, no5Aproductionauthorship. Controller read full review and freshly verified committed10path bytes/hash and exacttree9740816f3c5ea5c66573414c410e5a5f10265f1a/cachedempty/preserved.gitignore. Finalchild12/lib588/bin5/realworker6/desktopbuild/fmt/diff verified; no new source aftergates. EntireTask5 remains open; 5Bowner/codec/recovery and5Cembedding/search/automaticimport indexing remain required.

CanonicalTask5 refinement committedc8f7770 (oneplanfile only); nullableidentity/snapshotresolver/schema2/ownedinertstart/exact32MiB+16MiB heading policy now match acceptedrulings. Task5brief refreshed with committed5A producer note; historical proposals are explicitly superseded.
Ruling: Accept task-5b-index-owner-checkpoint.md typed IndexRepository private durableintent/persistedreceipt/publicationack with op_id/identity/gen/prior-seal-hash fences, clean-onlyunleasedLRU (otherwiseBusy), canonical64hex/UUIDpaths, and AppState ownedstart/recheck/actualjobs+directdrain/indexjoin sequence. Future5C seal/mapping/pointer/Ready publication must compose in oneSQLtransaction. — Durable flags and persistence success alone cannot establish source provenance or documentreadiness; queued/staleack and idlecached-engine lifetime need explicit ownership. — Cost if wrong: typedports may require narrow rework in5C; preserve focused realSrvDB/SQL/ownership fixtures and separate5B review before dependenthandlers.
Ruling: One transient retained join coordinator per stopping indexgeneration may finalize owner/pool handles, with no SrvDB/SQL/config work and no extra persistent indexexecutionworkers. Retain baseprocess lease through actual ownerjoin, release beforedone; spawnfailure must retainhandles and synchronousDrop fallback outsidefacadelocks cannot abandon them. — ThreadPoolDrop and cancellableTokio observers do not prove OSthreadexit or holdapp.lock throughjoin. — Cost if wrong: exceptional coordinator-spawn failure can make finalDrop wait synchronously; normal observer waits stay bounded and all actualwork remains owned. Exactmechanism recorded beforeproductionlifecyclecode; oneexecutionowner+twoRayonworkers remains binding.
Task5B dispatched sequentially to original available intelligence_runtime_impl (soleSOURCE/Cargoowner), BASEc8f77708ce959f9ab3e454147329fcff3771cc37, task-5b-index-owner-brief.md. Reuse instead of newauthor under recorded threadinitialization/diskincident; prior5A independentreview closed, no5B authorreview waiver. workbench_ui_impl_retry independentreviewer remains available; formalreview only afterfreeze. Task5C and6–10 remain required. No merge/push/hostrepair.

Task5C read-onlypreparation read in full: task-5c-model-search-checkpoint.md847words, acceptedfullresult/binding boundaries separated from concrete5B port proposals. No5Csource/test/implementationclaim.
Ruling: BeforefirstEmbeddings implementation, use crate-private mandatory EmbeddingExecution(owner,cancel) per-call carrier with private fields/required constructor and crate-private Embeddings/OpenAiCompatibleEmbeddings; long-lived collaborators stay unscoped, ephemeral config retains acceptedowner and completeprofile snapshot through real entered lookup. Directsearch owner also retains its2-searchpermit; jobs use pre-issued5Atoken. ExistingJsonHttp OnceTransient policy handles oneboundedEmbeddingretry, no customreplay; originalresolver/probes unchanged. — Cancellation-onlydraftcannotconveyactual-completion ownership, and exposinginternallease consumers is unnecessary for Tauri command boundary. — Cost if wrong: drafttraitcallers/fixtures need an intentional signature update beforefirstproductionconsumer; actual frozen5B ports and selectedsetting/binding/continuation seams remain5Ccheckpoint work. task-5c-ownership-decision.md records exactresolution.

Task5B dependency/environment checkpoint: /dev/shm/voxbridge-task5b.y3r0p9wa freshprobed/owned,5A temp preserved. Initialall-targetfetch session76211 cancelledafter366s byauthorSIGTERM onlyverifiedtaskPID1177863(exit143); this is cancelledsetup, notfailedtest. Targetedpin+Linuxfetch session58069 logs independentlyread byroot: cargo update --offline redb@4.5.0 --precise4.4.0 EXIT0, cargo fetch --locked --targetx86_64-unknown-linux-gnu EXIT0. ExactapprovedSrvDBgitpin/actualredb4.4.0 heldinlocalCargo.lock, directRayon/CRC32 justified byownerpool/envelope. Firstcodec/identity REDskeleton/tests prepared; no5Bpass/freeze/review/commitclaim. SoleCargoownership staysauthor.

Task5B firstcodec/identity behavioralRED independentlyread task-5b-codec-red.log: actualpinnedcrate compilecompleted,4failed/1passed,EXIT101. Failures use compiled minimalidentity/metadata skeleton returningConfiguration (canonicalvalididentity/full6000CJKreference/null-vs-empty/strictdecode control), not compilefailure. Invalidinputrejection already passes and is not claimednewRED. Author movesminimalcodec/identityGREEN then trustedowner/repository/lifecycle groups; nofreeze/passclaim.

Task5B codecGREEN independentlyread task-5b-codec-green.log/session92779 EXIT0:5/5,nodiagnosticwarnings. Canonicalsemanticidentity and boundedreferencecodec now implemented; actualfullcanonicalheading digest streamed, nullablepage required, null-vs-empty distinct. Source notfrozen; realrepository/trustedpreflight/owner/lifecycle remainauthoractive. No full5Bsuccess/commit/stage.

Task5B persistenceRED report/log independentlyread: session12515 actualexit101,3scenariofailures atcompiledcommoncreation-intentskeleton (notclaimedseparatedirty/tamperassertionRED). Temporarydeadfieldwarning mustbeconsumed byactualimplementation beforefinalgates.
Task5B materialdependencyfinding underSystematicDebugging: root independentlyread redb4.4 db.rs1885–1960/PageManager2373–2400. DatabaseDrop commits/trims allocatorstate and writes shutdownheader; Drop returnsvoid/ignoresorlogs errors, recovery_required clears onlyafterhealthyloaded state+flush/headerwrite/flush. Livepersist hash maychange afterengineDrop, so a preDrop seal cannotbeassumed stableoncleanreopen. Authorcompletesminimalrealroundtrip/hashesfirst; independentread-only reviewer workbench_ui_impl_retry prepares task-5b-redb-close-handoff.md boundedclosed-header/provenance/failureconditions. No arbitraryredbopen/hashweakening/pinchange or acceptednewcloseprotocolyet; awaitactualevidence before ruling.

Task5B Drop rootcause confirmed: root read complete task-5b-redb-close-handoff.md/checkpoint/evidence and receiptRED/GREEN logs. Actual72457 cleanroundtrip fails afterrealDrop whiledirty/tamper2pass;54776 characterization1pass, metadata544768B liveSHAda984877..2871ecd1/god2 vsclosedSHA2a91e8a8..7ce3e76/god5, vectorsunchanged. Source MetadataStore.commit165–178 usesImmediate aftervectorfsync; validsealedLive bytes remaincommittedtrustedprovenance. Receipt94653 RED proved repeatedpersist changesissuedhash;25947 GREEN1pass now reusesimmutableissuedreceipt/pinsmutation/exactlostack. Temporarydiagnosticwarnings are notfinalgates.
Ruling: Accept task-5b-redb-close-checkpoint.md Live/Closed storage tags asphysicalseal descriptions, preserving exacthash-matching cleanpublishedLive restart under fixedcurrentwriter provenance/header policy. Do not introduceClosed-only restart or reinterpretReady. Normal cleanLRU/shutdown/explicitclose must commitfencedCloseintent beforeactualpoolDrop, retainpreclosepersisted320B marker andownership, then requireboundedclosedmagic/version3/flags/recoveryclear/checkedgeometry/primaryXXH3checksum/currentprimaryselection/newertransaction/unchangeduser-root, sync_all files+supporteddirectory andwholefilehash beforeCloseReceipt/fencedClosedseal publication. Lostreceipt/ack staysimmutable andclosingrecordretainsproofforretry. — RealDrop commitsallocator/shutdownbytes, while successfulImmediatepersistalreadycertifiesLive data; hashingbeforeDrop failsnormalreopen andClosed-only wouldunnecessarilybreakcommitted-crashrecovery. — Cost if wrong: conservativepostconditions mayreject a libraryshutdown variant and requirelocalrepair; proofs bindonlyknownpersistedfixedwriter, notarbitraryredb integrity or everyinternalDrop return. XXH3librarydependency0.8.19/xxh3 accepted for exactformatchecksum; nohash/cap/pinweakening. Requirerealnormalclose/reopen andactualsubprocessskipDrop Live-restart evidence.
Ruling: IfnormalCloseintent fails, retainengine andreturnerror; finalfacadeloss/resource teardown cannot retainOShandlesforever merelybecauseSQLite isunavailable. Exceptionalfinalfallback maydropresources/joinunderretainedbase withoutnewintent onlywhenintentwritefailed, returnfailure/noClosedseal/no successfulclose, andpreservepriorsealed/dirty state+allfiles. Changedbytes failpreconstructorhash; unchangedexactpriorpublishedLive bytes remain validpreviouscommittedauthority. — DestructorscannotpropagatepreventiveSQLitefailure intoinfinitehandle retention, andexistingsealedhashgate preventsuncontrolledpostDropbytes beingblessed. — Cost if wrong: unavailableSQLite atfinalDrop mayleaveindex requiringexplicitrepair afterrestart andsynchronousjoinfallbackmaywait; no originaldata delete or falsepublishedreadiness. This is a narrowexception tointent-beforeDrop, notnormalmutation permission.

Task5B furtherfocusedgates verifiedfromfulllogtails/updatedauthorreport: recordsRED8917 EXIT1011failure/1already-rejectpass→GREEN74744 EXIT0,2/2,no warnings; actualfirst0/1 IDs/tombstones/appendreplacement2/half-open/search and invalidbatchbeforemutation. Preflightprotective3/3 EXIT0(noREDclaim), preservesunknownfiles/temps/badgraph/symlinks/nonalignedslack. XXH3dependency63312 EXIT0, unexpectedcombine4.6.8 restoredprior4.6.9 by47084 EXIT0. Closeprotocol/actualLivecrash childprocessfixtures compiling80948; noCloseGREEN/full5Bfreeze yet.

Task5B closecoreGREEN90546 actualexit0/fromauthor+rootlogread:18currentindex tests pass,31.04scompile/0.54stests. Realnormalcreate/persist/pub/ack/actualDrop/Closedreseal/reopenpasses; actualchildpublishesLive thenprocess::exit(noDrop), parentreopensrealSqlite/SrvDBsuccess. Bounded320Bheader/checksum/newercloseTxn/unchangeduserRoot anddirty/tamperpreservation pass. One temporaryunusedclose_generation warning remainsfornextfaultconsumer; notfinalgates/5Bfreeze. Authorcontinuesclosefailures/ownerqueue/enginecapacity/maintenance/AppStatejoinedlifecycle.

Task5B closefailure GREEN2760 EXIT0/fromauthor+rootlogread:5/5 includingactualLivechildprocessrecovery, badpostDropmarker/sync keepingdirtyClose/noClosedseal, exactlostcloseack andnormalSQLintentfailure retainingSrvDBlockuntilexceptionalfinalRuntimeDrop. Closed-header/faults remain narrowknown-writer proof, noarbitraryopen.
Ruling: AcceptboundedsharedStopControl(mutex/Condvar/coalescedattemptretry/forcefinal flag) so firststopcloseSender/JoinPayload canwaitactualowner while a normalclose-attempt SQLfailure retainstheengineforlaterexplicitobserverretry. Concurrentobservers coalescetoonependingattempt; stalepriorfailure cannotfulfillnewretry and attemptsuccess cannotprecedeactualjoincompletion. JoinPayloadretainsStopControl; unstarted/spawnfailedpayloadDrop mustforce+wake BEFOREsynchronousjoin outsidealllocks, asdoesfinalfacadeDrop. Normaljoincoordinatorneverforcesprematuredrop. — Withoutforce, single-threadTokio observer couldsynchronouslyjoinanowner waitingforafutureobserver, deadlockingfallback; withoutstop-control, firstsenderEOFpreventsnormalretry. — Cost if wrong: boundedper-generationattemptstateaddscoordination; exceptionalfallback mayforcefailedresource teardown/noClosedpub whenSQLitecannotwrite, whileholdingbaseprocessuntilactualjoin. Noextraqueue/persistentexecutionworker/arbitrarycallbacks. Requiretargetedsinglethreadspawnfailureandcoalescedretry/join-order tests.

Task5B basicdedicatedowner lifecycleRED73366 twoactualprocesslock failures→GREEN84112 EXIT0,2/2,21.98scompile/0.02stests; inertconstruction/idlebaselease/repeatedsameepochstart/actualshutdownrelease+freshepoch/finalfacadeDropafterTokioteardown. Unit-command scaffold is NOT finalqueue/operation implementation; envelope/ticket/observer/capacity +AppStateintegration remainactiveauthorwork. Rootcheckedscan actualpinneddb.is_deleted separatefrommissingmetadata, no newriskfinding. Cargo checksum resolution reroutedcssparser-macros0.7.1 synedge3.0.9→2.0.119; authorrestoredprior3.0.9 singleedge andmetadata--offline--locked--no-deps EXIT0, otherexistingpackageversionspreserved.

## Task5B controller resume frozen checkpoint — 2026-11-07

The prior implementation agent stopped after `task-5b-typed-commands-red.log`
(4 pass/2 `Interrupted`). Controller resumed without replacing completed 5A/4B
source. Root cause for the next failure was publication retiring the active
dirty intent before acknowledgment admission; if the acknowledgment path reused
normal intent validation it rejected its own successful publication. Ruling:
acknowledgment validates the committed exact `PublicationAck` at facade
admission and again inside the owner. It does not weaken mutation-intent
validation or accept a mismatched operation/generation.

Controller completed typed owner dispatch for open/add/scan/delete/replace/
search/persist/maintenance/ack; preserved a single 16-slot queue and per-request
lease; added source-frozen maintenance copy with at most 200 stored IDs per
step, same-source mutation/remap/close exclusion, immediate borrowed-vector
copies and exact f32 preservation; added clean-only two-engine LRU and dirty
capacity Busy; integrated the inert owner into AppState startup and close.
AppState close waits for actual job/child drain first, keeps index cleanup
available on job-close failure, acquires all 16 direct-operation permits, then
drains/closes/joins index before successful rearm. Successful reopen gets a
fresh index epoch. Last owner/facade teardown retains the independent process
lease through actual join.

RED/GREEN covered typed work roundtrip, failed-close observer retry, typed scan/
delete/replace/search, vector-preserving maintenance copy, dropped/cancelled
maintenance release, clean LRU versus dirty Busy, forced join-spawn fallback,
AppState close ordering/timeout retention and fresh reopen. Fresh gates at
frozen source: library627/627, main5/5, realdocument_worker6/6,
offlineVolcengine2/2 with1 paid ignored, loopback example3/3; therefore643
passed/0 failed/1
intentional ignore across `--all-targets`. Desktop bin build, cargo fmt check
and cached/working diff checks all exit0/no warnings. `cargo tree
-i redb@4.4.0 --depth 3` shows redb4.4.0 through the exact SrvDB pin only.

Frozen package: BASE `c8f7770`, cached tree
`77bf41215c5c4cc043bb94826976c2034a691c8d`,24 task-owned paths,
`task-5b-index-owner.diff`178644B SHA256
`d8827c5c62f06066b43358ef2deb54f141f3d713071072547bd7679660c56569`,
`task-5b-source-freeze.sha256` all24 OK. Reports:
`task-5b-index-owner-report.md`, `task-5b-review-brief.md`. Original
`.gitignore` SHA1e708ede... remains unstaged/unowned; no other path staged.
Independent round1 found startup/close admission and missing post-durability
proof; round2 found the publication/open transition gap and a weak startup
interleaving test. All four are repaired with deterministic GREEN tests.
Reviews remain round1/round2 files. Source is refrozen for round3 but **not
committed and not complete** until a fresh independent reviewer writes
`task-5b-index-owner-review.md` with
specPASS/qualityAPPROVED and zero unassigned load-bearing findings. Controller
must not self-review as independent. Task5C andTasks6–10 remain unstarted.

## Task5B round4 refreeze — 2026-11-07

Controller reproduced the prior full-suite failure
`admitted_starting_stop_suppresses_inflight_running_publication` on the second
immediate focused run (`Running` at the `Stopping` assertion). Root cause was
test synchronization, not a production state regression: Provider `finish`
may enter before the lifecycle observer commits durable `Stopping`, so it is not
a happens-before signal for the state being asserted. The test now waits on
`wait_status(Stopping)` before reading the runtime snapshot. The exact test
passed30 consecutive runs and the full17-test `session::orchestrator_tests`
group passed. This test-only change remains unstaged and outside Task5B.

Round3's three load-bearing findings are repaired and staged: publication is an
owner-queued operation serialized with shutdown and stale receipts return
`Interrupted`; startup failure drains all16 direct permits; transition fences
include `operation_id + owner_epoch`, so an old observer cannot clear a new
generation's fence. Focused regressions cover persist→shutdown→stale publish,
startup-failure direct drain, and delayed two-observer A/B cleanup.

Fresh final evidence at the refrozen source: library630/630,
`--all-targets`646passed/0failed/1 intentional paid ignore
(lib630/main5/document_worker6/offlineVolcengine2/loopback3), desktop binary
build exit0/no warnings, fmt exit0, working and cached diff checks exit0.
Freeze: BASE `c8f77708ce959f9ab3e454147329fcff3771cc37`, cached tree
`0aff5198ff518179ce165b43b45c5c8cd945701f`,24 owned paths,
`task-5b-index-owner.diff`186716B SHA256
`f80e6f1e37aa9a54ed89aa03b107d111e27bcdf3565bc5029125dcb74e5c223b`,
manifest SHA256
`435f116f7eddf82299214a909c7206ee0845d613e1e9e82107c10c94a1f03082`;
all24 manifest entries and exact patch reproduction pass.

`task-5b-index-owner-report.md`, `task-5b-review-brief.md`,
`task-5b-source-freeze.sha256` and `task-5b-round4-freeze.json` are updated for
round4. Only the24 owned source paths are staged; original `.gitignore` and the
session-test synchronization fix are unstaged. Task5B remains uncommitted.
A fresh independent reviewer must write `task-5b-index-owner-review.md` with
SpecPASS/QualityAPPROVED and zero unassigned load-bearing findings before commit.
Task5C and Tasks6–10 remain unstarted.

Task 5B: complete (commits
`c8f77708ce959f9ab3e454147329fcff3771cc37..99cc3b3`, independent round4
review clean). Independent `task-5b-index-owner-review.md` is SpecPASS /
QualityAPPROVED with zero Critical/Important and four non-load-bearing Minor
observations: the documented transition-mutex-over-SQL exception, blanket
dead-code allow to remove in5C, same-process clean reuse without rehash, and
conservative temporary rejection. Final source commit is
`99cc3b3 feat: add owned SrvDB index persistence`; the out-of-package session
test synchronization correction is `1ea0bdd`.

## Task5C architecture acceptance — 2026-11-07

Controller read the complete 849-line
`task-5c-architecture-reconciliation.md` produced read-only at `1ea0bdd` and
accepted its ten required decisions:

Ruling: Extend 5B with typed `PublicationPlan` through
`IndexOwner::publish`/`IndexRepository::publish` and a narrow facade close;
seal, mapping publication, generation pointer and affected Ready/chunk/repair
changes commit in that one fenced SQLite transaction. — Separate Ready
publication can expose bytes not certified by the persisted receipt and cannot
be recovered as one authority transition. — Cost if wrong: 5C adds a bounded
plan codec/envelope variant to reviewed 5B ports, without changing queue,
worker, engine or ownership limits.

Ruling: Add migration 5 with nullable `semantic_key`, `requested_dimension` and
`updated_at_ms`, two partial unique binding indexes, and preflight indexes; do
not rebuild m4 tables, backfill trusted rows, or encode state validity only in
CHECK constraints. — Existing clean generations cannot honestly invent missing
binding metadata, while typed repository validation can enforce new state
values additively. — Cost if wrong: legacy generations remain repair-required
and need explicit reindex; a future normalization migration may be required.

Ruling: Binding identity uses canonical semantic key
`{profile_id,normalized base_url,model,normalization_version}` plus recorded
`requestedDimension`; configured None resolves exactly one published actual
dimension locally, and concurrent first publishers use winner-first with the
loser failing its fence. — This avoids endpoint/model collisions and paid
discovery while retaining profile edits without reinterpretation. — Cost if
wrong: ambiguous local bindings require explicit repair/reindex.

Ruling: Use crate-private `Embeddings` with mandatory
`EmbeddingExecution{owner,cancel}` and `AppResult`; each call scopes exact
snapshot credential resolution under ConfigIo, uses shared4-request gate,
`OnceTransient` only, and limits32 inputs/128KiB serialized request/8MiB
response. — Caller cancellation alone cannot own entered native work, and
custom retry/replay would duplicate existing HTTP policy. — Cost if wrong:
transport trait fixtures require the mandatory ownership carrier.

Ruling: Add optional scheduler `AfterTerminal` and a single-gate
`selected_embedding_snapshot()`; only committed Parent Succeeded can reserve
the continuation token, and queue/exclusion/no-selection fallback keeps Parsed
with `auto_index_deferred` rather than failing the parent. — Triggering at
handler success loses the exclusion-release race; automatic replay on restart
would create unbounded paid work. — Cost if wrong: bounded continuation fields
and scheduler tests expand, while manual Reindex remains the fallback.

Ruling: Run `reconcile_owned(lease)` from `start_intelligence` after index
readiness and before jobs start, using zero paid embedding calls. — Startup
must classify orphan/stale mappings and dirty generations before healthy work
becomes searchable or schedulable. — Cost if wrong: startup may mark more
documents repair-required; it never erases source text or managed originals.

Ruling: Raise only the raw heading decoder to16MiB and add a conservative32MiB
aggregate estimator in the knowledge wire module; backend prefix budgeting is
authoritative and cannot assume JS/Rust float byte equality. — The prior8MiB
decoder contradicts accepted legal headings, while frontend allocation must
remain bounded. — Cost if wrong: the estimator may reject conservatively and
will be reconciled with Task8's full DTO plumbing.

Ruling: Delete stages and persists paged vector work before mutation, retains
work/mappings through persisted deletion and managed-file cleanup, and rebuild
publishes/acks the source refresh before closing then deleting the retired
generation only after the staging pointer swap. — Cleanup must survive removed
document/job rows and two-engine maintenance cannot safely retire a dirty
source. — Cost if wrong: retained mapping/work rows delay identity deletion
until explicit cleanup; they never reclaim capacity opportunistically.

Ruling: Compose `KnowledgeService` with store, config, scheduler, embeddings,
IndexOwner and data dir; direct search uses the AppState intelligence lease and
a two-outstanding cancel-on-replacement gate. — Managed-file cleanup requires
the data root, while search must retain real process/permit ownership across
observer loss. — Cost if wrong: the constructor gains one internal path
parameter and search capacity remains explicitly bounded.

Ruling: Approve test-only CommitPoint seams at SQLite intent, vector mutation,
vector persist and Ready publication, plus a real-SrvDB KnowledgeHarness. —
Crash recovery needs deterministic evidence at each authority boundary without
changing production transaction logic. — Cost if wrong: test fixtures are
broader; any test-only hook must be cfg(test)/injectable outside shipping paths.

Task5C implementation is authorized at BASE
`1ea0bdd6309e8fee1cacc28b4ae79ec58e2783ee`; no 5C source exists yet.

## Task5C active execution — 2026-11-07

Resumed from the actual committed worktree, not the earlier uncommitted 5B
summary. The independent round4 review and `99cc3b3` tree match
`0aff5198ff518179ce165b43b45c5c8cd945701f`; HEAD is `1ea0bdd`. The sole
pre-existing source difference is the unowned `.gitignore` at its recorded
SHA-256. No previous subagent remains live.

Ruling: Use durable paged work/mapping staging for plain reindex, not an
in-memory final-ID array carried in PublicationPlan. The architecture's
section 4 bounded plan and section 8 optional plain-reindex staging conflict
at the existing document chunk limit. The compact plan references an operation
and exact document/generation fences; its single SQLite transaction validates
and publishes all staged rows with bounded pages. — Preserves atomic Ready
publication and bounded temporary memory for large supported documents. — Cost
if wrong: extra local staging writes and later cleanup; unpublished rows remain
ineligible for search.

Execution brief: `task-5c-active-brief.md`; freshly extracted original
requirements: `task-5c-source-plan.md`. The old TMPDIR was gone; the task-local
replacement is `/dev/shm/voxbridge-task5c.PpMKwW`. An untouched-source offline
locked all-target baseline is running with the existing local Cargo and ALSA
environment; this is verification, not a new 5B implementation/review.
Task5C remains in progress; Tasks6–10 remain queued. Commit and source-freeze
windows are serialized and the main checkout is untouched.

Task5C baseline: command in `task-5c-baseline.log`, all-targets646 passed/
0 failed/1 intentional paid ignore (630 library,5 main,6 worker,2 offline
Volcengine,3 loopback), actual exit0. Fresh `git diff --check` exit0; no warnings
in the baseline log. Dependency artifacts were rebuilt under the current
toolchain before the test run; this does not modify Cargo.lock.

Task5C implementer `/root/knowledge_task5c_impl_retry` (isolated
gpt-6.1-sol/high) now owns all source changes and Cargo. The initial
`/root/knowledge_task5c_impl` had two model-service admission failures and made
no edits; fallback uses the available standard tier with the fully accepted
architecture. Root retains ledger, review-package and commit/index ownership.
Full task report is `task-5c-report.md`.

Ruling: The accepted Task5C additive binding migration owns schema version5;
shift not-yet-implemented Task6 summaries to `0006_summaries.sql` and Task7
research to `0007_research.sql`, with the summary snapshot index in migration6.
Updated the canonical plan's file names, version descriptions and migration
order. — The earlier5-summary/6-research reservation conflicts with accepted
Task5C D2; additive numbered migrations cannot reuse a version. — Cost if
wrong: future migration labels need adjustment, but no existing schema1–4 is
rewritten and no summary/research source exists. Root owns this one tracked
plan edit; it belongs in the eventual reviewed Task5C package.

Task5C runtime RED: transport stubs produced3/3 intended assertion failures,
then request-budget coverage produced4/4 failures; compilation-only API
diagnostics are retained separately and do not count as behavioral RED. Wire
slice author reports6/6 GREEN. Earlier migration GREEN attempt was invalidated
by unfinished API test compilation and is not accepted as success evidence.

Ruling: Isolate subsequent Cargo commands with task-local
`CARGO_TARGET_DIR=$PWD/.superpowers/toolchain/cargo-target-intelligence-20261107`.
The implementer observed an automatic `cargo check --workspace ...
--keep-going --all-targets` on the same manifest after its test process exited;
new zeroize fingerprints had identical compiler/features/flags but different
profile/path hashes, and shared-directory runs repeatedly recompiled deps.
— A separate target removes the shared artifact/lock boundary without editor
or global config changes. — Cost if wrong: one extra cache warm-up and disk
usage inside the ignored worktree toolchain directory; no source/dependency
change. Available worktree space774GiB; all gates must use the new target.

Ruling: Hydration/reconcile attributes corruption through the active
published SQLite inverse mapping, never an untrusted metadata document ID.
Unmapped/unpublished/obsolete candidates are excluded and later cleaned
locally; they must not switch Parsing/Parsed/Indexing/Deleting rows to repair.
Repairing a mapped Ready document rechecks its UUID/version/Ready fence in
the transaction. — The plan's broad "any mismatch" language otherwise
conflicts with authoritative SQLite ownership, replacement exclusion and the
AfterTerminal Parsed-state admission fence. — Cost if wrong: stale orphan
cleanup waits for local reconcile, but cannot expose wrong hits or interrupt
a current import. Carry this clarification and mixed-status/foreign-metadata
regressions into the final Task5C review.

Named producer check: `index/owner.rs:372–402` still collects strict decoded
metadata with `?`; missing/malformed metadata at399 aborts the entire candidate
list. This blocks the required mixed healthy/broken hydration even when a
bad mapped document was locally excluded. Ruling: Add one compact-candidate
port carrying validated ID/finite score and optional decoded metadata for5C;
retain strict `search` as a wrapper for existing5B consumers. Keep storage,
out-of-range ID and non-finite score failures strict. — Per-candidate inverse
mapping must retain the bad candidate ID to repair its actual owner while
returning healthy hits. — Cost if wrong: one bounded queue variant/private
port and compatibility wrapper, with unchanged workers, pool and limits.

Ruling: A witnessed delete-only dirty crash must finish via local fresh-
generation recovery, not indefinite Deleting. Pinned delete only stages
tombstones; unchanged-count vector sync does not write vector bytes
(`engine/delete.rs14–27`, `engine/commit.rs14–27`, `record_file.rs156–167`).
Allow an owner-scoped ordinary read-only vector File, exclusive source lock
and bounded reader lease ONLY when exact prior Seal/Intent/whole vector
length/hash/geometry match before and after copy. Reconstruct survivors from
authoritative published SQLite mappings, exclude deletion targets, and
atomically publish fresh mappings/pointer/deletion through the persisted
destination receipt. Never open dirty metadata/graph/SrvDB or bless its seal.
Witness/fence mismatch preserves source and Deleting/work state with zero
automatic embedding calls. — Required crash-delete local completion otherwise
contradicts5B's correct dirty-open prohibition. — Cost if wrong: new bounded
read-only recovery seam and one local generation copy; unmatched evidence
remains repair-required. Exact contract/tests are in
`task-5c-delete-recovery-ruling.md`.

## Task5C controller continuation — 2026-11-07

Controller resumed the existing sole implementer and sent the previously
unannounced dirty-delete recovery ruling; the implementer acknowledged reading
it in full and is proceeding with import/AppState integration before the new
recovery seam. Source remains uncommitted/unstaged at HEAD `1ea0bdd`.
Fresh read-only integrity check: `git diff --check` exit0; expected unowned
`.gitignore` SHA256 unchanged; main status only its pre-existing untracked
`src-tauri/Cargo.lock`. No additional source/Cargo owner was started.

Controller inspected the actual compact-candidate and search-ownership logs:
candidate runtime RED0pass/1fail -> GREEN1/1; search capacity runtime
RED0pass/1fail -> GREEN5/5. Search GREEN retains two transient dead-code
warnings for not-yet-integrated shutdown/reopen methods, so it is not a final
no-warning gate. These focused groups overlap with earlier consumer tests;
counts must not be summed. Full Task5C tests and independent review remain
pending. `task-5c-controller-gates.md` maps the full accepted contract to
evidence required for source freeze/review and carries the final gate order.

Task6 preparation was read, not dispatched. The old snapshot handoff/current
ownership note contain historical migration5/6 numbering; the accepted
Task5C rule and fresh `task-6-source-plan.md`/`task-7-source-plan.md` bind
summaries to migration6 and research to migration7. Owned LLM execution must
consume the reviewed actual-completion/profile-snapshot seam and automatic
terminal scheduling must not delay audio stop. Tasks6–10 remain queued.

Controller prepared `task-6-active-brief-draft.md` only; it explicitly forbids
Task6 source/Cargo until reviewed5C, and requires a concrete pre-source owned
transport/terminal callback/snapshot/legacy projection checkpoint. Narrow
producer reads found existing legacy summary fixtures use status `complete`
with `{title,text}`/`{text}` JSON; preserve them and reconcile the new strict
SummaryView/export projection rather than silently treating them as native.
No Task6 agent is dispatched and no source changed by the controller.

Task5C author reports import GREEN2/2, now verified in actual log:
committed success -> one Reindex; no selected profile -> Parsed/deferred/zero
embeddings. Transient unconsumed search shutdown warnings remain in this slice.
Command behavioral RED0pass/1fail is also present at the actual command
dispatcher before integration. Edge fallback/drain/startup tests, witnessed
delete recovery, large publication/budget/binding groups and final gates remain.

Command GREEN now independently read:1/1, exit0 in the actual log, no warnings.
Controller/author final-run coordination: author owns source/Cargo through all
remaining behavioral focused GREEN groups and self-review, then hands back
frozen source and exact path/evidence list. Root runs one final frozen-tree
knowledge/intelligence/all-targets/desktop/frontend/build/fmt/diff gate pass
and returns results for the author's final report. No duplicate full suite
solely to reconfirm a recorded pass; repeat only for fixes/failures/concrete
unresolved concerns. Until that explicit handoff, root does not run Cargo.
Active brief updated with this continuation protocol. Review draft is
`task-5c-review-brief-draft.md`; no task reviewer dispatched yet.

Dirty-delete actual runtime RED independently inspected: survivor completion
and last-document empty publication fail2/2; vector-witness tamper protection
already passes1/1 and is not claimed RED. `task-5c-recovery-red.log` has
actual2failed/1passed. Agent restored context, has no Cargo pending, accepted
the final-run handoff protocol and now implements the ordinary-File recovery
seam. Root compiled `task-5c-evidence-catalog.json` with23 historical log
hashes/results and explicit in-progress status; it is not a final frozen-code
gate. Root requested exact extra migration5 work association columns/bounds/
legacy fallback for the proposed compact document/location publication fence;
existing migration4 already has doc/version/chunk/content hash. No final
schema ruling is recorded until those exact fields are supplied.

Ruling: Extend additive migration5 work rows with nullable bounded
`document_fence_json` (<=1024 UTF-8 bytes, strict canonical UUID/version1..u32/
exact lowercase64 SHA/nonempty parser<=128 bytes/optional import UUID),
`location_sha256` (exact lowercase64 digest) and `previous_vector_id`
(canonical u64 decimal<=20, required for copied survivors). — Existing4 work
rows have doc/version/chunk/content hash but cannot fence same-version full
document/import/parser reassociation, location mutation or source-vector remap
after multi-document copy; compact PublicationPlan cannot hold all fences in
memory. — Cost if wrong:<=1024-byte document evidence and two compact refs per
staged row plus local writes/checks; NULL/invalid legacy proof stays repair/
Deleting with original bytes/pointer preserved. Exact byte/codec/rollback/
multi-page requirements: `task-5c-work-evidence-ruling.md`. No new cleanup
table, backfill, lower supported chunk/dimension cap, worker or paid call.
Agent received the accepted exact definition and continues implementation.

Dirty-delete first GREEN log independently inspected:3/3 exit0, real survivor
f32/search/repeated reopen, last-document empty destination, tamper byte/work
preservation. Log contains one unused RecoveryFence re-export warning; author
removed it after the run and is adding follow-up abandoned-destination/
post-pointer retired cleanup/source-lock/reader-ownership regressions. This is
not a final no-warning/full5C gate. Actual migration5 now includes the accepted
three compact evidence columns; final codec/fence/multi-page verification is
still required. No broad final run or source freeze has occurred.

Latest author focused `--lib knowledge::index_tests` GREEN independently read
from `task-5c-repair-recovery-green.log`:22/22, no warnings. This combined group
overlaps earlier slices, not22 additional tests. It includes all six dirty
recovery cases, source lock/reader200 bound, abandoned pre-pointer destination
cleanup, post-pointer source/work retirement, plus isolation of malformed
reconcile records and removal of earlier hits when a later candidate repairs
their same document. Follow-up cleanup runtime RED4pass/2fail and repair-
isolation runtime RED4pass/2fail were read at the actual assertions. Earlier
cleanup propagation after committed pointer initially failed and is repaired;
the final group is now clean. Source is not ready for freeze; large paged
publication/fence, byte budget, binding and remaining lifecycle/import edge
groups continue. Root has not run Cargo or dispatched review.

Multi-page publication evidence independently read:
`task-5c-publication-behavior.log`,2/2 no warnings, actual241-chunk Ready and
dense rebuild plus late-page240 document/version/SHA/parser/import/location/
pointer/evidence/binding rollback. Existing-path coverage, not claimed new
behavioral RED. Budget fixture's equal-score ordering diagnostic is not a
budget RED; actor is correcting scores before assessing actual32MiB behavior.

Ruling: Add only a cfg(test) child helper
`intelligence::jobs::test_support::replace_pre_start_handler` requiring
frozen=false, generation=None and an existing handler to adapt generic AppState
ownership fixtures before first start. Keep original Reindex/document
exclusions/assertions; no runtime/reopen replacement or production register
weakening. — Source read and actual0pass/1fail duplicate-registration RED show
new required production Reindex collides with old placeholder registration;
moving it to Research only postpones failure until Tasks6/7 fill all kinds.
— Cost if wrong: one private non-shipping fixture helper and preserved real
ownership tests. Exact guard/scope/evidence: `task-5c-test-fixture-ruling.md`.
Root applied SystematicDebugging investigation before this fixture correction;
sole source/Cargo owner still the original5C author, no root code fix.

Expanded focused evidence read: consumer group27/27 (overlaps prior groups),
`task-5c-consumer-expanded-green.log`, no warnings, including all four reindex
commit boundaries and actual escaped near32MiB prefix/no-lower-fill fixture.
The equal-score setup diagnostic was corrected, not claimed as budget RED.
`task-5c-repair-fence-red.log` is true0pass/1fail (Indexing row quarantined),
now repaired under current-version/Ready guard in the expanded GREEN.
Embedding expanded7/7 no warnings includes entered-HTTP shared4 gate/lease,
invalid indexes/dimensions/extremes and8MiB defense. Wire expanded8/8 is in
`task-5c-selection-wire-green.log` (the older wire-green log remains6/6);
empty-query RED is in query-wire-red. Source still owned by author; prestart
fixture helper implemented per ruling but AppState/import/binding/capacity
acceptance gaps continue. No final broad gate or source-freeze window yet.

AppState existing ownership group15/15 no-warning GREEN read, with the approved
prestart fixture helper. Real command selection/reindex/delete focused1/1
GREEN read. Maintenance begin-observer-loss true RED0/1 -> GREEN1/1 now creates
its guard before awaiting owner entry and retains runtime lease until actual
release. Manual Reindex ConfigIo admission observer-loss true RED0/1 (queued
commit never woke scheduler) -> GREEN1/1 now publishes/wakes in the actual
post-commit callback. Exact logs: task-5c-app-ownership-green,
task-5c-command-selection-green, task-5c-maintenance-owner-{red,green},
task-5c-admission-owner-{red,green}. New AppState entered-search lock/close/
rearm cases and remaining import/binding/pressure evidence are still pending.

Ruling: Use pure local rebuild on a compatible certified clean binding when
metadata>=12GiB, stored+actual intended chunks>200000 with reclaimable records,
or obsolete/tombstone records>=50% stored using checked integer arithmetic.
Recheck survivors+intended against200000 before any embeddings; empty count
does not divide or repeatedly rebuild, but watermark is independent. Consume
the fresh pointer fence before reindex. — Adds a concrete internal reclamation
trigger for the approved local rebuild without reinterpreting dirty space or
issuing paid repair. — Cost if wrong:50% may copy earlier than necessary,
adding local disk work while the source survives until publication/close/ack.
Exact scope/real-small-index/arith-only limits:
`task-5c-rebuild-trigger-ruling.md`. No new UI setting, smaller cap, worker or
maximum-size/performance claim. Source still owned by author, no broad gate.

## Task5C controller evidence reconciliation — 2026-11-07 continuation

Resumed the existing sole source/Cargo owner, knowledge_task5c_impl_retry;
no duplicate author, source edits or Cargo run by the controller. Actual
`task-5c-survivor-coverage-red.log` is runtime0pass/1fail at the missing
survivor-chunk coverage assertion; GREEN is1/1, no warnings. The original
`task-5c-app-search-owned-green.log` remains1pass/1fail and is not success
evidence. Corrected `task-5c-app-search-owned-rerun.log` is2/2 after restoring
the fixture's normal final-close observation allowance; actual entered-search
close/rearm/process-lock assertions remain. `task-5c-after-terminal-owned-
green.log` is2/2, no warnings, proving pre-terminal child reservation, actual
child drain and no success hook for failed/cancelled parents. Groups overlap;
counts are not summed.

Author has not frozen source or handed back Cargo. Remaining named work:
pre-copy document/location/source-vector evidence freezing and its mutation
regression; failed-rebuild source refresh lifecycle; import fallback/stale
notice evidence; binding transitions/first-publication race; malformed
candidate fields; narrow unused-port annotations, formatting/self-review and
the exact path/hash/report handoff. Controller final gates and independent
Spec/Quality review remain pending. BASE1ea0bdd is unchanged; the unowned
.gitignore hash is preserved. Tasks6–10 are queued, not dispatched.

Additional actual evidence inspected during this continuation: pre-copy
document/location/previous-vector association freeze runtime RED0/2 ->
GREEN2/2 (`task-5c-copy-fence-{red,green}.log`), and failed destination copy
source-refresh runtime RED0/1 -> rebuild groupGREEN4/4
(`task-5c-rebuild-source-{red,green}.log`). Failed-copy source is clean,
searchable and reopenable; no-warning GREEN groups overlap older tests.
Malformed borrowed source-field corruption runtime RED0/1 -> GREEN1/1
(`task-5c-source-fields-{red,green}.log`) now repairs the actual inverse owner
without hiding healthy neighbors. Wire finite-score runtime RED8pass/1fail ->
GREEN9/9 (`task-5c-finite-score-{red,green}.log`) removes an extra undocumented
numeric range rejection while preserving finite-number checks. Binding
existing-path evidence2/2 (`task-5c-binding-behavior.log`) covers None→same
dimension, profile/endpoint/model edits and concurrent conflicting first
dimensions with one winner; it is not claimed a new behavioral RED.

The root-owned evidence catalog now observes62 historical focused logs with
exact bytes/hash/result lines, nullable command exits and explicit in-progress
source-not-frozen flags. It is not final gate evidence; filenames do not
determine outcomes, and the final snapshot must reconcile renamed live logs.
Import fallback fixtures had a compilation-only attempt and are still under
author iteration. No source/Cargo handback, full final gates or independent
review has happened.

Late author self-review identified and root inspected three genuine runtime
RED0/1 cases: `task-5c-recovery-directory-red.log` accepts unexpected/symlink
source entries; `task-5c-delete-payload-red.log` accepts inconsistent target
document proof; `task-5c-startup-reindex-owner-red.log` changes a current
Indexing document to repair_required despite its matching unfinished Reindex
job. These remain with the original author for focused fixes, not a new
review/fix round. The preceding fixture Clone/visibility compilation logs
are preserved separately and are not behavioral RED. Review draft now names
these boundaries and the actual final-envelope trailing-hit trim evidence.
No source/Cargo handback or final gates have begun.

Late consumer GREEN independently read: `task-5c-final-consumer-focused.log`
43/43, no warnings, actual75.07s run. It includes the directory/delete/
startup-owner fixes and actual exact32MiB final trailing-hit trim. This is a
focused overlapping group, not a full Task5C final gate. Import fallback
fixture attempt failed before behavior on required created_at_ms; corrected
`task-5c-import-fallback-behavior.log`2/2 preserves Parsed under queue/exclusion/
missing-profile/shutdown and ignores stale/deleted notices. Production-only
check caught a cfg(test)-inherited Mutex import missing in shipping searches;
local import fixed and `task-5c-production-formatted-check.log` now finishes
clean. Formatted publication group2/2 is also read. Author formatted48 Rust
paths and both wire files; final ownership/report/hash handback still pending.

Formatter adaptation: default npx cache fetch failed EROFS at /home/mint/.npm;
per-command task-local npm cache was offered, without global configuration or
mount changes. Read-only inventory found existing offline Prettier3.7.4 at
`/home/mint/.cache/pnpm/dlx/5ec39437e8278f141a2042960ccd5aac/mw4tnriu-ft0e/node_modules/.pnpm/prettier@3.7.4/node_modules/prettier/bin/prettier.cjs`.
Root final format check will invoke this observed binary directly. npm update
notices in the intermediate write log are not a final format check. No root
Cargo/source edit, host repair, final broad gate or independent review yet.

## Task5C source freeze and controller final gates — 2026-11-07

Author explicit frozen source/Cargo handback received09:49UTC with no pending
commands/edits. Late delete paging runtime RED0/1 -> GREEN1/1 proves200 invalid
retained payload rows do not block valid deletion201; no embedding. Root
verified all52 paths against author manifest SHA256
`7941d11a1a0174a074fab7aef935992189f06dda0a7027075c7ee3b5f756a9c1`, exact
changed/untracked ownership set, preserved .gitignore and untouched main
Cargo.lock. Staged only52 task paths at cached tree
`56f69dc5ff0557615fed6d8f13a6e3a0377594db`; no commit. Freeze is
`task-5c-source-freeze.json`, full source diff558405B and prepared review
package are recorded there. Root now exclusively owns source/Cargo.

Fresh frozen-tree controller gates completed: knowledge filter163/163,
77.03s, no warnings; focused wire9/9; full frontend12files/96tests; observed
offline Prettier3.7.4 check exit0. Source hash/freeze unchanged. Intel filter
and frontend build are started; all-targets/desktop/fmt/final diff and
independent review remain pending. Exact command/session records are in
task-5c-final-execution-state.json and root tool state. These focused counts
overlap full gates and are not summed.

Controller final intelligence filter failed99/100 (exit101): the unchanged
legacy-v2 migration test still asserted current user_version4 while the
accepted additive migration5 correctly yields5. Root read the actual failure
and the migration dispatcher, then checked every user_version assertion in
storage/sqlite_tests.rs. A second legacy-runtime upgrade current-version
assertion also remains4; failure rollback assertions0/2/99 are intentional.

Ruling: Update only the two completed-upgrade current-version assertions4→5
in storage/sqlite_tests.rs, preserving every legacy row/export/reopen/rollback
assertion and without weakening them to >= or deriving expectations from the
implementation's migration count. — Migration5 is accepted and registered;
these assertions predate it, not a production migration defect. — Cost if
wrong: future accepted migrations require the same explicit fixture update;
all data preservation checks remain. Original author receives scoped source/
Cargo ownership for this file and its focused storage test group. Root will
resume final intelligence/all-targets/desktop gates only after re-freeze.
This is a pre-review gate correction, not an independent-review fix round.

Frozen-round0 frontend96/96, wire9/9, format and production build are complete;
build has the existing Vite PLUGIN_TIMINGS advisory. Knowledge filter163/163
also completed. These unchanged-source results are retained; no unnecessary
repeat of unaffected frontend or expensive knowledge filter. Original freeze/
path/hash manifest preserved as task-5c-round0-* artifacts. No independent
review dispatched and no commit yet.

Scoped fixture correction returned: storage/sqlite_tests15/15, exactly two
4→5 literals, format0. Root confirmed all original52 source hashes unchanged;
new53-path manifest SHA256
`172d891e12bb5dcd0997d27957439696f6ec827f61cd8c5b6c34fc7171f4222c`.
At re-freeze the live index was empty; source/HEAD were unchanged and the
original frozen Git tree still existed. The first ownership audit assumed
prior staging and stopped before mutations; corrected audit includes both
tracked diffs and untracked paths, then explicitly captures all53 reviewed
paths. No system repair or source workaround. Frozen tree is now
`76677da1a712b77a10f17fd59b1bfcc2c31553a2`, diff560422B SHA256
`28896f767f931a2807de84ed3bee10dc998b901627ce0d02f04d60d43e97aaa5`.
Immutable tree/file hashes, not assumed index persistence, bind review/commit.

Root final gates complete with actual exits0: intelligence-rerun100/100;
all-targets library693/bin5/realworker6/Volc offline2/loopback3 and1 intentional
paid ignore, no Rust warnings; desktop build; Rustfmt; independently captured
working/index/frozen-tree diff checks and source/package hashes. Earlier
unchanged knowledge163, frontend96, wire9, Prettier and frontend build passes
are retained. Existing Vite PLUGIN_TIMINGS advisory remains recorded. No
required gate or command is pending. Final proof is
task-5c-final-verification.json, execution-state and full actual logs.

Original author has a report-only follow-up; source/Cargo remain Root-owned.
Independent brief prepared in task-5c-review-brief.md with exact full diff,
53-path tree/hash/gate evidence and verbatim global constraints. Activation
waits only for the final author report update, then one clean-context capable
Spec/Quality reviewer. No independent verdict or task commit yet. Tasks6–10
remain queued.


## Task5C recovered final review dispatch — 2026-11-07T10:36:48.013801+00:00

Root recovered the exact frozen53-path tree76677da1a712b77a10f17fd59b1bfcc2c31553a2
and verified every source/tree/patch/package/input/final-log hash. The updated
final author report matches the activated review-input hash. All prior gate
commands are completed; do not duplicate unchanged-source suites.

Fresh independent gpt-6-astra/high reviewer creation failed before session
initialization: /home/mint/.codex/thread-writer-locks is read-only (errno30).
Existing knowledge_task5c_impl had failed implementation service admission
without any source authorship; Root reassigned it solely to independent
read-only review. Its first review turn also failed at temporary service
admission, and one follow-up retry was sent. No review verdict exists yet.
No host repair or global configuration change was performed.

Fresh staging audit observes54 indexed paths: exact53 task paths plus the
pre-existing protected.gitignore. No unstaged paths. The .gitignore content
SHA256 still matches1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a,
main still has only its original untracked Cargo.lock, and the immutable review
tree excludes.gitignore. Root did not stage, unstage or otherwise change it.

Ruling: Preserve the observed pre-existing.gitignore staging and use an
explicit task-only commit path list after independent approval. — Earlier
handback narratives assumed an unstaged index but the fresh audit is the
authoritative staging evidence; preserving unrelated user state is required.
— Cost if wrong: task commit packaging must be reworked; protected contents
remain intact and outside the reviewed commit.

Task6 current source-plan/owned transport/snapshot/legacy projection draft are
being reconciled read-only while the review is pending. No Task6 source/Cargo
or implementation dispatch before independently reviewed/committed5C.


## User-authorized integration with review deferred — 2026-11-07

User explicitly requested: “先不进行独立审查，先将其提交合并进主分支，
推送到远程仓库，并将未完成的审查记录，因为我的硬盘出问题了”.
This supersedes the skill's pre-commit/pre-merge independent-review gate and
its integration menu. No new review approval is required for this requested
backup. Independent Task5C Spec/Quality review is DEFERRED, not passed.
The ephemeral independent CLI exited1 at service rate-limit/high-demand
admission with no report, and the existing failed reviewer was interrupted.
No reviewer or Cargo command is running. Root will commit exactly53 frozen
source paths, preserve pre-existing.gitignore staging/content, archive the
pending-review requirements/rulings/source/log evidence in tracked docs,
fast-forward main if possible and push origin/main normally.

Task5C source is implemented and locally verified, but its independent
review remains outstanding. Task6–10 and final whole-branch review remain
queued. Do not claim full product or native/paid/package acceptance.
Preserve worktree/scratch and original main untrackedCargo.lock via a verified
local backup if it obstructs merge; no cleanup/host repair/force push.


User-requested source commit d802613670a1e3e6776edbb720b4fdda6d9d66e3
has exactly the verified tree76677da1a712b77a10f17fd59b1bfcc2c31553a2
and53 task paths. Protected.gitignore remains the sole unrelated staged path.
Main's original untrackedCargo.lock was copied byte-for-byte to
.superpowers/backups/2026-11-07-main-Cargo.lock, SHA256
6419265a9215752a40760df51d6f6a0c5efd61dfafc6abee0c9b20cef606d39f,
before merge. Pending-review requirements/evidence and future task drafts are
being archived under tracked docs/superpowers; no independent verdict exists.
