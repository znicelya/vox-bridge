# Workbench and Intelligence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the approved live/call desktop workbench, local history, configurable summaries, SrvDB document search and explicitly confirmed Tavily research.

**Architecture:** A typed frontend bridge owns runtime subscriptions and bounded display state. Rust owns credentials, durable intelligence jobs, document parsing/indexing and network operations. A separate application job channel outlives session channels; completed-session summaries never depend on an open session subscriber. SQLite is authoritative, SrvDB is a recoverable derived index, and every external service has an injected offline boundary.

**Tech Stack:** Existing React 19/TypeScript 6/Vite 8/Tauri 2/Rust 1.99; shadcn/ui, Tailwind v4, Lucide, locally packaged IBM Plex Sans/Mono; Vitest/jsdom/Testing Library/Playwright; reqwest 0.12.28, sha2 0.10, lopdf 0.38, zip 2.4.2, quick-xml 0.38, SrvDB git revision `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`.

**Spec:** [Approved workbench design](../specs/2026-11-04-live-translation-workbench-design.md).

## Global Constraints

- 首发最低版本：Windows 10 22H2、macOS 13 Ventura、Ubuntu 22.04 LTS。Linux 同时覆盖 PipeWire 和 PulseAudio 常见配置。
- Live sends translated speech to M1; calls independently route A→B to M1 and B→A to physical output. Facing another person in the room is outside first-release scope.
- 设备名称固定为逻辑端点 `VoxBridge Input`（M1，虚拟麦克风）和 `VoxBridge Output`（M2，虚拟扬声器）。应用不静默修改系统默认输入/输出设备。
- Preserve source/translation role, direction, ID, sequence, epoch and native/local correlation. Do not fabricate sentence pairs, timestamps, provider latency or playback completion.
- Use the approved Taste 2.1.5 `framed-sheet`: 228px sidebar, 48px top bar, white sheet inset 8px at right/bottom, 1px border, no resting shadow. IBM Plex Sans for interface text; IBM Plex Mono for session/device IDs and latency numbers. Colour share stays below 3%, on translated text, data/status and one primary action.
- shadcn/ui primitives and Lucide icons; accessible names, visible focus, labelled fields, required Sheet/Dialog titles. Tool panels move into a Sheet on narrow windows; subtitles remain primary.
- Final transcript, diagnostics, context and intelligence results are local. Audio and service keys never enter frontend output DTOs, ordinary logs, history exports or fixtures committed with real credentials.
- Key entry is write-only: the form temporarily holds the user's typed secret, submits it once, then clears it; saved keys are never read back into React. No key is put in localStorage, URLs, query keys, analytics or generic request logging.
- LLM/Embedding endpoints and models are configurable; SrvDB is pinned to the approved revision. Tavily is the first research provider; suggesting a candidate never performs a network request.
- Every network call, parser, queue, response body, page, worker count and cancellation wait is bounded. No paid endpoints, native keyring, audio hardware or privileged driver actions in default tests.
- Windows/macOS signing/packages, Linux utility behaviour, minimum-OS audio routes and actual paid-service acceptance remain external gates. Browser fixtures are not evidence of real sound or service quality.

## Prerequisites and verified decisions

Runtime bridge and feature integration execute after all six tasks of [runtime composition](2026-11-05-runtime-composition.md) are reviewed. The dependency-independent Task1 shell/tooling subset (1A) may execute via [frontend shell](2026-11-06-frontend-shell.md) while backend work continues; Task1B retains the complete bridge/wire/recovery requirements and Task1 is incomplete until both subsets pass. Its Task 6 is the API prerequisite, not the repository's older template command signatures. Reconcile the final runtime code once before dispatching this plan and record any differences in its ledger.

Runtime references include transcript commit a6ad766 and accounting-interface correction 2a1390b; read their final reviewed implementations before deriving frontend wire fixtures.

The runtime supplies `SessionLaunch`, `SessionRecord { context, runtime, ... }`, `AudioInventory`, `ProviderConfigView`, `LaunchValidation`, `TranscriptUpdate`, `TranscriptPage`, `DiagnosticPage` and `SubscriptionTicket { id, event_name }`. It exposes create/validate/stop/resume, provider-config CRUD/capabilities, audio inventory, transcript/diagnostic paging, session CRUD/export and owned subscriptions. A subscription's event name is returned by the backend; it is never reconstructed as a fixed URL.

Rulings to retain:

1. UI copy is Chinese by default, with language names in their native form and protocol IDs unchanged. There is no separate localization framework in this release.
2. Display source and translation as individually typed rows. A visible association is permitted only when native/local correlation and direction/epoch match; unassociated rows remain separate. Ordering by a single global sequence is incorrect because sequences are direction scoped.
3. Source files remain on the user's disk. Import copies into managed local storage; deleting a document removes managed copies/derived data only. No OCR in this release: scanned or encrypted PDFs return a specific explanation.
4. Service base URLs support HTTPS and explicit loopback HTTP for local compatible models. Reject userinfo, fragments and credential query parameters. Redirects are disabled; a saved endpoint change creates a new immutable model identity for indexing.
5. SrvDB 0.3 has `add`, `replace`, `delete`, `search`, `persist`, `get_metadata`, `all_metadata` and `live_count`; it has no `upsert`. `replace` returns a NEW u64 ID. `count` includes tombstones. `DatabaseConfig::new` accepts 128–4096 dimensions. One open instance holds an exclusive directory lock. Use `VectorEngine` in scope.
6. Summaries consume every final row in a fixed snapshot, with hierarchy when necessary. They fail visibly on configured/local limits rather than silently summarizing only the beginning/end. A failed model JSON response is retained locally, with hash, and is not a frontend error message.
7. New jobs use a dedicated application event channel and durable polling. Automatic summary starts after durable session termination even if its session event channel has already closed or no UI is present.
8. For parser resource isolation, use a supervised headless worker entry in the packaged executable. A dependency's decompressor cannot be made memory-bounded merely by putting it in `spawn_blocking`; the worker has OS resource limits and bounded IPC.

## File and dependency map

| Task | Dependencies | Owned units | Reviewable result |
|---|---|---|---|
| 1 | Runtime 1–6 | frontend bridge, test harness, shell/tokens | Real API and resilient bounded subscription state |
| 2 | 1 | workbench, audio/provider settings, driver command facade | Usable live/call start/stop/resume and setup |
| 3 | Runtime 1–6 | intelligence DTO/config/store/jobs/http | Durable bounded jobs and secure service configuration |
| 4 | 3 | knowledge parser/worker, managed documents | Bounded PDF/DOCX/text/Markdown parsing and local versions |
| 5 | 4 | embeddings, SrvDB owner, reconciliation/search | Consistent local document search with model isolation |
| 6 | 3 | LLM summary service, lifecycle hook, summary storage/export | Automatic/manual complete-transcript summary versions |
| 7 | 3, 6 LLM port | research extraction/Tavily/source storage | Confirmed research and cited optional overview |
| 8 | 1, 3–7 | intelligence settings, knowledge/research UI | Connected import/search/research/model controls |
| 9 | 1, 6–8 | history/detail/summary UI and history projection | Browse, compare, delete and export local sessions |
| 10 | 1–9 | UI smoke, packaging configuration, docs | Production-build visual and interaction gates |

Task 2 and Task 3 are separate deliverables; implementation/module edits still integrate sequentially. Task 4's parser and Task 5's index have separate tests so parser rejection is reviewable independently of cloud embeddings. Keep each task's production/test files focused; do not create a single large `commands.rs` or all-services file.

## Shared verification environment

Run Cargo with `login:false` and the existing local prefix where needed:

