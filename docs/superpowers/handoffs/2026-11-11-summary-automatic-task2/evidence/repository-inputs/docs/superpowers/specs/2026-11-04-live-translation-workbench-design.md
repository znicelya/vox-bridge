# Vox Bridge 实时同传工作台设计

## 目标

Vox Bridge 是一个 Windows、macOS、Linux 桌面应用，面向直播和电脑通话提供实时单向/双向同传。首发版本必须在三个平台完成虚拟音频设备安装与路由，软件内实时展示原文和译文，并把译文音频送入直播或通话链路。面对面拾音不属于首发范围。

## 已确认的范围

- 直播：用户选择任意输入设备，单向翻译并把译文音频写入 M1 虚拟麦克风；软件显示原文和译文。
- 通话：双向翻译，路由固定为：
  - 物理麦克风 A → 虚拟声卡 A → Vox Bridge 采集 → 同传 → M1 虚拟麦克风 → 通话软件麦克风 → 对方 B。
  - 对方 B → 通话软件扬声器 → M2 虚拟扬声器 → Vox Bridge 监听 → 同传 → 物理扬声器 → 我 A。
- 通话软件选择 M1 作为麦克风、M2 作为扬声器；应用不静默修改系统默认输入/输出设备。
- 会话语言由用户手动选择一对源语言/目标语言；不依赖自动语言识别。
- 首发真实 provider 为火山引擎 AST；阿里百炼和 Palabra 先提供统一适配器、能力声明和测试 fixture，不把真实联调列为首发验收条件。
- 会话记录默认保存完整最终原文、译文、时间戳、语言、provider、延迟指标和错误；只存本机，不上传 Vox Bridge 自有服务器。默认不保存原始音频或译文音频块。
- 会话结束自动生成总结，并支持手动重跑/更新；总结使用可配置的 OpenAI 兼容 LLM。
- 知识库手动导入 PDF、Word、文本和 Markdown；支持选择实时字幕检索或在独立搜索框输入关键词。向量索引使用 SrvDB，元数据使用 SQLite，Embedding 使用可配置的 OpenAI 兼容端点。
- 客户背调支持手动输入公司名/网址，或从字幕抽取候选实体后由用户确认；首发搜索 provider 为 Tavily，来源 URL、摘录和抓取时间必须随结果保存。
- 第三方服务密钥写入本机加密配置；首密钥由系统密钥库保护，用户不需要额外设置应用主密码。密钥不进入普通日志。

## 平台支持矩阵

首发最低版本：Windows 10 22H2、macOS 13 Ventura、Ubuntu 22.04 LTS。Linux 同时覆盖 PipeWire 和 PulseAudio 常见配置。首发安装流程可以申请管理员权限；macOS 可以要求用户在系统设置批准音频扩展并按系统要求重启。

三平台都必须提供真实可用的虚拟设备安装、卸载、升级、修复、设备枚举和健康检查。设备名称固定为逻辑端点 `VoxBridge Input`（M1，虚拟麦克风）和 `VoxBridge Output`（M2，虚拟扬声器）。平台适配器隔离驱动安装、权限、设备发现、采集和播放差异，上层会话代码不包含平台分支。

## 总体架构

应用沿用仓库现有 Tauri 2 + React/TypeScript 骨架。React 只负责界面状态、设置表单和事件展示；Rust 负责音频任务、驱动生命周期、WebSocket、provider 适配、密钥、SQLite/SrvDB 和会话生命周期。所有网络凭证只在 Rust 后端读取，前端收到脱敏后的状态与结果。

主要模块：

1. `SessionOrchestrator`：创建/停止直播或通话会话，管理语言对、provider、方向、设备和持久化状态。
2. `AudioCapture`：从指定物理输入、VoxBridge Output 或直播设备采集音频，输出带来源标记的规范化 PCM 帧。
3. `TranslationProvider`：统一的 start/send/finish/cancel/reconnect 接口和 provider 能力声明。
4. `AudioRouter`：把 provider 的译文音频写入 M1 或物理输出，并处理重采样、设备断开和有界背压。
5. `EventBus`：向前端发布临时原文、最终原文、译文文本、译文音频状态、连接状态、错误和延迟分段。
6. `LocalStore`：SQLite 会话、文本事件、总结版本、背调来源和知识库元数据。
7. `KnowledgeIndex`：文档解析、分块、Embedding 调用、SrvDB 写入和本地语义搜索。
8. `ResearchService`：Tavily 请求、来源存档、候选实体确认和可选 LLM 归纳。

## 实时数据流

`AudioCapture` 将所有输入先规范化为 16 kHz、16-bit、单声道 PCM，并以带单调序号的帧进入有界队列。默认帧目标为 80 ms；队列满时按来源丢弃最旧临时帧并发布背压指标，不能创建无界内存队列。

