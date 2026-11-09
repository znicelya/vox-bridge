**I1 — ADDRESSED for the original late-page Swap and two-target Recovery defects.** Fresh Swap, Recovery and Ready publication now build the complete source-bound cohort in the pointer transaction, including terminal retries and zero-work inputs ([cohort construction][cohort-build], [fresh publication][fresh-publication]). The actual private persisted receipt must prove a fresh generation, exact stored count and complete dense staged/mapping coverage ([fresh coverage][fresh-coverage]). Completion resumes from retained certificates after document removal, checks files, acknowledges original work and then removes source SQL rows ([completion consumer][completion-consumer]). Retirement independently guards retained references ([repository guard][repository-guard]). The real automatic-reindex fixture admits its target beyond vector 199 while the handler is Queued/Running, checks both jobs succeed, survivor f32 bytes, work removal and reopen; two-target Recovery and pointer-crash fixtures also pass in Root's final gate ([automatic rebuild fixture][automatic-fixture], [two-target recovery fixture][two-fixture]). N1–N3 below leave the expanded completion/retention contract unmet.

**I2 — ADDRESSED.** The shared current-document coverage predicate requires every current chunk to have its matching published active-generation mapping ([coverage predicate][coverage]). Reconcile audits authoritative Ready documents in 200-row pages; classification, hydration and the final search recheck apply the same eligibility and version-fenced repair ([reconcile audit][coverage-audit], [classification][classification], [hydration][hydration], [final recheck][search-recheck]). The mixed one-chunk healthy/two-chunk broken fixtures exclude all broken hits, preserve exact healthy title/body, mark only the broken owner unavailable and perform zero reconcile embeddings ([fixtures][search-fixtures]).

**I3 — ADDRESSED.** The shared borrowed, 64 KiB-bounded job decoder checks canonical object UUID, document exclusion, envelope, kind/profile/domain input and payload fingerprint ([job decoder][job-decoder]). Reindex proves this association before state changes or embedding and carries it into publication; Delete proves the durable job before work execution ([reindex entry][reindex-entry], [delete entry][delete-entry], [Ready publication][ready-proof]). Startup uses bounded matching ownership rather than JSON extraction. Terminal repair writes its reason only after the same version's Indexing transition succeeds ([startup ownership][startup-proof], [terminal repair][terminal-repair]). The five regression tests include ten startup variants, wrong-object live Reindex/Delete, publication drift and stale terminal reasons ([fixtures][job-fixtures]); their actual GREEN and Root's final gate pass.

**I4 — ADDRESSED.** Complete work validation precedes even the mutable Open intent ([delete preflight][delete-preflight]). It checks the full target fence, canonical fields, contiguous/count-complete pages, exact prior published mapping and current-version chunk association; historical versions rely on their durable mappings ([row proof][work-read], [complete capture][work-capture]). Execution and publication recheck the retained proof, and acknowledgment deletes only its exact mapping/work association ([acknowledgment][work-ack]). First/later foreign-vector, later foreign-association/fence, missing-work, invalid resume and historical-chunk-absence fixtures preserve the target proof and healthy neighbor ([fixtures][work-fixtures]). The earlier partial GREEN filename contains failures; the subsequent seven-case resume GREEN and final frozen gate pass.

**I5 — ADDRESSED.** Classification bounds the authoritative title through the current Ready inverse owner before copying it and returns a local repair; hydration also isolates a failed title read ([title classification][title-check], [hydration][hydration]). The ASCII 32,769-byte and multibyte 32,769-byte cases preserve the healthy neighbor and exact title/body, report one unavailable owner and do not re-embed during reconcile ([title fixture][title-fixture]).

