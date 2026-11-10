只读 close-interruption 调查已完成。不可变候选中存在一个可证明的正常 stop 误判路径：supervisor 在 stop future 返回 Pending 后可能获取原 worker 的正常真实 join，却无条件设置 failed；全部真实 drain 成功后仍提前返回，完成通道 EOF 被映射为 Interrupted。Repair034 确认了 scheduler 完成通道 EOF，但该日志未保存首个 worker join receipt，不能排他断定该次运行命中了这个窗口。

本报告基于 [brief.md](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/brief.md)、[inputs.json](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/inputs.json)、完整 [source.diff](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/source.diff) 和全部十一份 candidate 快照。基线 TREE 为 `d1dd2c7aa27501ac817c24f3ada28778dfd1cb86`。这是根因调查及最小修复建议；Spec/Quality acceptance verdict 未执行，brief 明确限定 “No approval verdict”。以下源码缺陷结论不评定 Root 后续修改，也不替代稍后的 scoped repair review。

**可证明的源码失败路径**

[candidate jobs.rs](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/intelligence/jobs.rs:315) 中，generation 保留两个原 worker 的 JoinSet、两个独立 SqlSlot 和唯一完成 sender。347 行的 select 使用 biased，先 poll cancelled，再 poll workers.join_next。350 行丢弃了首个实际 join 结果；351 行无条件设置 failed=true。

精确 unchanged producer `d1dd2c7:src-tauri/src/intelligence/http.rs:308` 的 cancelled 先检查 watch 值，再 await changed；值为 false 且未发生变化时可以返回 Pending。worker 的 691 行在收到 stop 后正常返回 Ok(())。三个涉及间歇 close 的夹具使用多线程 Tokio runtime：subscription retry 是四线程，issued Ready 和 knowledge replacement 各为两线程，因此另一个执行线程能在 supervisor 两次分支 poll 之间完成 stop/worker 退出。

accepted Cargo.lock 锁定 Tokio 1.54.1。额外只读核查了该缓存的 [select macro](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/toolchain/cargo-writable-20261106/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.54.1/src/macros/select.rs:674)：biased 的起点为 0；680 行逐分支 poll；712 行遇到 Pending 后继续后续分支；733 行在后续分支 Ready 时直接返回。biased 不会在同一次 poll 内重新检查先前已 Pending 的 stop 分支。

因此以下执行顺序成立，不需要数据库异常、缺失 frame 或异常 JoinError：

1. supervisor 的 cancelled 实际 poll 看到 false，注册观察并返回 Pending。
2. supervisor 尚未 poll join_next 时，另一个执行线程对同一 generation 发送 stop=true。
3. 至少一个原 idle worker 收到 stop，实际结束为 Ok(())，其 JoinSet receipt 变为 Ready。
4. supervisor 在同一次 select 评估中获取 Some(Ok(Ok(())))，但 351 行仍设置 failed=true。
5. 其余 worker 372 行真实 join、两个 SQL slot 375 行 drain、maintenance 378 行 join/drain、handler cleanup、children drain 和 reap.quiesce 均可成功。
6. 390 行因错误的 failed 标志 return，397 行成功通知未执行；done sender 被丢弃，shutdown 的 600 行把 changed 的 EOF 映射成 Intelligence(Interrupted)。

正常 stop 因而可以被永久留在失败 generation：该 generation 未发布成功、未调用 reap.joined；重复 shutdown 继续看到完成 sender EOF，App 无法把这次关闭标成成功。这里证明的是错误的失败分类和关闭活性缺陷，没有证据证明此路径丢失了已经进入的 native/child/SQL 所有权。它经过原有真实 drain 才返回，不能把通用错误文本解释成实际任务仍在运行或已经丢失。

**原始运行证据与归因边界**

下表只描述各记录实际覆盖的结果，不以 focused pass 推断全量或未控制窗口已通过。