直播只创建一个 provider 流。通话创建两个彼此独立的流：A→B 和 B→A；一侧重连、暂停或失败不能阻塞另一侧。每个流都包含采集开始时间、发送时间、服务商响应时间和播放时间，用于计算采集、网络、服务处理和播放分段延迟。约 1 秒是客户端回归目标，不是对第三方服务的硬承诺。

收到临时事件时只更新内存字幕；收到最终事件时写入 SQLite。字幕事件必须保留来源方向、provider 原始事件类型、序号、起止时间、临时/最终状态和关联音频句子 ID。

## 音频格式策略

内部播放总线使用 24 kHz、16-bit、单声道 PCM。火山和阿里直接发送 16 kHz 输入并请求 24 kHz PCM 输出。Palabra 的边界适配器把内部 16 kHz 输入重采样为文档要求的 24 kHz `pcm_s16le`，并把返回的 24 kHz PCM 交给播放总线。根据物理/虚拟设备协商结果，`AudioRouter` 再做最终重采样。

### 火山引擎首发适配器

实现依据 `docs/同声传译/火山引擎/doc.md`：

- endpoint：`wss://openspeech.bytedance.com/api/v4/ast/v2/translate`。
- 新版鉴权：`X-Api-Key` 和固定 `X-Api-Resource-Id: volc.service_type.10053`。
- WebSocket 业务协议使用 protobuf；收到 `SessionStarted`（150）后才发送音频。
- 发送事件：StartSession 100、TaskRequest 200、FinishSession 102。
- 接收事件：SourceSubtitleResponse 651、SourceSubtitleEnd 652、TranslationSubtitleResponse 654、TranslationSubtitleEnd 655、TTSResponse 352、SessionFinished 152、SessionFailed 153、AudioMuted 250。
- 输入是 16 kHz/16-bit/mono raw PCM，建议约 80 ms 一包；s2s 返回目标音频，首发请求 24 kHz PCM。
- provider 不支持在一个会话内切换语言或 mode；配置变更必须结束并重建连接。
- 适配器记录服务端 `X-Tt-Logid`，日志只存 logid、耗时和错误码，不存密钥与音频。

### 阿里百炼和 Palabra 适配器

两者共享 `TranslationProvider` 接口，但保留原生协议：阿里使用事件 JSON 与 Base64 PCM（16 kHz 输入、24 kHz 输出）；Palabra 使用带随机路径的 WebSocket、`set_task`、Base64 `input_audio_data`/`output_audio_data`，示例要求 24 kHz `pcm_s16le`、320 ms 分片。每个适配器必须提供能力声明、协议 fixture、错误映射和禁用原因；在 provider 选择器中显示“未联调”而不是伪装为可用。

## 本地数据模型

SQLite 表至少包括：

- `sessions`：模式、方向、语言对、provider 配置名、设备 ID、开始/结束时间、状态和总结状态。
- `translation_segments`：session ID、direction、sequence、source text、translated text、start/end time、final flag、provider event type、latency fields。
- `session_summaries`：session ID、version、LLM endpoint/model、structured JSON、raw response hash、created_at、status。
- `documents`：原文件路径、复制后的本地路径、SHA-256、类型、版本、解析状态和错误。
- `document_chunks`：document ID、chunk ID、页码/段落、文本、文本 hash、embedding model/dimension 和 SrvDB ID。
- `research_runs` / `research_sources`：查询、确认实体、Tavily 请求状态、标题、URL、摘录、发布日期、抓取时间和归纳版本。

会话删除使用事务和级联清理；导出提供 Markdown、JSON 和带时间戳纯文本三种格式，默认不导出音频。应用启动时检查 SQLite 与 SrvDB 的分块 ID/哈希一致性，缺失或版本不符的向量进入可重建队列。

## SrvDB 集成

使用 SrvDB 的 Rust crate，固定到已核验的 0.3 代码版本 `1eec46feb2b7e6a6e286fa3361a20cd771dd4e04`。SrvDB 数据目录与 SQLite 数据目录并列但分离。每个向量 metadata 保存 `document_id`、`chunk_id`、页码和内容 hash；SQLite 是文档元数据的事实来源。导入、更新、删除遵循 SQLite 元数据变更、SrvDB upsert/delete、`persist()` 的顺序，并在启动校验失败时修复。

## 总结与背调

`LlmProvider` 接受 Base URL、模型、API key、请求超时和最大上下文配置；总结输入只包含最终文本，输出要求严格 JSON：摘要、参与方/公司、需求、承诺与行动项、风险/待确认项、术语。解析失败不丢失原始响应，标记为失败并允许重跑。总结版本可查看、比较和删除。