**M1 — ADDRESSED.** The actual SQLite/publication fixtures accept a valid fence padded to 1,024 bytes, reject 1,025-byte ASCII and UTF-8 updates, and separately inject oversized retained evidence with CHECKs restored before production publication. They require zero published mappings, Ready chunks, clean seals and active pointers, with all 241 work rows retained ([byte fixtures][byte-fixtures], [rollback assertion][byte-rollback]). Existing NULL/malformed/later-page and canonical location/previous-ID cases remain in the final passing suite.

**Spec: FAIL. Quality: NEEDS_FIXES.** Original findings: 5 Important and 1 Minor addressed for their original failure routes. New findings: **0 Critical, 3 Important, 0 Minor**. The passing gates establish the tested frozen implementation; the three additional routes below prevent approval of the complete accepted repair.

**N1 — Important: semantically equal duplicate fence JSON cannot finish work acknowledgment.**

Two valid same-source duplicate operations can retain the same full document/mapping proof with different bounded JSON whitespace. `delete_work::read` parses and compares the typed fence, while keeping its raw JSON; `page_hash` hashes those raw bytes ([decoder][work-read], [raw page digest][work-page-hash]). Each operation passes preflight and is separately anchored by publication, with no compatibility check on their shared association roots ([preflight][delete-preflight], [headers][cohort-build]). The positive M1 fixture explicitly accepts JSON padding through successful publication ([1,024-byte acceptance][byte-fixtures]); this is valid-record behavior.

Acknowledging the first duplicate removes their common mapping. The second can justify that missing mapping only through an acknowledged peer with both an equal shared root and an equal original page hash ([peer match][peer-match], [acknowledgment consumer][ack-consumer]). Whitespace makes both hashes differ, so it fails. Production processes the batch in one transaction, rolling back the batch if the second member fails ([batch acknowledgment][batch-ack], [batch transaction][batch-transaction]). On fresh replacement, physical source removal has already completed before this work phase ([physical/work ordering][physical-before-work]). Both domain deletions are committed, valid retained work remains, and repeated completion cannot repair this mismatch. Direct Delete also cannot complete the affected batch.

Requested repair, owner **Root/fix author**: distinguish an operation's raw evidence fingerprint from the semantic association authority used for duplicate mapping acknowledgment. Canonicalize the semantic full fence/row association, or provide an equivalent exact compatible peer proof. Preserve bounded valid JSON padding and the existing original-page integrity checks.

Required narrow regression: two genuine same-source duplicate delete operations; pad only one operation's retained `document_fence_json` with valid whitespace before execution/publication. Both must complete safely, with exact mapping/work acknowledgment, healthy survivor bits and no extra embeddings. Exercise Direct Delete and fresh replacement with a pointer/cleanup restart. Do not classify the padded record as malformed to avoid the required behavior.

**N2 — Important: queued startup consumption can collect a genuine stale file receipt before fresh path validation.**

Use a source-bound deletion that remains **Queued** after a fresh Swap has genuinely cleaned its certified files, acknowledged its work and removed the physical source and SQL seal. Its acknowledged certificate remains because the queued job cannot yet be consumed. Preserve that crash state, recreate one certified original final or temporary path while the application is stopped, and reopen.

The actual application registers the new Delete handler and invokes `jobs.reconcile()` during composition ([startup order][app-startup]). Startup interrupts a Queued row with `awaiting_reap=false`; its terminal hook calls the new `consume` path ([interruption][startup-interrupt], [Delete terminal hook][delete-terminal]). Fresh scheduler lifecycle authority correctly allows proof of prior terminal work ([fresh reap authority][fresh-reap]). `consume` then immediately calls `collect` ([consume/collect][terminal-collect]). Running startup interruption passes `awaiting_reap=true` and retains its proof; the reproduction requires the Queued member.

Collection verifies the anchored file receipt and SQL metadata absence, but performs no actual-path check ([certificate validation][certificate-files], [collection eligibility][collect]). The genuine old receipt still matches, every cohort member can be consumed, and fresh publication is collectible because `source_removed` is proved and the SQL source is absent. Collection atomically deletes the complete journal before any owned knowledge file validation. The recreated bytes have neither a managed-file row nor a remaining certificate, so subsequent reconciliation cannot find their original cleanup authority.

