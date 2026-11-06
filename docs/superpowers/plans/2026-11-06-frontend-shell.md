# Framed Workbench Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Replace the template screen with the approved Vox Bridge application frame, real local navigation and responsive honest empty states.

**Architecture:** This is the dependency-independent shell portion (1A) of Workbench and Intelligence Task1. React owns local hash navigation only; runtime bridge, sessions, configuration forms and intelligence actions remain in their original dependent tasks. Backend code is untouched. Use shadcn primitives generated from the official registry and the approved Taste2.1.5 app-screen rules.

**Tech Stack:** Existing React19/TypeScript6/Vite8/Tauri2; Tailwindv4/shadcn Radix/Lucide/local IBM Plex fonts; Vitest/jsdom/Testing Library.

**Spec:** [Approved workbench design](../specs/2026-11-04-live-translation-workbench-design.md). Source scope: [Workbench and Intelligence](2026-11-04-workbench-intelligence.md), Task1 setup and empty-shell steps. The reviewed runtime-command prerequisite remains in force for Task1B and all feature integration.

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


## Dependency ruling

Shell setup, tokens, local navigation and empty states require no unfinished backend command. Execute them independently while runtime repairs continue; one owner touches frontend/package files only. Full Workbench Task1 is complete only after the runtime bridge (1B) and its tests are implemented against reviewed commands. Retain all later runtime/intelligence gates. No new visual direction, registry or product scope is chosen here.

### Task 1: Approved shell, navigation and frontend tooling

**Owned files:** package.json, pnpm-lock.yaml, vite.config.ts, tsconfig.json/tsconfig.node.json if needed for aliases/tests, index.html, src/{main,App}.tsx, src/App.css, src/index.css, components.json, src/lib/utils.ts, src/test/setup.ts, src/features/shell/AppShell.tsx and focused navigation tests, generated src/components/ui and required small shadcn hooks. A small repo-native SVG mark is allowed. Do not change Rust, drivers, commands, runtime DTOs, wire fixtures or intelligence modules. Do not add fabricated API implementations.

**Interfaces:** Export AppShell and a local route type with workbench, knowledge, history, settings. App renders the shell with truthful empty route content; future features replace those content slots. Hash navigation supports direct links and browser back/forward; unknown hashes fall back consistently to workbench. No feature imports Tauri invoke/listen in this task. The removed template greet UI is not a product feature.

- [ ] **1. Inspect the current package/lockfile and record the before.** Existing untracked pnpm-lock.yaml becomes intentionally owned here; inspect it before installation and preserve unrelated dependency choices. Use the existing Vite stack. Read the pinned Taste skill at `.superpowers/design-source/taste-skill-2.1.5/skills/taste-skill/SKILL.md` and its complete references/app-screens.md in bounded chunks. Read shadcn SKILL.md/rules. The approved direction is framed-sheet with fixed palette/type; do not re-open a style selection or ask for registry permission already settled in the source plan. Record the template before view using the skill's app inspector when available.

- [ ] **2. Install required frontend tooling through pnpm/official shadcn CLI.** Follow the source plan's explicit dependencies and APIs:

```bash
pnpm add class-variance-authority clsx tailwind-merge lucide-react @fontsource/ibm-plex-sans @fontsource/ibm-plex-mono
pnpm add -D tailwindcss @tailwindcss/vite vitest jsdom @testing-library/react @testing-library/user-event @testing-library/jest-dom
pnpm dlx shadcn@latest info --json
pnpm dlx shadcn@latest init --template vite --base radix
pnpm dlx shadcn@latest docs button sidebar sheet empty separator tooltip
```

Fetch/read returned official docs and search @shadcn before adding only the components consumed by this shell. Preserve semantic design tokens when init generates defaults. Configure @/*→src/*, Tailwind Vite plugin, jsdom/setup and test/test:run scripts. Read every generated component used; no preset switch, third-party registry or manual fetch of shadcn source. Follow full Sheet composition/titles, icon and accessible-name rules. Keep generated helpers small; do not implement session controllers or add unrelated packages.

- [ ] **3. Add meaningful failing navigation tests before behavior.** Verify direct hash selection and unknown-route fallback, user navigation updates the active destination/h1/hash, browser hash change selects the correct content, narrow navigation menu selection closes the menu and keeps a valid focus target, and StrictMode cleanup does not leave duplicate hash listeners. Use actual shell interactions with Testing Library. Do not test CSS class strings, cn() or generated primitive internals. Record RED, then implement and run GREEN.

- [ ] **4. Build the approved empty shell and states.** Use tokens: background #f5f7f8, surface #ffffff, border #e7eaed, foreground #16181b, muted #64686d, translation #1b6c80, primary #1d1f20, positive #247b50, warning #92400e, destructive #c4242a. Controls radius8px; sheet16px;1px borders; no resting shadow. Locally load IBM Plex Sans/Mono weights400/500/600 with platform CJK fallback. Main interface text uses Sans, IDs/numeric tokens use Mono. Main title24px, body/nav14px, readable12px metadata; every route has exactly one h1.

At1440px use228px sidebar,48px topbar and white sheet inset8px at right/bottom. Comment the grid arithmetic. Sheet padding24px. Below1100px fold sidebar into a labelled menu; below640px use16px content padding and44px targets. Avoid horizontal overflow at1440/768/390. Product mark/name Vox Bridge and scope 本机会话 stay clear. Active nav is neutral; colour stays below3% and has a job. The top bar carries the actual page/scope, not fake account/search/notification controls.

Each route is an honest empty frame with useful product copy and real navigation where an action exists. Do not display a fabricated session, customer, history, provider/driver connection state, setup percentage or latency. Do not add nonfunctional save/import/start buttons or implementation notes in the user flow. No loading fiction, decorative dashboards, charts, gradients, onboarding illustration or data for screenshot density. This is an empty shell awaiting the separately planned real feature content, as the source plan explicitly requests. Expose a reusable content slot so subsequent features do not need a second shell.

- [ ] **5. Verify interaction/build/design, self-review and commit.**

```bash
pnpm test:run src/features/shell
pnpm build
git diff --check
node .superpowers/design-source/taste-skill-2.1.5/skills/taste-skill/scripts/lint.mjs --app src
```

Use Taste inspect.mjs --app against the production preview at1440/768/390. Open every resulting screenshot; record the seven app-screen critique questions and fix material issues (max3 rounds). Record any tooling limitation or defensible warning accurately; do not claim an unavailable browser check. Use the approved direction/tokens, and do not re-deal colours/type to satisfy a tool. Existing skill scripts are pinned2.1.5; avoid upstream's later marketing-only workflow. Check navigation/menu with keyboard and focus in the rendered UI. The production preview should be left available for controller visual review, with URL/port recorded.

Commit only owned frontend files: `feat: add framed workbench shell`. Write implementation/report with RED/GREEN, exact commands/results, dependency/lock diff summary, screenshot paths/preview URL, visual critique, source boundaries and commit to this plan's SDD workspace. Controller provides independent code/spec/visual review. A frontend build is not backend/audio/service acceptance.
