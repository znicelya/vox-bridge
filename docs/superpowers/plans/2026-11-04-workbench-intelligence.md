# Workbench And Intelligence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the template React screen with the confirmed shadcn/ui live workbench and add local sessions, SrvDB knowledge search, LLM summaries, and Tavily-backed customer research.

**Architecture:** React subscribes to typed Tauri events and renders a framed-sheet desktop workbench with a collapsible sidebar and Sheet tools on narrow windows. Rust commands perform file parsing, embeddings, SrvDB operations, LLM calls, and Tavily requests. Frontend controls never receive secrets or raw audio.

**Tech Stack:** React 19, TypeScript, Vite, shadcn/ui, Tailwind CSS v4, lucide-react, IBM Plex Sans/Mono, Tauri event API, Rust `srvdb` pinned at `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`, reqwest, lopdf, docx-rs or a tested DOCX parser, serde_json.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- Use the confirmed Taste `framed-sheet` app direction: flat tinted frame, white sheet, no resting shadows, colour on data/status.
- Use shadcn/ui primitives and Lucide icons; do not hand-roll component variants or raw icon SVGs.
- Keep live subtitles as the primary content; tools move into a Sheet on narrow windows.
- Persist sessions and knowledge data locally; do not save audio by default.
- LLM/Embedding endpoints are OpenAI-compatible and user-configurable; Tavily is the first research provider.
- All loading, empty, error, disabled, selected, and permission states must be visible and actionable.

### Task 1: Initialize shadcn/ui and app tokens

**Files:**
- Modify: `package.json`
- Create: `components.json`
- Create: `src/lib/utils.ts`
- Create: `src/index.css`
- Modify: `src/main.tsx`
- Modify: `src/App.css`
- Test: `src/lib/utils.test.ts`

- [ ] **Step 1: Initialize the Vite project with shadcn.**

Run from the repository root:

```bash
pnpm add class-variance-authority clsx tailwind-merge lucide-react
pnpm dlx shadcn@latest init --defaults
pnpm dlx shadcn@latest add button badge card separator tabs sidebar sheet select command table alert empty skeleton tooltip
```

Use the generated Vite aliases and Tailwind v4 CSS entry; do not overwrite unrelated files with `--overwrite`.

- [ ] **Step 2: Define semantic tokens.**

Set the framed-sheet light tokens in `src/index.css`: `--background` `#f5f7f8`, `--card` `#ffffff`, `--border` `#e7eaed`, `--foreground` `#16181b`, `--muted-foreground` `#64686d`, `--primary` `#1b6c80`, success/warning/destructive tokens, 8px control radius and 16px sheet radius. Load IBM Plex Sans and IBM Plex Mono with local fallback stacks.

- [ ] **Step 3: Test `cn()` and build.**

```ts
import { cn } from "@/lib/utils";

it("merges conditional class names", () => {
  expect(cn("px-2", false && "hidden", "py-1")).toBe("px-2 py-1");
});
```

Run: `pnpm build`.

Expected: PASS with no template logos or default dark-mode CSS remaining.

- [ ] **Step 4: Commit.**

```bash
git add package.json pnpm-lock.yaml components.json src/index.css src/lib src/App.css src/main.tsx
git commit -m "feat: initialize shadcn workbench tokens"
```

### Task 2: Live workbench shell and session stream

**Files:**
- Create: `src/features/workbench/types.ts`
- Create: `src/features/workbench/mock-data.ts`
- Create: `src/features/workbench/LiveWorkbench.tsx`
- Create: `src/features/workbench/ConversationStream.tsx`
- Create: `src/features/workbench/AudioRoutePanel.tsx`
- Create: `src/features/workbench/ResearchRail.tsx`
- Modify: `src/App.tsx`
- Test: `src/features/workbench/LiveWorkbench.test.tsx`

**Interfaces:**
- `SessionSnapshot`, `TranscriptSegment`, `AudioRouteStatus`, `ProviderStatus` in `types.ts`.
- `LiveWorkbench({ snapshot, segments, onStart, onStop, onSearchKnowledge, onConfirmResearch })`.

- [ ] **Step 1: Write component behavior tests.**

