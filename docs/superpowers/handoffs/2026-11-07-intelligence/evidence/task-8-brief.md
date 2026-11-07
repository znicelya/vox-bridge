# Task brief — workbench-intelligence — Task 8

Prepared, NOT dispatched. Actual BASE, reviewed producer handoffs and ownership are supplied at dispatch. Read global-constraints.md and ledgered task-specific rulings. Do not invent unfinished earlier producer APIs or redo completed tasks.

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

- [ ] **2. Implement document/import/search views.** KnowledgePage uses Table with title/type/version/status/chunk count/last update from DocumentSummary.updated_at_ms; None displays `更新时间未知`. A missing title displays `未命名文档` plus the short ID, Unknown kind displays `类型未知`, Legacy origin displays `历史文档`; missing paths display `未记录源文件` and offer explicit Replace/reimport. These labels are not persisted fields. Provide explicit import with PDF/DOCX/text/Markdown filters, a search form and row reindex/replace/delete actions. File dialog returns selected native paths to import_document; cancellation is a no-op. Explain supported size/type near the field. Delete dialog names document and says it removes only local managed copy/index. Parsing/indexing status comes from jobs; no invented progress percentage when total unknown.

SearchSheet uses Field/Input and result rows containing title, page/heading, exact source text and score labelled `相似度` with fixed decimals, never a confidence percent. Selected-text search pre-fills and retains supporting IDs; editing the query clears the exact-selection IDs so backend semantics remain honest. RepairRequired offers reindex of affected documents, not a generic retry loop. Empty knowledge links to import/model setup. Changing the selected embedding model shows which documents need indexing for that model, preserving the old index until explicit replacement/rebuild.

- [ ] **3. Implement research/overview views.** Candidates remain editable before confirmation. Manual company/URL input always available. Confirmation button text explicitly mentions querying public information; show source list with title, domain, excerpt, publication date if available and fetched time. For a legacy source, use its validated URL domain (or `历史来源`) only as the display-title fallback, `未保存摘录` when excerpt=None, and `抓取时间未知` when fetched_at_ms=None. Keep a small `历史来源` origin indicator; never convert missing provenance to empty strings/current time or persist display fallback labels. Source excerpt and model output are text, no `dangerouslySetInnerHTML`. Overview facts render only with their stored citation links; unknowns are a separate labelled list. Summary/research errors stay beside their operation and keep prior successful data. The right rail shows compact current selection/results; the full titled Sheet/page holds longer content. On narrow widths, tools never cover the stop control or auto-open over live subtitles.

- [ ] **4. Complete five settings groups.** Audio and translation provider settings from Task 2; Models shows independent LLM/Embedding profiles and selected defaults, automatic summary switch, timeout/context/output/dimension constraints; Tavily selects the saved profile; Data reads managed-data location through get_managed_data_location, and shows supported exports, final-text retention explanation and explicit deletion entry points. No automatic retention deletion policy beyond the approved manual delete is introduced. Use one Save per settings panel, inline validation, status/last-test time, and clear pending key fields on success. Test buttons show `可能产生服务费用` adjacent to the action because they actually invoke configured metered services; no calls on page open.

- [ ] **5. Verify and commit.**

```bash
pnpm test:run src/features/knowledge src/features/research src/features/jobs src/features/settings
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --lib wire_contract
```

Commit `feat: connect knowledge research and model settings tools`.

## Prepared job-observation handoff

Read task-8-jobs-controller-handoff.md at dispatch. It reconciles reviewed3B actual DTO/camelCase APIs/ticket event names with Task1 original-native-Promise ownership, listener-before-snapshot, unsequenced/same-ms lifecycle merging, global4getter slots across retired runs,64visible/due bounds and1000mspolling. This is a proposal/preparation, not implementation or review. Job discovery is tracked unfinished jobs plus explicit paged history/current operation IDs; no fabricated status-filter command or automatic whole-history crawl. Mirror actual earlier producer DTOs when their reviewed handoffs arrive.
