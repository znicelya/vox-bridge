# Task5C repair round2 — scoped independent re-review

Activation: Root dispatches only after explicit round2 source/Cargo handback,
new immutable freeze and all five required final-round2 gates exit0. This
prepared brief does not authorize early review or imply passing tests.

Worktree: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation
Unchanged commit BASE: b72556091fc533dafb0a336907bf077fec377615.
Previous independently reviewed source: da23738987e178f23b1f5bda34c9c8250d5f4f4e.
The new exact tree/paths/input hashes are in final-round2/source-freeze.json.
This is fix round2/5 of the same Task5C repair, not a whole-branch review.
The original reviewer has no source authorship.

## Read these inputs

- Requirements: round2-repair-brief.md beside this file, and binding repair plan
  docs/superpowers/plans/2026-11-09-knowledge-review-fixes.md.
- Binding deletion contract: docs/superpowers/reviews/2026-11-09-delete-proof-ruling.md.
- Findings verbatim: docs/superpowers/reviews/2026-11-09-task5c-repair.md (SHA256
  1f467d5e6010a3e11f86ae2c0538b6da0883ab91f1f134210268c78242431dd5).
  Re-review N1, N2 and N3 individually. I1–I5/M1 were addressed for their original
  routes; check new breakage in the round2 source delta rather than reopening
  untouched earlier code as a new review wave.
- Author report: round2-repair-report.md, with exact behavioral RED/GREEN and
  final handback. Counts overlap; a passing pre-existing guard is not a new RED.
- Complete SCOPED package: final-round2/scoped-round2-review.diff. Read all of
  it in bounded passes, including new untracked test modules represented by the
  immutable tree. This range is previous reviewed tree→new tree, not HEAD→HEAD
  or the last commit. The separate complete-repair-review.diff is archived for
  source attribution; do not duplicate the completed prior review.
- Freeze and logs: final-round2/source-freeze.json and final-verification.json.
  Hash the exact sources/Git blobs/inputs/packages and inspect actual five final
  result/exit/warning rows. Root has already run the gates on this source;
  do not rerun unchanged suites. Original failed full gates and prior reviews
  remain immutable evidence, not substitutes for current results.

## Review scope

Return ADDRESSED/NOT ADDRESSED for N1–N3, both Spec and Quality verdicts,
and new Critical/Important/Minor breakage introduced by this scoped delta.
Assess the genuine producer paths and assertions in the new regressions:
normal duplicate delete admissions, valid padded raw evidence, pointer/cleanup
restart, actual Queued worker occupation and SQLite crash snapshot/native
receipts, new handler registration then jobs.reconcile(), recreated original
final/temporary paths, and two normally admitted native versions whose only
pre-publication drift is the historical managed-file document link.
Do not treat a chosen mechanism as proof that its authority is sufficient.

The author report also names two actual fixture/timing dependencies. Read the
original zero-work Swap failure and its new real owned completion call before
the unchanged absence assertion: terminal consumption now precedes a later
owned filesystem GC check. Read the N3 same-owner Busy diagnostic and joined
restart recovery: the rejected unpublished destination is retained in Awaiting
under the unchanged two-engine policy. Assess the complete captured rejection
state and actual joined recovery, not only the final result. These are supplied
concerns for verification, not instructions to dismiss or limit a finding.

Check full canonical semantic associations independently of each operation's
immutable raw evidence; a present conflicting mapping still rejects. Check
that SQL terminal consumption cannot mint current filesystem GC authority,
that current owner-issued absence is tied to the exact complete publication
cohort and is revalidated in atomic GC, and that complete inverse native owner
validation occurs before mutation and again at fresh publication. Preserve
historical paths/versions, page/count/byte limits, actual ownership and local
zero-embedding recovery. Check new authority/proof parsing rejects missing or
changed retained evidence rather than silently certifying a prefix.

Read-only: no Rust/Cargo/source/SQL database/SrvDB/staging/ref/commit/branch,
network/paid/native/keyring/audio/driver/global actions and no agents.
Write only docs/superpowers/reviews/2026-11-09-task5c-repair-round2.md.
If supplied evidence does not answer one concrete concern, state the exact
narrow behavior/test to Root. Identify real unchanged cross-task dependencies
separately; do not silently discard them or extend scope without a reason.
Return only verdicts, counts, report path/SHA256, no pending writes/processes.

## Root contract reconciliation


N1: Keep the original raw page fingerprint immutable for each operation.
Equivalent bounded typed fence JSON may have different raw whitespace and
must remain accepted. Duplicate acknowledgment therefore needs a separately
anchored semantic association authority (or equivalent exact compatible peer
proof); changing raw bytes after their certification must still reject.
The old phrase "identical original page" refers to canonical complete row
associations for peer compatibility, while each operation still validates its
own original raw evidence. Preserve every document, version, identity,
generation, ordinal, vector, chunk, content hash and full typed fence boundary.

N2: A terminal SQLite hook can record actual-reap consumption, but cannot
authorize filesystem GC from a prior cleaned receipt. GC must follow a current
original-path absence check performed outside SQLite under actual owned work,
then revalidate the complete closed cohort atomically. Startup `jobs.reconcile()`
must preserve the certificate until this owned check. A genuinely recreated
original final or temporary path must be retained and rejected until removed.
Preserve eventual local collection once actual cleanup is complete.

N3: Preflight and publication must account for the complete inverse original
file-owner set: direct document ownership, current import ownership and
historical original Import associations. A historical file whose document link
changes to NULL must fail before destructive source/domain/publication work;
do not synthesize a corrected owner or postpone its discovery until cleanup.
Native admission/version history, generated paths, import rows, healthy neighbors
and all original deletion work remain intact after rejection.


## Binding Global Constraints (verbatim)



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


