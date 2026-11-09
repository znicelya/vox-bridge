# Task6 continuation plan — 只读闭合自审

日期：2026-11-09。作者：`summary_task6_architecture_preflight`。状态：**准备报告；Task6 未激活，未实现，未运行 Cargo/测试。**

结论：manual 完整交付 → automatic joined delivery 的两项端到端拆分可行，也覆盖旧 controller preparation 的内部单位。启动前需要把下面的有限 proof、共用本地恢复和 delivery/audio 生命周期端口明确写入执行计划。它们是生产接缝与任务责任的闭合，不是重新设计产品，也不是 Task5C 再审。Root 已接受新增 `summary/job_proof.rs`，并在本轮裁定 A6-7 的独立 Native `job_payload_hash`；本报告不把裁定或旧 checkpoint 当成已存在源码。

## 输入与观察基线

本轮沿用已读 checkpoint，只补读与闭合点有关的接口段落和必要生产声明，没有重写其 1185 行设计，也没有阅读 Task5C reviewer internals。

| 输入 | SHA256 / 观察 |
| --- | --- |
| `docs/superpowers/plans/2026-11-09-session-summary-continuation.md` | `e238f009ef9c14ffa9051906cf6d067ad79e1c78d00bc231995b7d6b7d8783e1`；349 行；下文 P 行号针对这个输入版本。 |
| `docs/superpowers/handoffs/2026-11-09-knowledge-repair/evidence/task-6-controller-preparation.md` | `bef6313b46c16b3faaa9c11a54d8ee03603783c8a1463e3f3801a25495d10f5d`；A6-1–6。 |
| 同目录 `task-6-architecture-checkpoint.md` | `603b89424b79904d9b4d4517c156153b3d5f692b00229ee1d24766f6b6de25f5`；1185 行 / 77371 bytes；与旧 `.superpowers/sdd/2026-11-04-workbench-intelligence/task-6-architecture-checkpoint.md` 相同；下文 C 行号针对归档输入。 |
| 当前 HEAD | `b72556091fc533dafb0a336907bf077fec377615`。这是观察值，**不是已独立接受的 repaired BASE**。 |
| staged `.gitignore` | `git show :.gitignore` 的 SHA256 为 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`；保持不动。 |

知识模块仍有当前唯一源码作者的 unstaged 修改/新文件。本轮不要求全工作区 clean，不 stage、不 commit、不回滚、不接管该作者，不运行 Cargo/测试，不读实际凭据或使用付费/native 资源。唯一写入路径是本报告。Task 标题/brief 提取和 Spec 链接已由 Root 确认，本轮不重复审这项。

## 拆分与接缝清单

Root 的两项交付安排是有意选择：Task1 交付可独立使用的 manual transport/snapshot/planner/admission/storage/deletion/view/export；Task2 复用这些实现，添加 automatic journal/delivery/startup/audio close。旧 preparation 第49–53行的三层内部单位需要在 activation ledger 明确 supersede，不构成拆分缺陷。

| 端口 / 能力 | 当前真实生产状态 | 产出与消费责任 |
| --- | --- | --- |
| `JobContext::{operation_owner,publication_fence,commit}`、`JobAdmissionFence::run` | 已有。context/fence 检查实际 owner、current generation、Running/cancel；不证明 Summary domain association。 | Task1 复用并叠加 SummaryJobProof；Task2 复用，不加 generic scheduler pre-create callback。 |
| `JobScheduler::{admission_fence,event_port,wake_port}`、`JobEvents` | 已有窄 admission/event/wake 端口。 | Task1 admission gate 消费；Task2 delivery 消费。post-commit 发布留在实际 entered closure 内。 |
| permanent lookup / `admit_job` | 已有 lookup，但在 `intelligence/store.rs:89–102` 内联；尚无计划中的 factored helper。 | Task1 仅抽取非写 helper，保持 `admit_job` 行为；Task2 使用同一 helper 和 summary `_in_tx`。 |
| `SummaryJobRequest`、`LocalStore::{create_summary_job,create_summary_job_in_tx}`、counter/native version writer | checkpoint 提议；当前无 summary 模块/这些端口。 | Task1 产出；Task2 仅复用。migration6 包括 A6-7 backend-only `job_payload_hash` 和后续 journal 字段，Task2 才消费 automatic 生命周期。 |
| `ServiceConfigStore::scoped`、`resolve_snapshot_credentials` | 已有；resolver 比较完整 ModelProfile 和 opaque secret association。 | Task1 owned LLM transport 消费；Task2/后续 research 复用，不只按 ID/model 重解析。 |
| `with_summary_selection_completion`、`SummarySelection`、`SummaryConfigSnapshot` | 尚未存在；已有 embedding completion pattern 不能被当作 summary 已实现端口。 | Task1 产出 selected/explicit LLM + automatic flag 的单 ConfigIo gate（C188–218），Task2 消费。 |
| `JsonRequest.error_policy` / opt-in context classifier | 当前没有；JsonRequest 是显式 struct literal。 | Task1 新增精确策略类型和默认 Generic 调用点；Task2 复用 LLM transport。 |
| private owned Deferred borrowed snapshot seam | `LocalStore.connection` 私有；现有 `transaction` 固定 Immediate。 | Task1 在 `storage/sqlite.rs` 内产出私有/窄借用接缝，summary/snapshot 消费。不能直接拿 connection 或嵌套 `transaction`。 |
| `SummaryJobProof` | 当前没有；Root 本轮已接受 Task1 新小文件 `summary/job_proof.rs` 和 A6-7 witness。 | Task1 产出严格 domain proof 与四处 fresh 验证，以独立 Native witness 为预期，Running 另比较 immutable claimed digest；Task2 复用。见闭合项1。 |
| `JobReapPort` / `reap_port`、AttemptRegistry16 | 当前 scheduler 没有该 reap 端口；checkpoint C997–1023 是未来提议。 | Task1 产出实际删除/恢复所需证明。若 accepted repair BASE 已产出等价证明，复用同一语义。Task2 消费，不能假设准备文档已实现它。 |
| core local recovery / manual-ready authority | `JobHandler::reconcile_startup` 当前是同步无 owner 钩子；不存在 async summary recovery/ready 端口。 | Task1 必须有独立共用本地恢复接续，不能依赖 Task2 的 delivery UUID；Task2 扩展 automatic-only authority。见闭合项2。 |
| `SummaryView`、`SummaryCursor`、`SummaryPage` 与 summary commands | checkpoint 有完整 service/page shape，command 只写 `list_summaries(...)`。当前没有 summary commands。 | Task1 产出并冻结 frontend 参数/返回契约；Task2 不改 command shape。见闭合项5。 |
| `SummaryHandler::terminal_tx` | generic trait 钩子已存在；summary 实现尚不存在。generic transition 先更新 job，再调用当前 kind 的 hook。 | Task1 在已有 terminal transaction/control 内直接重验/复制，不重新取得 Running fence。 |
| `AutomaticAttempt`、`SummaryLocalFence`、`SessionTerminalPort`、`TerminalReservation`、`commit_terminal_lifecycle`、`SummaryDeliveryOwner` | 全是 checkpoint/计划中的 Task2 新端口，当前没有。 | Task2 产出 backend-private capabilities；由 orchestrator、AppState 启停和 summary admission 消费。 |
| actual audio join acknowledgment / delivery close-join | 当前 close 有并行 audio/frontend/intelligence 分支，但没有可传给 delivery 的实际 join 端口。 | Task2 明确 producer/receiver 和 exact close identity；不能以 stop caller 的 timeout/status/active=false 代替。见闭合项3。 |

上述表中“已有”仅是有限源码阅读事实；“产出”均为未来责任，不是测试或实现成功声明。

## 闭合项1：SummaryJobProof 是 domain 授权，不能由 generic fence 推导

P74–85、P187–197 目前没有把这项变成明确 file/interface/fixture 责任。`JobContext` 仅暴露可信 job ID 与 cancel；`commit`/publication fence 在 SQLite→generation control 中只验证当前 job Running/cancel（`jobs.rs:24–88`）。generic `decode_payload` 只约束 version=1、profiles≤3、profile validity/重复 ID（`store.rs:211–226`），不能证明 summary/object/session/exclusion、严格 domain input 或 Native association。

应把 Root 已接受的 `summary/job_proof.rs` 加入 Task1 文件表、接口和真实负面夹具。推荐有限且有界的证明顺序：

1. 以 context/terminal callback 的**可信 job ID**为锚，先唯一查找 `origin=native AND row.job_id=job_id` 的 Native 目标。Running preflight 拒绝缺失、多义、Legacy、orphan；terminal 对已经没有 Native row 的旧 job 安全 no-op，零 domain write，不视为获得 publication 授权。不先按 payload.input.summary_id 或 durable object_id 找目标，更不能失败后切到另一个 summary。
2. fresh读取这个job的durable kind/object_id/session_id/exclusion_key/permanent_key/payload，以及已锚定Native row的有限metadata。验证kind=Summary、job.object_id=Native.id、session三方相同、exclusion=`summary:{session_id}`；permanent_key沿已审核Task3的当前durable identity校验，只允许NULL或该session精确的`auto-summary:{session_id}`，foreign/unknown identity拒绝。
3. 严格解析 `JobPayload.input` 的恰好 `{summary_id,session_id}` 两字段；拒绝 extra/missing/wrong type/wrong UUID/foreign target。外层继续受现有 payload byte/version/profile validation 限制。
4. Summary profiles 只能0或1；有 profile 时必须是完整合法 LLM ModelProfile，所有字段都参与关联/比较，且 Native profile_id/model 匹配。无 profile 必须与 native nullable metadata 一致，只能走本地 Configuration 等失败，不能产生 paid complete。
5. 按 A6-7 对 fresh durable `payload_json` 原始 UTF-8 bytes（≤64KiB）重算 SHA-256，比较独立 Native `job_payload_hash`；Native witness 必须是64字节小写十六进制，缺失/损坏不得从 current payload 自证。Running 再与绑定完整 claimed snapshot 的 immutable digest比较。期望 digest可以跨await保留用于相等比较，**每次授权仍必须fresh SQL验证**；它不是可跨事务重复使用的proof/cache。不能只比profile_id/model，不能信任不可信input中自报的hash。
6. publication UPDATE/INSERT 必须同时限定已经锚定的 Native ID、job ID、Running、非删除的 summary/session/current generation。运行 owner/cancel proof 与 domain proof都成立才写；没有 fallback 或新建目标。

四处必须是生产路径中的重验点，不能仅在 harness 中先验证一次：snapshot 前；每次付费 `complete` 前；raw/coverage 和最终 root publication 各自所在事务；scheduler post-transition `terminal_tx`。不可从最初 generic decode 或 Running status 推导后续 domain 仍正确。

**Root 本轮已裁定 A6-7，闭合独立 admission witness。** 当前 `JobPayload` 只有version/profiles/input（`types.rs:150–154`）；checkpoint Native元数据仅有profile_id/model，`input_hash`是transcript hash（C657–663），`raw_response_hash`是response hash。这两者均不是admission payload hash。因此：

- migration6在现有`session_summaries`新增backend-only nullable `job_payload_hash`：Legacy=None；Native必须64字节小写SHA-256。它不进入DTO/export，不新建表或crypto journal，不占schema7。
- 仅Created分支在同一admission Immediate transaction里，对helper**实际持久化的job.payload_json有界借用原始UTF-8 bytes（≤64KiB）**计算hash；与Native job_id/profile/model/version一起插入。Existing不写witness/counter/version，正常失败整笔rollback。
- 四处fresh proof都核对独立Native witness + durable kind/object/session/exclusion/strict input/完整合法LLM profile；Running另对照immutable claimed digest。queued cancel/startup terminal使用Native witness，无需已经进入handler、ConfigIo读取或Running-only commit。
- hash口径是原始持久payload bytes，不是重序列化`serde_json::Value`后的JSON。若handler只收到claimed Value，可在首次有界读取中严格比较完整claimed/durable decoded payload，再将通过独立witness的raw digest绑定为该attempt的immutable期望；不能因对象键顺序/序列化口径不同而混用hash。
- localized corruption保证包括admission→claim前仅改变job payload/profile且witness未变，以及Running中的单记录drift。联合一致替换witness+job超出该保证范围；不声称任意数据库整体替换都可识别。

成本是每 Native 版本64B及有限迁移/测试；无新用户审批、generic scheduler callback、跨事务授权缓存或加密 admission journal。本报告仅记录 Root 裁定，没有自行新增 schema/source。

**Root已明确的witness边界：** `job_payload_hash`证明原始实际持久raw payload的input与完整profile，并配合四处fresh durable domain关联校验。permanent_key依据已审核Task3的当前durable identity，只允许NULL或该session精确auto key；拒绝foreign/unknown identity。单独合法同session的manual/automatic模式重写不新增独立provenance承诺，不新增automatic元数据或crypto journal。普通admission、删除version、callback/reopen的exactly-once仍必须完整真实测试；有限witness足以解决本次识别的domain/profile drift，不扩大计划范围。

### terminal_tx 的安全顺序

真实 `jobs.rs:681–708` 在同一 SQLite/control terminal 路径中先 `transition_tx`，随后才移除 active；`transition_tx` 在更新 job 后按 fresh durable kind 分派 hook（788–815）。因此：

1. 留在 callback 提供的已有 terminal transaction/control 内做有限 borrowed SQL 检查；不调用 `JobContext::commit`/`JobPublicationFence::commit`，不再要求 Running，不重锁 generation control，不 await ConfigIo/HTTP/文件系统。
2. 从可信callback job ID锚定唯一Native `row.job_id`，再读durable job/payload/native association。已合法删除而无Native row时安全no-op，不INSERT、不从input/object_id找别的行；存在Native row但witness缺失/损坏属于malformed proof，不能自证或fallback。
3. 验证current job.status/error正好等于传入terminal status/code，并完成上述有限association/strict input/full profile/独立Native payload hash校验。queued/startup使用A6-7 witness，Running-after-run再对照immutable claimed digest。
4. 正常 Cancelled/Interrupted 可遇到 `deleting=1`；只复制 exact status/error，不写 content/raw/coverage，也不使删除 fence 失效。valid proof 才以 anchored native ID + job ID 做受保护 UPDATE。
5. malformed proof fail closed：安全 Storage / no-foreign-write；不得 INSERT、切到别的 summary、fallback Legacy、补 content 或伪造 Succeeded。若返回 Err 让 generic terminal transaction rollback，不能宣称 Failed job/version 已持久化。
6. 若 durable kind 已被改成其他 kind，generic dispatcher 可能根本不调用 Summary hook。对应负面测试应断言零 snapshot/LLM/foreign write，不假设每一种坏 proof 都还能正常复制 summary status。

“正常状态复制”和“malformed no-foreign-write”是两类不同的验证目标。真实 queued cancellation / startup interruption 都要分别覆盖，不能只直接调用 hook 或以 mocked Running commit 代替 terminal 路径。

## 闭合项2：Task1 必须独立闭合本地恢复与 ready

P204–210 要求 Task1 startup 恢复 persisted deletion locally；P84–85 又将 delivery 端口留给 Task2。C139–148 的 SummaryLocalFence 绑定 delivery lifecycle ID，C527–558 把恢复/ready 放在 Bootstrapping delivery 内。若直接照此引用，Task1 manual 可交付会依赖尚未存在的 Task2 capability。

目前 `JobHandler::reconcile_startup(&self)->AppResult<()>` 是同步、无 owner 的本地钩子（`jobs.rs:115`）。不能把这个钩子当成可完成 bounded async delete/wait/config ownership 的端口；也不能从当前 `generation=None` 猜测旧 claimed handler/entered child 已不存在。

建议明确共用 core local recovery/ready 接续归 Task1，可放 `summary/recovery.rs` 或责任清楚的 operations 子模块。计划至少冻结以下语义与确切方法名：

- async `SummaryService::reconcile_local_owned(...)` 接受实际 retained process/startup owner、当前 epoch 和 fresh-process-or-exact-joined 证明，恢复 summary deletion fences；不做 automatic admission、零 LLM。
- core `SummaryReady` / `SummaryRecoveryFence` 由该成功恢复路径产生，支配 manual generate/delete/session-delete 的 admission/ready，失败/超时不发布 ready，不 rearm；entered work 保持 owner，startup failure cleanup 要等实际结束。
- `JobScheduler::reap_port()->AppResult<JobReapPort>` 和 `JobReapPort::is_reaped(tx,job_id)->AppResult<bool>` 保留 C997–1023 的非写、exact-generation predicate。no-generation startup mint 必须接受真实新进程或 prior completion 已 join 的证据，不能只有缺失 generation 状态。
- Task2 在这些共用本地能力之上增加绑定 delivery UUID/stage 的 `SummaryLocalFence`、automatic journal bootstrap 和 delivery-ready。它不能 retroactively 成为 Task1 删除恢复的必要 producer。

AppState startup gate 要明确先注册 SummaryHandler/做 generic job interruption，再 await 共用 summary local recovery，最后打开相应 admission。Task2 再在 ready 发布之前做 local-only automatic bootstrap。相同进程 reopen 和 genuinely fresh process 的 proof producer/retirement 来源要分别写明。方法/能力名称以上为建议，必须在激活的 producer/consumer 表中最终冻结，不把建议名当成当前方法。

## 闭合项3：Task2 delivery 生命周期与 actual audio acknowledgment 需要真实端口

P235–244 命名 reservation/admission，但只有“retained SummaryDeliveryOwner”，没有 start/Closing/ready/close-join 的方法和实际 audio proof 来源。P307–313 与 C479–508 的顺序本身正确；缺的是从现有 sibling audio branch 到 retained delivery owner 的生产接缝。

当前 `app/lifecycle.rs:201–235` 并行运行 audio/frontend/intelligence。audio 先 early stop，再持 admission 的 final stop；intelligence 持 startup gate 并在 `jobs?` 后才 direct16/index barrier（142–160）。它没有可传入 delivery 的实际 audio completion acknowledgment。只把 `close_intelligence_owner` 内调用顺序重排，不能得到 sibling 分支的 join proof。

建议在 Task2 Interfaces/file responsibility 中明确以下 backend-private 名称/关系（签名的 ownership error type可沿项目习惯）：

- `SummaryDeliveryOwner::{start_owned,signal_closing,close_join_owned}`；start 消费 Task1 local-ready/实际 process owner，产生 exact UUID 的 delivery-ready；signal_closing 绑定既有 close election/UUID，并在任何 jobs/probes wait 前同步关闭 live admission。
- `AudioCloseJoin` / `AudioCloseAck`：producer 是现有 audio close owner，receiver 被 retained intelligence/delivery close owner持有；private ack 绑定相同 close election/epoch，只由 actual runtime lifecycle/native stop/join 完成产生。observer timeout、SessionStatus、stop caller 返回或 `has_active_operation()==false` 都不能自行 mint ack。
- `close_join_owned` 等待该实际 proof，再 `Receiver::close()`，继续 recv 到 EOF，最后 indexed journal tail + child1 all-permits barrier + exact UUID delivery done。必须 drain 已排队 UUID 和 close 前已发出的所有 OwnedPermit；buffer empty 并不证明完成，stop completion publication/active release也可能早于 observer 的最终同步 notification。
- 上述启停/ready ownership 要在 `summary/terminal.rs` 与 `app/{mod,intelligence,lifecycle}.rs` 两侧写清，避免只记录消费者而遗漏 proof producer。若调用观察≤5s/whole close≤30s返回超时，actual worker/lease/join receiver继续保留，下一次 close join 同一 UUID。

已有 `jobs?` 错误不能成为放弃 delivery signal/join 的早返回点；应先保留/推进两侧实际关闭所有权，再记录/传播结果，失败仍保持 admission/rearm fenced。child1 在 entered ConfigIo closure 实际释放以前不得复用或重新发下次 admission；10s 只是 observer deadline，不是释放证明。无需新 generic scheduler callback。

## 闭合项4：opt-in HTTP 策略必须指定 type、默认值与旧 literal 调用点

P153–157/A6-6 接受 compatible LLM 的 bounded allowlisted context error，但尚无确切策略类型。`JsonRequest` 当前仅 endpoint/bearer/body/timeout/max_response_bytes/retry_policy（`http.rs:33–40`），不是可依靠结构默认自动填新字段的调用方式。

建议冻结 `JsonErrorPolicy::{Generic,CompatibleLlmContextErrors}`，新字段 `JsonRequest::error_policy`；Generic 为默认语义，compatible LLM 显式 opt-in。只在限定非2xx/body≤64KiB/精确 code allowlist 里产生 ContextTooLarge；不泄漏原 body，不改变 unrelated status/retry/bounds。LLM `retry_policy=Never` 独立保持。

Task1 Files 和相关回归必须覆盖旧 struct literals 的显式 `Generic`：

| 旧调用点 | Task1 必要处理 |
| --- | --- |
| `intelligence/probes.rs:110` | 新字段为 Generic；保留 explicit saved-ID、probe revision/full-profile-fenced persist，不改成 selected summary profile。 |
| `knowledge/embedding.rs:118` | 新字段为 Generic；仅增加默认 wiring，保持 embedding error/retry behavior。此路径须在 accepted BASE/唯一作者交接后归 Task1 的精确责任中。 |
| `intelligence/http_tests.rs:45` | request helper 显式 Generic，覆盖原 generic HTTP classification。 |
| `intelligence/http.rs` 内 fixtures / 新 llm caller | 按实际所有 JsonRequest 构造点补齐；LLM 才 opt-in。 |

这项是必要编译与行为责任，不是扩张 generic HTTP 功能范围。Root 已提供这三个具体旧调用点；本轮只确认接口，没有修代码或运行回归。

## 闭合项5：分页 command 的名字、wire 和默认值需要显式冻结

C127–129 已明确 service `list(session_id,after:Option<SummaryCursor>,limit:u32)->AppResult<SummaryPage>`；C918–952 已明确 canonical signed-decimal cursor、≤100/default50/page≤8MiB。C154 的 command 仅 `list_summaries(...)`，P79–82 仍引用未补全的 command。对于 Task1可评审交付和 Task8/9 handoff，这不是可省略的 frontend 参数。

建议保留 frontend 字段名称 `after_version`，其类型改为 `Option<SummaryCursor>`（wire 为 string），不是 `Option<u32>`。command 逻辑签名明确为：

```text
list_summaries(session_id: Uuid,
               after_version: Option<SummaryCursor>,
               limit: Option<u32>) -> SummaryPage