The existing terminal zero-work fixture forges flags without a genuine cleaned receipt ([terminal fixture][terminal-file-fixture]). The residual-path GC fixture retries through `reconcile_owned`, which runs fresh filesystem checks ([residual retry fixture][residual-fixture]). Neither exercises this startup terminal route. The startup ordering itself is unchanged; the repair's new terminal-to-GC attachment makes this an in-scope cross-task dependency.

Requested repair, owner **Root/fix author**: keep terminal SQL consumption separate from garbage collection. Collect only after current original-path absence is checked outside SQLite under the actual operation owner, or after equivalently current owner-issued authorization. Preserve the complete closed cohort across startup until that check runs.

Required narrow regression: retain a genuinely cleaned/acknowledged fresh-source certificate with the final member Queued and unconsumed, recreate its certified final or temporary path before a fresh-process startup, and run the actual `jobs.reconcile()` order. The full journal must survive with bytes present; subsequent owned cleanup must reject/retain it until the bytes are removed, then collect atomically without embeddings. Use controlled real worker occupation to establish Queued ownership where needed.

**N3 — Important: pre-publication file capture omits an independently owned historical native file.**

Normal native replacement retains versioned import jobs/file associations; each Import job durably names the document ([existing admission producer][native-history]). Start with two genuine native import versions for the same target. Change only the historical managed-file row's nullable `document_id` to NULL, preserving its original Import job, version, generated path and bytes. The current managed path and current import descriptor remain valid. This localized mutation is allowed by the existing schema ([nullable owner/history schema][native-schema]).

Preflight and publication both call `files::capture`, whose enumeration only selects `knowledge_managed_files.document_id=target` ([capture enumeration][file-capture]). Import validation and the no-sibling check run only for those captured descriptors, so they never inspect the omitted historical job/file ([descriptor validation][file-import-check]). The captured current path satisfies its completeness check. Publication anchors this incomplete file set and deletes the target/import rows; its pending-state update also selects only `document_id=target` ([domain deletion][domain-delete]).

Only subsequent cleanup runs the broader inverse owner query that also finds files whose original Import job's `object_id` names the target ([inverse metadata set][inverse-file-set]). It then rejects the unlisted historical file, after domain deletion and publication have already committed. The old bytes and row survive, but the immutable certificate has no original descriptor for them, so normal document-gone completion cannot finish without repairing the lost proof. The existing additional-original-import-job fixtures insert their extra row at `AfterDeletePublication`, so their passing results cover a later boundary ([current fixtures][additional-file-fixture]).

Requested repair, owner **Root/fix author**: prove the complete inverse original-owner set before mutable deletion execution and again in the publication transaction. Include direct document ownership, current import ownership and historical original Import associations, and reject an unaccounted or changed association before certifying/deleting the domain row. Keep historical-version support, native path codec limits, 100-descriptor pages and the existing cardinality bound.

Required narrow regression: create the two import versions through normal native admission, null only the historical file's document link before preflight, and invoke the real deletion handler. Require failure before domain/publication/source mutation, retained target/work/file/import evidence, intact healthy neighbors and zero recovery embeddings. Also exercise the same pre-publication state during fresh Swap or Recovery to cover the cohort producer.

**Frozen source and verification attribution.**

This review read all **10,160 lines** of the complete repair package, covering all **42 source paths**, including the new untracked modules represented by the immutable tree. It used the binding repair plan, full accepted deletion ruling, complete round-1 brief, author handback, post-full-gate repair handback, frozen manifests and raw logs. It did not substitute a HEAD-only diff or reopen the original 14,033-line review package.