| 运行 | 原始结果与 close 相关证据 |
| --- | --- |
| 018-all-targets | 898 pass / 2 fail。[log:913](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../018-all-targets.log:913) 的 subscription deletion retry 在 App close 返回 Internal("Intelligence work was interrupted.")；另一个失败为 document rollback event 断言。没有 scheduler result 或 close branch。 |
| 022-full-gate-failures-focused | 1 pass / 1 fail。[log:10](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../022-full-gate-failures-focused.log:10) 的 subscription case 通过；document rollback 仍失败。 |
| 024-publication-close-diagnostic | 1 pass / 1 fail。[log:11](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../024-publication-close-diagnostic.log:11) 的 subscription case 通过；document 诊断捕获旧 Import Succeeded event。该 event 不是 scheduler close EOF 的证据。 |
| 025-document-subscription-gate | 17 pass。[log:21](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../025-document-subscription-gate.log:21) 的 branch 1 是有意制造的 subscription admission timeout；retry close case 通过。 |
| 026-diagnostic-all-targets | 899 pass / 1 fail。两个历史 close case 均通过；[log:913](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../026-diagnostic-all-targets.log:913) 的唯一失败在 summary fixture wait，显示 Elapsed(())，不是 shutdown 内层 EOF。 |
| 027-raw-capacity-isolated | 同一 summary fixture 隔离运行 1 pass，测试耗时 4.27s，见 [log:7](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../027-raw-capacity-isolated.log:7)。这不证明产品 close 截止期限或全量执行时序。 |
| 029-final-affected-ownership-gate | 223 pass；两个历史 App close case 通过。[log:208](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../029-final-affected-ownership-gate.log:208) 的 injected actual SQL panic 属于通过的异常 join 回归，不应归入另一运行的失败原因。 |
| 030-final-all-targets | 899 pass / 1 fail。[log:913](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../030-final-all-targets.log:913) 的 cancelled issued Ready case 在 App close 返回同一 Internal 文本；subscription case 通过。没有失败 branch 或 scheduler result。 |
| 034-close-branch-all-targets | 899 pass / 1 fail；两个历史 App close case 通过。[log:913](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/../034-close-branch-all-targets.log:913) 的 knowledge automatic replacement case 在 scheduler.shutdown 失败；916 行明确为 Ok(Err(Intelligence(Interrupted)))。外层 runner 未超时。 |

034 的 command 是 `cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --all-targets`，exit 101，runner elapsed 173.250848s，test elapsed 95.26s，600s outer timeout 未触发。其 before/after 各 299 个 source entries 完全一致，十一份 candidate 均与两侧记录相同。其 source attribution 可以落到被调查的 scheduler 快照。

[shutdown observation](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/intelligence/jobs.rs:592) 内层只有成功 watch 值与 changed EOF 两种结果；Ok(Err(...)) 确认内层 EOF，实际 scheduler 观察 deadline 到期会显示 Err(Elapsed)。不能因为最终枚举同为 Interrupted 而把这两种情况混为一谈。034 的 job observer 显示两个 durable Succeeded 结果，只说明工作观察到了终态，不证明 worker、SQL、handler child 或 generation 已完成真实 join。

精确 unchanged failing producer 为 `d1dd2c7:src-tauri/src/knowledge/index_tests/review_replacement.rs:166`：fixture 释放 BeforeLocalCopy checkpoint、等待 reindex/delete 成功、验证向量和搜索后，215 行调用 close；`d1dd2c7:src-tauri/src/knowledge/index_tests.rs:239` 的 close 先 await jobs.shutdown，再 index.shutdown。034 因此前者失败，不能归因到后者，也不经过 App 或 summary delivery close。

[App close branches](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/app/lifecycle.rs:219) 把 jobs/probes/subscriptions/summaries/delivery 分别记录；249 行传播 jobs 错误。405 行统一转换为文本，424 行重新构造 Internal。因此 018/030 的通用 App 文本本身不区分 scheduler 和 delivery。delivery 的 [real joins and outcomes](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/summary/terminal.rs:255) 保留 genuine worker/closer handles，1220 行 close timeout 映射 Timeout，1273 行 observed 可能因 sender EOF 返回 Interrupted，正常 worker 内部错误也保留独立结果。历史日志没有对应分支，不能据此排除这些其他来源。

