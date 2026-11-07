# Task brief — workbench-intelligence — Task 10

Prepared, NOT dispatched. Actual BASE, reviewed producer handoffs and ownership are supplied at dispatch. Read global-constraints.md and ledgered task-specific rulings. Do not invent unfinished earlier producer APIs or redo completed tasks.

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