```tsx
it("renders both directions and keeps the active translation mode visible", () => {
  render(<LiveWorkbench snapshot={callSnapshot} segments={callSegments} />);
  expect(screen.getByText("Live translation")).toBeInTheDocument();
  expect(screen.getByText("You")).toBeInTheDocument();
  expect(screen.getByText("Customer")).toBeInTheDocument();
  expect(screen.getByText("Connected")).toBeInTheDocument();
});
```

- [ ] **Step 2: Compose the shell from shadcn primitives.**

Use `Sidebar`, `SidebarContent`, `SidebarMenu`, `Tabs`, `Button`, `Badge`, `Separator`, `Select`, `Table`, `Sheet`, and `Tooltip`. Keep one white work sheet with 24px content padding; use `data-icon` on icon buttons and `aria-label` on icon-only controls.

- [ ] **Step 3: Render live transcript states.**

Render interim text with muted styling, final text with source/translation rows, timestamps with tabular numerals, latency segment, provider status, and route status. Use `Empty`, `Skeleton`, and `Alert` for no session/loading/error states.

- [ ] **Step 4: Add responsive layout.**

At desktop, show the 228px sidebar and 250px research rail. Below 900px, collapse the sidebar into a Sheet and move audio/knowledge/research panels into a Sheet; keep the conversation stream first in DOM order.

- [ ] **Step 5: Subscribe to Tauri events.**

Use `listen<SessionEvent>(`voxbridge://session/${sessionId}`)` in a hook; unsubscribe on cleanup; update interim segments by event ID and append final segments by sequence. Never put provider credentials in React state.

- [ ] **Step 6: Run and commit.**

Run: `pnpm test -- --run src/features/workbench/LiveWorkbench.test.tsx` and `pnpm build`.

```bash
git add src/features/workbench src/App.tsx
git commit -m "feat: build shadcn live translation workbench"
```

### Task 3: Sessions, history, and exports UI

**Files:**
- Create: `src/features/sessions/SessionsPage.tsx`
- Create: `src/features/sessions/SessionDetailPage.tsx`
- Create: `src/features/sessions/exportMenu.tsx`
- Create: `src/features/sessions/useSessions.ts`
- Modify: `src/App.tsx`
- Test: `src/features/sessions/SessionsPage.test.tsx`

- [ ] **Step 1: Write history tests.**

```tsx
it("shows local sessions with provider and language pair", () => {
  render(<SessionsPage sessions={[fixtureSessionSummary]} />);
  expect(screen.getByText("Customer discovery")).toBeInTheDocument();
  expect(screen.getByText("Volcengine · 中文 → English")).toBeInTheDocument();
});
```

- [ ] **Step 2: Implement Tauri list/detail/delete commands.**

Call the foundation commands with cursor pagination; show date, mode, language pair, provider, duration, final segment count, summary status, and a delete confirmation dialog with a required title.

- [ ] **Step 3: Implement export menu.**

Use a shadcn `DropdownMenu` with Markdown, JSON, and plain text actions; invoke `export_session` and show the resulting path in a success Alert without exposing raw audio.

- [ ] **Step 4: Run and commit.**

Run: `pnpm test -- --run src/features/sessions` and `pnpm build`.

```bash
git add src/features/sessions src/App.tsx
git commit -m "feat: browse and export local sessions"
```

### Task 4: Knowledge documents and SrvDB search

**Files:**
- Create: `src-tauri/src/knowledge/mod.rs`
- Create: `src-tauri/src/knowledge/parser.rs`
- Create: `src-tauri/src/knowledge/chunker.rs`
- Create: `src-tauri/src/knowledge/embedding.rs`
- Create: `src-tauri/src/knowledge/index.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands.rs`
- Create: `src/features/knowledge/KnowledgePage.tsx`
- Create: `src/features/knowledge/SearchSheet.tsx`
- Test: `src-tauri/src/knowledge/parser_tests.rs`
- Test: `src/features/knowledge/SearchSheet.test.tsx`

**Interfaces:**
- `KnowledgeIndex::import(path: &Path) -> AppResult<DocumentSummary>`.
- `KnowledgeIndex::search(query: &str, limit: usize) -> AppResult<Vec<KnowledgeHit>>`.
- `search_knowledge(query, selected_segment_id)` Tauri command.

- [ ] **Step 1: Write parser and index tests.**

```rust
#[test]
fn markdown_import_preserves_heading_and_source_offset() {
    let document = parse_fixture("product.md").unwrap();
    assert_eq!(document.chunks[0].heading.as_deref(), Some("Thermal module"));
    assert!(document.chunks[0].text.contains("24 V"));
}
```