```

AppState/command error wrapper 沿项目现有形式；service 保留 `after: Option<SummaryCursor>, limit:u32`。command `limit=None` 使用50，service验证有限1..=100；非法值不要靠无界 SQL。cursor 私有 string/newtype 从 wire 严格反序列化为 canonical signed i64 decimal≤20 bytes；`None` 是没有 lower bound，不是0，不能舍弃负版本或让 JS rounding 改游标。返回 `SummaryPage { rows:Vec<SummaryView>,next_cursor:Option<SummaryCursor> }`，next_cursor也是 string wire。

Task1 command/wire fixtures 应实际穿过 command/service，覆盖省略 limit、signed/large exact cursor、非法 canonical representation 和 tagged Native/Legacy 返回。Task2维持相同 command shape；Task8/9移交 exact字段名/DTO/cursor/default，不再用省略号。

## 闭合项6：startup 的 local failure precedence 文字歧义要消除

P301–306 先要求 enabled missing marker固定 Failed/errorInterrupted，随后又说 NoFinalText/missing-model use safe local failure precedence；容易被实现成重启时以 NoFinalText/Configuration 覆盖历史 uncertainty。C291–304、C541–550 和 A6-5 已有一致裁定，应写成两条明确规则：

- forced LocalRetry / uncertain startup：Interrupted/Busy先于 NoFinalText/Configuration；未知旧历史 enabled+missing marker固定一个可见 Failed/error=Interrupted，零 LLM，第二次 reopen命中永久 Existing。config文件IO不可读不能猜 automatic flag，保持原 pending/readiness fenced。
- normal live admission：NoFinalText → missing/invalid LLM Configuration → normal unfinished capacity/exclusion Busy；有forced local reason时先用forced reason。ContextTooLarge来自已 Queued版本的完整 snapshot/handler，不扩展 generic initial_failure允许集。

permanent Existing 必须先于 prospective counter/allocation以及配置选择失败的后续 admission；配置不可用时可用可信 session/permanent identity 做**只读 Existing replay**，不得伪造默认配置。automatic output 被合法删除后 Native row缺失是正常 Existing 情况，不可用 Running SummaryJobProof的“必须有Native目标”去拒绝 replay，更不能因此创建第二个 automatic job/version/LLM。

counter耗尽只在可持久化时留下 truthful local journal ResourceLimit，不新建 job/version/permanent marker；不得把“零新输出”的耗尽分支写成已经产生 Failed summary。automatic disabled消费journal且不新建。正常首次local initial Interrupted为Failed/errorInterrupted，恢复既有Queued/Running job则保留scheduler Interrupted；manual Retry为新manual job/version、permanent=NULL。

## 闭合项7：小型执行接续修正

这些是应一并修掉的路径/例子问题，不需要升级成新产品设计：

1. P59 `export/{mod,json,markdown,text}.rs` 的真实文件是 **`plain_text.rs`**；Task1 Files、owned formatter/test paths用 `export/{mod,json,markdown,plain_text}.rs`。
2. P106–107 示例 `h.versions(session)[0].version` 无法直接访问 tagged `SummaryView` 的字段。明确该 fixture helper返回 `Vec<NativeSummaryView>`（只用于native案例），或在assertion中match `SummaryView::Native`。此例是计划代码，不是已运行runtime RED；compile wiring 与实际assertion failure继续分别记录。
3. P158–177 把 owned snapshot 放在 existing connection 是对的，但应命名 `storage/sqlite.rs` 内的窄 borrowed Deferred seam及owner/deadline职责。`summary/snapshot.rs` 不可访问私有 connection，现有Immediate `transaction` 不能套Deferred。first session read pins snapshot，5s acquisition/scan、borrowed type/UTF8/length checks、单次normalization/hash、release before HTTP均保留。
4. generic claim只把job状态改Running，terminal_tx只负责terminal copy；应说明 Native Running由handler entry的受保护UPDATE或view的可信job join取得。不能以为Native status自动跟generic claim同步，或在terminal hook再尝试Running commit。
5. 接缝新增责任要在activation ledger回填明确文件/producer/consumer，尤其 `summary/job_proof.rs`、共用recovery模块、old HTTP literals、audio ack producer和exact分页command。Root先冻结最终计划再派源码，继续一个Rust/Cargo作者，不能把本报告当成source activation。

## 最小真实负面夹具矩阵

下面是plan必须明确的行为覆盖，不是本轮已执行测试。harness必须组合真实SQLite/job/config路径；外部LLM仅recording double，拥有真实lease，没有production `Arc<()>` fallback。

| 夹具 / 修改点 | 必须观察的生产结果 |
| --- | --- |
| 两 session / 两 Native rows，仅修改 job A 的 object_id 或 session_id 指向B，payload 保持 generic-valid/Running | A不能 snapshot/首次LLM；B逐字段/内容/raw/coverage保持原值；不得从 input 重新选B。 |
| 修改kind、exclusion、wrong/foreign/unknown permanent identity，generic Running fence仍可成立 | domain fail closed、零foreign write；kind改派时不强求Summary hook正常status copy。合法同session模式重写不增加独立provenance承诺，普通exactly-once行为仍按完整真实路径覆盖。 |
| strict input extra/missing/wrong type/wrong UUID，或 summary_id 指向 foreign summary | 零 snapshot/LLM/foreign publication，安全错误，不 fallback。 |
| Native row.job_id/origin/profile_id/model不匹配；相似 Legacy row作为诱饵 | 拒绝缺失/错关联 Native proof，不能 Legacy fallback或INSERT新目标。 |
| profiles多项、错误ServiceKind；完整合法 profile的URL/model/timeout/context/output等字段或claimed/durable digest发生drift | 验证完整 snapshot关联及独立 Native witness；当前JSON仍generic-valid不代表允许。不拿 current hash自证。 |
| admission→claim前单独修改已持久 job.payload_json/profile（包括同ID/model但URL/timeout/context变化），Native witness保持原值 | 原始 payload hash不匹配，首次 proof拒绝、零 snapshot/LLM/foreign write；不需要handler已经持有claimed baseline。 |
| Native witness为NULL/非64字节/非小写hex/错误hash；健康Native对照与Legacy=None对照 | Native fail closed、不从payload补witness或fallback；legacy保持可读；新witness不进入DTO/export。 |
| 分别在snapshot前、后续每次complete前、response/raw/root publication前、terminal transition前受控改关联/profile | 四处真实重验均可阻止该阶段；首个map前失败零calls，后续map前失败不再dispatch；publication失败保留此前合法raw，不写新raw/root/foreign行。 |
| 真正Queued cancel和startup interrupt；有效独立witness与malformed witness两类；valid Cancelled/Interrupted时deleting=1；正常Succeeded/Failed/Cancelled/Interrupted终态 | valid exact status/error copy，不改content/raw/coverage；bad proof零foreign write。Err rollback时不声称failed version持久化。queued/startup不依赖handler或Running commit，不能只直接invoke hook。 |
| 已删除Native row的旧job进入真实terminal transition | terminal hook安全no-op、零INSERT/foreign write，正常job transition可保持；与存在Native但坏witness的malformed case分开。 |
| 删除highest/all automatic outputs后，配置不可用或counter exhausted时replay同一permanent identity | 合法Native row缺失仍Existing；零new job/version/counter advance/LLM，与Running证明缺失的坏case清楚分开。 |
| Task1在fresh process及same-process joined reopen恢复deletion；延迟entered resolver/claimed-unentered handler | async recovery保持actual owner，未actual reap不hard delete或ready；不能由generation=None证明结束。 |
| Task2 close中audio observer与actual join错开，提前reserved OwnedPermit晚send，job shutdown失败，entered ConfigIo observer timeout | 等待actual ack、drain to EOF、child1不复用、delivery owner不丢失、same UUID retry join；无metered replay。 |

fixtures需记录真实command/exit/assertion，且在Task1/Task2各自完整交付gate中落地。任何一项尚未写/跑，不能用旧checkpoint或本报告代替RED/GREEN。

## 已自洽的部分与交接

以下是文档/接口阅读层面的通过项；没有运行验证含义：

- A6-2的较小事务顺序已明确写在P123–138：factored permanent lookup → read/check counter无写 → unchanged admit_job → 仅Created写Native/counter → commit → event/wake。Existing不分配、u32耗尽在INSERT前、插入失败回滚job/version/counter/marker；不需要generic pre-create callback。
- Counter保留signed legacy语义：max(0,legacy MAX) seed、>u32正值保留exhausted、foundation legacy writer同事务前进、删除最高/全部版本不复用。
- fixed snapshot、manifest、actual escaped request预算、≤64 calls/≤8 levels、完整Unicode parts/children与streamed exact coverage已在Task1落位，不允许successful prefix或遗漏part。
- exact raw/hash/actual supplied coverage在同一fenced transaction且先于strict parse，root只在完整manifest/coverage之后publish；local raw不进入frontend/export。
- Native/Legacy tagged view及exact cursor保留truthful legacy signed version/status/seconds，不fabricate native metadata。JSON export维持legacy version/seconds/status/JSON原projection类型，不能把frontend cursor string规则套到export数字上。
- 完整JSON/Markdown/plain text export包含全部native字段及action owner/due、term meaning；projection≤32MiB/artifact≤128MiB，任何completeness/size failure保留destination。只需修真实plain_text文件名与exact command contract。
- actual AttemptRegistry16 + exact-generation reap、persisted summary/session delete fences、timeout保留行/fence、automatic permanent marker/counter存活均保持原裁定。
- Task2 A6-1 journal/reservation/notification顺序、worker1/channel32/child1/keyset32/rescan200ms与close观察界限自洽；把生命周期/audio proof producer补齐后可由Task1成果接续。

Root可在收到报告后一次修订当前plan并记录ruling：两项端到端split supersedes旧内部拆分；SummaryJobProof与已接受A6-7 witness；Task1共用recovery/ready；Task2 exact audio/delivery启停端口；HTTP default callers、分页command、precedence与小路径修正。之后仍需独立接受Task5C repaired BASE并重新比对真实producer，才允许Task6 source activation。

本轮只创建这个报告，没有改Rust/Cargo、计划、旧checkpoint或归档，没有stage/commit、Cargo/测试或付费/native访问。最终handback是只读闭合意见，不能标记任何Task6实施步骤为完成。
