# A6-8 三个 audio parent slots — 只读 scope follow-up

日期：2026-11-09。作者：`summary_task6_architecture_preflight`。状态：**仅审拟议 scope；没有实现源码、运行 Cargo/测试或调用 native/paid 资源。**

结论：**两个 stream/open parents + 一个 inventory parent，以及每个 Pulse stream 只允许一组最多两个 association helpers，足以闭合本轮识别的 ownership 缺口。** 前提就是 Root 提案中规定的：parent、helper pair 和 leaf 的实际所有权不随 observer timeout/drop 释放，直到 native thread/worker/helper child 实际 join/reap；Closing封闭新entry；actual ack与result/quarantine分离。无需第四个parent或扩大driver/limited-parser范围。

需要在执行计划里补清数字口径、cancel-safe helper/process trait接缝、replacement串行复用，以及inventory publication的epoch fence。它们是使现有提案可执行的最小澄清，不是新产品需求。

唯一写入是本报告。旧闭合/audio preflight报告、Rust/Cargo、计划、归档、全局设置均不改；无stage/commit、子代理或测试。知识模块唯一Rust/Cargo作者仍为`knowledge_delete_completion_impl`。观察HEAD为`b72556091fc533dafb0a336907bf077fec377615`，并非已独立接受的repaired BASE；staged `.gitignore` SHA256保持`1e708ede61f6522d88e4bea58f5c55e70831f404fd00f7a6710d424446e57a3a`。

以下行号均针对当前`src-tauri/src/`生产文件，没有重读整个Task6计划。

## 数量与真实路径

| 拟议parent | 真实生产路径 | 有界leaf口径 |
| --- | --- | --- |
| stream/open parent，最多2个 | `inventory.rs:245–264`现有四个endpoint名/two pairs限制；CPAL走`native_io::open_owned`，Pulse走`pulse_io::open`。 | 一个parent覆盖一次完整capture/playback pair的open、active、failed-open和retired实际工作；不等于单个capture child。 |
| 每个Pulse parent的association pair | `pulse_io.rs:516–525`open/readiness用capture/playback两个`associated()`；`828–847`monitor同样可能并行两个。 | **最多两个临时helpers**；同parent只能有一个pair在途。open phase与monitor phase交接也要继承该pair的actual drain，不能分别建两套可重入gate。 |
| Pulse parent的长期children | `pulse_io.rs:517–518`spawn一个capture与一个playback；584–633转移给capture/playback workers，并另有monitor task。 | 长期children为2个，另加最多2个association helpers；一个stream parent最多4个实际OS children。workers/monitor是同parent下的任务，不占新root parent。 |
| inventory parent，最多1个 | `inventory.rs:279–293`已有native_inventory_gate=1及owned blocking permit；Pulse分支276–277当前未使用该gate。 | Pulse `pulse_io.rs:136–140`**同时启动sources/sinks/modules三个pactl helpers**。inventory不是two-helper pair；一个inventory parent允许固定最多3个helper children，并等待全部actual exit。CPAL parent覆盖完整owned native_inventory closure。 |

按该拟议scope，Pulse路径同时拥有的OS children最多为`2 × (2 stream children + 2 association helpers) + 3 inventory helpers = 11`，其中临时helpers最多7个。这个界限不包含任意新进程/重复poll；它只有在timed-out/cancelled group仍占原slot、不能启动下一组时成立。一个helper先结束不会授权新pair替换另一仍未reap的helper。

native inventory closure内已有status/enumeration调用（inventory339–346）留在同owned parent内；无需改其driver算法或新增privileged动作。Capture/Playback现有stream-drop→worker.join内部实现保持不变。

## 最小澄清1：parent retention与endpoint policy是两件事

Root选择复用two-pair admission上限是足够的，但不能直接把当前lease表当actual join proof。

`Lease::release`（inventory100–109）在非quarantine时删除endpoint项；quarantine时即使late release也保留项。当前native owner在thread尾部、实际outer-thread join之前调用release（native_io325–336）；failed-open其他路径也在thread返回前release（216–237）。当前outer thread handle未保留（206–344）。

因此Task2要明确：pair parent的实际slot/receipt留到outer thread或Pulse aggregate真正join之后，才能复用；不能在当前release点就允许第三个未join stream owner进入。实际joined receipt独立于endpoint admission状态：已join但quarantined可以满足audio actual drain，endpoint仍NeedsRestart/拒绝复用。close不得等待“quarantined lease map为空”来证明结束，也不得因完成actual join清除quarantine。