- [ ] **Step 2: Add SrvDB pinned dependency and index wrapper.**

Use the Rust dependency at revision `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`; create one database directory per embedding model/dimension; store chunk metadata in SrvDB JSON and authoritative rows in SQLite; call `persist()` after import/upsert/delete.

- [ ] **Step 3: Implement PDF/DOCX/text/Markdown parsing and chunking.**

Extract page/paragraph offsets, normalize whitespace, split into 400-800 token-equivalent chunks with 80 token overlap, and retain source file/page/heading metadata. Copy source files into the app data directory and verify SHA-256 before indexing.

- [ ] **Step 4: Implement OpenAI-compatible Embedding client.**

POST `{base_url}/embeddings` with model and input array; require a consistent dimension; reject NaN/empty vectors; persist model and dimension with every chunk. Use a bounded concurrency of four requests.

- [ ] **Step 5: Implement local semantic search and Tauri commands.**

Embed the query, call SrvDB `search`, hydrate results from SQLite, and return score, source file, page/heading, snippet, and chunk ID. On index mismatch, return `IndexRepairRequired` and queue rebuild.

- [ ] **Step 6: Build knowledge UI and selection search.**

Use `Input`, `Table`, `Badge`, `Sheet`, `Empty`, `Skeleton`, and `Alert`; allow file import, keyword search, and selected transcript text search. Render source path/page and score; never send a knowledge query to the translation provider.

- [ ] **Step 7: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml knowledge`, `pnpm test -- --run src/features/knowledge`, and `pnpm build`.

```bash
git add src-tauri/src/knowledge src-tauri/Cargo.toml src-tauri/src/commands.rs src/features/knowledge
git commit -m "feat: add local SrvDB knowledge search"
```

### Task 5: Automatic and manual LLM summaries

**Files:**
- Create: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/src/llm/openai_compatible.rs`
- Create: `src-tauri/src/summary/mod.rs`
- Create: `src/features/sessions/SummaryPanel.tsx`
- Modify: `src-tauri/src/session/orchestrator.rs`
- Test: `src-tauri/src/llm/openai_compatible_tests.rs`
- Test: `src/features/sessions/SummaryPanel.test.tsx`

**Interfaces:**
- `LlmProvider::complete(request: LlmRequest) -> AppResult<LlmResponse>`.
- `SummaryService::generate(session_id: Uuid, scope: SummaryScope) -> AppResult<SummaryVersion>`.

- [ ] **Step 1: Write JSON parsing and redaction tests.**

```rust
#[test]
fn summary_parser_rejects_unstructured_model_output() {
    let result = parse_summary("plain prose without json");
    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
}
```

- [ ] **Step 2: Implement OpenAI-compatible chat request.**

POST `/chat/completions` with model, system instruction, final transcript, JSON schema instruction, timeout, and token limit; accept `choices[0].message.content`; redact endpoint credentials from tracing spans.

- [ ] **Step 3: Implement summary schema and version persistence.**

Require `summary`, `participants`, `companies`, `requirements`, `action_items`, `risks`, and `terms` arrays/strings; save raw response hash and parsed JSON; allow a new version without overwriting prior versions.

- [ ] **Step 4: Trigger automatic summary after session completion.**

Queue a background Tokio task after `Completed`; publish `SummaryStarted`, `SummaryReady`, or `SummaryFailed`; do not delay session stop or export.

- [ ] **Step 5: Build summary panel and rerun controls.**

Show version, model, generated time, structured sections, failure reason, and a `Regenerate` button; use `AlertDialog` with a required title for deletion of a summary version.

- [ ] **Step 6: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml llm summary` and `pnpm test -- --run src/features/sessions/SummaryPanel.test.tsx`.

```bash
git add src-tauri/src/llm src-tauri/src/summary src-tauri/src/session/orchestrator.rs src/features/sessions/SummaryPanel.tsx
git commit -m "feat: summarize completed sessions with configurable LLMs"
```

### Task 6: Tavily customer research

**Files:**
- Create: `src-tauri/src/research/mod.rs`
- Create: `src-tauri/src/research/tavily.rs`
- Create: `src/features/research/ResearchPanel.tsx`
- Create: `src/features/research/EntityConfirmation.tsx`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/research/tavily_tests.rs`
- Test: `src/features/research/ResearchPanel.test.tsx`

