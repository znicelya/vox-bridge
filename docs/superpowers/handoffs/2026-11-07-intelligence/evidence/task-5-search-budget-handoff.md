# Future Task 5B search hydration/result budget

Read-only consumed-contract preparation atBASE735284125838e9c1b0fce9a8a4c002482dff281e,2026-11-07. Future checkpoint proposal, not accepted behavior/5Averdict. No source/Cargo/parser-test/native/network/index-open work.

**Correction and exact bounds.** My earlier8MiB location/heading assumption incorrectly applied normalized-body scalar limits to rawheading. parser_worker.rs175 preserves line.trim_start_matches('#').trim() inside heading; Blocks::push normalizes only text (:28) and clones heading untouched (:43). protocol::validate_blocks counts only text scalars (:81–83). normalize preserves tabs, collapses runs of spaces (:57–69).

Static literal/compact-JSON calculations (page:null,paragraph:1,start:0):

- "# a"+9MiB internal spaces+"b": source9437188B; rawheading9437186B; normalizedtext "# a b",5scalars; canonical locationJSON9437254B. Worker response remains<16MiB; one derived chunk<32MiB.
- Rawheading8387608B (=8MiB−1000), one contiguous1500-tab segment plus one spaces segment: normalizedbody1504scalars, one chunk; locationJSON8389179B (>8MiB); compact worker response8392235B (<16MiB); raw text+heading8389112B before small struct storage (<32MiB).

The proposed7MiB pure-tabs counterexample is invalid: preserved tabs exceed2m body scalars. No parser policy change or measurement claimed. Future per-row canonical locationJSON cap16MiB and heading rawUTF8 wire cap16MiB preserve consumed legal cases; existing bodytext wire8MiB remains unchanged.20legal9MiB headings already repeat180MiB;<=100prefetch amplifies further. Count limits alone do not bound hydration/IPC.

**Smallest result design.** Keep<=100compact IDs/scores/reference metadata; validate active identity/generation, published mapping, currentReady/version/hash, inline fields and full canonical-location digest. Hydrate one row through bounded borrowed ValueRef before RustString copies/deserialization: title<=32768UTF8bytes, chunk<=2400scalars/9600UTF8bytes, complete locationJSON<=16MiB. Preserve exact rawheading/text; invalid rows use local repair policy. SQL internal memory is outside this copy bound.

Propose32MiB compact **serialized KnowledgeResults**, all fields/punctuation included. Checked count-only io::Write+serde_json::to_writer on borrowed hits/envelope measures escapes/numbers; cache per-hit counts and checked-add commas/envelope before prefix admission/copy. RawUTF8 sums are insufficient: four tab-variant headings fit32MiB raw but their JSON alone exceeds it. Keep accepted prefix plus one bounded candidate and cap final writer; recheck changed unavailable count, removing only trailing whole hits if needed. Stop at first eligible overflow; preserve score prefix/<=requestedlimit without filling from lower scores. Budget shortening invents no repair/unavailable state. Defensive one-valid-hit failure is ResourceLimit; empty/unhealthy-library semantics remain separate.

**IPC/DTO.** DesktopPort invokesTauri and invokeDecoded validates after allocation;10s is observation only. wireSchema has per-fieldUTF8/count checks, no knowledge aggregate; opaque8KiB is diagnostic-only (desktop.ts:10–15,37–43;wire.ts:34–39,70–73,380). Backend cap precedes serialization; future wire adds aggregate guard, with no JSON.stringify/Rust-float byte-equivalence assumption.32MiB bounds result body, not Tauri-envelope/UTF16 heap/RSS/native performance.

identity:String cannot honestly express truly empty/unbound success with no profile or dimensions=None. Propose identity:Option<String>: None for empty/unbound,Some64hex for bound indexed results; no empty-string/discovery-fingerprint substitute. Requires checkpoint acceptance.

Later evidence: full6000CJK/rawspace/tab fidelity; exact escaped/body/envelope boundary; incremental hydration/prefix order; one-hit failure; late readiness invalidation; aggregate decoder. No source/DTO/UI change now.
