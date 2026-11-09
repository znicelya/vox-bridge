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

