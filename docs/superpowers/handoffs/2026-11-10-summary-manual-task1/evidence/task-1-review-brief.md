# Manual summary Task1 — prepared independent task review

Activation: Root dispatches only after explicit Task1 source/focused-Cargo
handback, a new complete isolated freeze and all required final gates exit0.
This file is preparation only and authorizes no early review or source work.

Worktree: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation
Accepted BASE: 73760d609a050eaddcd8002cec572fbc73c65164.
Frozen tree/package/source/input identities come from
final-manual-task1/source-freeze.json. This is one complete manual-task review;
automatic delivery and joined audio close belong to Task2.

Read first task-1-brief.md, the same full requirements supplied to the author.
Read task-1-report.md and task-1-execution-context.md. The latter points at the
exact manual checkpoint sections and binding interface amendments. Read the
complete final-manual-task1/manual-task1-review.diff in bounded passes,
including all new modules/migrations/fixtures in the immutable tree. It spans
the accepted BASE to the exact reviewed tree; current HEAD or HEAD~1 is not
an alternative range. Read final-manual-task1/final-verification.json and the
actual covering author logs; counts overlap and API compile failures are
separate from runtime behavioral RED. Verify report claims against the diff.

Concrete cross-task risks include the reused actual-reap producer and terminal
transaction/control ordering, owned ConfigIo and HTTP callers, application
startup/close/manual command composition, existing legacy version writers,
and export consumers. Inspect unchanged code only for a named concrete risk
and cite the focused producer/consumer check. If supplied evidence cannot
verify a specific requirement, return the exact narrow concern/test to Root.
No repeated broad suite or fresh review of the accepted knowledge repair.

Check admitted raw-payload witness and trusted-ID domain authority at every
specified effect, rollback/counter/permanent lookup order, actual entered
attempt/snapshot/resolver ownership and local-ready/reap deletion recovery.
Check the fixed once-normalized snapshot, complete Unicode part manifest,
actual escaped request budget, map/reduce coverage and complete root; strict
bounded Native/Legacy projection, signed exact wire cursors, complete safe
exports, and all generic HTTP callers affected by the opt-in LLM policy.
Use the exact limits and required cases from the brief and constraints below.
Do not infer a proof merely from the chosen mechanism or report rationale.

Return both Spec and Quality verdicts, Critical/Important/Minor counts and
file:line evidence for each finding. Mark genuinely unverifiable unchanged
requirements explicitly; Root must resolve them before task completion.
Report only this task and introduced breakage. Maintainability and honest
behavioral test evidence are part of quality. Write only
docs/superpowers/reviews/2026-11-09-summary-manual-task1.md in the worktree.
No Rust/Cargo/database/SrvDB/staging/refs/network/native/keyring/driver/global
writes and no agents. Root owns all heavy validation. Return report path,
SHA256, both verdicts/counts and an explicit no-pending-writes/processes
handoff. Root archives the report before any authorized source integration.

## Binding Global Constraints (verbatim)

# Binding Global Constraints (verbatim)



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
- Keep the owner queue at 16, exactly two joined Rayon workers, at most two engines, 200-record pages, 200,000 stored records and the accepted 12 GiB watermark. Retain ownership until actual work and readers end. Never open a dirty source with SrvDB or add an embedding call to local recovery.
- Preserve the pre-existing staged `.gitignore` and its SHA256 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`. Root owns source freeze, serialized Cargo, final review and integration.

- Summary metadata belongs to migration6; research belongs to migration7. Existing legacy signed versions/status/seconds remain readable without fabricated native fields.
- Each Native version stores the SHA-256 of its actual admitted bounded raw job payload in a backend-only `job_payload_hash`; Legacy keeps NULL. Fresh domain checks precede snapshot, every model call, every response/root publication and terminal status copy. This witness stays outside DTOs and exports.
- Snapshot≤100000 final rows/16MiB once-normalized JSON; raw metadata parsing separately≤16MiB; acquisition/scan≤5s under an actual owner; release SQLite before HTTP.
- Every request's actual escaped system/user UTF-8 bytes+1024≤context-output. Full manifest before calls; ≤64 calls/≤8 levels; no omitted row/part/child or successful prefix.
- LLM request scopes4; request≤1MiB, HTTP envelope≤8MiB, content≤2MiB, response total≤32MiB/summary, no automatic HTTP retry. Raw and actually supplied map coverage are retained before strict parsing.
- Native content uses the seven required fields, summary≤20000 Unicode scalars, other strings≤2000, arrays≤100; strict bounded deserializers reject unknown/type/limit violations.
- Views≤100/default50 rows, native row≤2MiB+64KiB, legacy content≤2MiB, status≤64KiB, page≤8MiB; cursor canonical signed i64 decimal≤20 bytes. Legacy JSON depth64/nodes100000/decoded string+key bytes≤2MiB.
- Complete summary export projection≤32MiB; rendered artifact≤128MiB. Any completeness/size failure preserves destination. Keys/audio/raw responses stay outside export.
- Automatic delivery worker1/channel32/admission-child1/keyset page32/rescan200ms. Busy/closed/uncertain restart is local retry with no paid replay. Deletion actual-attempt registry16/wait≤10s; delivery close observation≤5s; whole close retains its30s observation fence.
- Window-close audio proof covers two actual stream/open parents and one actual inventory parent, independent of endpoint lease policy. Each Pulse stream owns two stream children and at most two shared association helpers; inventory owns at most three helpers, total OS children≤11. Slots remain held until actual join/reap and same-epoch publication end. Normal terminal notification keeps the existing bounded runtime cleanup/stop order; Failed sessions with retained finals remain eligible.

