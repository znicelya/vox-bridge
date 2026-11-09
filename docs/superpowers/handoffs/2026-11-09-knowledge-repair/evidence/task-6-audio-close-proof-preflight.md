# Task2 audio close proof — 只读 producer 核对

日期：2026-11-09。作者：`summary_task6_architecture_preflight`。状态：**准备报告；没有实现 Task6，没有 Cargo/测试或 native/paid 执行。**

结论：已有 runtime actor join、CPAL 内部 worker join、native destructor 后的最终 close reply、Pulse 正向 child-exit/worker/monitor acknowledgment 可复用。但当前没有覆盖失败、观察超时、failed-open/retired owner 的统一 actual audio join 端口。Task2 不能用 `stop_active()->Ok`、`RuntimeOutcome::Failed`、`active=false` 或 sticky lease/quarantine 自行构造 `AudioCloseAck`。最小接续必须保留独立的 actual completion/reap 证明，并由既有 AppState audio close owner 汇聚后交给 delivery 分支。

本报告只核对生产 producer，不重读 Task6 架构/执行计划、不改 Rust/Cargo/旧报告/归档/全局配置、不 stage/commit、不派子代理。知识模块唯一 Rust/Cargo 作者仍为 `knowledge_delete_completion_impl`。唯一输出是本文件。

## 观察基线与源码范围

观察 HEAD：`b72556091fc533dafb0a336907bf077fec377615`；不是已独立接受的 repaired BASE。staged `.gitignore` SHA256 保持 `1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`。

| 生产输入 | SHA256 |
| --- | --- |
| `session/orchestrator.rs` | `b2832c91004883d0f31f5e343bf59795e85788e65586e9f47c81c5b9c5633a4a` |
| `session/runtime.rs` | `ab303379d031c2a766ff08d9f421d10a0d6252208010dcf71dbad5d7e03408dd` |
| `session/resources.rs` | `2afd900de10fdfbc3a9c53c3b3d74620f6b1224f083519e3e2bea3c6726f8f47` |
| `session/direction.rs` | `27263b5909b3ce971921e4f74be17f05a5a8bcb3ab3bde43b0dec1cf66cbb6c0` |
| `audio/runtime.rs` | `6df4a5d9d49226eebfd1431e24e95e4818be662617c9f0f5547273b2c2df46dd` |
| `audio/platform/native_io.rs` | `7c8fa9cd7266468fecbab5b2e73e1c0b4edc2dd3d51a9bb41a9d98a5d0f069c8` |
| `audio/platform/pulse_io.rs` | `7dc421ab7952aed919e2264d962001c788d2b7108a03446e672192333aa942e9` |
| `audio/platform/process_io.rs` | `69157c332d110211360af13092625b62856d8f7071213b19f8176aab15be890e` |
| `audio/platform/inventory.rs` | `60eb1467b53a68df0eeff07e382c8c62f4b4c4e9df4f2d75b8be77683bd56e09` |
| `audio/capture.rs` | `f713e0043d5c0f9f1a0d949f9263fe63f91090b17ca25e9bbae08d99dfd2181f` |
| `audio/playback.rs` | `9d7c636362471721f3fff77192122c269b9423b95a53de6e124dec4fc6870b3a` |
| `app/mod.rs` | `ad58a4d065dbfd2fdaf4d50edc700849a5101c95325a1e0ffc319ed86bfcfc08` |
| `app/lifecycle.rs` | `6025a9e929c8082fb22d0ede58df9cebbb61975e76eeabb11d939dcd5dbf0394` |

路径均位于 `src-tauri/src/`。没有阅读 Task5C reviewer internals 或实际凭据。未要求正在写入的全工作区 clean。

## 现有证明到底覆盖什么