session observer 的 [actual runtime join](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/session/orchestrator.rs:586) 先 join_actual，再做 terminal SQL/diagnostic 和 SqlSlot drain；678 行只有真实 runtime/SQL joins 都成功才释放 actual owner。[runtime handle slots](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/session/runtime.rs:75) 保留两个 actor slots，并用借用的真实 handles join。其 session/cleanup 错误文本与 034 的 direct scheduler EOF 分开处理，未观察到可以独立解释 034 的 session 错误证据。

026 的 32 MiB raw-response fixture 的旧 5s guard，与当前 [fixture timeout parameter](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/summary/tests/harness.rs:194) 的有限测试观察分开；特定重夹具现已按 brief 调到 30s。本调查不建议延长任何 scheduler、App、delivery 或产品 deadline。

**仍缺少的具体诊断**

034 确认成功 completion 未发布，但 supervisor 提前返回的来源仍被抹掉。350 行没有保留 first joined receipt；372、375、379、380 行只聚合布尔 failed；387 行 reap.quiesce error、390 行 failed return、394 行 reap.joined error 均不记录原因。supervisor panic 也会造成同一完成通道 EOF。

因此要排他归因一个实际失败运行，至少需要同一 generation ID 下的首个 genuine receipt、在 supervisor 自己发送 stop 之前读取的 stop 值、其余 worker/每个 SQL slot/maintenance join 与 drain 的失败来源，以及 quiesce/joined 的实际 error 分支。诊断只需来源与有限错误类别，不需要业务 payload、数据库内容或新执行 owner。

[Registry audit and quiesce](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/intelligence/jobs/reap.rs:97) 的 prune 只审计账本并移除 ready frame；正常 pending uncommitted claim 本身不会触发 prune error，PendingClaim 回滚 drop 还会移除自己的 frame。quiesce 在真实 workers/SQL/cleanup/children joining 后拒绝未 committed、未 closed 或 handler_done=false 的 frame，这个 fail-closed 条件必须保留。不能把“有 pending claim”猜成这次失败原因，也不能靠补造 reap frame 或跳过 quiesce 让 shutdown 成功。

[SqlSlot actual handle](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/storage/owned.rs:35) 对 outstanding/sticky failed slot 保持失败，并保留借用的真实 blocking handle；abnormal join 记 sticky failure。[maintenance owner](/mnt/data/workspack/github/vox-bridge/.worktrees/vox-bridge-implementation/.superpowers/sdd/2026-11-09-session-summary-continuation/task-6-oo1-validation/repair/close-investigation/candidate/src-tauri/src/intelligence/jobs/maintenance.rs:1) 仍是唯一 maintenance SQL owner。现有日志未标出其中哪个结果设置 failed，不能用 SQL 空闲、status、空队列或 semaphore availability 替代这些结果。

**最小命名回归与修复建议**

建议用 `oo1_normal_worker_join_after_pending_stop_poll_completes_shutdown` 标定这个单一窗口。测试必须在生产 selector 的真实 cancelled poll 返回 Pending 之后才触发同代 stop，并使至少一个原 worker 的真实 join_next 在同一次 select 中 Ready；以捕获的 Some(Ok(Ok(()))) 证明该分支实际可选。只在进入 select 前发送 stop，会让 biased stop 分支直接胜出，无法覆盖缺陷。要求两个 worker 在暂停期间都 is_finished 也超出了这个回归需要，且该观察不能代替 receipt。

