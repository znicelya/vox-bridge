# Live Translation Implementation Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this roadmap task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the approved Vox Bridge live translation workbench in dependency order while keeping each subsystem independently testable.

**Architecture:** Execute four plans in sequence: session foundation, platform audio, provider adapters, then the React workbench and intelligence tools. The foundation provides the event/storage contracts; audio and providers plug into the orchestrator; the final plan binds the contracts to the UI and local research features.

**Tech Stack:** Tauri 2, Rust 1.99, Tokio, cpal, rubato, tokio-tungstenite, prost, rusqlite, SrvDB, React 19, Vite, shadcn/ui, Tailwind CSS v4, TypeScript.

**Spec:** `docs/superpowers/specs/2026-11-04-live-translation-workbench-design.md`

## Global Constraints

- First release has real virtual audio routing on Windows 10 22H2, macOS 13, and Ubuntu 22.04 LTS.
- First release has a real Volcengine AST adapter; Aliyun and Palabra remain capability-safe stubs until separately credentialed and validated.
- Session data stays local; raw audio is not persisted by default.
- SrvDB is pinned to revision `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`.
- UI follows the confirmed Taste `framed-sheet` direction and shadcn/ui composition.
- Every phase ends with tests and a commit; no phase may hide errors behind a generic success state.

### Task 1: Establish dependency order and release gates

**Files:**
- Create: `docs/superpowers/plans/release-gates.md`

- [ ] **Step 1: Record the execution order.**

```markdown
1. Session Foundation: creates typed state, storage, secrets, commands, and exports.
2. Platform Audio: creates real M1/M2 devices and canonical PCM routing.
3. Provider Adapters: connects canonical audio to Volcengine and fixture providers.
4. Workbench And Intelligence: renders the UI and adds SrvDB, summaries, and Tavily.
```

- [ ] **Step 2: Record release gates.**

List the exact gate commands: `cargo test --manifest-path src-tauri/Cargo.toml`, `pnpm build`, platform loopback tests on all three OSes, opt-in Volcengine integration, and UI smoke tests. Require signed/redistributable driver artifacts before packaging.

- [ ] **Step 3: Commit the roadmap metadata.**

```bash
git add docs/superpowers/plans/release-gates.md
git commit -m "docs: record live translation release gates"
```

### Task 2: Execute the subsystem plans

**Files:**
- Read: `docs/superpowers/plans/2026-11-04-session-foundation.md`
- Read: `docs/superpowers/plans/2026-11-04-platform-audio.md`
- Read: `docs/superpowers/plans/2026-11-04-provider-adapters.md`
- Read: `docs/superpowers/plans/2026-11-04-workbench-intelligence.md`

- [ ] **Step 1: Finish and verify Session Foundation.**

Run its full Rust and TypeScript checks before starting audio work. Verify a session can be created, stopped, listed, deleted, and exported without a provider.

- [ ] **Step 2: Finish and verify Platform Audio.**

Run the platform acceptance harness on all three supported OSes. Do not continue to provider integration until M1/M2 installation and loopback are recorded as passing.

- [ ] **Step 3: Finish and verify Provider Adapters.**

Run fixture tests for all adapters and the opt-in Volcengine test. Verify one real 2-second audio fixture produces final source text, final translation text, and TTS audio.

- [ ] **Step 4: Finish and verify Workbench And Intelligence.**

Run UI smoke, Rust, build, and app-screen inspection checks. Verify the UI stays usable with no provider, no index, no Tavily result, a disconnected device, and a failed summary.