| 现有接口 / producer | 可以证明 | 不能证明 |
| --- | --- | --- |
| `RunningSession::request_stop()`（runtime214–216） | 同步 cancellation request，既有 owner 继续执行。 | 未 join；不是 cleanup acknowledgment。 |
| `RunningSession::stop()` 的 **completion Some**（218–230；supervise346–418） | 当前 producer 在 DirectionActor 的 JoinSet 排空后发布 completion；实际等待这个源可证明 direction tasks 已 join。 | RuntimeOutcome 本身无 native-owner/reap 能力；channel closed 分支还会合成 Failed/CleanupTimeout，不能当 join receipt。 |
| runtime `Stopped` snapshot（411–418） | 在当前真实 producer 中最后一次 Stopped/outcome 发布发生在 JoinSet 排空之后。 | 这是状态投影，且先于 completion 发布；不包含 native owner actual completion。orchestrator 的状态分支不能升级它的保证范围。 |
| `AudioOperation::join/cancel_and_settle`（resources146–183） | 保留 async acquisition JoinHandle；abort 后实际 await，并检查可能已取得的 ports/typed failure。 | async open task 被 join 不等于它派生的 native thread/child 也已结束。failed-open返回无ports时尤其缺少可继续等待的 native receipt。 |
| `CaptureHandle/PlaybackHandle::close_inner` | 先 drop native stream，再同步 join 自己的转换/播放 worker（capture119–128；playback146–167）。返回后可复用这个真实顺序。 | 如果 stream destructor 或 worker.join阻塞，外层 observer timeout不终止它；不包含外层 native owner thread 的可重看 join 端口。 |
| native owner 的 **最终 close reply**（native_io282–336） | 真正回复只在 `owner.close`、`drop(owner)`、capture close 和 lease policy之后发送；正常正向回复具有实际 native teardown依据。 | `NativeControl::close` 的 Err可能是观察超时/queue不可用；late reply可被丢弃。外层std thread JoinHandle当前未保留，不能声称该thread已实际join。 |
| Pulse `WorkerEnd.acknowledged=true` 与 `ProcessControl::wait/terminate -> Ok(ProcessExit)` | 已观察正向子进程退出；非零exit也证明退出，但未必成功drain。`Inner::close`还等待capture/playback与monitor完成。 | WorkerEnd false、EOF、cleanup Err、monitor_done单独都不证明全部子进程已reap；cached request timeout会失去后续实际状态。 |
| endpoint `Lease::release/quarantine`（inventory99–123） | endpoint admission policy，未确认清理时保持封锁。 | sticky quarantine即使late exit也不清空；lease map是否为空/lease released flag不是exact-owner join receiver。 |
| `SessionCompletion` / `stop_session`（orchestrator408–419、520–556） | 附着调用者等待既有observer完成lifecycle/diagnostic/bus final，并取得其result。 | result只有record/string，不携带native proof；无active时还能从durable Completed/Failed直接返回Ok。 |
| `AppState::close_all`（lifecycle181–254） | whole30s只观察；外层timeout/drop不取消spawned close owner。audio early stop后还有admission-fenced final stop。 | 目前audio分支返回`early.and(final_stop)`，没有actual-native aggregate ack；失败后新close调用可只看到active=None。 |

以上是源码事实，不是运行测试结论。

## 失败与超时仍保留了哪些工作

### RuntimeOutcome 与 stop reply

`resources::close_audio`（55–78）给 discard/close共用生产2s deadline。timeout/typed error只进入CleanupReport，然后返回statistics；失败还会同步suppress。DirectionActor 的 graceful/abort路径把report记录后允许结束（direction666–864）；supervise只join actor，并不额外等待native owner。

所以可以同时发生：actor已join → RuntimeOutcome Failed/CleanupTimeout → orchestrator durable Failed/bus closed/stop reply发布 → active=None，而底层native destructor或子进程清理仍未确认。`close_audio`的2s、NativeControl/PulseControl的2s，以及whole30s都是观察/操作界限，不能当actual completion。

orchestrator `observe` 的stop/storage-error路径await `running.stop()`；自然Stopped路径直接读snapshot outcome（323–370）。随后先写最终lifecycle（378–385），再写diagnostic（394–404），关闭bus，发布completion，再移除active（408–419）。有三种不同事实：

- runtime/音频可能已经实际结束，但phase/lifecycle/diagnostic存储失败，使session或stop reply为Failed/Err；实际join证明不能因此被抹掉。
- lifecycle已持久化但之后diagnostic失败：stop reply为Err；未来Task2仍必须保留成功commit的terminal intent，不能以`stop_session`是否Ok决定是否通知。
- cleanup timeout导致RuntimeOutcome Failed：native actual join可能仍pending；Failed status不能自动mint ack。

