# Task 1B implementation report

Status: implemented and locally verified; controller-supplied independent review remains the phase gate. Approved producer/base: `6365830af2a9e435c4d4b9b985659d76a78f2b47`. Worktree: `.worktrees/vox-bridge-implementation`, branch `codex/vox-bridge-implementation`. No subagents or self-dispatched reviewers were used.

## Owned paths and boundary

- `src/lib/{desktop,wire}.ts` and their `.test.ts` files.
- `src/features/session-stream/{controller,reducer,types}.ts`, `controller.test.tsx`, `reducer.test.ts`.
- `src/test/desktop-fixture.ts` and the narrow fixture-reset addition to `src/test/setup.ts`.
- `tests/contracts/runtime-wire.json` and `src-tauri/src/commands/wire_contract_tests.rs`.
- `src-tauri/src/commands.rs`: four lines registering the cfg(test) wire-contract module only.
- `vite.config.ts`: one test-discovery include line; controller explicitly agreed after the broad-run failure below.
- This report. Logs remain alongside it in the workspace.

No shell/components/styles/routes/main/App, dependencies/lockfiles, production runtime behavior, Task 2 controls, or existing `tests/fixtures/runtime-command-wire.json` were changed. The pre-existing `.gitignore` edit and parallel backend repairs are excluded from this task's commits.

## Architecture and contract

`DesktopPort` is the injected transport interface; `desktopPort` is the real Tauri adapter. Only `desktop.ts` imports Tauri invoke/listen; it unwraps event payloads and provides bounded invocation/decoding plus local error mapping. Tests mock only that native transport for the adapter test; controller tests use a deterministic protocol simulator with named listeners, actual ticket ownership, exclusive rowid pages, captured command envelopes, programmable replies/rejections and deferred completion. The simulator does not implement the reducer or controller.

`wire.ts` derives exported TypeScript wire types from strict bounded decoding schemas. It preserves PascalCase audio enums, external diagnostic tags, internal session/event/device/outcome tags, opaque provider strings, required nullable fields, SessionRecord epoch seconds, transcript/diagnostic milliseconds, SystemTime objects and Duration objects. Integer decoders check JS safety and the narrower Rust u16/u32/i32 or subsecond range. Malformed known diagnostic variants reject. Future diagnostic variants retain a cloned/frozen opaque payload with an 8 KiB content/structure budget, 512-node and eight-level bounds; only the fixed label `其他诊断` is provided for display. No decoder logs raw payloads or error objects.

The shared corpus is constructed independently from real Rust DTOs and compared through `serde_json::to_value` (via `json!` serialization). Frontend tests decode the identical checked JSON. It covers live/call launch, Starting/terminal/legacy records, reverse source interim/final and translation final, legacy null details, seven diagnostic variants including both accounting additions and legacy Playback, all event variants, all lifecycle/failure/gap/playback/sample/export variants, mixed driver/inventory values, ticket/pages, capabilities, public provider nested/nullable fields and launch validation. Future unknown diagnostics are intentionally frontend-only tests because the present Rust enum has no unknown variant.

The reducer replaces/deduplicates by row UUID and preserves insertion order, never comparing A/B sequence values for shared ordering. It keeps 500 finals, 128 interims and 200 diagnostics; `hasEarlierRows` records final truncation. Finals cannot regress to visible interims; epoch replacement clears old interims only in that direction. Legacy details remain null.

The controller subscribes, attaches the returned event listener, then reads the current record and both page families. Each generation owns one recovery task, two independent last-non-null cursors, at most 512 buffered events, and one two-second active-session refresh. Pages use `after` plus `limit: 200`; terminal pages retain the previous cursor for rereading. Resync/overflow clear interims, invalidate the current snapshot pass and resume from confirmed cursors. Buffered rows already seen in pages are removed before the 500-row display window can evict their IDs, preventing old duplicates from being appended again. Recovery rejects nonadvancing/wrong-session/non-final pages, with a 1,000-page bound per family/pass.

Generation checks fence stale starts, pages, listeners and stop completion. Cleanup unlistens and unsubscribes without stopping audio. One transient unsubscribe rejection is retried; persistent failure retains the ticket for the next stop/start cleanup attempt and exposes a local error. The original subscribe/listen Promise retains late-resource cleanup even after the local 10-second deadline. Status refresh also retains its outstanding native request after a local timeout, preventing overlapping native polls. Terminal records with null runtime trigger final page reconciliation even without any terminal event.

## RED/GREEN evidence

Commands ran from the implementation worktree. The initial missing-module failures are explicitly **not behavioral proof**.