这个澄清不要求第三个stream slot或重写capture/playback，只要求`inventory/native_io/pulse_io`的source把已有pair identity与独立actual completion正确绑定。

## 最小澄清2：owned helper/process接缝必须在spawn前存在

当前`ProcessSpawner::spawn`（process_io40–41）没有parent/actual-child能力；`ProcessControl`仅wait/terminate（30–32）。`output_with_deadline`把spawn/read/wait放在observer deadline内，error后另给terminate2s，但会返回原result（158–201）。ordinary supervise可在kill失败或reap timeout后发布Err并结束（99–110）。这些返回值不能承载A6-8所需的actual leaf retention。

最小执行责任是一个窄的**owned audio spawn/helper接缝**，例如`spawn_audio_owned`/`output_audio_owned`或等价private adapter；名字可冻结，但它必须具有以下能力：

- 调用helper之前已持有原parent及association-pair/inventory-group permit；actual leaf在OS spawn/native进入边界登记。不能只在await拿到ProcessPipes之后登记，不能假定spawn observer返回Err就没有已进入child。
- 每个child有独立positive actual-exit/reap port；observer error/timeout/drop不销毁该port，不释放parent或pair slot。ordinary audio owner继续拥有Child并承担actual reap，直到真实wait完成；kill_on_drop或cached Err不代替reap。
- pair owner持有两child的actual receipts，pair result与actual completion分开。`associated`结果false、JSON parse error、被select取消或monitor coroutine返回都不会释放仍存活leaf的名额。
- default/fixture实现若没有真实actual source必须fail closed，不能用Result/boolean/`Arc<()>`合成成功receipt。优先audio专用端口，保持unrelated `spawn_limited`/limited-parser与privileged driver语义不变；若扩展shared trait，明确相应implementor/wiring责任。

open/readiness的helper调用在`pulse_io524`，monitor调用在845；两者必须使用**同一个parent的pair gate**。目前monitor的cancel/abort select可drop整个check future（840–846）；拟议实现应让retained pair/child owner独立完成actual cleanup。相同parent在该pair实际reaped前不进入下一poll，也不以新的pair代替失败旧pair。

inventory的三个`process_io::output`调用（136–140）全部走同一inventory parent的owned group。即使其中一个result已Err，另外两child及其retained reap仍归该group；下一inventory request不能复用root slot。JSON/result观察结束后也不能绕过actual child drain。

## 最小澄清3：retired replacement必须串行，不借inventory或第三个stream slot

当前`AudioOperation::replace`先suppress old control，再spawn一个operation；operation先bounded close old（resources120–123），无report failure/cancel时立刻`audio.open`（129）。actor recovery走这一生产路径（direction572–575、604–609）。

在A6-8中，**close reply与旧parent实际join必须分开**：new open只能在old native aggregate已actual join、原pair可安全复用之后进入。不能仅以`close_audio`无failure授权复用，而old outer thread/helper still awaiting join；也不能暂借inventory parent为第三个stream owner。若old actual join仍pending或Closing，保留旧parent，返回/记录已有bounded observation失败，停止新open。

`cancel_and_settle`即使abort后await到AudioOperation JoinHandle，也可能只有Err/no ports或retired control（resources157–183）。actual parent receipt应在backend进入时已独立登记，因此不依赖该operation结果是否仍有AudioPorts；observer/acquisition task退出不释放未join parent。

resume当前会inventory再open（direction866–881）。existing runtime可能使用一个stream parent同时等待inventory parent，这不会需要第四个root slot；但inventory helpers不能反过来申请stream root，且Closing后的internal capability只允许既有工作的cancel/discard/drain/join，不得用来创建新的resume/open/inventory/poll。必要in-flight teardown使用原permits，不重新走external Ready admission。

## 最小澄清4：backend entry/record必须按同epoch封闭

Root已把scope定为elected audio backend generation，解决了原报告中的inventory范围歧义。必须在backend入口执行，不能仅依赖AppState的session admission锁：`app/mod.rs:306–310`endpoint/driver-status调用直达inventory；325–339的validation也可走prepare/inventory；只有create_session353–359持该锁。

