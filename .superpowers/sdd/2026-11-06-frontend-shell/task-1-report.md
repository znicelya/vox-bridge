# Task 1 implementation report

Status: DONE. Commit: pending at report time. Base: `c61ac69`.

## Implemented scope

Replaced the Tauri/Vite greeting template with the approved Vox Bridge framed-sheet application shell. `AppShell` exports a local `AppRoute` union for `workbench`, `knowledge`, `history`, and `settings`, plus a `renderContent(route)` slot for later feature work. `App` renders the shell with honest empty content only.

Hash navigation supports direct links, user navigation, browser hash changes/back-forward behavior, and consistent normalization of missing or unknown hashes to `#workbench`. The active destination, top bar, single route `h1`, description, and content change together. No shell source imports Tauri `invoke` or `listen`, and no runtime DTO, service mock, fabricated session, connection state, latency, history row, or setup percentage was added.

The responsive frame uses the approved 228px desktop sidebar, 48px top bar, 8px right/bottom inset, 16px sheet radius, 1px borders, and no resting shell shadow. Below 1100px the sidebar becomes a labelled full-screen Sheet; at phone width the content padding is 16px and interactive targets are at least 44px. The app uses the fixed semantic palette and locally bundled IBM Plex Sans/Mono 400/500/600 with platform CJK fallbacks.

Only consumed shadcn primitives were generated: Button, Sheet, Empty, and Separator. The Sheet has a visible title/description, an accessible close control, focus containment, close-on-navigation behavior, and trigger focus restoration.

## Package, lockfile, and CLI record

The starting package retained React 19, TypeScript 6, Vite 8, the Tauri API/opener packages, the Tauri CLI, and the existing audio-package scripts. The pre-existing untracked `pnpm-lock.yaml` was inspected and then intentionally adopted as Task 1 dependency state; unrelated dependency choices were not removed or downgraded.

Installed application dependencies cover IBM Plex fonts, class variants/merging, Lucide, the official Radix-based shadcn runtime, and Tailwind animation support. Development dependencies add Tailwind v4's Vite plugin, Vitest/jsdom, and Testing Library. The final lock importer preserves the original package specifiers and resolves the complete new frontend/test graph reproducibly.

The required official CLI preflight, `pnpm dlx shadcn@latest info --json`, could not return project metadata before a `components.json` project existed. This was treated as an uninitialized-project preflight limitation, not permission to fetch component source manually. The project was initialized with the official CLI as Vite + Radix/Nova, official docs/examples and the `@shadcn` registry were queried, and only the four consumed primitives were added. CLI-added Geist font state was removed because the approved type system is IBM Plex. No third-party registry or preset switch was used.

Configuration added `@/* -> src/*`, the Tailwind v4 Vite plugin, jsdom/setup files, and `test` / `test:run` scripts. The generated primitives and helper were read after generation.

## TDD evidence

### RED

Before shell behavior existed:

```bash
pnpm test:run src/features/shell
```

Result: the focused file ran and all 5 tests failed against the greeting template. The failures demonstrated the missing direct/unknown hash routing, navigation-driven hash/heading/current state, external hash synchronization, narrow Sheet close/focus behavior, and StrictMode listener lifecycle.

### GREEN

After implementation, the same command passed all five tests. The fresh final run reported:

```text
Test Files  1 passed (1)
Tests       5 passed (5)
Duration    1.81s
```

The tests use rendered shell interactions rather than implementation class assertions and cover:

- direct `#knowledge` selection and unknown-hash normalization to `#workbench`;
- labelled navigation updating the URL hash, `h1`, and `aria-current` destination;
- external browser `hashchange` updates;
- narrow navigation selection closing the dialog and restoring focus to the menu trigger;
- StrictMode mount/unmount leaving exactly one active hash listener and then none.

## Final verification

Fresh commands run in the implementation worktree:

```bash
pnpm test:run src/features/shell
pnpm build
git diff --check
pnpm exec tsc -p tsconfig.node.json --noEmit
node .superpowers/design-source/taste-skill-2.1.5/skills/taste-skill/scripts/lint.mjs --app src
```

Results:

- focused shell tests: 1 file passed, 5 tests passed;
- production build: TypeScript and Vite succeeded, 2,173 modules transformed, final JS 304.01kB / 98.57kB gzip;
- `git diff --check`: exit 0;
- Node config TypeScript check: exit 0;
- Taste source lint: 12 files, `0 FAIL, 0 WARN`;
- source scan: no Tauri `invoke`/`listen` import or call in the owned frontend paths;
- production preview: HTTP 200 at `http://127.0.0.1:4173/`.