`BackgroundResearchProvider` 首发实现 Tavily。字幕抽取只产生候选，不自动联网；用户确认公司名/网址后才请求。原始来源先本地保存，LLM 归纳必须引用来源 URL；没有来源的推测不能写入事实字段。API 超时、限流、来源失效和无结果均显示在相应操作附近。

## 界面设计

界面采用已确认的 Taste `framed-sheet` app-screen 方向，组件实现遵循 shadcn/ui：淡色 L 型外框、228 px 侧栏、48 px 顶栏、右下 8 px 内缩的白色工作表、1 px 边线、无静态阴影。IBM Plex Sans 为界面字体，IBM Plex Mono 用于 session ID、设备 ID 和延迟数字。颜色预算低于 3%，颜色只用于实时译文、连接/设备状态、主数据和一个主要动作。

实时工作区包含：直播/通话 Tabs、语言对、provider、连接状态、延迟分段、双向最终字幕、M1/M2 路由、知识库选中检索、Tavily 候选确认和会话结束入口。窄窗口把侧栏折叠为菜单，将研究/知识库工具移入 Sheet，字幕流保持首要位置。

设置按音频设备与驱动、同传服务商、LLM/Embedding、Tavily、数据与导出五组组织。每组提供连接测试、权限状态、能力限制和最后测试时间。错误信息必须靠近触发点并给出下一步。

## 错误和恢复

- 网络短抖动：provider 自动重连，字幕标记缺口并继续计时。
- 超过重试窗口：暂停该方向，保存已有最终段，保留重连按钮；不影响另一方向。
- 服务商格式/鉴权/限流错误：停止该方向，显示 provider 错误码、logid（如有）和修复动作。
- 音频设备断开：立即停止向当前设备写入，显示设备断开；设备恢复后必须用户明确继续。
- 驱动权限未批准：显示平台专属安装步骤和重新检测按钮，不假装设备可用。
- SrvDB/SQLite 不一致：阻止该文档的检索，后台修复或要求重新索引；不删除原文件。
- LLM/Tavily 失败：保留会话/来源，标记总结或背调失败，可单独重跑。

## 验收标准

- 三个平台完成安装、权限、卸载、修复、设备枚举和 M1/M2 路由；最低版本为 Windows 10 22H2、macOS 13、Ubuntu 22.04 LTS。
- 直播单向能稳定采集用户选择的设备、连接火山、显示最终原文/译文并把译文送入 M1。
- 通话双向同时运行两个独立 provider 流，A→B 写入 M1，B→A 播放到物理输出，软件显示两侧文本。
- 火山 protobuf 事件与 TTS PCM 能在 fixture 和真实凭证测试中正确归一化；阿里/Palabra stub 能通过合同测试并正确显示能力限制。
- 断线、设备断开、服务商错误和背压行为符合恢复规则，不发生无界队列或误输出。
- 会话最终文本可本地查看、删除和导出 Markdown/JSON/纯文本；结束后自动总结并可重跑。
- PDF/Word/文本/Markdown 导入能建立 SQLite 元数据和 SrvDB 向量；选中字幕和关键词两种检索均返回来源段落。
- Tavily 只在用户确认实体后请求，结果保存 URL/摘录/时间并可由 LLM 生成带引用的概览。
- 密钥不出现在前端、普通配置明文或日志；本机加密配置可在应用重启后读取。
- 采集、网络、服务商处理、播放延迟分段可见；约 1 秒作为客户端回归目标，不承诺第三方服务延迟。

## 分阶段交付

1. **会话底座**：Rust 会话状态机、SQLite schema、加密配置、事件总线、导出和 provider trait。
2. **跨平台音频**：三平台虚拟设备安装/权限/枚举、规范化采集、重采样、M1/M2 路由和故障恢复。
3. **同传适配**：火山真实 WebSocket/protobuf 适配、阿里/Palabra capability/stub、fixture 和真实凭证集成测试。
4. **工作台与智能工具**：shadcn 风格 React UI、SrvDB 知识库、Embedding、LLM 总结、Tavily 背调、会话历史和设置页。

## 主要风险

- 虚拟音频驱动签名、macOS 系统扩展批准和 Linux 音频服务差异是首发最大交付风险，必须以三平台真实设备测试作为发布门槛。
- 火山 protobuf 定义和示例附件需要在实现阶段纳入可构建的 proto 资源；文档中仅有协议说明不能替代编译输入。
- 第三方 provider 的网络、并发、计费和服务质量不可由客户端保证，客户端只能测量、限流、重连和提示。
- SrvDB 版本/磁盘格式固定后，升级必须经过索引迁移和恢复测试。