| Frozen identity | Value |
| --- | --- |
| Fix BASE and unchanged HEAD | `b72556091fc533dafb0a336907bf077fec377615` |
| Reviewed source tree | `da23738987e178f23b1f5bda34c9c8250d5f4f4e` |
| Complete package | 402,039 bytes / 10,160 lines / 42 source paths |
| Package SHA256 | `8f4212dc633a694f665cb3f5673a8988b294be7566bb992a3a2520d7ebcbee66` |
| Protected staged/worktree `.gitignore` SHA256 | `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a` |

Independent read-only hashing checked every frozen source and input, every source Git blob, the exact tree changed-path set, package bytes/hash and all five final gate-log hashes. The 17 recovered archive items (15 raw logs) and 47 Task4 archive items also match their recorded byte/hash identities; Task4 original paths match their archived counterparts. HEAD remains BASE and `.gitignore` remains the sole staged path. This report is the reviewer's only written file. No reviewer Cargo/test run, source edit, Git mutation, database/index open, service/network/native action or agent spawn was performed.

Root's final frozen verification records these actual results ([final verification][final-verification], [all-targets raw log][final-all-targets]):

| Required gate | Recorded result |
| --- | --- |
| Offline locked all-targets, default test concurrency | Exit 0; 748 library passes plus 5, 6, 2 and 3 passes in other targets; one existing paid opt-in test ignored |
| Production library check | Exit 0 |
| Desktop build | Exit 0 |
| Rustfmt | Exit 0 |
| Frozen diff check | Exit 0 |

The final five logs contain no compiler warnings. The earlier tree `3afb692d013bbc8e4830b130f504d18fa8f63e33` and its 745-pass/3-failure full run remain separately archived as failed evidence. They are not the current verified source.

**Concrete focused evidence inspected.** Counts below overlap; they are not additive. Earlier author runs commonly used one test thread. Current completion attribution comes from Root's default-concurrency final frozen gate.

| Behavior | Actual raw RED/GREEN evidence |
| --- | --- |
| I2/I5 coverage/title isolation | [Search RED][search-red]: 0 passed/4 failed, including two hits instead of one, Ready instead of RepairRequired and title ResourceLimit. [Search GREEN][search-green]: all four pass. |
| I3 durable association | [Jobs RED][jobs-red]: 4 passed/5 failed, with foreign operations succeeding, invalid startup owners preserved and stale reason overwritten. [Jobs GREEN][jobs-green]: all nine pass, including the prior search group. |
| I4 complete work/resume | [Work RED][work-red]: 1 passed/5 failed; one failure is the initial foreign-FK fixture setup. Corrected [association RED][association-red] proves a mutable Open intent appeared before rejection. The misleading [work GREEN][work-partial-green] still fails four later-page retention assertions. [Resume GREEN][work-green]: all seven pass. |
| M1 raw bytes | [Byte-boundary log][bytes-green]: all three SQLite/publication boundary cases pass. |
| I1 source cohorts/fresh exclusion | [Original replacement RED][replacement-red]: 19 passed/4 failed, including later-page consumer failure, retained same-source work FK loss and incomplete pointer-crash cohort. [Task4 runtime RED][runtime-red]: 20 passed/5 failed. [Protocol GREEN][protocol-green]: 28 pass; [connected replacement GREEN][replacement-green]: 24 pass. |
| Original file ownership/path guards | [File-owner RED][file-red]: 2 passed/4 failed, including successful incomplete cleanup and ignored full-path drift. [Consumer GREEN][file-green]: all six pass. [Path/flag RED][file-flags-red]: all three fail at actual-byte/terminal-retention assertions; these cases pass in the final group. |
| Actual reap authority | [Waiter RED][waiter-red]: terminal-but-unjoined collection fails retention. The misleading [waiter GREEN][waiter-failed-green] still fails with IndexRepairRequired. [Verified waiter][waiter-green]: passes. [Producer GREEN][reap-green]: seven pass; [shutdown-window RED][shutdown-red] fails completion-after-slot ordering, followed by [quiescence GREEN][quiescence-green] with nine passes. The complete [scheduler log][scheduler-green] has all 29 passes, including the unfinished-factory case. |
| Settled history/atomic GC | [No-replay RED][replay-red]: 2 passed/1 failed; [GREEN][replay-green]: all three pass. [GC edge RED][gc-edge-red] catches reversed removal-flag/receipt disagreement; [retry RED][gc-retry-red] then catches missing closed-cohort retry. [Retry GREEN][gc-green]: all five pass. The final knowledge group also passes the later original-temporary-path extension. |