控制点可以放在现有 supervisor stop/join poll 边界的 cfg(test) seam。暂停或 receipt marker 仅用于排定时序；保留 JoinSet receipt 及剩余 worker 的原 drain。随后完成 SQL/maintenance/handler/children/reap 的真实 drain，等待实际 process lease 释放，再断言正常 shutdown 成功和下一代能够启动。未修复分类的快照应因错误的 Interrupted 失败；测试不得依赖反复运行、sleep 猜测、过强的双-worker finished 守卫或 fake join/frame。

最小生产修复只修改首个 actual join 的分类：保留该 receipt，先读取现有 stop 意图，再让 supervisor 自己关闭 admission/发送 stop。分类应满足：

| genuine first receipt | supervisor 自己发送 stop 前的 stop 意图 | 结果 |
| --- | --- | --- |
| Some(Ok(Ok(()))) | 已请求 | 正常 stop 退出；不新增 failed |
| Some(Ok(Ok(()))) | 未请求 | 非预期退出，failed |
| Some(Ok(Err(...))) 或 Some(Err(JoinError)) | 任意 | failed |
| None | 任意 | 无实际 receipt，failed |

stop 值只解释一个已经真实 join 的 worker 为何退出，不能证明其他任何任务结束。所有后续真实 joins、sticky SQL failure、quiesce/joined 失败、未进入的 committed frame 仍需 fail closed。异常 worker 在 stop=true 时也不能被吞掉。保留两个原 scheduler workers、容量 2 token channel、每 worker/maintenance/session observer/delivery 各一个 SqlSlot、两个 actor handles、共享 16 child slots 和现有 frame 上限。正常 claim 不占共享 child slots，不扩展 owner、任务、SQL 容量或 native/child/lease 释放条件。

Root 后续已通报新增控制回归的进度：035 为 helper 可见性编译失败，036 捕获正常真实 receipt 与事先 stop=true，但失败落在过强的双-worker is_finished fixture guard。该消息没有经过本调查的原始记录/源码核验，不计作行为 RED，也不改变当前不可变候选的缺陷结论。受控 RED、最小修复、GREEN 与新证据核对由 Root 接手。

**输入完整性与交还**

完整 source.diff 已读 2,115 行、88,152 bytes；十一份 candidate 快照均全量读完。以下 SHA256 在报告写入前重新核查，与初始输入一致：

- brief.md：`26fcbbc16d6224f41d8d0a1431064ebe0fde3eaee034fcf203d93f07fa542b3a`
- inputs.json：`8dc3a86314ee3d9a3eb78cf85b31b405f5c513e68b4b1b69496e64156247f972`
- source.diff：`c779cae513a4b52a4afc84b93eb53f6f48cbc91763d757ec2f6a0069b2d61776`

| candidate 相对路径 | SHA256 |
| --- | --- |
| `src-tauri/src/storage/owned.rs` | `b8be5599997ec31447ee0c7004ee43a06d6a94507df178b32fbc5c5b796ccb7e` |
| `src-tauri/src/intelligence/jobs.rs` | `67d6be53d1c32d9bf641e08e295a9c073381764997ba97cea38f40e8f85a9924` |
| `src-tauri/src/intelligence/jobs/maintenance.rs` | `1723764c9bc1e127677331bd6a378d0461db0105e3deaec6deaa0fb8eebf1453` |
| `src-tauri/src/intelligence/jobs/reap.rs` | `7ee138f5ee90079815d0be297658c102986ce95448de15f9edac0d6def6e0d4e` |
| `src-tauri/src/summary/terminal.rs` | `6c3a35d05998fa4b219ecbdfa2add5aa52ec91debb192be8918116643fd0fb14` |
| `src-tauri/src/session/runtime.rs` | `77a32bdadffcede38ea1c2e28db0aaf0132ef05d1e5df8cc90854ff5ede72f50` |
| `src-tauri/src/session/orchestrator.rs` | `30bfdc102d00b133a32a410194cd40f8754f3051970b2797c400b92ba5183044` |
| `src-tauri/src/app/lifecycle.rs` | `23ec853257d29ff5323a13fb0f1835eca4135d2a39dd3fabd7f895e2923384cf` |
| `src-tauri/src/app/subscription_tests.rs` | `d9bcef16a3b646be430af2992aa042aa818786820ed71b91485e39fb76f76a82` |
| `src-tauri/src/app/intelligence/tests/summary/readiness.rs` | `97cd5c525c92042bb748b96c9c7b662e8c4cf3996c1fba3f3aaa63613b5eb141` |
| `src-tauri/src/summary/tests/harness.rs` | `6c5600f54e3409b1bc021e3dd891817ff4444d235c5323920e90bfe86609c255` |