| Stage / exact command | Result and retained log |
| --- | --- |
| `pnpm test:run src/features/session-stream src/lib` (initial absent modules) | Exit 1; 2 import-resolution failures, 0 executed tests. `task-1b-red.log`. Adding controller test source yielded the analogous missing-module `task-1b-controller-red.log`. |
| `pnpm test:run src/lib/wire.test.ts` against identity decoder stubs | Behavioral RED: exit 1, 8 failed/1 passed; unsafe numbers and malformed values were accepted, and the opaque label was missing. `task-1b-wire-behavior-red.log`. First real decoder GREEN: 9 passed, exit 0, `task-1b-wire-green.log`. |
| `pnpm test:run src/features/session-stream/reducer.test.ts` against no-op reducer | Behavioral RED: exit 1, 4 failed; identity/order, epoch clearing and memory bounds absent. `task-1b-reducer-behavior-red.log`. |
| `pnpm test:run src/features/session-stream/controller.test.tsx -t 'recovers a final emitted\|handles.*rejection\|continues after unknown\|survives StrictMode'` (actual argument uses literal `|` alternatives, not shell escaping inside the quoted argument) | Behavioral RED: exit 1, 6 failed/7 skipped with the no-op controller. `task-1b-controller-behavior-red.log`. First implementation focused GREEN: 28 passed, exit 0, `task-1b-first-green-attempt.log`. |
| `pnpm test:run src/features/session-stream/controller.test.tsx` | New race tests RED: exit 1, 3 failed/15 passed. Observed old buffered row reinserted after snapshot truncation, transient unsubscribe left a ticket, and old stop cleared new interims. `task-1b-races-red.log`. Corrected buffering, bounded retry and stop-generation publication; focused GREEN 33 passed, exit 0, `task-1b-races-green.log`. |
| `pnpm test:run src/features/session-stream/controller.test.tsx -t 'one native status'` | RED: exit 1, 1 failed/18 skipped; 5 native requests instead of 2 after a stalled request's local deadline. `task-1b-status-timeout-red.log`. Retaining the shared request until actual native completion gives GREEN 1 passed/18 skipped, exit 0, `task-1b-status-timeout-green.log`. |
| `pnpm build` (first compilation) | Exit 2: ES2020 lacks Object.hasOwn/Array.at, and Tauri listen generic inference required explicit T. Corrected locally without changing TS target or dependencies. `task-1b-build-first.log`. |
| `pnpm test:run` (first broad discovery) | Exit 1: all 40 actual frontend tests passed, but three standalone `.mjs` Node scripts, including two ignored snapshots, were incorrectly collected by Vitest and had no Vitest suites. `task-1b-all-frontend.log`. Restricting include to `src/**/*.test.{ts,tsx}` fixes only discovery. |

Cargo commands use `login:false` and this exact prefix:

```sh
env PKG_CONFIG_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig" PKG_CONFIG_SYSROOT_DIR="$PWD/.superpowers/toolchain/alsa/sysroot" LIBRARY_PATH="$PWD/.superpowers/toolchain/alsa/sysroot/usr/lib/x86_64-linux-gnu"
```

`cargo test --manifest-path src-tauri/Cargo.toml --lib wire_contract`:

1. Original corpus: exit 0, 1 passed/404 filtered, `task-1b-rust-first.log`.
2. Deliberate fixture mutation of `callRecord.started_at` from seconds to milliseconds: exit 101, 0 passed/1 failed, exact `shared wire fixture: callRecord` mismatch; `task-1b-rust-mutation-red.log`. The mutation was restored. This is a mutation check, not an alleged producer defect.
3. Adding complete enum/public-provider/preflight expected corpus before its independent Rust constructors: exit 101, 18 actual keys versus 21 expected keys; `task-1b-rust-corpus-red.log`.
4. Final constructors and corpus: exit 0, 1 passed/404 filtered, compile 17.66s, no warnings; `task-1b-rust-green.log`.

Cargo coordination note: one prematurely queued second command blocked on the same target lock after a yielded command was mistaken for completion. It was explicitly terminated, exit 143; its lock-only output is `task-1b-rust-queued.log`, not verification evidence. The final required run occurred after the prior command exited. The controller was notified, all real exit codes were collected, and the target was returned before parallel backend validation. An editor-owned Cargo check in the original checkout was left alone.

## Final gates and self-review

- `pnpm test:run src/features/session-stream src/lib`: exit 0, **35 passed** in four files, `task-1b-focused-green.log`.
- `pnpm test:run` after discovery correction: exit 0, **40 passed** in five files, including all five existing shell tests; `task-1b-all-frontend-green.log`.
- `pnpm build`: exit 0, `tsc && vite build`, Vite completed in 569ms; JS/CSS bundle sizes unchanged at 304.01/55.76 kB; `task-1b-build-final.log`.
- Cargo wire-contract final gate above: exit 0, **1 passed**, no ignored matching tests. No full runtime/service/audio acceptance claim is made.
- `pnpm dlx prettier@3.7.4 --check src/lib/desktop.ts src/lib/desktop.test.ts src/lib/wire.ts src/lib/wire.test.ts src/test/desktop-fixture.ts src/test/setup.ts src/features/session-stream/*.ts src/features/session-stream/*.tsx tests/contracts/runtime-wire.json vite.config.ts`: exit 0; `task-1b-format-final.log`. Prettier ran from the transient dlx cache; no manifest/lockfile change.
- `rustfmt --edition 2024 --check src-tauri/src/commands/wire_contract_tests.rs`: exit 0.
- `git diff --check`: exit 0. Reviewed production Rust diff: cfg(test) registration only. Reviewed Tauri imports: production imports only in desktop.ts.

Additional tests cover strict-mode mount/unmount, late subscribe/listener cleanup, stale pages/session switches, subscription/listen/page rejection, cross-session event and snapshot fences, resync during an in-flight page, overflow, terminal-without-event reconciliation, nullable units/ranges, unknown diagnostic followed by a valid final, wrong page cursors, source/translation in either arrival order and independent directional status changes.

No paid endpoints, real audio, native keyring or credentials were used. No new UI was introduced; Task 2 will consume the exported port/controller/wire modules. Persistent native hangs cannot be cancelled by JavaScript; local waits are bounded and late resource cleanup remains attached, while status polling does not multiply hanging requests. Persistent unsubscribe failure is surfaced locally and remains retryable rather than being reported as released. Very large histories beyond 1,000 pages per family/pass fail locally instead of allowing an unbounded recovery loop; durable rows remain in storage. These limits are implementation boundaries for review, not evidence of native service/audio quality.

## Commits

Implementation commit: `49b568e` — `feat: establish framed workbench and recoverable runtime bridge` (15 owned source/test/config/fixture paths). This report-only follow-up commit records the implementation evidence; its hash is returned to the controller.
