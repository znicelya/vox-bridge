# Current ten-task Workbench and Intelligence preflight

Read-only planning refresh, 2026-11-06. This report is for controller rulings; it does not amend the approved plan, task status or historical ledger. No source changes or tests were performed.

References below use `P:L` for [the current implementation plan](../../../docs/superpowers/plans/2026-11-04-workbench-intelligence.md), `S:L` for [the approved spec](../../../docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md), and `H:L` for [the historical ledger](progress.md). Line references are to the files read for this report.

## Scope and ownership retained

- Current plan has Tasks 1–10 (P:48–59); H:5–24 describes an older eight-task breakdown and must not be used as the current dispatch graph.
- Task 1A shell/tooling is complete at `5bc114f` / `b64af92`, under [the frontend-shell plan](../../../docs/superpowers/plans/2026-11-06-frontend-shell.md):7,28–36 and [its ledger](../2026-11-06-frontend-shell/progress.md):35–36. Reuse its content slots, hash routes, tokens, official primitives, fonts, dependencies and tests. Do not repeat initialization or reopen the approved design. The deferred generated `EmptyDescription` prop-type minor remains its recorded minor, not a new preflight finding.
- Task 1B still owns real bridge/serde fixtures/subscription recovery and waits for reviewed runtime Task 6 (P:29,75–107). The controller supplied current runtime status: Tasks 1–4, 6A configuration and 6C subscriptions reviewed; runtime Task 5 owns ongoing session/orchestrator, storage/app/commands edits; 6B is queued. This report does not treat their temporary source state as missing implementation or authorize overlapping edits.
- Final runtime reconciliation required by P:29,1049 remains pending until 6B is reviewed. The only source checks here were the existing command error envelope, data-directory exposure, and schema-number boundary. No broad source review, native acceptance or SrvDB re-review was performed.

## Actionable gaps for controller disposition

### N1 — Explicit partial text selection conflicts with whole-row derivation

**New concrete contradiction; Tasks 2, 5 and 8.** Task 2 produces exact `SelectionContext.text` with supporting segment IDs (P:175,211,219). Task 8 preserves those IDs until the query is edited (P:922). Task 5 then says the backend derives text from selected IDs whenever present (P:635), but also says only explicit selected text goes to Embedding (P:684; S:18,121). Selecting a few words inside a long row would therefore send/search the complete row instead of the text shown in the query box.

**Minimum resolution:** Make the submitted bounded query the authoritative explicitly selected text and validate the IDs only for session membership/provenance. Alternatively define exact selection spans and derive only those spans. Pin a partial-row case and a multi-row case at the command boundary. Do not silently clear provenance or enlarge selected text. This is narrower than, and not resolved by, historical H:9's general typed selection callback ruling.

### N2 — Crash reconciliation omits failed terminal sessions

**New concrete contradiction within Task 6.** P:790 requires automatic scheduling after both durable `Completed` and `Failed`, including failed sessions retaining final text. The same paragraph limits startup reconciliation to recently `Completed` sessions. A crash after a durable `Failed` commit but before the automatic marker is admitted leaves that eligible session without the promised visible Interrupted/retry record. The permanent-key and no-automatic-paid-retry rules already solve duplicate admission; they do not cover an omitted terminal state.

**Minimum resolution:** Use the same eligible `Completed | Failed` terminal predicate for startup reconciliation, including sessions interrupted during runtime restart recovery with retained final text, preserving the existing automatic-summary policy, absence-of-marker condition and local Interrupted/retry result. Add the failed-with-final-text crash-window case and restart-interrupted case alongside the completed case; do not issue a paid request during recovery. This does not change runtime stop ordering or give Workbench ownership of current runtime Task 5 edits.

### M1 — Document last-update column has no public timestamp

**Concrete producer/consumer omission under an existing ruling; Tasks 4, 5 and 8.** KnowledgePage must show last update (P:920). Migration 4 stores `updated_at_ms` (P:548), but `DocumentSummary` exposes only `created_at_ms` (P:498–503), and `DocumentDetail` adds no update timestamp (P:505–509). Using creation time or the latest visible job would fabricate the column after replacement/reindex or paging.