`stop_active`只复制attached operation ID，调用`stop_session`，无attached时直接Ok（506–517）。它不会枚举已经退出active状态的native owners，也不会等待没有附着runtime的native preparation工作。

### CPAL native owner

`native_io::open_owned`保留non-Send owner在独立std thread上（206–338）。Factory open、capture.stop、playback.stop、owner.close以及drop(owner)都可能在该thread上阻塞。Capture/Playback内部有真实worker.join，保持原线程语义可复用，不需要把native handles移给delivery。

当前`.spawn(...)?`得到的外层thread JoinHandle被丢弃。NativeControl只保留四个command的watch结果（61–122）；每个独立observer在deadline后缓存Timeout，丢掉oneshot receiver，置NeedsRestart/quarantine。late native completion不会把该watch改成actual joined。因此重复`control.close()`只能得到原缓存结果，不能用于late join。

failed-open同样不能忽略：ready超时返回Err/StartupTimeout、quarantine并drop接收端（345–363），而factory thread仍可以继续。它之后若ready.send失败，会suppress、close、drop(owner)（226–237）；但当前没有把这个真实结束留给session/close owner继续观察的端口。async AudioOperation取得Err/no ports不能证明该thread消失。

close返回native error后也要区分“资源destructor已经返回”和“观察错误”。前者实际完成后可产生Joined receipt并保留失败/NeedsRestart；后者没有该证明。不能以统一AppError类别猜测是哪一种。

### Pulse workers 与 ordinary process owner

Pulse Inner::close（318–339）等待capture/playback WorkerEnd和monitor_done，要求两个acknowledged为true才release lease。Control request在2s观察超时后缓存Err、suppress、stop monitor、quarantine（348–405）；底层worker/ProcessControl仍可能继续。Drop Control另外spawn一个bounded close观察（419–435），也没有export一个exact actual join receipt。

capture worker先drop reader，再bounded terminate，失败可发布WorkerEnd{acknowledged:false}（688–707）。playback EOF并不是drain，只有真正wait/terminate成功才有正向child exit；failed terminate仍可发布false（759–813）。monitor_done只证明monitor coroutine返回；其`associated`使用`process_io::output`，被select取消时辅助pactl子进程不因此自动成为已reap。

ordinary `process_io::supervise`（99–110）在kill失败或2s reap timeout时发布Err，随后结束并drop OwnedChild；System child有kill_on_drop（122–147），**这是终止请求/运行时清理机制，不是实际wait成功的证明**。当前cached `ProcessControl::wait`无法在该Err之后继续读late positive exit；不能说所有错误分支都仍有一个可join的Child owner。本报告没有重审limited process实现，也不将其不同ownership保证套到ordinary audio children。

为完整native join，必须增加独立positive actual-exit receipt，并在observer error后保留actual reap责任。streams、failed-open rollback与被取消的monitor/helper child都要归入相应owner，不能只取三个worker completion布尔值。

### 未附着runtime的native inventory

`PlatformAudioBackend::inventory`（279–293）以native_inventory_gate单许可进入spawn_blocking；2s timeout后native枚举仍持permit继续。Runtime.prepare调用它（runtime123），但AppState `list_audio_endpoints`、`audio_driver_status`、`validate_session_launch`（mod306–339）也调用inventory/prepare且不持session admission锁。被取消的preparation可释放Reservation/active，blocking closure仍在工作。

因此final admission锁只证明其覆盖的session admission future退场，不能声称所有audio backend native工作已结束。若`AudioCloseAck`保证包含这些entered native operations，应在backend层封闭相应entry并等同一generation的actual owner/单许可屏障；同一backend的inventory重入必须受该Closing检查。若选择更窄的runtime-stream scope，必须明文说明独立inventory/utility工作不在该ack保证内，并保留它们自己的actual owner；不能用窄ack宣称整个backend/native已join。这个边界需要写在producer契约里。

## 最小 AudioCloseJoin / AudioCloseAck 契约

以下都是**建议新增的backend-private接缝，当前没有**。不添加frontend owner字段、schema或generic scheduler callback，不改paid/native验收边界。