The [interrupted direct-peer GREEN filename][interrupted-green] ends at exit 130 with no completed test-result row. It is not passing evidence. The final author knowledge/scheduler runs record 106/29 passes, and the later post-full-gate default-parallel knowledge/document runs record 106/5 passes ([default knowledge][default-knowledge-green], [formatted documents][documents-green]).

**Post-full-gate repair assessment.** The ordinary no-certificate cleanup now resolves its strict managed directory only after finding a certificate, preserving the existing public Storage and durable RepairRequired/Storage symlink outcome ([guard][managed-guard]). The diagnostic raw RED actually records IndexRepairRequired with Failed/Cancelled; the five-case document GREEN retains the original assertions ([diagnostic RED][documents-red], [formatted GREEN][documents-green]). The binding fixture occupies a genuine first-scheduler worker before starting the second Core, proves one embedding entry in each, cancels the spare and preserves the atomic `(active, ready)=(1,1)` winner check ([binding fixture][binding-fixture]). The shared observer subscribes before its first authoritative read, rereads status after notifications/lag and has a finite 60-second final-resync bound ([observer][wait-harness]). Diagnostic default-parallel RED measured the original cohort at 3,520 SQL reads/15.23-second observer expiry with actual Succeeded completion at 16.04 seconds; the repaired default-parallel log observes completion with two reads. No test is added/ignored, durable assertion weakened, production capacity/deadline changed or serial-only success substituted.

The audited producer/consumer composition registers claims before Running visibility with rollback ownership, pre-reserves continuation before permanent factory closure, releases process then slot then child completion, bounds/prunes genuine live frames and waits for final completion publication before generation join ([claim registration][claim-registration], [continuation/end][handler-end], [child ownership][child-ownership], [reap registry][reap-registry]). Consumption is independently bound to the immutable header and actual reap evidence ([consumption receipt][consumption-receipt]); closed-cohort SQL deletion is atomic. These repairs address the measured lifetime and partial-GC failures. N2 concerns current filesystem authority at the terminal GC boundary.

**Out-of-scope observations and verification limits.**

The two-scheduler binding fixture shares a store to test publication competition. The actual-reap ledger proves claims issued by its own scheduler; the current production composition constructs one scheduler under the application process lock ([composition][app-startup]). No unsupported multi-scheduler production deletion claim is included as a finding. Extending that ownership model would require a separate audited lifecycle contract from Root.

**Cannot Verify — runtime RED/GREEN for N1–N3.** The exact counterexamples are absent from the supplied runs, and this reviewer was explicitly read-only. Root/fix author owns the narrow regressions specified above, their repairs, a new frozen source/package and required final gates before re-review. The findings are based on the fully traced current producers, transactions and consumers; no reviewer reproduction is claimed.

Minimum-OS audio routes, actual paid-service acceptance, signing/packages and Linux native utility behavior remain the existing external release gates owned by Root/release validation. This scoped repair review supplies no additional acceptance evidence for them.