**Minimum resolution:** Add a durable `updated_at_ms` projection to the document DTO, define which document mutations update it and preserve unknown legacy values honestly where needed. Include it in Task 8's shared wire fixtures. H:30 already authorizes real metadata/interface extensions; this is a concrete application of that ruling, not a request to approve additional product scope.

### M2 — Data settings location lacks an assigned frontend producer

**Concrete producer/consumer omission under an existing ruling; Tasks 3 and 8.** Data settings must display the managed-data location (P:926), but model/settings commands and DTOs have no such field (P:244–303); per-document managed paths (P:505–509) cannot supply it for an empty library. The runtime's planned command list likewise has no data-location command (runtime plan:996–1012). A narrow existing-API check found backend `AppState::data_dir` only (`src-tauri/src/app/mod.rs`:32–34), with no corresponding registered command (`src-tauri/src/lib.rs`:32–41).

**Minimum resolution:** Assign a small read-only safe application-data-location view/command to the settings producer, or explicitly choose an existing Tauri path API through the shared desktop boundary and pin how it matches the backend managed-data directory. It must work for an empty library and expose no secrets or broad filesystem permission. Recheck the final runtime API before adding anything. H:29–30 already permits necessary wiring and real metadata; this is an unassigned producer, not a reason to redo runtime 6A/6C or shell 1A.

## Existing rulings and non-findings

| Boundary | Existing decision to retain |
|---|---|
| Design/tooling | H:28, P:19–20,155–159 and the completed shell govern Taste 2.1.5, official shadcn and framed-sheet. Later tasks add only consumed primitives. No style reapproval or shell replay. |
| File lists and real metadata | H:29–30 permits minimal necessary module/command/test wiring and genuine backend metadata. M1/M2 apply that rule; absence from a file list alone is not a finding. |
| Runtime wire | P:29,33,79,107 and runtime plan:1084 require actual final serde and camelCase command envelopes, including mixed legacy shapes. Temporary template commands are not authoritative. |
| Command errors | P:263–267,366 already requires stable intelligence subcodes. Existing `commands.rs`:11–12 formats `[code] message`; `error.rs`:25–33 supplies the code. Task 3 must carry its granular code through the final command mapping, and the bridge must decode it safely. This can extend the existing boundary; a new universal error DTO is not required by this preflight. |
| Migrations | P:1051 explicitly orders 0003 intelligence → 0004 documents → 0005 summaries → 0006 research. The current narrow source check shows `user_version` capped at 2 (`storage/sqlite.rs`:45–56); no 3–6 collision was observed. Runtime Task 5 is active, so confirm again after runtime completion. Task 6's service dependency on Task 3 does not override the global requirement to integrate migration 4 first. |
| SrvDB | H:39–46 and P:41,658–684 already resolve valid dimensions, new IDs from replace, tombstones, ID 0, serialized ownership, identity isolation and recoverable generation swap. No new SrvDB API finding. |
| Job lifetime and summaries | P:321,372–437,739–790 supplies setup-before-start, atomic shared admission, permanent automatic markers, version counters, raw-response retention and complete snapshot coverage. Do not reimplement admission in consumers or depend on the closed session event bus. N2 is only the inconsistent startup terminal filter. |
| Parser | P:552–583 already distinguishes macOS allocation budget from RSS, Windows stdin gating from suspension, and real packaged execution from injected evidence. No fresh process API assumption was introduced here; P:1052's platform preflight still precedes Task 4. |
| Release acceptance | H:31, P:24–25,1029–1043 keep native OS/driver/package/paid-service gates separate from offline implementation evidence. This report does not clear them. |

## Pairwise shared-boundary table

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

## Dispatch implications

The controller can resolve N1/N2 before the affected feature briefs and apply the already-approved real-metadata rule to M1/M2. No workbench source implementation was started. After runtime 6B is reviewed, reconcile its final launch/serde/error/subscription/config APIs and schema boundary once, then dispatch Task 1B with the completed 1A ownership intact. Continue integrating shared registry/AppState/migration files sequentially, with migration order 3 → 4 → 5 → 6. This preflight does not replace the pending final runtime/API, parser-platform, spec-coverage or release acceptance gates.