```text
AudioCloseKey = 当前 elected close identity + delivery UUID + audio/admission epoch
AudioCloseJoin = 同key的可重复观察receiver + retained actual close owner
AudioCloseAck = 只由actual producer构造的同key immutable joined receipt

AudioCloseJoin::wait() -> Result<AudioCloseAck, safe observation error>
AudioCloseOwner::run_owned(key, existing audio/runtime/native join sources)
SessionOrchestrator::capture_close_join(...) -> exact runtime/lifecycle owner receipts
AudioControl / AudioBackend actual_join_port(...) -> retained native completion/reap ports
```

私有构造、无Serialize/raw Debug；consumer不能从UUID、Outcome、bool或`Arc<()>`构造ack。没有actual native receipt的trait implementation/default必须fail closed，不能返回synthetic success。方法名可按项目习惯冻结，但producer、receiver和以下语义不能省略：

1. **先捕获、后停止。** 在既有close election下先signal delivery Closing，建立/保留同identity的join receiver，交给并行intelligence/delivery分支；audio close owner获得exact runtime/lifecycle与native source。actual registry必须在native thread/child/failed-open工作进入前登记，不能成功返回AudioPorts后才登记。
2. **实际joined独立于result。** 复用runtime的真实completion源，而不是`stop()->RuntimeOutcome`返回值的synthetic channel-closed分支。native receipt只在destructor/worker/native thread实际结束、所有owned process child取得真实exit/reap后发布。普通Outcome/diagnostic/health/close error另存，不抹除已取得的joined receipt，也不把unknown升级为Joined。
3. **late owner仍有接续。** CPAL保留outer thread handle或同样强的实际结束接续，late final reply不再是唯一被deadline observer丢弃的证明。Pulse保留worker/monitor及每个process actual-exit接续；ordinary child的observer Err后继续保持actual reap责任。failed-open无ports、AudioOperation abort/join、retired replacement都不丢native receipt。
4. **不以active=None mint。** 等final admission pass及captured runtime owners实际结束，再等其native receipts；缺失active只能在同close scope登记/封闭并证明实际owner集合已drain时作为no-work结果。新进程空集合与同进程旧owner pending必须区分。
5. **保证范围明确。** 若ack覆盖整个audio backend，backend的open/inventory/相关native helper entry一起按同epoch封闭，并复用inventory actual许可；retained owner集合有明确上限。未知cleanup保持原实际slot，不能observer超时释放后再积累新child。若scope仅runtime streams，禁止将它用于声称inventory/global native quiescence。
6. **native policy不被join覆写。** Joined receipt可与Failed/NeedsRestart共存；不清除sticky quarantine、不把late成功改成自动恢复endpoint admission，不虚构播放drain或真实音质证据。
7. **ack之后仍drain reservations。** ack至少证明runtime lifecycle commit/bus final/stop completion/active release相关producer已quiesce。completion publication可以先于最后同步UUID send；所有pre-issued OwnedPermits必须仍被保留。delivery拿到ack后Receiver.close，再recv到EOF，不能buffer empty即退场；随后local journal tail、child1 barrier、delivery done。
8. **错误不放弃owner。** audio stop/result/diagnostic失败也走actual join接续；joined后可发布ack供delivery安全drain，同时whole close仍按原错误语义报告。unknown native join不发布ack；观察≤5s/whole30s超时保留原owner/receiver，关闭和rearm保持fenced。retry观察同AudioCloseKey/delivery UUID，不能因为stop_active无active就重新mint成功proof。

保留的registry/接续应只含actual owner或安全completion端口，不靠强持NativeControl sender导致其自己的disconnect cleanup永远不触发。不新建每session delivery任务；复用现有runtime/native owner，actual join接续在既有有界owner数量下保留。

## Task2 最小文件责任