```bash
env PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu" cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Keep checked wire fixtures for every new DTO variant; optional metadata for legacy rows stays absent, not guessed.

Every task follows RED → GREEN, its focused checks, `git diff --check`, and a task-scoped commit/review. Test substantive behaviour rather than `cn()` or generated component internals. The controller records test evidence and any decisions. Stage only task-owned paths; the existing untracked `pnpm-lock.yaml` becomes intentional only in Task 1's dependency installation, after inspecting its diff.

### Task 1: Frontend foundation and recoverable runtime bridge

**Files:** Modify package.json, pnpm-lock.yaml, vite.config.ts, tsconfig.json, src/{main,App}.tsx and src/App.css. Create components.json, src/index.css, src/lib/{utils,desktop,wire}.ts, src/test/{setup,desktop-fixture}.ts, src/features/shell/AppShell.tsx, src/features/session-stream/{controller,reducer,types}.ts and tests. Add src-tauri/src/commands/wire_contract_tests.rs and tests/contracts/runtime-wire.json; modify the relevant command module only to register this test.

**Interfaces produced:** `src/lib/desktop.ts` owns the only imports of Tauri `invoke` and `listen` used by feature code. `wire.ts` mirrors actual Rust serde DTOs verbatim; views normalize enum shapes at this boundary, never rename the Rust protocol speculatively.

```ts
export interface DesktopPort {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
}
export interface SessionStreamState {
  session: SessionRecord | null;
  finals: TranscriptUpdate[];
  interims: TranscriptUpdate[];
  diagnostics: RuntimeDiagnostic[];
  recovering: boolean;
  hasEarlierRows: boolean;
  error: string | null;
}
export interface SessionStreamController {
  start(sessionId: string): Promise<void>;
  stop(): Promise<void>;
  snapshot(): SessionStreamState;
  subscribe(changed: () => void): () => void;
}
export function createSessionStream(port: DesktopPort): SessionStreamController;
export function reduceTranscript(
  state: SessionStreamState, update: TranscriptUpdate,
): SessionStreamState;
```

`SessionRecord` and other names above are wire types imported from `wire.ts`. Include every runtime variant/nullable field, not `any`. Rust contract fixtures serialize real live/call launch, Starting/terminal records, reverse source, translation, legacy row, diagnostic, resync, inventory and ticket values. A Rust test compares `serde_json::to_value` to checked JSON; frontend tests decode those same fixtures. Reject unsafe JS integers or malformed values at the bridge with a local error; do not coerce missing latency to zero. The fixtures contain synthetic IDs/text only. Observed a6ad766 wire shape: SessionEvent is internally tagged by `type` (`transcript` contains `update`, `diagnostic` contains `diagnostic`); DiagnosticPayload is externally tagged snake_case (for example `{"latency":{...}}`), not a `{kind:...}` object. Preserve legacy Playback plus additive CaptureAccounting/PlaybackAccounting from runtime Task 3. A future unknown diagnostic variant retains its safe envelope in an opaque, size-bounded value and renders only `其他诊断`; it must not terminate transcript subscription or dump its payload. Known variants remain strictly decoded. Add a contract test for an unknown diagnostic followed by a valid final event.

- [ ] **1. Set up testing and shadcn without overwriting the confirmed design.**

```bash
pnpm add class-variance-authority clsx tailwind-merge lucide-react @fontsource/ibm-plex-sans @fontsource/ibm-plex-mono
pnpm add -D tailwindcss @tailwindcss/vite vitest jsdom @testing-library/react @testing-library/user-event @testing-library/jest-dom
pnpm dlx shadcn@latest info --json
pnpm dlx shadcn@latest init --template vite --base radix
pnpm dlx shadcn@latest docs button sidebar sheet select field tabs alert empty skeleton tooltip message-scroller message bubble marker
```

Read returned official docs before adding components. Search `@shadcn` for components that are not in the installed registry; inspect the available chat primitives and their actual API, rather than inventing imports. Use the official registry selected by the approved shadcn instruction; no third-party block/preset switch. Add only needed components through the CLI, then inspect generated files. Configure `@/*` → `src/*`, `@tailwindcss/vite`, Vitest `jsdom`, and setup importing `@testing-library/jest-dom/vitest`; add `test` and `test:run` scripts. Do not test generated primitives' own internals.

- [ ] **2. Write RED tests for identity and recovery.** The fixture desktop is a real protocol simulator implementing DesktopPort: bounded named listeners, programmable command replies, deferred command completion and recorded calls. It does not implement the reducer being tested.

```ts
it("keeps reverse source and translation as separate rows during final replacement", () => {
  const interim = wire.reverseSourceInterim;
  let state = reduceTranscript(emptyStreamState(), interim);
  state = reduceTranscript(state, wire.reverseTranslationFinal);
  state = reduceTranscript(state, { ...interim, is_final: true, text: "pump" });
  expect(state.interims).toHaveLength(0);
  expect(state.finals.map(row => row.id)).toEqual([
    wire.reverseTranslationFinal.id, interim.id,
  ]);
  expect(state.finals[1].details?.kind).toBe("source");
});
it("recovers a final emitted before the listener attaches", async () => {
  const port = runtimeDesktopFixture(wire);
  port.beforeSubscribeReply(() => port.persistFinal(wire.reverseSourceFinal));
  const stream = createSessionStream(port);
  await stream.start(wire.callRecord.id);
  expect(stream.snapshot().finals).toContainEqual(wire.reverseSourceFinal);
  await stream.stop();
  expect(port.liveListenerCount()).toBe(0);
  expect(port.activeTicketCount()).toBe(0);
});
```

Define `emptyStreamState()` as null session, empty arrays, false flags and null error. Define `runtimeDesktopFixture(wire)` in the test fixture module with the methods used above plus `emit`, `persistFinal`, `rejectNext`, `calls`, `activeTicketCount`, `liveListenerCount`; command fixtures must implement backend paging and ticket lifecycle, not return a hardcoded UI state. Test StrictMode mount/unmount, stale async completion after session switch, subscription rejection, ResyncNeeded while pages are in flight, terminal status with no last event, and source/translation arriving in either order.

- [ ] **3. Implement bounded subscription/snapshot reconciliation.** Start invokes `subscribe_session({sessionId, subscriberId})`, listens to the returned `event_name`, then loads `get_session`, transcript pages and diagnostic pages. This order deliberately recovers finals/status lost between subscribe and listen. Interims may start with the next live update; never make one durable. Buffer at most 512 incoming events during recovery; overflow restarts the snapshot from the last confirmed cursor and clears interims, not finals from storage. One recovery task runs at once.

Pages use `after`/`limit:200`. Keep the last cursor that had a non-null `next_cursor`; the final page can be reread on recovery because it lacks a terminal high-water cursor. Deduplicate by row UUID while merging; never compare A and B sequence numbers as a shared ordering key. Preserve final arrival/paged insertion order. Retain at most 500 final rows, 128 interims and 200 diagnostics in live display memory; older finals remain available in history. Track `hasEarlierRows` rather than claiming this is the entire record. Interim with an already-final ID cannot regress the row. New epoch replaces old interims only for that direction.

On resync, clear interims, recover SQLite pages and current SessionRecord, then apply buffered events. A single two-second active-session status refresh catches per-direction changes and terminal races; skip overlapping requests. Cleanup increments a generation token, unlistens and invokes `unsubscribe_session` even if a late subscribe call just returned. Errors are local user messages, never dumped objects. No frontend task owns audio stop.

- [ ] **4. Build the empty shell and tokens.** Keep the existing Vite stack. Use semantic theme tokens: background `#f5f7f8`, surface `#ffffff`, border `#e7eaed`, foreground `#16181b`, muted foreground `#64686d`, translation `#1b6c80`, near-black primary `#1d1f20`; positive text `#247b50`, warning text `#92400e`, destructive `#c4242a`. Controls radius 8px, sheet radius 16px. Package font weights 400/500/600 locally; no remote font fetch at runtime. CJK falls back to the platform sans face.

The shared test setup constructs userEvent per test and resets fixture listeners/jobs after cleanup. Keep test harness conveniences under src/test or cfg(test), not in production controller methods.

AppShell has named routes `workbench`, `knowledge`, `history`, `settings`; use hash navigation to avoid adding a router. Product name `Vox Bridge`, scope `本机会话`, no made-up account or KPI. At 1440px, arithmetic is `228 sidebar + 8 sheet gap + remaining sheet`; 24px sheet padding; workbench may use `minmax(0,1fr) 280px` with 24px gap. Below 1100px fold sidebar; below 900px move the right tool rail into a titled Sheet; below 640px use 16px content padding and 44px targets. Every page has one h1. Put each route's honest empty state in place; no fake session is loaded by default.

- [ ] **5. Verify and commit.**

```bash
pnpm test:run src/features/session-stream src/lib
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib wire_contract
```

Commit `feat: establish framed workbench and recoverable runtime bridge` after formatting/diff checks.

### Task 2: Live/call workbench and translation/audio settings

**Files:** Create src/features/workbench/{LiveWorkbench,ConversationStream,AudioRoutePanel,LaunchControls,ToolRail}.tsx, hooks/tests; src/features/settings/{SettingsPage,AudioSettings,ProviderSettings}.tsx; src-tauri/src/commands/driver.rs and driver tests. Modify AppShell/App, AppState and command registrations narrowly. Extend runtime audio/platform owner only for the explicit driver facade.

**Consumes:** Task 1 bridge and runtime Task 6 commands. **Produces:** real start/stop/resume/setup interaction plus a `SelectionContext { sessionId: string; segmentIds: string[]; text: string }` passed to the later knowledge/research tools. Selection is a local explicit action, not a network trigger.

Driver facade signatures:

```rust
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverOperation { Install, Repair, Uninstall }
#[async_trait]
pub trait DriverOperations: Send + Sync {
    async fn perform(&self, operation: DriverOperation) -> AppResult<DriverStatus>;
}
// Thin AppState/Tauri command, rejects while any session owns audio:
pub async fn manage_audio_driver(operation: DriverOperation) -> AppResult<DriverStatus>;
```

Use one owner worker for the existing non-Send platform managers. No unsafe Send/Sync or new signed-driver implementation here. Serialize actions with queue capacity one, retain an uncertain worker/lease on timeout, and do not spawn replacement workers. Return package/privilege/approval gates honestly. User clicks an action in the product before the platform helper may request OS authorization; app startup never installs. Raw subprocess output is not a UI error. Default tests inject this port.

- [ ] **1. Write RED interaction tests.**

```tsx
it("launches the selected call routes and resumes only the failed direction", async () => {
  const port = runtimeDesktopFixture(wire);
  render(<App desktop={port} />);
  await user.click(screen.getByRole("tab", { name: "通话" }));
  await chooseCallRoutes(user, wire.callLaunch.devices);
  await user.click(screen.getByRole("button", { name: "开始同传" }));
  expect(port.lastArgs("create_session")).toEqual({ launch: wire.callLaunch });
  port.setSession(wire.oneDirectionPaused);
  await user.click(await screen.findByRole("button", { name: "继续对方到我" }));
  expect(port.lastArgs("resume_session_direction")).toEqual({
    sessionId: wire.callRecord.id, direction: "target_to_source",
  });
});
```

`chooseCallRoutes` selects the visible labelled controls by backend IDs through userEvent; it does not mutate app state directly. Add delayed start/double click, stale endpoint rejection, provider disabled/unknown, wrong language pair, stop during Starting, changing mode during an active call disabled, failed direction while peer remains Running, selection preserving exact text, and no network calls on selecting text.

- [ ] **2. Implement launch and route state.** Tabs `直播`/`通话`; explicit source/target language selectors; provider selector from saved profiles/capabilities. Live selects input and M1 render. Call selects A input, M2 capture, M1 render, physical output. Options come from current inventory roles; show logical M1/M2 and actual backend display names without asking users to choose monitor/render internals by guesswork. Exclude invalid roles, but backend revalidates every launch. Never auto-select a fallback device after loss.

`validate_session_launch` runs on explicit start before create; create does its own fresh validation. Disable duplicate start/stop and preserve user selection when validation fails. Config/device editing is unavailable while a session is active; stop is always reachable. Route rows expose independent status and explicit resume controls, with safe provider code/logid in an expandable detail. CaptureAccounting distinguishes dropped_native_blocks from dropped_normalized_frames; neither is multiplied by 1280 to invent lost samples. PlaybackAccounting labels input samples by input_sample_rate_hz (24k) and native mono counts by their own sample_rate_hz. accepted_for_processing_samples is backend dequeue accounting, not an audible-played count. Keep finalized/tail_disposition_known and nullable boundary/native explicit; unknown tail is a small status message, not a derived duration. Do not subtract live counters to assert conservation or convert a native-rate drop to an exact 24k drop. Detailed counters stay collapsed unless useful; normal UI prioritizes direction status and unknown-tail indication. Latency labels the legacy capture_to_send_ms field as `采集到服务商队列` (provider admission, not socket transmission/network delay), renders measured stages and `—` for unknown, with callback/server-drain labels from diagnostics; never show a invented overall 1-second SLA.

- [ ] **3. Compose conversation and tools.** Use the installed shadcn conversation primitives for scroll-follow/anchor/jump-to-latest, with rectangular neutral rows rather than chat balloon decoration. Each row labels `我 → 对方`/`对方 → 我`, `原文`/`译文`, language and timestamp. Translation uses the single accent, source stays ink, interim is marked `识别中`. Native/local association can group a pair only under the same direction and epoch. Unknown legacy role says `历史文本`.

Do not autofocus every arriving row or announce every interim through aria-live. Announce status/final additions politely in a bounded region. Maintain user scroll position when reading older text; the scroller's jump control restores follow. Keyboard selection uses a row action `用此段检索`; text selection also exposes a `检索所选内容` action. ToolRail shows real empty prompts for local knowledge and customer research, with disabled actions only until the corresponding service is configured, not fake results.

- [ ] **4. Implement audio/provider settings.** Five settings destinations are visible from the start; model/Tavily/data views connect in later tasks. Audio shows logical routing, installed driver state, explicit recheck/install/repair/uninstall and platform guidance. Uninstall uses an AlertDialog naming the devices; reject during active audio ownership.

Provider form uses FieldGroup/Field, saved display name, endpoint, write-only key and capability status. Aliyun/Palabra display `未联调` and their reason; they cannot be chosen as runnable providers. Keep/replace/clear credential semantics match runtime Task 6 exactly. Backend public views never return keys; clear typed key on successful save and when the form is destroyed. A masked configured indicator is boolean state, not a fake stored password. Profile deletion is confirmed and rejects an active profile. Connection tests for actual providers are added through Task 3's common bounded probe service; until then show `检查配置` for local preflight and label it truthfully.

- [ ] **5. Verify and commit.**

```bash
pnpm test:run src/features/workbench src/features/settings
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib commands::driver
```

Commit `feat: connect live and call workbench controls`.

### Task 3: Durable intelligence configuration, jobs and bounded HTTP

**Files:** Create src-tauri/src/intelligence/{mod,types,config,http,jobs,store,test_support}.rs with tests, migrations/0003_intelligence.sql, commands/intelligence.rs. Modify config module, error.rs, storage migration dispatcher, AppState, lib/commands registration and Cargo manifests. All paths hereafter are under src-tauri unless prefixed src/.

**Produced DTOs (the same wire conventions apply to new DTOs in Tasks 4–9):** enums serialize snake_case; tagged enums explicitly use `#[serde(tag="kind",rename_all="snake_case")]`. All timestamps are i64 epoch milliseconds. Safe result DTOs derive Clone/Debug/Serialize/Deserialize/PartialEq; credential requests do not derive Debug/Serialize.

```rust
pub enum ServiceKind { Llm, Embedding, Tavily }
pub enum RecordOrigin { Native, Legacy }
pub struct ModelProfile {
    pub id: Uuid, pub kind: ServiceKind, pub name: String,
    pub base_url: String, pub model: Option<String>,
    pub timeout_ms: u32, pub max_context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>, pub dimensions: Option<u32>,
}
pub struct ModelProfileView {
    pub profile: ModelProfile, pub credential_configured: bool,
    pub last_probe: Option<ConnectionProbe>,
}
pub struct SaveModelProfile {
    pub profile: ModelProfile, pub credential: CredentialChange,
}
pub struct IntelligenceSettings {
    pub summary_profile_id: Option<Uuid>, pub embedding_profile_id: Option<Uuid>,
    pub tavily_profile_id: Option<Uuid>, pub automatic_summary: bool,
}
pub enum JobKind { Import, Reindex, DeleteDocument, Summary, Research, ResearchOverview }
pub enum JobStatus { Queued, Running, Succeeded, Failed, Cancelled, Interrupted }
pub enum IntelligenceErrorCode {
    Configuration, Busy, Cancelled, Interrupted, Timeout, Network, Authentication,
    RateLimited, ResponseTooLarge, InvalidResponse, ContextTooLarge,
    UnsupportedDocument, ParseFailed, ResourceLimit, IndexRepairRequired, NoFinalText, Storage,
}
pub struct JobView {
    pub id: Uuid, pub kind: JobKind, pub status: JobStatus,
    pub session_id: Option<Uuid>, pub object_id: Option<Uuid>,
    pub completed_units: u32, pub total_units: Option<u32>,
    pub error: Option<IntelligenceErrorCode>,
    pub created_at_ms: i64, pub updated_at_ms: i64,
}
pub struct JobPage { pub rows: Vec<JobView>, pub next_cursor: Option<i64> }
pub struct ConnectionProbe {
    pub checked_at_ms: i64, pub ok: bool, pub latency_ms: Option<u64>,
    pub error: Option<IntelligenceErrorCode>,
}
pub enum IntelligenceEvent {
    JobChanged { job: JobView },
    ResyncNeeded { missed_events: u64 },
}
```

`CredentialChange` reuses the runtime request enum. It must remain input-only. Config profiles are independent from translation provider profiles; do not abuse the current optional `ProviderConfig.llm/embedding/tavily` fields as a shared selection bucket. Preserve those legacy fields on load/save; new service settings are separately versioned.

AppState/command methods:

| Command | Arguments | Result |
|---|---|---|
| list_model_profiles | none | Vec<ModelProfileView> |
| save_model_profile | request:SaveModelProfile | ModelProfileView |
| delete_model_profile | profile_id:Uuid | () |
| get_intelligence_settings | none | IntelligenceSettings |
| save_intelligence_settings | settings:IntelligenceSettings | IntelligenceSettings |
| test_model_profile | profile_id:Uuid | ConnectionProbe |
| test_translation_provider | profile_id:Uuid | ConnectionProbe |
| get_job | job_id:Uuid | Option<JobView> |
| list_jobs | after:Option<i64>,limit:Option<u32> | JobPage |
| cancel_job | job_id:Uuid | JobView |
| subscribe_intelligence | app handle,subscriber_id:String | SubscriptionTicket |
| unsubscribe_intelligence | subscription_id:Uuid | () |

**Configuration owner:**

```rust
impl ServiceConfigStore {
    pub(crate) fn new(io: ConfigIo, store: Arc<LocalStore>) -> Self;
    pub async fn save(&self, request: SaveModelProfile) -> AppResult<ModelProfileView>;
    pub async fn load(&self, id: Uuid) -> AppResult<ModelProfileView>;
    pub(crate) async fn resolve_credentials(&self, profile_id: Uuid) -> AppResult<SecretString>;
    pub async fn list(&self) -> AppResult<Vec<ModelProfileView>>;
    pub async fn delete(&self, id: Uuid) -> AppResult<()>;
    pub async fn settings(&self) -> AppResult<IntelligenceSettings>;
    pub async fn save_settings(&self, value: IntelligenceSettings) -> AppResult<IntelligenceSettings>;
    pub fn path(&self) -> &Path;
}
```

AppState holds Arc<ServiceConfigStore>, Arc<JobScheduler> and Arc<dyn JsonHttp>. Later service constructors consume these collaborators and Arc<LocalStore>; build/register all handlers before starting the dispatcher. `register` uses an interior mutex only during setup and rejects duplicate kinds and changes after start. Registered job handlers hold storage/config/ports, never an Arc<JobScheduler>; command services hold the scheduler separately, avoiding an Arc ownership cycle. Add `JobScheduler::start(&self) -> AppResult<()>`; `new` creates no background tasks or network calls. Tests explicitly start workers after registering fixture handlers.

**Injected HTTP boundary:**

```rust
pub struct JsonRequest {
    pub endpoint: url::Url, pub bearer: SecretString,
    pub body: serde_json::Value, pub timeout: Duration,
    pub max_response_bytes: usize,
}
pub struct JsonResponse { pub status: u16, pub body: Vec<u8> }
#[async_trait]
pub trait JsonHttp: Send + Sync {
    async fn post(&self, request: JsonRequest,
        cancel: watch::Receiver<bool>) -> Result<JsonResponse, IntelligenceErrorCode>;
}
// Defined in intelligence/http.rs; the inner value and header access stay private there.
pub struct SecretString(String); // no Clone/Debug/Serialize/Deserialize/Display/AsRef/Deref
impl SecretString {
    pub(crate) fn from_store_bytes(value: Vec<u8>) -> AppResult<Self>;
    fn authorization_header(&self)
        -> Result<reqwest::header::HeaderValue, IntelligenceErrorCode>;
}
pub enum JobAdmission {
    Manual { exclusion_key: String },
    AutomaticSummary { session_id: Uuid },
}
pub struct JobContext { pub id: Uuid, pub cancel: watch::Receiver<bool> }
#[async_trait]
pub trait JobHandler: Send + Sync {
    async fn run(&self, context: JobContext, payload: serde_json::Value)
        -> Result<(), IntelligenceErrorCode>;
}
impl JobScheduler {
    pub fn new(store: Arc<LocalStore>) -> Self;
    pub fn register(&self, kind: JobKind, handler: Arc<dyn JobHandler>) -> AppResult<()>;
    pub async fn enqueue(&self, kind: JobKind, session_id: Option<Uuid>,
        object_id: Option<Uuid>, admission: JobAdmission, payload: serde_json::Value)
        -> AppResult<JobView>;
    pub async fn cancel(&self, id: Uuid) -> AppResult<JobView>;
    pub async fn shutdown(&self) -> AppResult<()>;
    pub(crate) fn wake(&self); // bounded dispatcher notification after an external store transaction
}
```

Add `AppError::Intelligence(IntelligenceErrorCode)` whose Display is a local message chosen from IntelligenceErrorCode, never a body/URL. Map service port failures through this variant so the UI can choose a retry/setup action from a stable code.

Job payloads are validated versioned internal inputs (IDs/config snapshots), never secrets. `resolve_credentials` is crate-private and is not a Tauri command: through the shared runtime ConfigIo::run/ConfigTransaction, verify the profile exists, derive service `com.voxbridge.intelligence` and key `{profile UUID}/api-key` internally, call SecretStore::get, map missing/decrypt/UTF-8 errors to a local Configuration error, and return `SecretString::from_store_bytes`. Never accept a supplied secret reference. Release file/keyring locks before any HTTP await. This is the only credential path later LLM/Embedding/Tavily adapters need from ServiceConfigStore; they move the opaque value into JsonRequest, and never receive a plaintext getter.

`from_store_bytes` requires 1–4096 bytes of visible ASCII with no whitespace/control bytes (the same validation runs on Replace). It creates no display/logging trait. `authorization_header` is private to http.rs, creates a `Bearer ` HeaderValue, calls `set_sensitive(true)` and returns only that header to the reqwest adapter. Do not add `expose`, `AsRef<str>`, `Deref` or serialization to make service code convenient. JsonRequest also has no Debug/Serialize implementation. Invalid-byte errors do not format the input. Default tests resolve only synthetic memory-backend keys; a private http.rs unit test can inspect the resulting synthetic header, not a production plaintext-access method.

Scheduler admission is distinct from transport credentials. Reuse this store helper for enqueue and Task 6's atomic summary-version creation:

```rust
pub(crate) struct JobDraft {
    pub id: Uuid, pub kind: JobKind, pub session_id: Option<Uuid>,
    pub object_id: Option<Uuid>, pub admission: JobAdmission,
    pub payload: serde_json::Value,
    pub initial_failure: Option<IntelligenceErrorCode>,
}
pub(crate) enum JobAdmissionResult { Created(JobView), Existing(JobView) }
pub(crate) fn admit_job(tx: &rusqlite::Transaction<'_>, draft: JobDraft)
    -> AppResult<JobAdmissionResult>;
```

Normal enqueue supplies initial_failure=None. Only an automatic-summary draft may create an initially Failed terminal marker (for Configuration/Busy/NoFinalText/Interrupted), which does not consume unfinished capacity. `admit_job` never commits or wakes workers; its caller commits the job and any result-version row together, then calls wake. Existing means no new job or result-version row is inserted. The helper implements all permanent-key lookup, in-flight exclusion and capacity checks below once; Task 6 must not reimplement those checks outside the transaction.

- [ ] **1. Write RED tests for key isolation, migrations and actual HTTP limits.** `IntelligenceHarness::new()` supplies a temporary LocalStore, memory KeyBackend, real encrypted SecretStore, configurable RecordingHttp and JobScheduler; fields are store, secrets, config, http, jobs, temp_dir. RecordingHttp records only safe request shape/count for assertions; synthetic bearer inspection is confined to tests.

```rust
#[tokio::test]
async fn profile_round_trip_never_returns_the_saved_key() {
    let h = IntelligenceHarness::new().unwrap();
    let p = test_llm_profile();
    let view = h.config.save(SaveModelProfile {
        profile: p.clone(),
        credential: CredentialChange::Replace { api_key: "synthetic-model-key".into() },
    }).await.unwrap();
    assert!(view.credential_configured);
    assert!(!serde_json::to_string(&view).unwrap().contains("synthetic-model-key"));
    assert!(!std::fs::read_to_string(h.config.path()).unwrap().contains("synthetic-model-key"));
    assert_eq!(h.config.load(p.id).await.unwrap().profile, p);
}
```

Define test_llm_profile with loopback endpoint, model `fixture-model`, timeout 1000, context 4096 and output 512; other fields valid/None. Credential tests must additionally resolve two profiles with different synthetic keys, prove each request gets only its own sensitive Authorization header, and reject missing/foreign-profile/invalid-byte credentials before RecordingHttp sees a request. Test profile public JSON and command registration contain no credential resolver. Scheduler tests cover each terminal state (Succeeded/Failed/Cancelled/Interrupted) releasing a manual exclusion key, concurrent manual admission yielding one Busy, permanent automatic duplicates returning the same UUID after terminal transition and reopen, and a full queue returning an existing automatic job before capacity rejection. Test that a duplicate automatic request and an exclusion failure insert no extra job row; Task 6 covers atomic summary-version admission. Add migration from version 2 twice, rollback on injected migration failure, rejection of newer schema versions, a blocked HTTP body cancelled without a retained task, streaming body limit independent of Content-Length, malformed JSON/non-JSON safe errors, TLS verification and loopback-HTTP policy.

- [ ] **2. Add additive migration and config transactions.** Leave 0001/0002 intact. Version 3 transaction creates:

```sql
CREATE TABLE intelligence_jobs (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL,
 session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
 object_id TEXT, exclusion_key TEXT NOT NULL, permanent_key TEXT UNIQUE, payload_json TEXT NOT NULL,
 completed_units INTEGER NOT NULL DEFAULT 0, total_units INTEGER,
 error_code TEXT, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
);
CREATE INDEX intelligence_jobs_status ON intelligence_jobs(status, created_at_ms);
CREATE UNIQUE INDEX intelligence_jobs_in_flight
 ON intelligence_jobs(exclusion_key) WHERE status IN ('queued', 'running');
CREATE TABLE service_probes (
 profile_id TEXT PRIMARY KEY, checked_at_ms INTEGER NOT NULL, result_json TEXT NOT NULL
);
CREATE TABLE intelligence_meta (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
```

`permanent_key` is NULL for every manual job. Only JobAdmission::AutomaticSummary generates `auto-summary:<session UUID>` and the matching `summary:<session UUID>` exclusion key; its supplied session_id must match the job's session_id and kind Summary. It retains the permanent key after Succeeded/Failed/Cancelled/Interrupted. Terminal job history is retained for the session lifetime; deleting a summary version must not delete this job marker. Session deletion may cascade both. An automatic callback first looks up its permanent key and returns that existing job regardless of terminal status or queue fullness, without creating another version.

Manual callers choose a bounded internal exclusion key (1–160 ASCII characters), never a string from frontend input: Summary uses `summary:<session UUID>` across ALL versions; Import/Reindex/DeleteDocument share `document:<document UUID>` across those mutation kinds; ResearchOverview uses `research-overview:<run UUID>` across versions; Research uses `research:<run UUID>`. A partial unique index reserves the key only while Queued/Running; any terminal transition automatically frees it. Manual reruns/retries always create a fresh UUID with NULL permanent_key once the old job is terminal. Two simultaneous manual requests for the same scope yield one new job and one Busy result. A permanent duplicate is idempotent Existing, while an unrelated in-flight exclusion collision is Busy; neither path inserts an orphan result row. Capacity and admission use one SQLite write transaction before any worker wake.

ServiceConfigStore stores the shared ConfigIo clone and derives its data directory from io.data_dir(), so its service-configs.json cannot use a mismatched lock directory. Its constructor is backend-only and performs no I/O. Model configuration uses ConfigIo::run and the runtime whole-transaction config lock, unique temp/fsync/atomic replace and ConfigTransaction secret methods. Apply runtime Keep/Replace/Clear ordering and opaque encrypted before-image compensation to model profiles too; unreferenced orphan entries do not count as configured credentials. Service `com.voxbridge.intelligence`; deterministic key `{profile UUID}/api-key`; never accept another profile's reference. Validate names 1–80 chars, model 1–200, URL <=2048 bytes, timeout 1000–120000ms, context 2048–131072, output 128–16384 and <=context/2. Embedding dimension is None until probe/index or 128–4096. Tavily profile has no model/context/dimension; endpoint defaults to `https://api.tavily.com/search`. Loopback HTTP is only localhost/127.0.0.1/[::1]; no redirect-following or URL credential text in errors. Preserve a base path such as `/v1`; append `/chat/completions` or `/embeddings` once.

`automatic_summary` defaults true. Missing selected profile gives a visible Configuration result when automatic work is scheduled. Save settings validates selected profile kinds. Deleting a selected/in-use profile rejects with a safe explanation; edits apply to future jobs. Capture immutable public config/model identity at enqueue; keys are resolved when the job starts and never stored in payload. Config/secret two-file atomicity remains limited as documented by runtime Task 6.

- [ ] **3. Implement bounded jobs and application event forwarding.** At most 32 unfinished jobs, two active job handlers, one Queued/Running job per exclusion_key and one active parser. Admit capacity/exclusion/permanent identity and persist Queued in one write transaction before notifying workers; persist terminal status before event publication. Queue full returns Busy without spawning a task; automatic-summary code records a failed version instead of silently discarding it. Use one dispatcher plus two workers and bounded wake/control channels; no task per queued item. Cancellation is idempotent, selects alongside network/IPC waits, and terminal states cannot return to Running.

At startup under the app process lock mark former Running/Queued jobs Interrupted; retain files/text/results and expose retry. Do not automatically re-call a paid endpoint from a previous interrupted job. Reconciliation may repair local SrvDB metadata without network; embeddings need explicit reindex retry. Shutdown requests cancel then bounds join to five seconds and leaves Interrupted state if cleanup is uncertain. Job rows page 1–200 by rowid. Intelligence subscriptions reuse the runtime registry's capacity/cancel/lag pattern in a separate namespace; events never include raw model bodies or keys. Frontend can always recover by `get_job/list_jobs` after lag/closed session.

- [ ] **4. Implement HTTP and explicit connection probes.** Add the direct dependency exactly as verified against reqwest 0.12.28; retain Tauri's independent reqwest 0.13 dependency if present instead of upgrading it:

```toml
reqwest = { version = "=0.12.28", default-features = false, features = ["json", "stream", "rustls-tls-manual-roots-no-provider"] }
# These already exist for Volc; retain compatible versions/features rather than duplicate them.
rustls = { version = "0.23", default-features = false, features = ["std", "ring", "tls12"] }
webpki-roots = "1"
```

Define ReqwestJsonHttp::new() -> AppResult<Self> with this per-client connector (Arc and Duration use std imports):

```rust
let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
    rustls::crypto::ring::default_provider(),
))
.with_safe_default_protocol_versions()
.map_err(|_| AppError::Configuration("HTTP TLS configuration failed".into()))?
.with_root_certificates(roots)
.with_no_client_auth();
let client = reqwest::Client::builder()
    .use_preconfigured_tls(tls)
    .redirect(reqwest::redirect::Policy::none())
    .connect_timeout(Duration::from_secs(5))
    .timeout(Duration::from_secs(120))
    .build()
    .map_err(|_| AppError::Configuration("HTTP client configuration failed".into()))?;
```

Pass the concrete rustls::ClientConfig, NOT Arc<ClientConfig>: reqwest 0.12.28 downcasts this exact type in use_preconfigured_tls. Its dependency is rustls 0.23.4-compatible; keep the direct and reqwest Rustls versions unified in Cargo.lock. The no-provider feature exposes the Rustls connector; the direct rustls ring feature supplies crypto, selected explicitly by builder_with_provider. Do not call install_default, use invalid-cert/hostname flags, substitute use_rustls_tls after the preconfigured call, or silently fall back to another backend. new builds a local client without requests. In post, use `.header(reqwest::header::AUTHORIZATION, request.bearer.authorization_header()?)` and `.timeout(request.timeout)`; selected total deadlines still cover streamed body/cancellation. Add a constructor regression with no global provider installed and local test-only trust-root fixtures proving matching trusted certificates work while untrusted roots/wrong hostnames fail. These are offline local TLS tests, never service endpoints.

reqwest has redirects disabled, connect timeout five seconds, selected total timeout and streamed body caps. General JSON limit 2MiB; embeddings 8MiB; Tavily 1MiB. Error bodies are not surfaced/logged; safe category and optional HTTP status suffice. No automatic retries for model chat; one retry for 429/503 only when a validated Retry-After <=5 seconds fits the original total deadline for embeddings/Tavily. Cancellation cancels both sleep and body reading. No arbitrary server error string goes into a command result.

Probes are explicit UI actions, globally one at a time: LLM sends `Reply with OK.` with max 8 output tokens; embedding embeds `connection test` and checks dimensions; Tavily sends an explicit tiny search query with max_results 1 and include_answer/raw_content false; translation provider uses prepare/connect/handshake then finish/cancel without sending audio, bounded to 15 seconds. Label tests that invoke metered services in the settings copy. Save last probe status/time locally. Local validation alone says `配置检查通过`, never `连接成功`.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
cargo test --manifest-path src-tauri/Cargo.toml --lib config
cargo test --manifest-path src-tauri/Cargo.toml --lib storage
```

Commit `feat: add durable intelligence jobs and secure model profiles`.

### Task 4: Bounded document parsing and managed document versions

**Files:** Create knowledge/{mod,types,parser,parser_worker,documents,store,parser_tests,document_tests}.rs, allocation_budget.rs, migrations/0004_knowledge.sql, commands/knowledge.rs, test document fixtures under tests/knowledge. Modify main.rs/lib.rs for the headless worker entry, AppState/Cargo manifests, storage migrations and the runtime process boundary only for reusable bounded worker limits. Add native file-dialog dependencies/capabilities when the UI uses them in Task 8.

**DTOs and interfaces:** Import shared RecordOrigin from Task 3 intelligence::types; do not define it in the later summary module.

```rust
pub enum DocumentKind { Pdf, Docx, Text, Markdown, Unknown }
pub enum DocumentStatus { Parsing, Parsed, Indexing, Ready, RepairRequired, Failed, Deleting }
pub struct TextLocation {
    pub page: Option<u32>, pub paragraph: u32, pub heading: Option<String>,
    pub start_char: u32, pub end_char: u32,
}
pub struct TextBlock { pub text: String, pub location: TextLocation }
pub struct DocumentSummary {
    pub id: Uuid, pub title: Option<String>, pub kind: DocumentKind, pub version: u32,
    pub origin: RecordOrigin,
    pub sha256: String, pub status: DocumentStatus, pub chunk_count: u32,
    pub created_at_ms: i64, pub error: Option<IntelligenceErrorCode>,
}
pub struct DocumentPage { pub rows: Vec<DocumentSummary>, pub next_cursor: Option<i64> }
pub struct DocumentDetail {
    pub summary: DocumentSummary, pub original_path: Option<PathBuf>,
    pub managed_path: Option<PathBuf>, pub legacy_uri: Option<String>,
    pub index_identity: Option<String>,
}
#[async_trait]
pub trait DocumentParser: Send + Sync {
    async fn parse(&self, managed_path: PathBuf, kind: DocumentKind,
        cancel: watch::Receiver<bool>) -> Result<Vec<TextBlock>, IntelligenceErrorCode>;
}
impl DocumentService {
    pub async fn import(&self, path: PathBuf) -> AppResult<JobView>;
    pub async fn replace(&self, document_id: Uuid, path: PathBuf) -> AppResult<JobView>;
    pub fn page(&self, after: Option<i64>, limit: u32) -> AppResult<DocumentPage>;
    pub fn get(&self, id: Uuid) -> AppResult<Option<DocumentDetail>>;
}
pub fn chunk_blocks(blocks: &[TextBlock]) -> AppResult<Vec<TextBlock>>;
```

Commands `import_document(path)`, `replace_document(document_id,path)`, `list_documents(after,limit)`, `get_document(document_id)` use these results. Document UUID is stable across replacements; versions advance transactionally. Task 5 owns index/reindex/delete after parse. DocumentService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, parser:Arc<dyn DocumentParser>, data_dir:PathBuf) -> Self stores collaborators without I/O. A private job handler performs each admitted import/replacement.

- [ ] **1. Write RED parser and file-ownership tests.** Checked fixtures include a two-page born-digital PDF with known text/page markers, a DOCX with headings/table cells/tabs, UTF-8 Chinese/English text and Markdown. Generate fixtures reproducibly in a test script or commit their small binary inputs plus source text; no customer data.

```rust
#[test]
fn chunks_preserve_cjk_text_and_never_cross_a_page_silently() {
    let text = "泵的额定电压为24V。".repeat(180);
    let end_char = u32::try_from(text.chars().count()).unwrap();
    let blocks = vec![TextBlock {
        text,
        location: TextLocation { page: Some(2), paragraph: 1,
            heading: Some("电气参数".into()), start_char: 0, end_char },
    }];
    let chunks = chunk_blocks(&blocks).unwrap();
    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|c| c.location.page == Some(2)));
    assert!(chunks.iter().all(|c| c.text.chars().count() <= 2400));
    assert!(chunks.iter().all(|c| c.text.is_char_boundary(c.text.len())));
}
```

Also assert reconstruction using each chunk's offsets covers the complete original text with the specified overlap, which catches dropped middle content. Test encrypted/scanned PDF, corrupt DOCX, zip inflation limit, malformed XML/DOCTYPE, empty text, non-UTF8 text, cancellation/timeout, output cap, symlink/path race during copying, duplicate import and managed-copy cleanup after failure. Use injected worker/process boundaries for default timeout tests.

- [ ] **2. Migrate document metadata additively.** Version 4 adds document columns kind (legacy `unknown`), origin (legacy `legacy`), version (default 1), nullable original_path, nullable managed_path, status (legacy `repair_required`), error_code, updated_at_ms and index_identity. Add chunk columns location_json, content_hash, document_version, embedding_identity, vector_id (TEXT decimal u64), index_state. Keep existing id/text/chunk_index columns and legacy rows. Add `knowledge_index_generations(identity TEXT PRIMARY KEY, profile_id TEXT, dimension INTEGER, active_generation TEXT, state TEXT NOT NULL)` and `knowledge_repairs(document_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE, reason TEXT NOT NULL)`. Legacy rows need reindex, never fabricated vector IDs. Every touched INSERT/SELECT lists columns. Foundation documents contain only uri and nullable title, so preserve them verbatim: DocumentDetail exposes the old uri as legacy_uri=Some(value), original_path=None and managed_path=None until the user explicitly supplies a source file. Never reinterpret uri as a verified native path or create/copy a managed file during migration. Existing NULL titles stay None, kind stays Unknown and origin stays Legacy; no extension-derived guesses. New imports write origin Native, a validated known kind and Some paths/title. The parser accepts only known kinds and a verified managed path; legacy rows without those show an explicit Replace/reimport action rather than attempting a network/file fetch. Task 5 index/search excludes these RepairRequired legacy documents.

Add a version-2 fixture database with uri="legacy-reference", title=NULL and one chunk. After migration/reopen twice, assert uri/text unchanged, title=None, kind=Unknown, origin=Legacy, both paths None, no managed file created and no parser/embedding call. UI fallback labels are presentation only and must not be written into SQLite or exports.

- [ ] **3. Implement managed copy and parsing isolation.** Only regular selected files, extensions/pdf/docx/txt/md matched against content signature. Input maximum 32MiB, copied by streaming into a UUID temp under app data, SHA-256 while copying, checked file metadata before/after, fsync/atomic replace. Never follow a changed symlink into a different source during import; open once and copy that handle. Store canonical original path for attribution, but parse only the managed copy. Duplicate SHA-256 with unchanged parser version returns the existing document/job; replacement creates a new version transactionally and marks prior vectors unsearchable before mutation.

Headless command `--voxbridge-parse-worker` is recognized in main.rs before Tauri initialization. Production launches `std::env::current_exe()`; the offline integration target launches Cargo's `CARGO_BIN_EXE_vox-bridge`, never the test harness's current_exe. Worker mode initializes no secrets, storage, window, audio or networking, creates no descendants, handles exactly one request and exits. Keep actual installed deb/AppImage, signed macOS bundle and installed Windows executable invocation as native acceptance gates.

Supervisor bounds the entire worker operation to30 seconds, the selected input file to32MiB, JSON request to64KiB and JSON result to16MiB. The request/result protocol is a little-endian u32 length followed by exactly that many JSON bytes. Check the length before allocating, reject truncation/trailing frames, and close stdin after the single request. Before the request, require the fixed eight-byte readiness preface `b"VBPW\x01\0\0\0"`. Worker emits it only after its in-process startup checks succeed; parent also requires successful OS-limit installation before sending any request bytes. Use null stderr, matching the existing process owner, rather than claiming captured diagnostics. Timeout/cancel/oversize/protocol/read/write failure terminates and observes shared reap within2 seconds; a kill request alone is not acknowledged cleanup. Keep max2000 pages/50000 paragraphs/2 million extracted Unicode scalars and10000 chunks independently of memory limits.

Extend SystemProcessSpawner with `spawn_limited(&self, spec:ProcessSpec, limits:ProcessResourceLimits) -> AppResult<ProcessPipes>`, where ProcessResourceLimits contains memory_bytes:u64 and cpu_seconds:u32. Use1GiB and30 CPU seconds in production. Retain job/process handles inside the existing private child owner and reuse its replayable wait/terminate/reap protocol. The parser path uses safe intelligence/configuration failures, not audio-specific missing-tool categories. Install the actual per-platform limits as follows:

- Linux: set both soft/hard RLIMIT_AS=1GiB and RLIMIT_CPU=30s in an allocation-free Unix pre_exec callback. Failed limit installation fails spawn before parsing; successful spawn acknowledges the OS limits. Verify loader/startup headroom with the actual packaged executable.
- macOS: set RLIMIT_CPU=30s in pre_exec. Do not use RLIMIT_AS/RLIMIT_RSS as a hard memory guarantee: Apple's RSS limit is advisory. Apply the hard Rust allocation budget below before readiness; keep this distinct from a whole-process RSS cap.
- Windows: configure a Job Object with PROCESS_MEMORY=1GiB, PROCESS_TIME=30*10000000 user-time ticks and KILL_ON_JOB_CLOSE. Spawn with piped stdin, assign the child before sending bytes, retain the job handle through wait/reap and terminate the job on abort. The child may emit readiness then waits for its request; this is stdin gating, not OS suspension. Assignment failure writes zero request bytes and kills/reaps. Do not pass CREATE_SUSPENDED to Tokio without a custom primary-thread handle/resume implementation. The small loader/pre-assignment window is outside the job limit; document that native acceptance must exercise assignment under an existing parent job. Add only needed windows-sys Foundation/JobObjects/Threading features.

**Hard allocation budget for Unix parser workers.** Add allocation_budget.rs to the binary root, with a System GlobalAlloc wrapper counting live requested Layout bytes from process start. Ordinary desktop mode never enables enforcement. Linux workers also use this budget so its real allocator/exit path is exercised by offline worker tests, in addition to Linux's OS AS cap. In exact worker mode, parse the parent's trusted decimal allocation ceiling, enable it once before creating any worker threads/runtime, then emit readiness and dispatch the library's headless parser entry. Production ceiling is1GiB including existing live Rust allocations. Export only this binary-private API:

```rust
const PARSER_ALLOC_LIMIT_EXIT: i32 = 86;
const PARSER_ALLOC_INVARIANT_EXIT: i32 = 87;
fn enable_parser_allocation_limit(bytes: usize) -> Result<AllocationBudgetSnapshot, EnableBudgetError>;
struct AllocationBudgetSnapshot { limit_bytes: usize, live_bytes_at_enable: usize }
```

Use fixed atomics, checked arithmetic and no allocator-internal allocation, formatting, logging, locks or unwinding. Always track from startup so freeing a pre-enable allocation cannot underflow. Reserve layout.size before System allocation; undo on null. Deallocate then subtract exactly the original Layout size. While enforced, realloc conservatively reserves the full new size, System-allocates with the old alignment, copies min(old,new), frees old, then subtracts old size; allocation failure releases the new reservation and leaves old accounting intact. This bounds old-plus-new peak rather than only net growth. The disabled ordinary-app path may delegate to System.realloc and account its successful size difference; enablement happens once before worker-created threads, so no concurrent mode transition is supported. Reject zero/invalid/repeated enable and baseline above limit. Budget/checked-add failure exits with allocation-free libc::_exit(86); accounting invariant failure exits87. Under-budget System failure remains generic OOM/crash, never a falsely precise configured-limit result. Keep unsafe System/pointer operations isolated in this module with explicit safety invariants; no unsafe Send/Sync markers.

Parent maps exit86 (even before readiness) to safe ResourceLimit (`文档解析超过本地内存限制`), exit87 to ParseFailed, and exposes no raw worker body as an error. This guarantees a ceiling on requested Rust heap bytes and conservative realloc peaks in the pure-Rust parser paths. It does not cap allocator metadata/rounding, dyld/framework mappings, stacks or native malloc/mmap; do not label it a1GiB macOS RSS ceiling. Readiness for Windows is combined with the parent's successful job assignment; the Unix-only allocator is not claimed there.

Select `lopdf = { version = "0.38", default-features = false }` and `zip = { version = "2.4.2", default-features = false, features = ["deflate"] }`; retain pure-Rust miniz_oxide/weezl/quick-xml and disable unneeded Rayon. Inspect `cargo tree --locked -e features -i flate2` during verification: feature unification must not introduce zlib/zlib-ng/cloudflare-zlib or another native decompressor outside the allocation budget. No new allocator/decompressor crate or lopdf fork is needed.

Add pure accounting tests for exact boundary, checked overflow, deallocation/reuse, invariant underflow, failed allocation rollback and conservative realloc peaks. Subprocess tests use a small trusted worker allocation ceiling while retaining sufficient OS startup allowance: verify budget failure before/after readiness maps86/ResourceLimit, malformed readiness/exit87 is safe, parent sends zero bytes before readiness/limit acknowledgement, no success JSON on failure, and one terminate/reap path. Exercise real high-inflation PDF/DOCX plus a below-budget successful file; supervisor must survive. Keep injected tests for OS forwarding/assignment failures and native acceptance for actual AS/CPU/job enforcement, packaged startup and macOS13 budget behavior. Never infer native guarantees from injection or cross-compilation.

Dependency API verified during planning: lopdf 0.38 provides `Document::load`, `is_encrypted`, `get_pages` and `extract_text(&[page_no])`; extract one page at a time. Its decompression has no size-limit parameter, which is why the process and hard Rust-allocation limits are required. Reject encrypted/no-text pages with explicit UnsupportedDocument and `需要可提取文字的 PDF；此版本不执行 OCR` guidance. DOCX uses zip 2.4.2 with only deflate support and quick-xml 0.38: read `word/document.xml`, optional styles for heading attribution; never extract paths onto disk or fetch external relationships. Check entry size and a streaming inflated-byte cap (16MiB), XML depth <=64, reject DTD/entity declarations, decode predefined entities and preserve paragraphs, table-cell boundaries, line breaks and tabs. A ZIP entry named `../../...` is never opened by name except the two allowed internal names.

- [ ] **4. Normalize and chunk deterministically.** Normalize CRLF and repeated horizontal whitespace, retain paragraph/page structure and meaningful numeric punctuation. Use Unicode-scalar windows of 1600–2400 characters (roughly 400–800 token equivalents), overlap 320 characters, prefer paragraph/sentence boundaries within the range; never split a UTF-8 scalar or merge different PDF pages. Small paragraphs combine only under the same page/heading. Exact location offsets refer to normalized source text. Chunk hash is SHA-256 of text plus versioned normalization/parser ID. Do not pretend characters are exact model tokens. Maximum 10000 chunks per document; reject exceeding limits visibly.

After parse, persist text/location/hash in a SQLite transaction and mark Parsed. Job progress reports page/chunk units actually completed. Task 5 registers an import continuation that indexes when an embedding profile exists; otherwise Parsed shows `选择 Embedding 模型后建立索引`, retaining local text with no cloud call.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge::parser
cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge::documents
cargo test --manifest-path src-tauri/Cargo.toml --test document_worker
```

Register the offline worker test target explicitly in Cargo.toml. Commit `feat: parse managed documents with bounded worker isolation`.

### Task 5: Embeddings, SrvDB consistency and document search

**Files:** Create knowledge/{embedding,index,reconcile,search,index_tests,reconcile_tests}.rs. Extend knowledge store/documents/commands, Cargo manifests, AppState and job registration. Add small SrvDB integration fixtures/tests; use the actual crate in default tests with 128-dimensional synthetic vectors.

**Interfaces:**

```rust
pub struct EmbeddingIdentity {
    pub profile_id: Uuid, pub base_url: String, pub model: String,
    pub dimensions: u32, pub normalization_version: u32,
}
pub struct KnowledgeHit {
    pub document_id: Uuid, pub document_version: u32, pub chunk_id: Uuid,
    pub title: String, pub location: TextLocation, pub text: String, pub score: f32,
}
pub struct KnowledgeSearch {
    pub query: String, pub selected_segment_ids: Vec<Uuid>,
    pub session_id: Option<Uuid>, pub limit: u32,
}
pub struct KnowledgeResults {
    pub hits: Vec<KnowledgeHit>, pub identity: String,
    pub unavailable_documents: u32,
}
#[async_trait]
pub trait Embeddings: Send + Sync {
    async fn embed(&self, profile: &ModelProfile, inputs: &[String],
        cancel: watch::Receiver<bool>) -> Result<Vec<Vec<f32>>, IntelligenceErrorCode>;
}
impl KnowledgeService {
    pub async fn search(&self, request: KnowledgeSearch) -> AppResult<KnowledgeResults>;
    pub async fn reindex(&self, document_id: Uuid) -> AppResult<JobView>;
    pub async fn delete(&self, document_id: Uuid) -> AppResult<JobView>;
    pub async fn reconcile_local(&self) -> AppResult<()>;
}
```

KnowledgeService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, embeddings:Arc<dyn Embeddings>, index:Arc<IndexOwner>) -> Self; IndexOwner::new(data_dir:PathBuf) -> AppResult<Self> starts one bounded worker and owns shutdown.

Commands `search_knowledge(request)`, `reindex_document(document_id)`, `delete_document(document_id)` match these methods. Explicit selected transcript IDs must belong to the supplied session; backend derives their text when present, with selected text/query length 1–4096 chars. No automatic query on every subtitle or keystroke. Search max limit 20, default 5; at most two outstanding search requests, cancellable on replacement.

- [ ] **1. Write RED real-index persistence/recovery tests.** `KnowledgeHarness` extends IntelligenceHarness with a real temp SrvDB owner, DocumentService and deterministic Embeddings returning normalized 128-dimensional axes. Its methods import_text(name,text), index(id), search(query), crash_at(CommitPoint), reopen(), delete(id) use real SQLite/index operations; embedding_calls() returns the deterministic embedding port's observed request count. `CommitPoint` is a test-only injected callback at SQLite intent, vector mutation, vector persist and SQLite ready update; it must not alter production transaction logic.

```rust
#[tokio::test]
async fn crash_after_vector_persist_never_exposes_an_uncommitted_document() {
    let mut h = KnowledgeHarness::new().unwrap();
    let id = h.import_text("pump.md", "额定电压 24 V").await.unwrap();
    h.crash_at(CommitPoint::AfterVectorPersist);
    assert!(h.index(id).await.is_err());
    let h = h.reopen().await.unwrap();
    let before = h.embedding_calls();
    assert!(matches!(h.search("额定电压").await,
        Err(AppError::Intelligence(IntelligenceErrorCode::IndexRepairRequired))));
    assert_eq!(h.embedding_calls(), before);
    h.index(id).await.unwrap();
    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
}
```

Also test replacement returns/persists a new vector ID; deletion across reopen; stale hash, missing metadata, missing vector, wrong dimension, same model name at different endpoint/profile, NaN/infinity/zero vectors, out-of-order embedding indexes, cancellation during import, duplicate retry, orphan vector and persisted generation swap. Check independent saved profile edit does not reinterpret an old vector space.

- [ ] **2. Implement embedding transport and identity.** OpenAiCompatibleEmbeddings::new(config:Arc<ServiceConfigStore>, http:Arc<dyn JsonHttp>) -> Self stores the collaborators; embed resolves config.resolve_credentials(profile.id), moves its SecretString into JsonRequest, and maps resolver failure to Configuration before invoking HTTP. POST normalized `/embeddings` with model/input, encoding_format float and optional dimensions. Batch <=32 texts and <=128KiB JSON, concurrent requests <=4 globally; response <=8MiB. Match `data[i].index` to input, reject duplicate/missing/out-of-range indexes, non-finite/empty/zero vectors and inconsistent dimensions. First valid response fixes actual dimension; it must be 128–4096 and match configured dimension if provided. Hash canonical JSON of EmbeddingIdentity to a lowercase hex directory key; keys are excluded. L2-normalize with checked finite norm. Avoid holding SQLite/index locks during network calls.

- [ ] **3. Use actual SrvDB APIs through one owner.**

```rust
use srvdb::{DatabaseConfig, SrvDB, Vector, VectorEngine};
let config = DatabaseConfig::new(identity.dimensions as usize)
    .map_err(|_| AppError::Configuration("unsupported embedding dimension".into()))?;
let mut db = SrvDB::new_with_config(index_dir.to_str().ok_or_else(
    || AppError::Configuration("index directory is not valid UTF-8".into())
)?, config).map_err(index_error)?;
let metadata_json = serde_json::to_string(&metadata)
    .map_err(|_| AppError::Internal("index metadata could not be encoded".into()))?;
let id = db.add(&Vector::new(vector), &metadata_json)
    .map_err(index_error)?;
db.persist().map_err(index_error)?;
```

`index_error` maps local categories without logging content. Define metadata `{ schema_version:1, document_id, document_version, chunk_id, content_hash, location }`. Start flat full-precision with quantization disabled; use the pinned crate's documented automatic index behaviour without claiming a fixed HNSW mode. One dedicated blocking index worker owns one open database per active identity, max two open identities with idle LRU close; command queue 16, no unbounded channel. All `add/replace/delete/persist/search` happen on that owner; no duplicate directory opens.

- [ ] **4. Make SQLite-first operations recoverable.** In SQLite mark affected document/index rows Indexing/RepairRequired and record desired version/hash before SrvDB mutation. Batch embeddings then add/replace vectors, retain returned IDs, call `persist()`, and only then transactionally store IDs and mark Ready. Until this final transaction, search excludes the document. On failure keep desired text and repair marker; do not delete original or managed source. Replacement never returns old-version hits while new rows are partial.

Deletion first marks Deleting plus a durable cleanup intent containing prior vector IDs/identity, excludes it from search, deletes/tombstones vectors, persists, then removes SQLite rows and managed copies. If a crash leaves an orphan vector, later hydration excludes it and reconciliation deletes it. Cleanup intent must survive removal of document rows, so store it in intelligence job payload or a non-cascading cleanup table until persisted completion.

On startup under the app lock, check all Ready chunk IDs/hashes/model identities in bounded batches of 200. Avoid unbounded `all_metadata()` in normal operation: scan vector IDs over the half-open range `0..count()` in batches, use `get_metadata` and skip tombstones, with checked u64 iteration and idle yields. This matches the pinned RecordFile::append returning the prior count, flat append_batch returning `first..first+n`, and delete rejecting `id >= count()`; ID 0 is valid and ID count is excluded. Snapshot count while holding the serialized index owner so mutations do not move the scan boundary. Cap each open database to 200000 stored records INCLUDING tombstones; a bounded generation rebuild recovers capacity after deletion, and new imports fail visibly until space is available. SrvDB's inherent open-memory/index cost remains bounded by this supported limit. Unmatched/malformed metadata marks the document RepairRequired; do not return that document's results. Local orphan cleanup needs no embedding call. Reembedding is an explicit reindex job. A whole-index rebuild writes a new generation directory, persists and validates it, then atomically switches the SQLite generation pointer; only afterwards delete the old managed generation. Never overwrite the only good index during rebuild. Do not call bare SrvDB::compact on an active generation: it renumbers live IDs densely and returns only a removed count. The new generation must publish every new chunk/vector mapping in SQLite with its pointer swap. Add a first-ID-zero/last-ID-count-minus-one/tombstone scan regression and a generation-rebuild mapping regression.

- [ ] **5. Search and hydrate authoritatively.** Before any embedding request, inspect local document/index readiness for the selected embedding identity. If the library has documents but none is healthy/searchable because its index is missing, partial or RepairRequired, return AppError::Intelligence(IndexRepairRequired) with zero embedding calls. A truly empty library returns an empty result locally. With at least one healthy searchable document, embed on explicit search, request up to `min(limit*5,100)` candidates, then validate metadata ID/version/hash against SQLite Ready rows and active identity before returning. Scores are finite cosine similarities, not percentages/probabilities. Return <=20 hits with source title/page/heading and full bounded chunk text. If readiness becomes invalid during the request and no healthy searchable documents remain, return IndexRepairRequired rather than empty success; the preflight test above covers the already-known-invalid case before any metered call. Mixed healthy/broken collections may return healthy hits plus `unavailable_documents` and a local repair action. Selection search sends only the explicit selected text to the configured Embedding service; no translation provider receives knowledge queries.

- [ ] **6. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib knowledge
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
```

Commit `feat: index and search local documents with recoverable SrvDB state`.

### Task 6: Automatic/manual summaries with complete input coverage

**Files:** Create llm/{mod,openai_compatible,tests}.rs, summary/{mod,types,store,planner,tests}.rs, migrations/0005_summaries.sql and commands/summary.rs. Modify session/orchestrator final-commit notification, AppState/jobs registration and export mapping. Do not change audio stop ordering.

**Interfaces and strict schema:**

```rust
pub struct LlmRequest {
    pub system: String, pub user: String, pub max_output_tokens: u32,
}
pub struct LlmResponse { pub content: String, pub response_hash: String }
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, profile: &ModelProfile, request: LlmRequest,
        cancel: watch::Receiver<bool>) -> Result<LlmResponse, IntelligenceErrorCode>;
}
#[serde(deny_unknown_fields)]
pub struct SummaryContent {
    pub summary: String, pub participants: Vec<String>, pub companies: Vec<String>,
    pub requirements: Vec<String>, pub action_items: Vec<ActionItem>,
    pub risks: Vec<String>, pub terms: Vec<Term>,
}
#[serde(deny_unknown_fields)]
pub struct ActionItem { pub text: String, pub owner: Option<String>, pub due: Option<String> }
#[serde(deny_unknown_fields)]
pub struct Term { pub term: String, pub meaning: String }
pub struct SummaryView {
    pub id: Uuid, pub session_id: Uuid, pub version: u32, pub status: JobStatus,
    pub profile_id: Option<Uuid>, pub model: Option<String>, pub created_at_ms: i64,
    pub content: Option<SummaryContent>, pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>, pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>, pub origin: RecordOrigin,
}
impl SummaryService {
    pub async fn generate(&self, session_id: Uuid, profile_id: Option<Uuid>) -> AppResult<JobView>;
    pub async fn after_terminal(&self, session_id: Uuid) -> AppResult<()>;
    pub fn list(&self, session_id: Uuid, after_version: Option<u32>, limit: u32)
        -> AppResult<Vec<SummaryView>>;
    pub fn delete(&self, summary_id: Uuid) -> AppResult<()>;
}
```

SummaryService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, llm:Arc<dyn LlmProvider>) -> Self; its handler is registered under JobKind::Summary before scheduler start.

Add this crate-private Task 6 store boundary so version allocation cannot race permanent duplicate detection:

```rust
pub(crate) struct SummaryJobRequest {
    pub session_id: Uuid, pub profile: Option<ModelProfile>,
    pub automatic: bool, pub initial_failure: Option<IntelligenceErrorCode>,
}
impl LocalStore {
    pub(crate) fn create_summary_job(&self, request: SummaryJobRequest) -> AppResult<JobView>;
}
```

create_summary_job opens one write transaction, chooses a new summary UUID for the prospective object_id/payload, and calls Task 3's admit_job with the appropriate admission. Existing returns the prior job with no version allocation. Created advances the session's durable summary-version counter and inserts its corresponding Queued/Failed version inside that SAME transaction; any insert failure rolls back job, permanent marker and counter/version together. Deleting a summary cannot rewind that counter or reuse a version number. Only after commit does SummaryService call jobs.wake. Manual requests use Manual { exclusion_key: format!("summary:{}", session_id) } and no initial_failure. Automatic requests use AutomaticSummary; failed automatic admission/preflight may be represented by the terminal failed marker, but a permanent existing job always wins. This shares the tested admission helper rather than duplicating scheduler checks.

Commands generate_summary(session_id,profile_id), list_summaries(session_id,after_version,limit), delete_summary(summary_id). SummaryContent derives Deserialize/Serialize with strict types, max 20000 chars summary, <=100 entries per array, <=2000 chars per string; reject unknown fields/type mismatches, not silently dropping them. Empty arrays are valid. Model returns JSON object directly; optional fenced JSON is not silently repaired—invalid output is a failed version with retry.

- [ ] **1. Write RED tests for full coverage, stop independence and invalid JSON durability.** `SummaryHarness` owns real LocalStore/JobScheduler, RecordingLlm and a fixture runtime/orchestrator; methods completed_session(finals), run_jobs(), responses(), summary_versions(id) are implemented against those collaborators. RecordingLlm queues exact controlled response strings and records request text only in tests.

```rust
#[tokio::test]
async fn closed_session_channel_does_not_prevent_one_automatic_summary() {
    let h = SummaryHarness::new().unwrap();
    let id = h.completed_session(vec!["customer asks for 24 V pump"]).await;
    h.bus.close(id);
    h.service.after_terminal(id).await.unwrap();
    h.service.after_terminal(id).await.unwrap();
    h.run_jobs().await;
    assert_eq!(h.llm.request_count(), 1);
    assert_eq!(h.summary_versions(id).unwrap().len(), 1);
    assert_eq!(h.summary_versions(id).unwrap()[0].status, JobStatus::Succeeded);
}
```

Add tests for successful manual rerun after a Succeeded version and manual retry after each Failed/Cancelled/Interrupted version, with strictly increasing versions and distinct job UUIDs; two concurrent manual summary requests must create only one new version. Replay after_terminal after an automatic job finishes and after reopening storage; it must keep the original job/version. A failed automatic job followed by manual retry must retain one automatic permanent marker and add exactly one manual version. Deleting the successful automatic summary version must not allow after_terminal to launch another automatic request. Inject a summary-version insert failure and assert neither a job nor a permanent marker remains after rollback.

Delete the highest version, reopen storage, then manually generate again: the new version must exceed the deleted version. Also delete every version and verify the next version still advances; a rolled-back admission consumes no version. Cover checked u32 exhaustion with a safe local failure before any job/version insertion.

Add delayed LLM proving stop already returned, failed JSON raw/hash persisted, selected-model switch creates a new version, cancellation, deletion during an active summary, storage failure, no configured LLM, empty transcript, and a transcript larger than one request whose middle sentinel appears in the recorded map requests. Assert coverage set equals the complete final snapshot IDs; never test only the first/last lines.

- [ ] **2. Extend version storage safely.** Version 5 adds profile_id, model, input_hash, input_final_count, snapshot_last_rowid, error_code and raw_response_hash to existing session_summaries. Add `summary_responses(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, ordinal INTEGER, phase TEXT, raw_text TEXT NOT NULL, response_hash TEXT NOT NULL, PRIMARY KEY(summary_id,ordinal))` and `summary_coverage(summary_id TEXT REFERENCES session_summaries(id) ON DELETE CASCADE, segment_id TEXT NOT NULL, PRIMARY KEY(summary_id,segment_id))`. Add `summary_version_counters(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, last_version INTEGER NOT NULL CHECK(last_version >= 0))`, seeded from each existing session's MAX(version); new sessions allocate from zero on first admission. Counter increment uses checked u32 range inside the admission transaction. Summary deletion leaves the counter; session deletion cascades it. Keep old structured_json/status rows readable as legacy versions; missing metadata remains None.

Use create_summary_job to allocate the next durable counter value together with job admission, create a Queued version before cloud work, and update its status/JSON on success/failure. Raw response is written before parsing. Store each map/reduce response <=2MiB and <=32MiB total per summary; exceeding the cap fails visibly while preserving prior responses. Return hashes/status/parsed data to UI; do not put raw model content into generic diagnostics/logs. Summary deletion cascades responses/coverage and rejects or cancels an in-flight version with bounded completion before removal. Export includes structured summaries and safe model/version metadata; raw responses remain local detail data, outside default export.

- [ ] **3. Implement complete final snapshot and bounded hierarchy.** Capture a SQLite read snapshot of final rows with session-filtered rowid upper bound, each row retaining ID, direction, role/language and text. Max 16MiB normalized input and 100000 rows; exceeding limit creates ContextTooLarge, never truncates. No interim or audio. Order by storage insertion order; use direction-specific labels, not inferred speaker names.

Budget each request with a conservative UTF-8-byte estimate plus 1024 prompt/schema overhead against `max_context_tokens - max_output_tokens`; document this as an estimate for arbitrary compatible tokenizers, not exact token accounting. Provider context-limit errors fail safely. Split oversized individual rows at Unicode boundaries with `(segment_id,part_index,part_count)`; every part appears in coverage. Map chunks use the same strict schema. Hierarchically reduce bounded intermediate summaries until one fits, max 64 requests and 8 levels; if no reduction progress/context limit, fail ContextTooLarge. Never omit a chunk to meet a limit. Preserve request coverage independently of what the model chooses to mention; do not promise semantic completeness beyond the supplied input.

OpenAiCompatibleLlm::new(config:Arc<ServiceConfigStore>, http:Arc<dyn JsonHttp>) -> Self uses the same crate-private resolver and opaque JsonRequest bearer; its complete method cannot read a plaintext key. ResearchService and explicit probes use that resolver for their own saved profile ID as well.

Chat POST `/chat/completions`: model, system/user messages, temperature 0, max_tokens, non-streaming, no proprietary tools/schema capability requirement; the system instruction requests the strict JSON and the local parser verifies it. System instruction treats transcript text as quoted data and requests no invented participants/actions/dates. LLM output never triggers tools or network. Parse `choices[0].message.content` as a bounded string; missing choices/refusal/wrong types fail InvalidResponse. Secrets resolve backend-only at execution.

- [ ] **4. Wire exactly-once automatic scheduling after durable termination.** Add an injected terminal observer or callback on the lifecycle owner's successful Completed/Failed commit; it receives only session ID and cannot delay audio cleanup/network stop. Schedule with JobAdmission::AutomaticSummary and its permanent key `auto-summary:<session UUID>` using create_summary_job. Repeated observer calls return the same job/version even after that job is terminal; a duplicate does not reserve another version. Existing session bus closes normally; intelligence events/polling carry progress. At startup, local reconciliation finds recently Completed sessions with automatic-summary enabled and no automatic job/version, then creates a visible Interrupted/retry record rather than silently reissuing a paid call after uncertain crash history. New normal terminal sessions start automatically when final text exists, including a failed session with retained text; an empty terminal session records a local NoFinalText result without calling a model. Missing model/queue-full records a failed summary version with safe code and retry control. Manual regeneration or retry uses the same in-flight exclusion key `summary:<session UUID>` with permanent_key=NULL, creates a NEW job UUID/version after the prior job is terminal, and never overwrites prior output. A failed/interrupted automatic job retains its permanent marker; clicking Retry creates a manual job, not a second automatic job. Removing a summary version does not remove the automatic marker. Missing model, queue-full or NoFinalText creates the failed automatic marker and failed version atomically so repeated terminal callbacks cannot keep creating failures. Stop result is independent of LLM success.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib summary
cargo test --manifest-path src-tauri/Cargo.toml --lib llm
cargo test --manifest-path src-tauri/Cargo.toml --lib session
cargo test --manifest-path src-tauri/Cargo.toml --lib export
```

Commit `feat: summarize complete session transcripts with durable versions`.

### Task 7: Explicitly confirmed Tavily research and cited overview

**Files:** Create research/{mod,types,entities,tavily,store,tests}.rs, migrations/0006_research.sql and commands/research.rs. Extend AppState/jobs registration and LLM integration; preserve stored sources independently of overview success.

**Interfaces:**

```rust
pub struct EntityCandidate {
    pub id: Uuid, pub text: String, pub session_id: Uuid,
    pub supporting_segment_ids: Vec<Uuid>, pub kind: String,
}
pub struct ResearchRequest {
    pub query: String, pub session_id: Option<Uuid>,
    pub supporting_segment_ids: Vec<Uuid>, pub confirmed: bool,
}
pub struct ResearchSource {
    pub id: Uuid, pub run_id: Uuid, pub title: Option<String>, pub url: String,
    pub excerpt: Option<String>, pub published_at: Option<String>,
    pub fetched_at_ms: Option<i64>, pub origin: RecordOrigin,
}
pub struct ResearchClaim { pub text: String, pub source_ids: Vec<Uuid> }
pub struct ResearchOverview { pub facts: Vec<ResearchClaim>, pub unknowns: Vec<String> }
pub struct ResearchListItem {
    pub id: Uuid, pub session_id: Option<Uuid>, pub query: String,
    pub status: JobStatus, pub created_at_ms: i64, pub source_count: u32,
}
pub struct ResearchPage { pub rows: Vec<ResearchListItem>, pub next_cursor: Option<i64> }
pub struct ResearchOverviewVersion {
    pub id: Uuid, pub run_id: Uuid, pub version: u32, pub status: JobStatus,
    pub profile_id: Option<Uuid>, pub model: Option<String>, pub created_at_ms: i64,
    pub content: Option<ResearchOverview>, pub error: Option<IntelligenceErrorCode>,
}
pub struct ResearchRunView {
    pub id: Uuid, pub session_id: Option<Uuid>, pub query: String,
    pub status: JobStatus, pub created_at_ms: i64,
    pub sources: Vec<ResearchSource>, pub overview: Option<ResearchOverview>,
    pub error: Option<IntelligenceErrorCode>,
}
impl ResearchService {
    pub fn suggest(&self, session_id: Uuid) -> AppResult<Vec<EntityCandidate>>;
    pub async fn search(&self, request: ResearchRequest) -> AppResult<JobView>;
    pub fn get(&self, run_id: Uuid) -> AppResult<Option<ResearchRunView>>;
    pub async fn overview(&self, run_id: Uuid, profile_id: Option<Uuid>) -> AppResult<JobView>;
}
```

ResearchService::new(store:Arc<LocalStore>, config:Arc<ServiceConfigStore>, jobs:Arc<JobScheduler>, http:Arc<dyn JsonHttp>, llm:Arc<dyn LlmProvider>) -> Self; register Research and ResearchOverview handlers before scheduler start.

Commands suggest_research_entities(session_id), start_research(request), get_research(run_id), generate_research_overview(run_id,profile_id), list_research(session_id,after,limit) and delete_research(run_id). list_research returns ResearchPage with 1–100 rows. list_research_overviews(run_id,after_version,limit) returns Vec<ResearchOverviewVersion> with 1–50 rows; delete_research_overview(overview_id) returns (). List returns rows without nested sources plus rowid cursor; get returns at most eight sources and the current overview. Overview versions use the same durable version pattern as summaries, with separate research overview table and list/delete commands; do not overwrite a successful version with a failed retry.

- [ ] **1. Write RED tests proving confirmation and attribution.**

```rust
#[tokio::test]
async fn unconfirmed_candidate_never_queues_a_tavily_request() {
    let h = ResearchHarness::new().unwrap();
    let result = h.service.search(ResearchRequest {
        query: "Northstar Fluid Systems".into(), session_id: None,
        supporting_segment_ids: vec![], confirmed: false,
    }).await;
    assert!(result.is_err());
    assert_eq!(h.http.request_count(), 0);
    assert_eq!(h.store.unfinished_job_count().unwrap(), 0);
}
```

ResearchHarness uses IntelligenceHarness/RecordingHttp and real source storage. Add suggestions only local, unsupported URL schemes, foreign-session segment IDs, empty/no-result response, 429/timeout, oversized/malformed body, duplicate source URLs, invalid citation ID, overview failure leaving sources intact, prompt-injection text rendered as text and no tool calls, cancel/retry and source deletion cascades.

- [ ] **2. Add source/provenance migration.** Version 6 adds query, profile_id, error_code, response_hash and confirmed_at_ms to research_runs; nullable excerpt, nullable published_at, nullable fetched_at_ms, origin (legacy `legacy`) and ordinal to research_sources. Retain the foundation source title column as nullable. Add `research_support(run_id,segment_id)` and versioned `research_overviews` with parsed_json, raw_text/hash, profile/model/status/error, created time and unique(run_id,version). Preserve legacy sources with title=None/excerpt=None/fetched_at_ms=None and origin=Legacy when those fields were absent; never invent an empty excerpt, page title or fetch time. Native Tavily writes Some validated title/excerpt/fetched time and origin=Native; optional published_at remains None if not supplied. Source metadata is saved before optional overview begins. Add a foundation fixture with title=NULL and no provenance columns, migrate/reopen twice, and assert nullable values and original URL survive unchanged. Wire fixture/UI tests must cover this legacy source beside a complete native source.

- [ ] **3. Implement local candidates and confirmation.** Read only final subtitles, cap the most recent 200 rows/128KiB. Extract HTTPS/HTTP domain references and bounded company-like spans, including Chinese suffixes 公司/集团/科技/有限公司 and English company suffixes. Return <=12 deduplicated candidates with supporting IDs. Candidate generation uses deterministic rules, not an automatic LLM/network call; confidence is not a fabricated percentage. Empty result keeps manual company/URL input available.

`start_research` requires confirmed=true, query trimmed 2–256 characters, <=8 supporting IDs belonging to the supplied session. Manual query with no session is supported. Confirmation is the user's explicit button action; selecting a subtitle or typing cannot queue the job. Normalize URL queries for display and allow http/https only; discard URL credentials/fragments and reject embedded credentials rather than leaking them. Never crawl the entered website directly; Tavily owns search.

- [ ] **4. Implement Tavily and citations.** POST selected endpoint with `Authorization: Bearer <backend key>` and body `{query, search_depth:"basic", max_results:8, include_answer:false, include_raw_content:false}`. This release uses basic depth with eight-result cap; no unsupported depth setting is exposed. Use Task 3 HTTP limits/cancellation; no secret in JSON/logs. Accept documented `results` entries with url/title/content and optional published_date. Canonicalize valid http/https URLs for deduplication, cap URL 2048 bytes/title 300 chars/excerpt 8000 chars, skip invalid entries with bounded local counts. Store request hash and response hash, actual fetched time and original source URL. No results is successful empty data, not a network failure.

Optional overview sends only stored source IDs/title/url/excerpt to the selected LLM; exclude legacy rows with absent/empty excerpts, never send UI fallback text as evidence. If no source has usable stored evidence, return a local InvalidResponse with re-search guidance before any LLM request. Strict schema `{facts:[{text,source_ids}],unknowns:[...]}` with <=30 facts/unknowns, text <=2000 chars. Every fact requires at least one source ID from this run; reject invented IDs and missing citations, and show links from stored URLs. Unknowns may contain explicit unanswered questions but cannot be promoted to facts. Persist a failed raw overview response with hash and retain all source rows. Opening a result uses the system browser only on user click and revalidates http/https; never render remote HTML or auto-load remote previews.

- [ ] **5. Verify and commit.**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib research
cargo test --manifest-path src-tauri/Cargo.toml --lib intelligence
```

Commit `feat: research confirmed customer entities with stored citations`.

### Task 8: Knowledge/research tools and model settings UI

**Files:** Create src/features/knowledge/{KnowledgePage,DocumentDetail,SearchSheet,useKnowledge}.tsx/ts, src/features/research/{ResearchPanel,EntityConfirmation,SourceList}.tsx, src/features/settings/{ModelSettings,TavilySettings,DataSettings}.tsx, src/features/jobs/{controller,JobStatus}.ts/tsx and tests. Modify ToolRail/App/settings routes. Add @tauri-apps/plugin-dialog and Rust plugin registration/capability for open/save dialogs only; no broad filesystem command permission.

**Interfaces consumed:** Tasks 3–7 command DTOs mirrored in wire.ts with added Rust-generated contract fixtures. Job controller uses subscribe_intelligence's returned ticket, listens, snapshots list/get jobs, and polls only tracked unfinished jobs every second with max four concurrent getter calls; cleanup unlistens/unsubscribes. Keep <=64 visible job states; history remains paged. Never subscribe on the already-closed session event channel for summary completion.

- [ ] **1. Write RED flows with the desktop fixture.**

```tsx
it("shows the selected transcript as a query but waits for explicit search", async () => {
  const port = intelligenceDesktopFixture(wire);
  render(<SearchSheet desktop={port} selection={wire.pumpSelection} open />);
  expect(screen.getByRole("textbox", { name: "检索内容" })).toHaveValue("额定电压 24 V");
  expect(port.calls("search_knowledge")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "检索知识库" }));
  expect(port.calls("search_knowledge")).toHaveLength(1);
  expect(await screen.findByText("产品手册 · 第 2 页")).toBeInTheDocument();
});
it("requires a confirmation action before researching a candidate", async () => {
  const port = intelligenceDesktopFixture(wire);
  render(<ResearchPanel desktop={port} sessionId={wire.callRecord.id} />);
  await user.click(await screen.findByText("Northstar Fluid Systems"));
  expect(port.calls("start_research")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "确认并查询公开信息" }));
  expect(port.lastArgs("start_research").request.confirmed).toBe(true);
});
```

Fixtures implement service job transitions and stored source results at the command boundary. Test paused repair state, changed embedding model, indexing progress/error/retry/cancel, file-picker cancel, import duplicate, confirmation edit, no results, failed overview with sources still visible, invalid external link blocked, secret save/clear/reload, stale async model test, and all five settings groups reachable via keyboard.

- [ ] **2. Implement document/import/search views.** KnowledgePage uses Table with title/type/version/status/chunk count/last update. A missing title displays `未命名文档` plus the short ID, Unknown kind displays `类型未知`, Legacy origin displays `历史文档`; missing paths display `未记录源文件` and offer explicit Replace/reimport. These labels are not persisted fields. Provide explicit import with PDF/DOCX/text/Markdown filters, a search form and row reindex/replace/delete actions. File dialog returns selected native paths to import_document; cancellation is a no-op. Explain supported size/type near the field. Delete dialog names document and says it removes only local managed copy/index. Parsing/indexing status comes from jobs; no invented progress percentage when total unknown.

SearchSheet uses Field/Input and result rows containing title, page/heading, exact source text and score labelled `相似度` with fixed decimals, never a confidence percent. Selected-text search pre-fills and retains supporting IDs; editing the query clears the exact-selection IDs so backend semantics remain honest. RepairRequired offers reindex of affected documents, not a generic retry loop. Empty knowledge links to import/model setup. Changing the selected embedding model shows which documents need indexing for that model, preserving the old index until explicit replacement/rebuild.

- [ ] **3. Implement research/overview views.** Candidates remain editable before confirmation. Manual company/URL input always available. Confirmation button text explicitly mentions querying public information; show source list with title, domain, excerpt, publication date if available and fetched time. For a legacy source, use its validated URL domain (or `历史来源`) only as the display-title fallback, `未保存摘录` when excerpt=None, and `抓取时间未知` when fetched_at_ms=None. Keep a small `历史来源` origin indicator; never convert missing provenance to empty strings/current time or persist display fallback labels. Source excerpt and model output are text, no `dangerouslySetInnerHTML`. Overview facts render only with their stored citation links; unknowns are a separate labelled list. Summary/research errors stay beside their operation and keep prior successful data. The right rail shows compact current selection/results; the full titled Sheet/page holds longer content. On narrow widths, tools never cover the stop control or auto-open over live subtitles.

- [ ] **4. Complete five settings groups.** Audio and translation provider settings from Task 2; Models shows independent LLM/Embedding profiles and selected defaults, automatic summary switch, timeout/context/output/dimension constraints; Tavily selects the saved profile; Data shows managed-data location, supported exports, final-text retention explanation and explicit deletion entry points. No automatic retention deletion policy beyond the approved manual delete is introduced. Use one Save per settings panel, inline validation, status/last-test time, and clear pending key fields on success. Test buttons show `可能产生服务费用` adjacent to the action because they actually invoke configured metered services; no calls on page open.

- [ ] **5. Verify and commit.**

```bash
pnpm test:run src/features/knowledge src/features/research src/features/jobs src/features/settings
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib wire_contract
```

Commit `feat: connect knowledge research and model settings tools`.

### Task 9: Local history, summary comparison and exports

**Files:** Create src/features/history/{SessionsPage,SessionDetail,SummaryPanel,SummaryCompare,ExportMenu,useHistory}.tsx/ts and tests; commands/history.rs and history storage projection/tests. Modify App routing, export modules and session deletion coordination with intelligence jobs.

**Produced history DTOs/commands:**

```rust
pub struct SessionHistoryItem {
    pub record: SessionRecord, pub final_count: u32,
    pub latest_summary: Option<SummaryView>,
}
pub struct SessionHistoryPage {
    pub rows: Vec<SessionHistoryItem>, pub next_cursor: Option<String>, pub total: u64,
}
// Cursor uses the existing (started_at,UUID) order; 1–100 rows.
pub fn list_session_history(after: Option<String>, limit: Option<u32>)
    -> AppResult<SessionHistoryPage>;
```

This projection does not change runtime `list_sessions`. Read count/latest metadata without loading all transcript or raw summaries. Timestamps/duration use durable started/ended state; Running duration is a view clock, stopped duration is frozen. Legacy provider/context says `未知` or `历史配置`, never a current profile name guessed retrospectively.

- [ ] **1. Write RED paging/version/export tests.**

```tsx
it("compares two stored summary versions and never regenerates on selection", async () => {
  const port = intelligenceDesktopFixture(wire);
  render(<SummaryPanel desktop={port} sessionId={wire.completedRecord.id} />);
  await user.click(await screen.findByRole("button", { name: "比较版本" }));
  await user.selectOptions(screen.getByLabelText("旧版本"), "1");
  await user.selectOptions(screen.getByLabelText("新版本"), "2");
  expect(await screen.findByText("需确认耐压参数")).toBeInTheDocument();
  expect(port.calls("generate_summary")).toHaveLength(0);
});
```

Use the actual Select user interactions if generated shadcn does not render native select. Add local history empty/error/paging, legacy role, transcript pages with cross-direction sequences, failed summary version/retry, deletion of a version, export dialog cancel/error/success, active-session delete rejection and deletion while a summary/research job is running. Backend tests prove the session delete first cancels/waits boundedly for owned jobs, then cascades; no worker can repopulate deleted rows afterward.

- [ ] **2. Implement paged history/details and summary versions.** History rows show date/mode/language/provider/duration/final count/latest summary status, with one action menu. Detail reads transcript/diagnostics pages incrementally and shows source/translation metadata, gaps and actual measured stages. It is separate from the live 500-row display window. Keep <=1000 rendered rows using windowed display or page navigation; all history remains accessible through paging.

SummaryPanel reads version/status/profile/model/time/coverage count. Regenerate explicitly enqueues a new job; prior successful version remains displayed during it. Compare exactly two versions side by side (stacked on narrow widths), highlighting changed structured fields via stable field/path comparison, not a fabricated model comparison. Show empty arrays as `未提及`, not hidden errors. Delete version with named confirmation; deleting the whole session explains transcript/summary/research cascade. It never deletes knowledge documents referenced during the call.

- [ ] **3. Wire real export paths and metadata.** Use native save dialog with Markdown/JSON/txt extension; cancel is no-op. Invoke export_session with the returned path; show actual resulting path and an explicit open-folder action if available. Default exports include complete final transcript, typed roles/directions/metadata, diagnostics, context and structured summary versions; no audio/keys/raw model bodies. Retain existing atomic replacement and Windows path behaviour. UI cannot create a success toast until the backend reports success.

- [ ] **4. Verify and commit.**

```bash
pnpm test:run src/features/history
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib history
cargo test --manifest-path src-tauri/Cargo.toml --lib export
cargo test --manifest-path src-tauri/Cargo.toml --lib session
```

Commit `feat: browse compare and export durable session history`.

### Task 10: Whole-product smoke, visual review and release checks

**Files:** Create playwright.config.ts, tests/ui/{live-workbench,settings,knowledge-research,history}.spec.ts and fixtures, test preview entry; modify package.json, tauri.conf.json, capabilities, README.md and release-gates.md. Production App/bridge changes are limited to issues found by these checks and reviewed with the task.

- [ ] **1. Add an explicit offline browser fixture build.**

```bash
pnpm add -D @playwright/test
```

Use Vite mode `test-ui` with a dedicated entry/import selected at build time; the production bundle contains no fixture secret/provider selector or automatic demo session. A visible `测试数据` label appears in the test-ui build. Playwright's webServer runs that built preview on loopback. The real browser-only production build shows a clear `请在桌面应用中使用同传` message when Tauri is absent, while leaving documentation/layout visible; it never simulates successful audio/network activity. Assert production output excludes fixture-only modules using the build manifest.

- [ ] **2. Exercise real controls through the command/event fixture.**

```ts
test("live final survives lag recovery and stop, then appears in history", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "开始同传" }).click();
  await expect(page.getByText("额定电压为 24 V")).toBeVisible();
  await page.getByRole("button", { name: "停止同传" }).click();
  await page.getByRole("link", { name: "会话历史" }).click();
  await page.getByRole("button", { name: "查看会话" }).first().click();
  await expect(page.getByText("额定电压为 24 V")).toBeVisible();
});
```

Fixture scenario injects an actual ResyncNeeded and persisted final through DesktopPort; no direct DOM manipulation to make tests pass. Cover call direction loss/recovery, keyboard operation, every settings view, provider key clearing, document import/index/reindex, manual selection search, candidate edit/confirm, sources after overview failure, summary rerun/compare/delete, export cancellation/failure and responsive Sheets. At 1440×900 all named live blocks show header/first content; at 768 and 390 no page-level horizontal overflow and stop remains reachable. Include reduced-motion and 200% zoom. No native/paid service runs here.

- [ ] **3. Run Taste against the production build and inspect screenshots.** Read the app-screen rules and existing approved style record, not page motion/hero rules. Record product face (IBM Plex and translation teal) and signature element (two direction-labelled speech routes adjoining typed subtitles). No redrawing/reapproval of the chosen style.

```bash
pnpm build
node .superpowers/design-source/taste-skill-2.1.5/skills/taste-skill/scripts/lint.mjs --app src
node .superpowers/design-source/taste-skill-2.1.5/skills/taste-skill/scripts/inspect.mjs --app http://127.0.0.1:4173
```

Serve built production output with `pnpm preview --host 127.0.0.1` for the real empty/browser state and built test-ui output separately for populated/error states. Open every 1440/768/390 screenshot. Apply the app-screen seven-question critique, fix the worst three issues, rerun for at most three rounds. Zero inspector FAIL; each remaining WARN has a concrete written rationale. No eight-animation floor applies to an app. Inspect local fonts actually loaded, translation/source hierarchy, <3% accent share, no resting shadows, field focus, disabled readability, Sheet titles, overflow and real empty states.

- [ ] **4. Complete production verification and packaging metadata.** Set title/productName `Vox Bridge`, initial window 1280×820 and sensible minimum 760×560; narrow browser view remains supported for responsive inspection. Restrict CSP to local assets/IPC (no frontend service requests), packaged fonts and allowed Tauri custom protocols; remote source links open externally, not in an unrestricted WebView. Preserve debug-only audio commands and audio package release gates. Record build/test counts and native target limitations.

```bash
pnpm test:run
pnpm exec playwright test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm test:audio-packages
git diff --check
```

Update README with actual setup/configuration, live/call route selection, final/history/export behaviour, metered connection tests, local managed data, model/index replacement/recovery, scan/OCR limits and manual Tavily confirmation. Keep release gates for signed Windows/macOS drivers, PulseAudio/pipewire-pulse subprocess association/drain, minimum OSes, real Volcengine and chosen intelligence endpoints. Do not label the application release-ready on fixture/browser tests alone.

Commit `test: verify workbench intelligence flows and production presentation`.

## Controller preflight checklist

- [ ] Reconcile runtime Task 6's final DTO/serde/command shapes with Task 1 wire fixtures and Task 2 controls.
- [ ] Check every task pair sharing storage migration dispatcher, AppState/commands, job interfaces, DTOs or frontend routing; record the table in this plan's SDD ledger.
- [ ] Keep migration order 0003 intelligence → 0004 documents → 0005 summaries → 0006 research; confirm no unrelated task has consumed these version numbers.
- [ ] Verify parser-worker OS limits/packaged executable invocation against platform process APIs before Task 4; preserve minimum-OS gates.
- [ ] Confirm SrvDB dependency/API/dimension/ID semantics and config-profile snapshots survive actual integration tests.
- [ ] Validate entire final-transcript coverage and local failed raw-response retention; no closed-session event dependency.
- [ ] Check fixture-only code is excluded from production and credentials never appear in public DTOs/artifacts.
- [ ] Verify legacy/new accounting diagnostic fixtures preserve rate units, finalized/unknown-tail flags and forward-compatible unknown variants without inferring acoustic timing.
- [ ] Complete controller spec coverage and type/interface self-review before execution; this refinement report is not that review.