**Interfaces:**
- `ResearchService::suggest_entities(session_id: Uuid) -> AppResult<Vec<EntityCandidate>>`.
- `ResearchService::search(confirmed: ConfirmedEntity) -> AppResult<ResearchRun>`.

- [ ] **Step 1: Write tests that block unconfirmed requests.**

```rust
#[tokio::test]
async fn tavily_is_not_called_for_an_unconfirmed_candidate() {
    let service = fixture_research_service();
    let result = service.search(EntityCandidate::unconfirmed("Northstar Systems")).await;
    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    assert_eq!(service.request_count(), 0);
}
```

- [ ] **Step 2: Implement Tavily request and source persistence.**

POST the configured Tavily endpoint with query, search depth, max results, and API key header; save title, URL, snippet, published date, fetched time, and raw result hash in `research_sources`; redact API key and page content from logs.

- [ ] **Step 3: Implement transcript candidate extraction.**

Use deterministic rules for URLs and capitalized company-like phrases first; return candidates with supporting segment IDs and a confidence label. Never send a candidate to Tavily without explicit confirmation.

- [ ] **Step 4: Build research panel.**

Show candidate confirmation, search progress, source list with links, empty/no-result state, rate-limit/error state, and optional LLM overview with source references. Use `Alert`, `Badge`, `Sheet`, `ScrollArea`, and `Button`.

- [ ] **Step 5: Run and commit.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml research` and `pnpm test -- --run src/features/research`.

```bash
git add src-tauri/src/research src-tauri/src/commands.rs src/features/research
git commit -m "feat: add confirmed Tavily customer research"
```

### Task 7: Settings and secret management UI

**Files:**
- Create: `src/features/settings/SettingsPage.tsx`
- Create: `src/features/settings/ProviderSettings.tsx`
- Create: `src/features/settings/AudioSettings.tsx`
- Create: `src/features/settings/ModelSettings.tsx`
- Create: `src/features/settings/useSettings.ts`
- Test: `src/features/settings/SettingsPage.test.tsx`

- [ ] **Step 1: Write settings visibility tests.**

```tsx
it("shows provider capability limits without exposing a secret", () => {
  render(<ProviderSettings config={fixtureProviderConfig} />);
  expect(screen.getByText("X-Api-Key configured")).toBeInTheDocument();
  expect(screen.queryByText("test-secret")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Implement the five settings sections.**

Use `FieldGroup`/`Field`, `Input`, `Select`, `Switch`, `Button`, `Alert`, `Badge`, and `Separator`; include device install/repair/uninstall, provider config, OpenAI-compatible LLM/Embedding endpoints, Tavily, retention/export, and connection-test timestamps.

- [ ] **Step 3: Add capability-aware validation.**

Disable unsupported language/audio options from provider capabilities; show `NeedsApproval`, `NeedsRestart`, `Unavailable`, and `Ready` states; save through Rust commands and clear secret input after successful save.

- [ ] **Step 4: Run and commit.**

Run: `pnpm test -- --run src/features/settings` and `pnpm build`.

```bash
git add src/features/settings
git commit -m "feat: configure audio and service providers securely"
```

### Task 8: End-to-end smoke checks and release build

**Files:**
- Create: `tests/ui/live-workbench.spec.ts`
- Create: `tests/ui/settings.spec.ts`
- Create: `tests/ui/knowledge-research.spec.ts`
- Modify: `README.md`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Add UI smoke tests.**

Cover: start a fixture live session, render an interim then final segment, select transcript text, open knowledge Sheet, confirm a research candidate, open settings, and export a session. Use fixture Tauri commands so tests never call paid APIs.

- [ ] **Step 2: Run production checks.**

Run: `pnpm build`, `cargo test --manifest-path src-tauri/Cargo.toml`, and the Taste app lint/inspect commands against the built Vite output. Fix every failed accessibility, overflow, blank-state, or placeholder-data check.

- [ ] **Step 3: Document required external gates.**

README must list driver signing/redistribution evidence, Volcengine credentials for the opt-in integration test, supported OS versions, required system permissions, and the fact that provider latency is not guaranteed by the client.

- [ ] **Step 4: Commit.**

```bash
git add tests/ui README.md src-tauri/tauri.conf.json
git commit -m "test: verify workbench flows and release checks"
```