以下原始 record/log 均被完整解析并核对 hash；相关失败/结果片段逐行读取。每个记录的 before/after 均为 299 entries 且运行期间不变，每个 log SHA256 均与 JSON 内字段匹配。历史记录间存在正常的候选修改，未把历史 focused run 当成 034 完整源相同的验收。

| 原始 evidence stem | JSON SHA256 | log SHA256 |
| --- | --- | --- |
| 018-all-targets | `15d1300d4548acc5bcd265a23d692a35301d1dfa5ee25484cdaa75842674ead8` | `52d2bdb379653ca3af5e40cdb7571988a47612ec1d4be78034dd5b1dd60c13b3` |
| 022-full-gate-failures-focused | `51997819f585716864460224af40001b0c50c9223be27617834af51bb3a5230a` | `8630b9d9fedf3bee959d4cdca78d73d78f92f7d5577fcf769295ad3600231e3d` |
| 024-publication-close-diagnostic | `76f9b05a162945293b3cf4b3373dcc428fe6d541a7a221cb3b988eef8249e41d` | `48b76349588f71bc59ea971803992aaaa21a0ad56e676b3a5f531828fde0c2f3` |
| 025-document-subscription-gate | `cb85f4678d239217189e00c139a3dd8c453760ea85f1ad6d69e6434f9b1c2c82` | `77b9525eb5f90f46d66d2fa397aad01efb47cd46540fac5334d4559fea46ec9b` |
| 026-diagnostic-all-targets | `ae55d7e6a5de5d9ecb76f6e8f0caa6f2e97411586d4c04c22386eb3683bca56a` | `99f05381fb959e3118736f7bb60730713562a8f8356ddb2e68683df598d5b8a5` |
| 027-raw-capacity-isolated | `22e5d9300d1a287289fa375ff6d0b46e713354d9bc0953c11b3ff3f5e6d4ed38` | `4d2ccd70481158039048b893526ea6cfa0329d19f27bade090a2f1274b56754c` |
| 029-final-affected-ownership-gate | `d310800f504f4ae3c9ab17a361a1d81a8b41a6c1c7ce8ddaf5b66de7987ea928` | `84d2b9f9f5b62df3bce15a30501053ec3b75da2b26929bde720ab793ad656477` |
| 030-final-all-targets | `f75a811bb70d3b352737819baec9b1d8a762cc41bee0db4d7075b38ad2a3823c` | `54b63a0d3a2b0a57b84671069afdb08ccd9e369607e57a59d3ef856686d8e79a` |
| 034-close-branch-all-targets | `d915dfa40f3fb86fa923f48f8adc968ff60f326cfbf84ff602900706bec5911c` | `3eeb5b36eaf7343b07eb346a40eb8b6b2bbbc41e334185dcd8d2296590422e6c` |

本调查仅写本目录 report.md；未编辑生产代码、测试、原快照、source.diff、历史记录或既有 Task6/Task8A/OO1 报告，未改变 index、refs、用户 staged .gitignore 或全局配置。未运行 Cargo、测试、安装、浏览器、native 或付费操作，未启动代理或后台任务。所有只读命令已经退出，没有 reviewer 持有的 pending 工作、测试进程或执行 owner；source/Cargo 与受控回归的所有权交还 Root。最新 live repair/source 状态由 Root 自行核验，本报告只对上述固定输入负责。

