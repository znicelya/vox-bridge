# Task brief — workbench-intelligence — Task 9

Prepared, NOT dispatched. Actual BASE, reviewed producer handoffs and ownership are supplied at dispatch. Read global-constraints.md and ledgered task-specific rulings. Do not invent unfinished earlier producer APIs or redo completed tasks.

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