[cohort-build]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/publication.rs:249
[fresh-publication]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/publication.rs:202
[fresh-coverage]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/publication.rs:648
[completion-consumer]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/completion.rs:124
[repository-guard]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/repository.rs:46
[automatic-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_replacement.rs:166
[two-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_replacement.rs:243
[coverage]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:146
[coverage-audit]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/reconcile.rs:169
[classification]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:404
[hydration]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/search.rs:349
[search-recheck]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/search.rs:199
[search-fixtures]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_fixes.rs:4
[job-decoder]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/job_proof.rs:51
[reindex-entry]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/reindex.rs:16
[delete-entry]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:210
[ready-proof]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/publication.rs:66
[startup-proof]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/job_proof.rs:181
[terminal-repair]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service.rs:267
[job-fixtures]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_jobs.rs:44
[delete-preflight]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/publication.rs:142
[work-read]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_work.rs:38
[work-capture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_work.rs:255
[work-ack]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_work.rs:127
[work-fixtures]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_deletion.rs:48
[title-check]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/state.rs:408
[title-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_fixes.rs:94
[byte-fixtures]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/publication.rs:187
[byte-rollback]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/publication.rs:269
[work-page-hash]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_work.rs:238
[peer-match]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:133
[ack-consumer]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:504
[batch-ack]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:444
[batch-transaction]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/completion.rs:120
[physical-before-work]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/completion.rs:176
[app-startup]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/app/mod.rs:225
[startup-interrupt]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs.rs:867
[delete-terminal]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/service/delete.rs:179
[fresh-reap]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs/reap.rs:247
[terminal-collect]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:555
[certificate-files]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/journal.rs:485
[collect]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:600
[terminal-file-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_replacement/flags.rs:111
[residual-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_replacement/settlement.rs:420
[native-history]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/documents/admission.rs:35
[native-schema]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/migrations/0004_knowledge.sql:76
[file-capture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/files.rs:210
[file-import-check]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/files.rs:82
[domain-delete]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/publication.rs:446
[inverse-file-set]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/files.rs:382
[additional-file-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/review_replacement/files.rs:209
[final-verification]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/final-round1/final-verification.json
[final-all-targets]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/final-round1/final-all-targets.log
[search-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-search-red.log
[search-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-search-green.log
[jobs-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-jobs-red.log
[jobs-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-jobs-green.log
[work-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-delete-work-red-all.log
[association-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-delete-association-red.log
[work-partial-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-delete-work-green.log
[work-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-delete-work-resume-green.log
[bytes-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-evidence-bytes.log
[replacement-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-5c-repair-replacement-red.log
[runtime-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-replacement-runtime-red.log
[protocol-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-protocol-green-check.log
[replacement-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-replacement-connected-green.log
[file-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-file-owner-canonical-red.log
[file-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-file-owner-consumers-green.log
[file-flags-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-file-paths-and-flags-red.log
[waiter-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-terminal-reap-waiter-red.log
[waiter-failed-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-terminal-reap-waiter-green.log
[waiter-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-terminal-reap-waiter-verified.log
[reap-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-reap-producer-green.log
[shutdown-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-reap-shutdown-window-red.log
[quiescence-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-reap-quiescence-green.log
[scheduler-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-final-scheduler-focused.log
[replay-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-settlement-no-replay-red.log
[replay-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-settlement-no-replay-green.log
[gc-edge-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-settlement-gc-edge-red.log
[gc-retry-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-settlement-gc-retry-red.log
[gc-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-settlement-gc-retry-green.log
[interrupted-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/task-4-direct-peers-batched-green.log
[default-knowledge-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/final-repair-knowledge-default-parallel-green.log
[documents-green]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/final-repair-documents-formatted-green.log
[documents-red]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-knowledge-review-fixes/final-repair-documents-trace-red.log
[managed-guard]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/files.rs:471
[binding-fixture]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests/binding.rs:136
[wait-harness]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index_tests.rs:157
[claim-registration]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs.rs:795
[handler-end]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs.rs:684
[child-ownership]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs/child_operations.rs:17
[reap-registry]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/intelligence/jobs/reap.rs:18
[consumption-receipt]: /mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/src-tauri/src/knowledge/index/delete_completion/retirement.rs:25