The generated `tsconfig.node.tsbuildinfo` verification artifact was removed and is not part of the change.

## Browser/design verification

Pinned Taste 2.1.5 inspected the production preview in app mode at 1440, 768, and 390. The final result is `0 FAIL, 2 WARN`. Every final fold and full-page screenshot was opened:

- `.taste-skill/after-final/desktop-fold.jpg`
- `.taste-skill/after-final/desktop-page-01.jpg`
- `.taste-skill/after-final/tablet-fold.jpg`
- `.taste-skill/after-final/tablet-page-01.jpg`
- `.taste-skill/after-final/mobile-fold.jpg`
- `.taste-skill/after-final/mobile-page-01.jpg`

The inspection artifacts remain untracked under `.taste-skill/` and are not committed. Earlier review rounds are retained there as `before`, `after-round-1`, and `after-round-2` for visual comparison.

The two final warnings are deliberately defended:

- `hero-void`: the large first-screen rest is the truthful empty workbench canvas; filling it would require the fake session/data/dashboard content explicitly prohibited by the brief.
- `panel-void`: the blank band inside the empty route frame is the same intentional future content slot; shrinking it would weaken the stable shell frame, while populating it would invent product state.

No inspector exception was allowed. The approved palette/type were not re-dealt to silence a warning. CUA browser creation was attempted but failed with a plugin `timeoutMs` error; the pinned Taste CDP browser completed the real Chrome inspection and measurements instead.

Real Chrome measurements at 390px:

- menu trigger: 110.53 × 44px;
- workbench empty action: 90 × 44px;
- route `h1`: 24px;
- horizontal overflow: 0px;
- Sheet close control: 44 × 44px;
- Sheet navigation links: 357 × 48px;
- Sheet dialog: 390 × 844px, full screen;
- initial Sheet focus: close button.

### Seven-question app-screen critique

1. **Question and reading order.** Each route first answers “what area am I in, and what exists here?” through the 24px `h1`, plain description, and dominant empty-state explanation. On Workbench the only action is the real navigation to Settings; no invented figure competes with the route question.
2. **Edges and fullness.** Sidebar, top bar, inset sheet, header rule, and route slot share a closed grid with no dead side strip or horizontal overflow. The only blank bands are the intentional empty product canvas identified by the two defended warnings.
3. **Colour and type.** White/light-neutral surfaces hold nearly all pixels. Colour is limited to the product mark, focus/selection semantics, and the one primary navigation action. IBM Plex Sans renders the interface; IBM Plex Mono is reserved for later IDs/numeric tokens rather than used decoratively.
4. **Numeric truth.** The empty shell displays no product figures, totals, deltas, duplicated facts, latency, or timestamps, so there is nothing fabricated or arithmetically inconsistent.
5. **Zero/missing/action badges.** The shell uses prose empty states rather than fake zeroes, dashes, queue counts, notification badges, or statuses. Each absence is named in product language.
6. **Legends and labels.** There are no charts, legends, swatches, or overlapping data labels. Icons reinforce already-labelled destinations and controls rather than carrying meaning alone.
7. **Phone behavior.** Navigation is folded into a labelled full-screen Sheet at 768 and 390; all destinations remain visible, targets meet the measured sizes above, focus returns after selection, and each route retains its `h1`, description, and honest empty answer.

Style record: approved `framed-sheet` direction, fixed by the source plan rather than randomly dealt; no seed or redraw was required. The signature element is the quiet neutral rail around the inset white work sheet. The chosen face is IBM Plex Sans with IBM Plex Mono reserved for data tokens.

## Files and boundaries

Owned change set:

- package/tooling: `package.json`, `pnpm-lock.yaml`, `components.json`, `vite.config.ts`, `tsconfig.json`;
- entry and styling: `index.html`, `src/main.tsx`, `src/App.tsx`, `src/App.css`, `src/index.css`;
- shell/tests: `src/features/shell/AppShell.tsx`, `src/features/shell/AppShell.test.tsx`, `src/test/setup.ts`;
- generated consumed primitives/helper: `src/components/ui/{button,empty,separator,sheet}.tsx`, `src/lib/utils.ts`;
- this report.

All `src-tauri` changes in the shared worktree belong to other tasks and were neither modified for this task nor staged. `.taste-skill/` is also excluded. This frontend acceptance proves the shell, navigation, accessibility-focused interactions, build, and browser layout only; it does not claim backend, audio, driver, runtime, provider, or paid-service acceptance.

Preview for controller review: `http://127.0.0.1:4173/`.