| 文件 | 必须明确的producer/consumer工作 |
| --- | --- |
| `audio/runtime.rs` | 公开Rust内部opaque actual audio join能力或窄trait port，既有close/request outcome保持分离。对不支持actual proof的实现fail closed。 |
| `audio/platform/native_io.rs` | CPAL/non-Send owner的实际结束producer，保留outer thread join及failed-open/late close receipt；command timeout不覆盖实际completion。 |
| `audio/platform/pulse_io.rs` | 聚合stream workers、monitor和其native child actual receipts；failed-open rollback、late request/drop、EOF、acknowledged=false不自证。 |
| `audio/platform/process_io.rs` | ordinary audio child独立positive exit/reap接续；observer kill/wait error之后保留actual child责任。优先窄audio port；若扩展共用trait，列出受影响fixture/implementor，不能擅改limited/knowledge的既有证明语义。 |
| `audio/platform/inventory.rs` | 把owned open/failed-open/retired native接续与exact backend scope关联；明确Closing entry fence、native inventory单许可实际屏障和ack范围。lease/quarantine只作admission policy。 |
| `session/resources.rs` | CleanupReport/AudioAcquired/AudioOperation保留actual join capability，特别是no-ports失败与async abort已经join但native仍pending；不把timeout转为joined布尔值。 |
| `session/direction.rs` | graceful/abort/recover/resume所有路径保留/传递same-owner native receipt；diagnostic/accounting失败不抹除实际结束证明。 |
| `session/runtime.rs` | 从真实JoinSet/completion producer提供exact runtime join接续，并聚合direction native owners；不从Outcome/Stopped状态值构造证明。 |
| `session/orchestrator.rs` | retain/capture runtime与observer/lifecycle收尾receipt，active移除后仍能join旧owner；durable terminal intent成功与stop reply/诊断错误分开。 |
| `app/{mod,lifecycle,intelligence}.rs` | 在既有close election中保留key/AudioCloseJoin，audio sibling产ack并交给delivery；early/final stop后仍joinactual source。startup/rearm不得越过old pending，jobs?等错误不弃置delivery owner。 |
| `summary/terminal.rs` | 消费同identity的ack，close/drain OwnedPermits到EOF，local tail/child1与worker join；不能自行生成audio witness。 |
| focused production-collaborator fixtures | actor-only join、blocked native open/drop、late actual exit、queued permits、diagnostic失败和close retry真实行为。修改trait时同步明确已有audio/session fixture wiring；本轮不写或运行测试。 |

`audio/capture.rs`/`audio/playback.rs`现有stream-drop→worker.join顺序可复用，**无证据需要改其底层行为**。不要为新增ack重新实现音频capture/playback、driver操作、未涉及的limited process或付费服务。

## 必要真实行为夹具

- native close/drop被hold：actor/stop reply可Failed、active可None，但ack仍pending；release后actual join receipt可见，原timeout/NeedsRestart/quarantine不被伪改。
- native factory open超时返回无ports；release后走ready-send-failed的close/drop，close owner仍join该原线程，不把AudioOperation abort/join当native结束。
- actual native已join但lifecycle/diagnostic存储失败：ack可以成立，stop/result仍Err；成功terminal commit的intent不丢，失败commit没有通知/新journal。
- Pulse非零exit、EOF、WorkerEnd false、process kill/reap timeout分别覆盖；only真实positive exit证明child结束，monitor返回或kill_on_drop不替代reap。late child结束后同join port可以观察，不重新spawn。
- native inventory closure超时仍hold单许可、unattached preparation/查询路径进入；按已冻结ack scope测试Closing拒绝新entry或明确不在其保证内。
- whole30s/配送5s observer timeout、drop调用者、jobs shutdown失败：原actual close owner/receiver仍被保留；retry同key/UUID，open_window不得靠active=false越过pending。
- stop completion/active release早于pre-issued OwnedPermit的最终send：ack后Receiver.close仍recv至EOF，不丢已commit通知；journal local tail/child1/worker join才发布delivery done。

这是待Task2实现后观察的行为清单，不是已运行RED/GREEN。default fixtures不用真实audio hardware/native keyring/付费服务，真实native平台验收仍是外部gate。

最终结论：**现有正向teardown/actor acknowledgment可复用；失败和late native ownership需要新独立actual join producer。** Root应把上述backend/native/process/source责任与scope加入Task2，避免只列`AudioCloseAck`消费者而无真实producer。本轮仅写本报告，其余源码、Cargo、计划、归档和protected `.gitignore`保持不动。