建议执行口径：external inventory/open先按generation admission取owned slot，并在可能的await/queue之后、进入native工作之前原子复验Ready/同epoch；Closing封闭新external entry，不能让一个旧Ready检查后的等待者在ack后spawn。已经登记的parent则由close捕获并实际join，不因caller取消消失。Pulse和CPAL的inventory parent gate统一放在branch之前，保持root capacity1。

还有一个小型source接续点：当前`inventory`在native await之后调用`self.record(driver,rows)`（inventory295），记录/替换snapshot。这也是old operation的publication。owner应覆盖bounded parse/该completion或按同epoch复验后拒绝late record；不能native children刚reap就释放slot/发布ack，而一个旧observer随后更新新generation snapshot。不要把post-result publication交给可能已经消失的observer来完成所有权释放。

## Close ack与notification消费

拟议顺序可以保留：同close key捕获backend generation及runtime/lifecycle接续 → 同步Closing且封闭external entry → prompt early stop → reacquire final admission/stop → await实际parent/thread/worker/child join与lifecycle producer quiescence → publish same-key AudioCloseAck。

现有close election先signal jobs/probes，再运行audio early/final分支（lifecycle193–219）。Task2必须在这些wait之前加入sync audio/delivery Closing与同key AudioCloseJoin捕获；result/Jobs错误仍不能弃置该receiver/owner。orchestrator先completion publication再active移除（408–419），`stop_active`无attached时直接Ok（506–517），所以这两个现有值都不能替代captured actual receipts。

consumer保持同key/UUID join观察；timeout不rearm、不重开scope。真正ack之后Receiver.close并recv到EOF，等待所有preissued OwnedPermits，不以buffer empty作为done；再走local journal tail/child1/delivery+jobs join。stop/RuntimeOutcome/diagnostic failure与actual joined各自保留，quarantine/NeedsRestart不改。整个工作不新增用户审批、schema或paid/native动作。

## Exact source / trait responsibility补充

| 文件 | 相对Root提案应补清的最小责任 |
| --- | --- |
| `audio/runtime.rs` | backend-generation Closing/actual join窄端口；AudioControl的parent actual-join port独立于close result。public command shape不变，opaque能力无Serialize/default-success。 |
| `audio/platform/inventory.rs` | two-pair actual parent identity/receipt与lease policy分离；一个owned inventory gate同时覆盖Pulse/CPAL；post-await entry与record同epoch复验，late observer不释放root slot。 |
| `audio/platform/native_io.rs` | 保留outer thread/failed-open actual join；lease/parent复用点移到真实join接续，command observer cache不覆盖actual receipt。 |
| `audio/platform/pulse_io.rs` | 同parent的一个association-pair gate统一覆盖open+monitor；stream长期children2、helpers≤2；inventory group helpers3；monitor/drop/failed-open取消后leaf仍retained。 |
| `audio/platform/process_io.rs` | owned-audio spawn/helper与positive actual-exit/reap source，登记在spawn边界；ordinary observer Err后仍有actual Child责任。shared trait默认不能伪造proof，limited/privileged逻辑保持原范围。 |
| `session/resources.rs`、`session/direction.rs` | retired old parent actual join后才new open；no-ports/aborted acquisition仍有backend receipt；Closing只允许原scope teardown，resume/poll不能借internal能力重入。 |
| `session/runtime.rs`、`session/orchestrator.rs` | 真实actor/native与observer/lifecycle producer receipt汇聚，active消失后仍能joinold owner；不能从Outcome/status生成ack。 |
| `app/{mod,lifecycle,intelligence}.rs`、`summary/terminal.rs` | close election中的同步signal/capture producer；同key retained receiver与error路径不丢owner；实际ack后drain OwnedPermits到EOF。 |

无需修改audio capture/playback internals；也没有证据需要扩张privileged driver、limited-parser或generic scheduler。上述新增端口当前均未实现，不能由报告或计划名字自证。

必要行为验证仍归未来Task2：hold旧helper pair actual reap后重复tick不能spawn新pair；hold某个inventory child后下一inventory不进；failed-open无ports仍占slot；old close reply已到但thread未join时replacement不产生第三parent；late inventory record被同epoch fence拒绝；Closing排队entry与teardown能力分开；诊断/result失败但joined有效；ack后preissued permit晚send与timeout同key retry。这些须穿过真实production collaborators，**本轮没有写或运行测试**。

范围判断：拟议三个retained parent与two-helper pair是可接受的最小闭合，按上述source接缝实现即可；不再增加parent capacity或新增provenance要求。本轮只创建这个follow-up报告。
