# auto-game 开发演进全记录（CHANGELOG）

> 本文档按**时间与里程碑**完整记录 auto-game 从 0 到 1 的每一次功能变更、新增、修改与优化，并逐项解释**「为什么」**：新增的原因、修改的原因、优化的原因、依赖选型的原因。
> 项目周期：**2026-08-31 ～ 2026-09-02**（当前 HEAD 见文末）。版本 `0.1.0`。
> 阅读建议：先看「一、顶层设计决策」理解全局，再看「三、里程碑演进」看过程，最后看「四、机制复盘」理解关键实现原理。

---

## 一、项目定位与顶层设计决策

### 1.1 项目定位

**auto-game = 通用电脑游戏自动化测试框架**。不针对单一游戏，而是把「**截图 → 识别 → 决策 → 输入**」抽象成可插拔的动作原语，用 **TOML 配置文件**描述测试流程，对任意 PC 游戏执行自动化测试。

> 一句话：**游戏就是被测对象，场景文件就是测试用例，报告就是测试结果。**

### 1.2 为什么选「纯视觉方案」（最重要的架构决策）

开发初期在「读内存 / 注入 / 纯视觉」三条技术路线之间做了取舍，最终**坚持纯视觉**（只截图 + 模拟键鼠，不读内存、不注入）：

| 路线 | 优点 | 致命缺点 | 结论 |
|---|---|---|---|
| 读内存（游戏基址+偏移） | 精确、快 | **触碰反作弊红线，极易封号**；每个游戏/版本都要重做偏移表 | ❌ |
| DLL 注入 | 可控性强 | 反作弊检测高危、维护成本极高 | ❌ |
| **纯视觉（截图+模板匹配+键鼠模拟）** | **不触碰游戏进程，反作弊风险极低**；跨游戏通用 | 慢一点、对画面变化敏感（有对策） | ✅ |

> 该决策被写进 `docs/design.md` 的原则："**纯视觉方案（不读内存、不注入），规避反作弊风险**"，并在「九、不做的事（边界）」中固化：不做内存读取、DLL 注入、反作弊绕过。

### 1.3 为什么「复用成熟核心库」而不是自研

调研发现（`design.md` §1）：**「截图 + 输入 + 模板匹配」这三件事都有成熟、被生产验证的 Rust 库**，自研既慢又容易出 bug。因此上层价值不在底层能力，而在：

- **脚本引擎**（TOML 配置驱动，不写代码也能编排）
- **报告系统**（文本 + HTML，状态/耗时/证据链）
- **模板采集**（代码内截图生成模板，形成闭环）
- **匹配加速**（金字塔 + 并行，解决朴素 NCC 太慢的痛点）

这一层正是当时开源空白，也是本项目真正的增量。

### 1.4 配置驱动的意义

测试场景 = TOML 文件。**换游戏不换代码**：两个目标游戏（龙之谷、阴阳师）的差异全部通过「场景文件 + 模板资源 + region」隔离，代码层面零改动。

---

## 二、依赖选型决策详解（为什么选这些库）

选型在 `docs/design.md` §1 有结论表，以下是**每个选择的完整理由**：

| 能力 | 候选 | 选中 | 详细理由 |
|---|---|---|---|
| 截图 | rustautogui / **xcap** / screenshots | **xcap 0.2**（实现）<br>rustautogui 2.5（保留） | xcap 支持**多显示器、指定窗口捕获**（`capture_window`），这是后续「窗口级捕获」和「模拟器多开」适配的前置；rustautogui 自带截图与找图同源，保留作备选/内置兜底 |
| 输入模拟 | enigo / rustautogui / autocontrol | **enigo 0.2** | enigo **跨平台**、被 **RustDesk 生产环境验证**；通过 trait 抽象便于后续替换后端；rustautogui 保留在 Cargo.toml 作备选 |
| 模板匹配 | imageproc / rustautogui(分段/FFT) / template-matching(GPU) / opencv | **imageproc 0.25** | `template_matching`（**CrossCorrelationNormalized**）+ `find_extremes` 是标准 NCC 方案，**纯 Rust 无重依赖**（不像 opencv 需要绑定）；GPU 版留给未来性能升级 |
| 紧急停止 | device_query | **device_query 4.0.1** | 轮询 `F9` 键状态实现 failsafe；`new()` 失败时自动降级禁用，不阻塞主流程 |
| 并行加速 | rayon | **rayon 1** | 金字塔候选精匹配用 `par_iter` 并行，Rust 数据并行的事实标准 |
| 图像缓冲 | image | **image 0.25** | 截图缓冲、模板加载、PNG 存盘的统一类型（`RgbaImage`），与 imageproc 同生态 |
| 配置 | serde + toml | **serde 1 + toml 0.8** | Rust 标准做法，`#[derive(Deserialize)]` 直接映射 TOML → 结构体 |
| 错误处理 | anyhow | **anyhow 1** | 库 + 应用层统一 `Result`，`{:#}` 打印上下文链 |
| 日志 | tracing | **tracing 0.1 + tracing-subscriber 0.3** | 结构化、分层过滤；用于 `verify_exact` 开启提示、failsafe 降级告警等 |

> **rustautogui 为什么保留但实现未用**：它是 PyAutoGUI 的 Rust 版，自带截图 + 找图，作为「一键式」备选集成在 Cargo.toml；但它的找图（分段/FFT）在纯 Rust 生态里不如 imageproc 标准、可控，故**实现走 imageproc**，rustautogui 留作后续对比/替换的选项。这是「广调研、精落地」的体现。

---

## 三、里程碑与功能演进全记录

> 每个小节 = 一个里程碑；每项功能均标注：**功能、原因、涉及文件/提交、验证**。

### 阶段 0：项目初始化（2026-08-31）

| 变更 | 内容 | 原因 |
|---|---|---|
| 创建 GitHub 仓库 `auto-game` | 空仓库 + README | 用户需求：在 GitHub 建一个 Rust 仓库，「完成电脑游戏的自动化测试」 |
| `Cargo.toml` 初始化 | 声明包名/版本/依赖 | Rust 项目骨架 |
| `main.rs` 程序入口 | 占位入口 | 可运行的最小可执行体 |
| `.gitignore` | 忽略 target、本地验证截图等 | 避免把构建产物/本地验证图提交进仓库 |
| `docs/design.md`（首次 179 行） | 完整设计文档：定位、架构、模块、脚本格式、风险 | 用户要求「先看详细的设计文档」——先设计后编码 |
| 依赖选型更新 | 按设计文档调整 Cargo.toml（14+/9-） | 设计定稿后反哺依赖清单 |
| 游戏适配章节（第 10 节） | 龙之谷（PC 端游）+ 阴阳师（安卓模拟器）适配要点 | 用户指定首批目标游戏，把差异写进设计 |

**设计阶段的关键结论**（提前定死，后续未走样）：
- 龙之谷：**固定窗口模式** + 模板只针对静态 UI + WASD/key_press 移动 + 等待步骤设足 timeout（网络波动）
- 阴阳师：**模拟器窗口**捕获 + 多开须指定标题 + 随机弹窗用 `if_image` 容错 + 点击类操作为主

### M0：骨架（2026-08-31，提交 b47f6ab ~ c564629）

| 变更 | 内容 | 原因 |
|---|---|---|
| `lib.rs` 库入口 | 对外库 API | 供二次开发 / 将来 GUI 调用（预留） |
| `adapter/mod.rs` 模块入口 | Region / Key 等公共类型汇总 | 「可插拔后端」的代码落点 |
| `adapter/input.rs` 输入后端 | enigo 封装（move/click/key/type） | 输入能力落地 |
| `adapter/vision.rs` 识别后端 | Vision 抽象雏形 | 识别能力占位（M1 落地） |
| `adapter/capture.rs` 截图后端 | xcap 全屏截图 | 采集能力落地 |
| `main.rs` 接入验证 | 截图保存 PNG + 模拟一次键鼠 | M0 验收：**跑通采集与输入链路** |

> M0 的意义：**先打通「能截图、能输入」的最小闭环**，验证 xcap/enigo 在本机可用，为 M1 视觉识别铺路。

### M1：视觉原语（2026-08-31，提交 cebfabb ~ 997e947）

| 变更 | 内容 | 原因 |
|---|---|---|
| 添加 imageproc 依赖 | `Cargo.toml` +2 | 模板匹配核心库落地 |
| `vision.rs` 落地模板匹配 | `CrossCorrelationNormalized` + `find_extremes` 实现 `find_template` | M1 核心：从「截到图」到「认得出」 |
| `action.rs` 动作原语 | `find/click/wait/assert` 四个图像动作 | 把「找图→点击→验证」封装成可复用动作 |
| `main.rs` 模板匹配演示 | 手写调用演示 | 验证「找图→点击→验证」闭环 |
| 性能问题暴露 | **全屏朴素 NCC 太慢**（500×400 debug 约 78s，全屏更久） | 实测暴露；**修改原因**：朴素算法扫全图是 O(图×模板) 暴力，不可用 |
| `main.rs` 改小区域匹配演示 | 用 `capture_region` 只匹配小区域 | **优化原因**：临时规避全屏性能瓶颈，等加速方案（打磨轮解决） |
| 风险与对策条目 | design.md 记录「模板匹配计算量大」 | 把已知性能风险写进文档，留待专门优化 |
| 提交 Cargo.lock | 锁定依赖版本 | 保证可复现构建 |

> M1 留下的核心矛盾：**「认得出」但「太慢」**。这条线一直延伸到打磨轮的「金字塔 + 并行加速」（见 3.6.7）才彻底解决。

### M2：流程引擎（2026-08-31，提交 8c4b083）

| 变更 | 内容 | 原因 |
|---|---|---|
| `script.rs` TOML 解析 | `Scenario`/`Step` 反序列化，`[[step]]` 映射 | **配置驱动**的核心：场景 = 文件，不写代码 |
| `engine.rs` 流程引擎（206 行） | 顺序执行 + 超时 + 动作分发 | 把步骤序列真正跑起来 |
| `report.rs` 报告（80 行） | 文本报告 + 通过/失败/耗时汇总 | 自动化测试必须能看出「哪步过了、哪步挂了」 |
| `scenarios/demo.toml` 冒烟场景 | 全链路冒烟 | M2 验收：**纯配置文件跑通一个冒烟场景** |
| `main.rs` 改造为 CLI | `run <场景.toml>` 入口 | 从「演示代码」升级为「可执行工具」 |

### M3：控制流与 HTML 报告（2026-08-31，提交 6b69897 ~ 6ab5611）

| 变更 | 内容 | 原因 |
|---|---|---|
| `script.rs` 支持 `count` | repeat 循环计数 | 真实游戏场景需要「重复刷本 N 次」 |
| `engine.rs` 循环/条件分支编译执行（+210） | `repeat/end_repeat`、`if_image/else/end_if` 编译成带跳转指令 | **真实场景需求**：处理「遇弹窗关闭」「重复刷本」；未闭合控制结构编译期报错，不带病执行 |
| `report.rs` HTML 报告（+89） | 状态着色、耗时统计、HTML 转义防注入 | 控制台报告不够直观；转义防注入是安全基线 |
| `scenarios/m3_demo.toml` | 控制流演示（循环+分支） | 验证新能力 + 提供可跑示例 |
| design.md 控制流语法说明 | 语法 + 迭代进度同步 | 设计文档与实现对齐 |

> M3 的意义：**脚本从「线性执行」升级为「可编程」**——能循环、能分支，这是覆盖真实游戏复杂流程（如自动刷本 + 弹窗处理）的前提。

### 打磨轮（2026-09-02，提交 2b177a9 ~ 3fe3e8d）——用户驱动的密集优化

这一阶段全部由**用户的实际使用痛点/提问驱动**，是理解本项目「为什么」最丰富的部分。按时间顺序：

#### 3.6.1 key_combo 组合键 + 全量按键扩展（提交 0b58f54 / 4c1a285 / f1ed970 / c482abd）

- **功能**：`key_combo` 支持 Ctrl+A 等组合键；`Key` 枚举扩展到 WASD / 数字 / 方向键 / F1-F12 / 编辑键 / 修饰键，含别名（esc/return/pgup…）。
- **原因（新增）**：真实游戏操作不止单键——「选中全部」「技能组合」「菜单快捷键」都需要组合键；龙之谷的 WASD 移动、F1-F12 技能键也要能按键。
- **原因（修改）**：原始 `Key` 枚举只覆盖少量按键，无法支撑游戏操作，故扩展为全量游戏键位。
- **涉及**：`input.rs`（+275）、`action.rs`、`engine.rs`、`script.rs`（keys 字段）、`features.toml` 特性场景、按键映射单测。

#### 3.6.2 region 区域限定匹配（提交 88d50e6 / 4c1a285）

- **功能**：`Region { x, y, w, h }` 结构；`find_image_region` 只截取/匹配目标区域。
- **原因（优化）**：M1 实测全屏 NCC 慢到不可用；**限定区域后计算量从「全屏」降到「小窗口」**，配合 `--release` 才可用。文档强制建议「图像动作都尽量指定 region」。

#### 3.6.3 F9 failsafe 紧急停止（提交 f788073 / f1ed970）

- **功能**：运行中随时按 `F9` 中止场景并记一条 FAIL；非交互会话自动降级禁用。
- **原因（新增）**：脚本失控时用户必须能**立即夺回鼠标键盘**，否则可能乱点造成损失。这是安全兜底，不是功能增强。

#### 3.6.4 template 模板采集子命令（提交 225ea1f / cbddf9f / 17ace69）

- **功能**：`auto-game template --name X --x --y --w --h`（或 `--at-mouse --center` / `--full`）用**代码截图生成模板 PNG** 到 `assets/`，并打印坐标与可直接粘贴的 TOML 片段（`image` + `region`）。
- **原因（新增，用户直接提问驱动）**：用户问「**需要自己截图然后放到 assets 目录下，然后编写 toml 么，可不可以通过代码中的截图功能，这样既可以获取到图形，也能知道相对位置**」——于是把「截图→模板→坐标→场景片段」做成**代码内闭环**，彻底免去手工截图/裁图/量坐标。
- **涉及**：`main.rs`（+165~170）、README 新章节。

#### 3.6.5 失败自动存档 + 新旧对照图（提交 0cb54dd / 96454d6 / 5c14132 / e12b26b）

- **功能**：任一步骤 FAIL 自动保存现场截图（`fail_step_<n>.png`，有 region 截区域否则全屏）；带模板的步骤再生成「**左=旧模板 / 右=现场**」对照图 `diff_step_<n>.png`。
- **原因（新增，用户直接提问驱动）**：用户问「**当定位的内容和实际不匹配的时候，例如由于更新，UI 发生了变更，会怎么样**」——答案是：不猜测、留证据。失败自动存档让 UI 变更「一眼可见」，再用 template 命令重采新模板即可。**这是把「游戏更新导致模板失效」这个预期内的风险转成可排查流程**。
- **涉及**：`engine.rs`（save_failure_snapshot）、`action.rs`（暴露 capture_region）、对照拼接单测。

#### 3.6.6 jitter 拟人化点击（提交 39fa59e / 023ead8 / 8326c6f / ea5eac3）

- **功能**：`click` / `click_image` 支持 `jitter`，每次点击位置在目标 ±N 内随机分布；`click_image` 偏移自动限制在模板范围内；使用**零依赖 xorshift64\* PRNG**（启动时按时间播种）。
- **原因（新增，用户直接提问驱动）**：用户问「**针对点击事件，可以保证每次点击的位置进来不用完全一致么，可以是动态分布的点**」——需求是**拟人化**：不总点同一个像素，更接近真人操作，降低机械重复特征（对挂机检测和「多开识别」都有意义）。
- **为什么零依赖 PRNG**：xorshift64* 十几行即可实现、线程安全、启动播种随机，**不需要引入 rand 整个依赖树**——符合「轻量、可控」的项目风格。
- **涉及**：`engine.rs`（jitter_offset + 报告显示实际坐标）、`script.rs`（jitter 字段）、README 章节。

#### 3.6.7 模板匹配加速：图像金字塔 + Rayon 并行（提交 39106b5 / 96420a6 / a9c0a95 / d4fc761 / 0cd1b30 / 87b27ac）

- **功能**：`find_template` 默认走**图像金字塔粗到细**——先在低分辨率全图粗定位 top-K 候选，再逐层在候选邻域（窗口 = 模板尺寸 + 2×radius）**Rayon 并行精匹配**，直到顶层；小图/小模板自动回退精确匹配。
- **原因（优化，用户直接提问驱动）**：用户问「**还有哪些可以修改的地方么**」→ 明确说「**优先模板匹配加速吧**」。背景是 M1 就暴露的朴素 NCC 全屏太慢（78s+），region 只是临时规避。
- **为什么金字塔 + 并行**：纯暴力是 O(图×模板) 不可行；金字塔把「全屏大匹配」降为「若干小窗口匹配」，并行进一步吃满多核。**语义不变**：仍返回全局最高置信度位置（不是只找局部）。
- **实测（debug，400×300 + 50×40 模板）**：**~14s → ~0.16s（≈88x）**。
- **涉及**：`vision.rs`（+348）、Cargo.toml（+rayon）、design.md/README 同步、Cargo.lock 更新。

#### 3.6.8 fast→exact 确认开关（`[meta] verify_exact`）（提交 978c30c 等）

- **功能**：`[meta] verify_exact = true` 时，每个模板在金字塔定位后再走「**fast 粗定位 → 最终位置精确确认**」：在模板 + 2×radius 邻域内做一次精确 NCC，返回**像素级精确位置 + 准确置信度**；金字塔粗层无候选（极端漏检）自动回退全图精确匹配兜底。
- **原因（新增，用户直接指令驱动）**：用户说「**加一个 fast→exact 确认开关**」。背景：金字塔加速返回的位置 ±1px、置信度是近似值，对绝大多数点击足够，但**"点击必须落在确切像素"的极端需求**（精确像素断言、极小 UI 区域点击）需要确认步骤。**语义**是「确认」而非「全图再 exact」——后者会丢掉加速收益。
- **实测（debug，400×300 + 50×40 模板）**：fast=142ms vs 确认后=166ms（**仅多 ~24ms**）vs 全图精确=11.5s（确认仍 ≈69x 于全图精确）。
- **涉及**：`vision.rs`（match_template_verified + VERIFY_RADIUS=3）、`action.rs`（5 方法透传）、`engine.rs`（Engine.verify_exact + run 同步）、`script.rs`（Meta.verify_exact）、README/design.md 同步、+2 测试。

#### 3.6.9 step 级 verify_exact 覆盖（提交 3fe3e8d）

- **功能**：`Step` 新增 `verify_exact: Option<bool>`——某步骤 `[[step]] verify_exact = true/false` **显式声明时覆盖 `[meta]` 全局值**，缺省回退全局。
- **原因（修改，用户直接指令驱动）**：用户说「**把确认开关做成按步骤粒度……单独覆盖 meta 全局设置**」。背景：全局开关是「一刀切」，但真实场景往往是「**大部分走快路径、个别关键步骤要像素级精确**」（如等 Boss 血条要精确、高频攻击按钮走快路径）。步骤级覆盖让精确只花在刀刃上。
- **语义设计**：`Option<bool>` = 三态（显式 true / 显式 false / 缺省回退），`step.verify_exact.unwrap_or(self.verify_exact)`。
- **涉及**：`script.rs`（字段 + 解析单测）、`engine.rs`（find_match 透传）、README/design.md 同步，测试 25→26。

### M4：窗口级捕获 + 坐标映射（2026-09-02）

- **功能**：`[meta] window = "<标题关键字>"` 启用窗口模式——xcap `Window::all()` 枚举窗口（**已按 z 序返回**），忽略最小化窗口、按标题关键字 `contains` 匹配；多开命中多个实例时取 z 序最前者并打印告警；`capture_window` 捕获窗口图。
- **坐标映射设计（核心）**：窗口模式识别结果统一映射回**屏幕坐标**供输入——`capture_target` 返回（窗口图, 窗口左上角屏幕偏移 ox, oy）；`find_image` 匹配到窗口内坐标后经 `offset_match` 加窗口原点得屏幕坐标；`[[step]] region` 在窗口模式下语义为**窗口内坐标**（相对窗口左上角），经 `region_match_to_screen` 映射回屏幕；`snapshot` / `snapshot_region` 为窗口感知接口（`screenshot` 动作与失败存档只含窗口内容）。
- **原因（新增，M4 里程碑）**：用户拍板「开始 M4」。目标游戏龙之谷是 PC 端游（可窗口化）、阴阳师跑在安卓模拟器（本身即窗口）——**全屏截图会被桌面遮挡、多开时无法区分实例**；窗口级捕获是这两款游戏落地的硬前置。
- **为什么窗口选择规则是「忽略最小化 + 标题 contains + 多开取 z 序最前」**：最小化窗口 `capture_image` 拿不到有效画面必须跳过；标题用 `contains` 而非全等，是因为游戏/模拟器标题常带后缀（版本号、实例名）；多开取 z 序最前 = 最顶层实例，配合「每实例独立场景文件 + 独立标题关键字」精确分流。
- **过程记录（冲突修复）**：用户此前自行 merge 时在 `engine.rs` / `script.rs` / `README.md` / `docs/design.md` 残留**未解决的 git 冲突标记**（`<<<<<<< HEAD` … `>>>>>>>`），导致 `cargo test` 报 `engine.rs:785 unclosed delimiter`（行号实为错位）。逐文件解决冲突（保留带 step 级 verify_exact 的新版），并清理 `find_match` 之后残留的重复 `}`。
- **涉及**：`adapter/capture.rs`（WindowShot/WindowMeta/capture_window/select_window + 4 单测）、`action.rs`（`Actions.window` 字段 + `set_window`/`in_window_mode` + `capture_target` + `offset_match`/`region_match_to_screen` + `snapshot`/`snapshot_region` + 3 单测）、`adapter/mod.rs`、`engine.rs`（run() 接 window + exec_screenshot/save_failure_snapshot 改窗口感知）、`script.rs`/README/design.md 冲突解决；测试 26→**33**。

### M4 后续①：点击随机延时（2026-09-02）

- **功能**：`[[step]] click_delay`（秒，随机延时上限）+ `click_delay_min`（可选下限）——`click` / `click_image` 在鼠标到位后、按下前随机等待 `[min, max]`；`click_delay = 0.3` → 随机 `[0, 0.3]s`，配 `click_delay_min = 0.1` → 随机 `[0.1, 0.3]s`。
- **原因（新增，用户直接指令驱动）**：用户说「点击随机延时」——延续拟人化路线：`jitter` 已让**位置**随机，`click_delay` 让**时机**也随机。真实玩家连点从不精确等长；固定节奏点击是明显的「机器人特征」。
- **为什么「鼠标到位后、按下前」延时**：更贴近真人（先移动到位、略作停顿再点击），而非点击后才等。
- **为什么纳秒整数采样**：`(max - min) × 1e9` 转成整数做 `rng_next() % span_ns`，再换算回秒——避免浮点取模误差，区间采样均匀。
- **为什么复用 xorshift64\***：与 `jitter` 同一零依赖随机源，不引入 `rand` 依赖树。
- **边界**：`click_delay` 缺省 / `≤0`，或 `min ≥ max` → 不延时（向后兼容）；`min` 自动 clamp 到 `[0, max]`。
- **涉及**：`script.rs`（Step 加 `click_delay`/`click_delay_min` + `Default` derive + 解析单测）、`engine.rs`（`exec_click`/`exec_click_image` 插延时 + `random_click_delay` 函数 + 报告显示延时 + 4 单测）、README/design.md 同步；测试 33→**37**。

### M4 后续②：OCR 文字识别（2026-09-02，本地未推送）

#### 需求与选型决策链（为什么最终选 paddleocr_rs_onnx）

用户指令仅"OCR"二字，目标：识别游戏界面中文文字（血量/数值/标题/弹窗文案），与既有模板匹配互补。**选型三连败后定案**：

| 候选 | 失败原因 | 结论 |
|---|---|---|
| tesseract | 本机未安装系统引擎；需用户手动装 tesseract + 中文语言包 | ❌ 依赖系统环境 |
| ocr-rs（MNN） | 编译期须从 GitHub releases 下载预编译 MNN 库；**GitHub HTTPS 443 在本机不可达**（贯穿全程的痛点，推送也走 MCP） | ❌ 网络不可达 |
| ocrs + RTen | 纯 Rust ONNX 推理最诱人（crates.io 走 rsproxy 可编译），但 `Model::load` **只认 .rten 私有 flatbuffers 格式、不认标准 ONNX protobuf**；且 ocrs 官方明示**只支持 Latin 字母**、预处理硬编码灰度，与 PP-OCRv4 中文 RGB 不匹配 | ❌ 格式/能力双不兼容 |
| **paddleocr_rs_onnx 0.2.7** | 全 Rust ONNX Runtime 绑定 + 标准 ONNX 模型 + 完整中文支持 + `OcrBlock` 带置信度/坐标 + MIT 协议，恰好匹配已下载的 PP-OCRv4 标准 ONNX | ✅ **选中** |

#### 关键工程处理

| 事项 | 处理 | 原因 |
|---|---|---|
| 上游编译 bug | **vendor 本地打补丁**：`configure_session_builder` 无条件引用 `ort::ep::{DirectML,CUDA,OpenVINO,NNAPI,CoreML,CANN}`，被 ort 2.0.0-rc 的 feature gate 掉 → 默认 features 下 6 个 E0433；为各 EP 分支加 `#[cfg(feature)]` 门控、未启用落回 CPU | 上游默认配置编译不过，vendor 是最小侵入修复 |
| rec 输入动态 shape bug | **识别全空（0 行）的根因修复**：rec 模型输入为全动态 shape `[-1,3,-1,-1]`，上游 `(-1).max(1)=1` 把高度算成 1 → rec 输入被压成 1px 高、argmax 全 blank。修正为动态维度回退 **PP-OCRv4 标准高度 48** | 通过 probe 分段 + vendor 插桩定位到 `rec_shape.get(2)=1`；修复后中文实测全部命中 |
| onnxruntime.dll 获取 | pip 清华镜像（pypi.org 默认不可达）下载 onnxruntime wheel → 解压提取 `onnxruntime.dll` + `onnxruntime_providers_shared.dll` 到 `libs/` | 运行时需 `ORT_DYLIB_PATH` 指向 dll |
| C 盘磁盘耗尽 | C 盘仅剩 564MB（target 3.8GB）→ **target 整体迁移 `D:\auto-game-target`**，项目加 `.cargo/config.toml` 指向 | 腾出 C 盘 4GB；D 盘空闲 172.9GB |
| 模型下载源 | ModelScope RapidAI/RapidOCR 仓库成功（hf-mirror/huggingface.co 401/超时/404 不可用）→ `assets/ocr/`（det/rec ONNX + ppocr_keys_v1.txt 6623 字符） | 随仓库提交，克隆即用 |

#### 功能实现

- `adapter/ocr.rs`：`OcrTrait` + `OcrBackend`（`load` 读 det/rec/keys 三文件、`recognize`、`recognize_region` 区域裁剪+坐标偏移）；`OcrLine{text,x,y,w,h,confidence}` + `center()`；白图 smoke 测试。
- `engine.rs` 接入 4 个 OCR 动作：`ocr_text`（识别输出）/ `if_text`（文字条件分支，与 `if_image` 共用控制流编译）/ `click_text`（按文字点击，jitter 限制在文字行内）/ `assert_text`（轮询等待文字出现）；`Engine` 持 `Mutex<Option<Arc<OcrBackend>>>` **懒加载**（纯图像场景零开销）。
- `script.rs`：复用 `text` 字段（type_text 输入 / OCR 期望子串）与 `region`，无新字段。
- 文字匹配语义：`text` 子串 contains（大小写敏感），命中多行取置信度最高者。
- 测试 37→**39**（+if_text 编译分支、全量回归）。

**验证**：中文测试图（600×140 微软雅黑渲染"龙之谷 阴阳师 12345 / 进入副本 开始游戏"）实识别 2 行全部命中、置信度 1.0；白图 smoke 1.11s 无异常；`cargo test -j 1` 39 passed + 3 ignored。

### M4 后续③：egui GUI 录制器（2026-09-02，本地未推送）

#### 需求与取舍

用户从三个候选功能（GUI 录制器 / `validate` 子命令 / GitHub Actions CI+打包）中明确选择**「优先完成 egui」**。目标：可视化录制手动点击/按键 → 自动生成场景 TOML，把「手写 TOML + 量坐标 + 手工截图」变成「录一遍就有」，并复用既有引擎运行。

| 候选 | 定位 | 成本 | 结论 |
|---|---|---|---|
| GUI 录制器（egui） | 录制 + 预览 + 编辑 + 导出 + 运行 | 最大（全新桌面应用） | ✅ 用户选定 |
| `validate` 子命令 | 运行前静态检查（TOML/动作/参数/模板/控制流闭合） | 最小 | 后续 |
| GitHub Actions CI+打包 | 云端自动测试 + 发布 exe | 中（不受本机 443 影响） | 后续 |

#### 依赖选型决策

| 决策 | 内容 | 原因 |
|---|---|---|
| GUI 框架 | **eframe 0.35**（egui 官方桌面后端） | Rust 生态最成熟的即时模式（immediate mode）GUI，纯 Rust、无额外 UI 框架；用户此前用 eframe/egui 开发过 `music_studio` 桌面应用，技术栈熟悉 |
| 全局输入捕获 | **复用 device_query 4.0.1**（与引擎 failsafe 同源） | 零新增依赖；`get_mouse().button_pressed[1]`（1-based 左键）+ `get_keys()` 轮询即可捕获全局点击/按键 |
| 渲染后端 | **glow（OpenGL），显式禁用默认 wgpu** | eframe 0.35 **默认 features 含 wgpu**，其 `wgpu-hal` 的 D3D12 代码与 xcap/enigo 引入的 `windows` crate 版本冲突（`ResourceCategory: From<&D3D12_RESOURCE_DESC>` 等 10 个 E0277）→ 编译失败；改 `default-features=false + features=["glow","default_fonts","accesskit"]` 后编译通过 |
| 序列化 | **手写 TOML 导出，不给 Step 加 Serialize** | `script.rs` 的 Step 仅 Deserialize（TOML 只读）；为避免为核心数据结构引入 Serialize 派生污染，GUI 侧按字段手写 TOML 文本 |

#### eframe 0.35 重大 API 适配（与 0.34 不兼容）

| 0.34 旧 API | 0.35 新 API | 影响 |
|---|---|---|
| `fn update(&mut self, ctx: &Context, frame)` | **`fn ui(&mut self, ui: &mut egui::Ui, frame)`**（App trait 主入口；另可选 `fn logic(ctx, frame)` 跑后台逻辑） | 主循环入口变更 |
| `egui::SidePanel::left(id).show(ctx,…)` / `TopBottomPanel::top(id)` | 统一 **`egui::Panel::left/top(id).show(ui,…)`**（接收 `&mut Ui` 而非 `Context`） | 面板系统重构 |
| `egui::CentralPanel::default().show(ctx,…)` | `CentralPanel::default().show(ui,…)` | 接收 `&mut Ui` |
| `egui::SidePanel::width_range(..)` | `Panel` 无该方法 | 移除，仅 `resizable` |
| `Keycode` 枚举 `A..=Z` 范围匹配 | 枚举**不支持 range pattern**（E0029）→ 显式列 26 字母 + 10 数字 + F1-12 + 控制键 | 录制键映射重写 |

#### 功能实现

- `src/gui.rs`：`GuiApp`（实现 `eframe::App`）+ `run_gui(assets_dir)`；`main.rs` 新增 `gui` 子命令；`lib.rs` 注册 `pub mod gui`。
- **录制线程** `spawn_recorder`：device_query 8ms 轮询，检测左键「按下沿」→ 点击事件、按键「出现边沿」→ 按键事件，经 `mpsc::channel` 回 GUI；按住键用 `Vec<Keycode>` 集合差集判定，支持 ctrl 等组合键防重复。
- **画面预览**：复用 `CaptureBackend.capture_full()` / `capture_window(窗口标题关键字)`，截图转 `egui::ColorImage` 纹理实时显示；录制中 150ms / 空闲 500ms 刷新，避免过度占用。
- **框选模板**：预览图上拖拽 → 实时显示像素坐标/尺寸 → 「保存模板」存 `assets/<name>.png`（复用 `capture_region` 按显示比例换算回真实像素）或「插入 region」生成带 region 的 `wait_image` 步骤。
- **步骤编辑**：左侧列表，每步可改 action（14 种）/ 参数（x/y/jitter/precision/timeout/image/text/key/keys/region…）；上移/下移/删除/清空；「→模板」把坐标 `click` 以点击坐标为中心截 64×64 模板并转 `click_image`（保留 jitter/click_delay 拟人化参数）。
- **导出**：手写 TOML → `scenarios/<name>.toml`；**运行**：后台线程调 `engine::run_scenario` 复用既有引擎，结果经 channel 回状态栏，运行中可 F9 中止。
- **忽略录制干扰**：录制线程只录左键与字母/数字/F1-12/控制键；小键盘、F13+、标点不录（避免误录 GUI 自身操作）。

#### 验证

- `cargo build -j 1` 通过、无 warning；
- `cargo test -j 1` = **39 passed + 3 ignored**（既有功能回归无影响）；
- 实际启动 `auto-game gui`，进程正常存活 7s 未崩溃（无 panic 输出）。

### M4 后续④：validate 场景静态校验（2026-09-02，本地未推送）

#### 需求与定位

用户从三个候选功能中依次推进，本项为 **`validate` 子命令**：不实际运行场景，预先检查常见错误，把「写错 TOML → 跑起来才发现」变成「跑之前就报出来」。它是三者中**成本最低、收益最直接**的一项（不引入新依赖、不新增运行时能力，纯静态分析 + 复用引擎既有检查）。

#### 检查项设计（每项对应引擎真实运行语义，非表面语法）

| 检查 | 级别 | 依据 |
|---|---|---|
| TOML 解析失败 / 缺字段 | ERROR | `Scenario::load` 的 serde 错误 |
| 未知动作类型 | ERROR | 引擎 `execute` 的 match 白名单（18 种：14 普通 + 4 控制流关键字之外的 `end_repeat/else/end_if` 属控制流） |
| 必填参数缺失 | ERROR | 各动作真实依赖（click/move_mouse→x,y；key_press→key；key_combo→keys；type_text/if_text/click_text/assert_text→text；图像类→image；repeat→count） |
| 模板文件不存在 | ERROR | `resolve_asset` 语义：相对 assets 目录拼接 |
| 非法按键 / 组合键 | ERROR | `key_from_str` 白名单 |
| precision 超出 [0,1] | ERROR | 匹配阈值语义 |
| region 宽高为 0 | ERROR | 裁剪/匹配必然失败 |
| OCR 模型文件缺失 | ERROR | `OcrBackend::load` 依赖 det/rec/字典三文件 |
| 控制流未闭合 / 悬空 else | ERROR | **复用 `engine::compile`**（单一事实来源，不带病执行） |
| 空场景 / 未命名 | WARN | 报告可读性 |
| timeout ≤ 0 / count=0 / jitter 过大 / 延时区间非法（min≥max） | WARN | 引擎有回退默认值或静默失效，不致命但应提示 |
| region 用在无意义动作上 | WARN | 冗余字段 |

#### 关键实现决策

- **控制流检查复用 `engine::compile`**：新增 `engine::check_control_flow(steps) -> Result<()>`（内部 `compile(steps).map(|_| ())`），validate 直接调用——不复制第二套配对逻辑，避免两处不一致（例如将来新增控制结构时只改一处）。
- **退出码语义**：发现 ERROR 时 `exit(1)`（CI 友好），仅 WARN 时 exit 0 但提示；空场景只是 WARN（引擎本身允许空场景）。
- **不影响运行时**：validate 是纯 lib 层静态函数（`auto_game::validate::validate_scenario`），不实例化 Engine、不碰屏幕/输入/模型加载。

#### 验证

- `cargo test -j 1` = **50 passed + 3 ignored**（新增 11 个 validate 测试：合法场景通过、未知动作/缺参数/模板缺失/控制流未闭合/悬空 else/非法按键/OCR 缺模型/precision 越界/坏 TOML 报错、空场景仅警告）。
- 实测 `validate scenarios/demo.toml` → ✅ 通过；构造坏场景 → 精确报出 7 个错误（控制流、缺 x/y、未知动作、非法按键、模板缺失、precision 越界）且退出码 1。

### M4 后续⑤：GitHub Actions CI + 打包（2026-09-02，本地未推送）

#### 需求与定位

三个候选功能的最后一项：**云端自动构建 + 测试 + Release exe 打包**。它恰好绕开贯穿全程的本机痛点——**GitHub HTTPS 443 不可达**（推送一直走 MCP 工具通道），而 GitHub Actions 云端 runner 有完整网络（crates.io / GitHub releases 均可访问）。

#### 设计决策

| 决策 | 内容 | 原因 |
|---|---|---|
| 平台 | **windows-latest** 单一平台 | 项目纯 Windows 定位（xcap/enigo/device_query/eframe-glow 均为 Windows 目标）；Linux 需额外装 X11/GL 系统库，收益低 |
| 触发 | push / PR 到 main | 常规 CI 语义 |
| 两个 job | `test`（build --all-targets + test + validate 冒烟）→ `package`（release exe + 打包，**仅 push main**，`needs: test`） | 测试与发布分离；PR 只测不打包 |
| **target-dir 覆盖** | 顶层 `env: CARGO_TARGET_DIR: target` | **关键坑**：仓库根 `.cargo/config.toml` 指向本地 `D:/auto-game-target`（用户 C 盘告急时迁移的），CI runner 无此路径语义 → 必须覆盖；`cargo metadata` 实测 env 生效后 target_directory 由 `D:/auto-game-target` 变为 `.../target` |
| 缓存 | `Swatinem/rust-cache@v2` | 减少云端重复编译（OCR/eframe 依赖树大） |
| 自包含 bundle | exe + `libs/onnxruntime.dll` + `onnxruntime_providers_shared.dll` + `assets/`（含 OCR 模型）+ `scenarios/` + `README.md` | 解压即用，OCR 运行时无需再下载 |
| 上游 vendor 的 `.github/workflows/` | 位于 `vendor/paddleocr_rs_onnx/.github/`，GitHub 只识别仓库根 workflow，不会误触发 | 无需处理 |

#### 验证

- `pyyaml` 解析 workflow → YAML 结构合法（jobs: test/package、env 正确）；
- `cargo metadata` 实测 `CARGO_TARGET_DIR` 覆盖语义成立（`D:/auto-game-target` → `target`）；
- CI 打包脚本在本地模拟执行成功：bundle 结构正确（exe + dll + 模型 + 场景 + README），debug 体积 ~106MB（release 更小，大头是 onnxruntime dll ~15MB + OCR 模型 ~15MB）；
- **说明**：GitHub Actions 真实执行需推送后由云端跑（本机无法运行 Actions），首次云端运行结果需推送后确认；若 ort 2.0.0-rc 在云端下载预编译 onnxruntime 失败，可再调整（如禁用 download-binaries）。

---

## 四、关键技术机制复盘（深入原理）

### 4.1 模板匹配加速：金字塔 + 并行 为什么有效

```
朴素 NCC：对全图每个位置算一次模板相关 → O(W×H×w×h)，慢
金字塔：  低分辨率全图粗定位 top-K 候选（计算量骤减）
          → 候选坐标 ×2 映射回上层，在「模板+2×radius」邻域并行精匹配
          → 逐层精化到顶层 → 语义等价于全局最优，但计算量从「全屏」降为「K 个小窗口」
```

- **加速原理**：粗层一张小图覆盖全屏，只有 top-K 候选进入精细层；精细层每层只在候选附近算。
- **并行原理**：每层多个候选的窗口匹配彼此独立 → `rayon::par_iter` 并行。
- **兜底**：小图（<128px）或小模板（<16px）直接精确匹配，避免金字塔得不偿失。

### 4.2 verify_exact 两级粒度（meta 全局 + step 步骤级）

```
取值优先级：[[step]] verify_exact（显式） > [meta] verify_exact（全局默认） > false（代码默认）
执行路径：fast 金字塔粗定位
          → （verify_exact=true 时）最终位置 ±VERIFY_RADIUS(3px) 邻域一次精确 NCC
          → 精确位置 + 准确置信度
          → 金字塔漏检时自动回退全图精确匹配（保证不漏检）
```

- **为什么是「邻域确认」而非「全图 exact」**：全图精确 = 11.5s，会丢掉 88x 加速收益；邻域确认只在最终位置 ±3px 内做一次精确 NCC，**成本 ~24ms，位置像素级精确**。
- **为什么 step 级是 `Option<bool>`**：三态语义才能表达「覆盖全局开/关」和「跟随全局」三种情况。

### 4.3 jitter 拟人化点击（零依赖 xorshift64*）

- `jitter = 0` → 精确点击（与旧行为一致，向后兼容）。
- `click`：基座坐标 ± jitter。
- `click_image`：以模板中心为基座，偏移**自动 clamp 在模板范围内**（不会点出元素）。
- PRNG：`xorshift64*` 静态原子 + 启动播种，零依赖、线程安全、报告显示实际点击坐标（可追溯）。

### 4.4 控制流编译（M3）

- TOML 扁平 `[[step]]` → `compile()` 生成带跳转的指令序列。
- `repeat/end_repeat`、`if_image/else/end_if` 编译期配对；**未闭合直接报错**，不带病执行。
- 运行时用 `Frame` 栈维护循环计数与 then/else 分支状态。

### 4.5 失败证据链（template 闭环）

```
失败 → 现场截图（region 或全屏）→ fail_step_<n>.png
     → 新旧对照（左=模板 / 右=现场）→ diff_step_<n>.png  → UI 变更一目了然
     → 用 template 命令重新采集新模板 → 场景更新 → 闭环
```

### 4.7 OCR 接入复盘（为什么这条路才通）

```
需求：读游戏界面中文文字（血量/数值/标题/弹窗文案）
模板匹配做不到（只能认"图"，认不了"字"）
→ tesseract     需系统引擎+中文包 → 依赖用户环境，弃
→ ocr-rs(MNN)   编译期从 GitHub 下载预编译库 → 443 不可达，弃
→ ocrs(RTen)    纯 Rust 最诱人，但只认 .rten 私有格式 + 仅 Latin → 弃
→ paddleocr_rs_onnx  ONNX Runtime + 标准 ONNX + 中文 + 置信度/坐标 → ✅
   └ vendor patch × 2：EP feature gate + rec 动态 shape 高度=48
   └ 实测中文识别准确（"龙之谷阴阳师12345" 全部命中）
```

- **为什么 vendor 而不是提 PR/绕开**：上游默认 features 编译不过（ort 2.0.0-rc 的 EP 引用被 feature 门控），本地 vendor + 最小补丁是当时唯一能编译通过且可控的路径；`Cargo.toml` 以 `path` 依赖指向 vendor，行为与正常依赖一致。
- **为什么 rec 高度=48**：PP-OCRv4 mobile rec 输入固定高度 48、宽度动态（最大到训练宽度）；上游从动态 shape 读 `get(2)=-1` 后 `max(1)` 得到 1——这是"能编译、能跑、但识别全空"的典型隐性 bug，靠**分段插桩 + shape 打印**才定位到。
- **懒加载设计**：OCR 模型 ~15MB，纯图像场景不应付出加载代价 → `Mutex<Option<Arc<OcrBackend>>>`，首次用到才 load，之后克隆 Arc 复用。

### 4.8 已知坑（design.md §8，踩过并记录）

| 坑 | 原因 | 对策 |
|---|---|---|
| 纯色/无纹理模板 NCC「处处命中」 | NCC 对纯色区域无区分度 | 模板选有纹理 UI 元素（按钮文字、图标边缘） |
| image 路径带 `assets/` 前缀 | 路径相对 assets 目录再拼接 | 直接写文件名 |
| 非交互会话输入被 UIPI 拦截 | Windows 会话隔离 | 真实桌面 + 前台窗口 + 管理员/白名单；move_mouse 不受限可先验证坐标 |
| 远程 443 不可达 | GitHub HTTPS 不稳 | 走 MCP API 推送（工具通道） |
| OCR rec 模型全动态 shape 导致识别全空 | 上游 `(-1).max(1)=1` 把高度算成 1 | vendor patch：动态维度回退 PP-OCRv4 标准高度 48 |
| OCR 运行时缺 onnxruntime.dll | Windows 无全局 ONNX Runtime | 随仓库提供 `libs/`，设 `ORT_DYLIB_PATH` 指向 dll |

### 4.9 egui 录制器机制复盘

**录制事件模型**：device_query 是「状态轮询」不是「事件回调」→ 用**边沿检测**模拟事件：`button_pressed[1]` 从 false→true 记一次点击（按下沿），按键从「不在按住集合」→「在」记一次按键；按住集合用 `Vec<Keycode>` 差集维护，天然防抖、支持组合键。

**为什么预览截图用 `CaptureBackend` 而非 egui 内置**：引擎截图链路（xcap）与运行态完全一致，所见即所得——预览里框的 region 就是引擎实际匹配的区域，避免「预览与实际不一致」。

**为什么 eframe 默认 wgpu 必须换 glow**：eframe 0.35 默认 features 新增 `wgpu`，而 wgpu 29 的 `wgpu-hal` 依赖的 `windows` crate 与 xcap/enigo/device_query 已锁定的版本在 D3D12 接口生成上冲突（同一 `ID3D12Heap` 的 `Param` trait 在不同 windows crate 版本下不满足）→ 编译期 10 个 E0277。glow（OpenGL）后端无此冲突，且 0.35 的 glow 仍是完整渲染后端，功能等价。

**为什么 GUI 导出用「手写 TOML」而非「给 Step 加 Serialize」**：Step 是引擎核心只读结构（仅 Deserialize），为其加 Serialize 会扩大序列化契约面、且 `Option` 字段的默认值语义在导出时需要过滤（None 不输出）——手写序列化函数 40 行内可控，且不触碰核心结构。

---

## 五、数据与验证记录

### 5.1 性能基准演进

| 阶段 | 场景 | 耗时 | 备注 |
|---|---|---|---|
| M1 | 朴素 NCC，500×400 区域（debug） | ~78s | 暴露性能问题 |
| 打磨轮 | 金字塔+并行，400×300+50×40（debug） | ~0.16s（**≈88x**） | 语义不变 |
| 打磨轮 | fast 单次 | 142ms | 默认路径 |
| 打磨轮 | fast→exact 确认 | 166ms（+24ms） | 像素级精确 |
| 打磨轮 | 全图精确 | 11.5s | 对照，确认 ≈69x 于它 |

### 5.2 测试数量演进

| 里程碑 | 常规单测 | ignored | 覆盖内容 |
|---|---|---|---|
| 打磨轮前 | 无 | 0 | — |
| 打磨轮 | 22 | 2 | 解析/控制流/报告/按键/jitter/对照图 |
| fast→exact 后 | 25 | 3 | +确认一致性/兜底/解析默认值 |
| step 级后 | 26 | 3 | +步骤级覆盖解析 |
| M4 窗口捕获后 | **33** | **3** | +窗口捕获 4 单测、坐标映射 3 单测 |
| 点击随机延时后 | **37** | **3** | +click_delay 解析 1 单测、延时区间 4 单测 |
| OCR 接入后 | **39** | **3** | +if_text 条件编译 1 单测、OCR smoke 1 单测 |

### 5.3 工程状态

- 本地 HEAD：`a4d0df4`（feat: random click delay）之后**未提交**——OCR 相关改动（engine/script/ocr.rs/vendor patch/libs/assets）均在本地工作区
- 构建/测试：`cargo test -j 1` → 39 passed + 3 ignored，全绿
- 依赖新增：`paddleocr_rs_onnx 0.2.7`（path → vendor）、`log 0.4`（paddle 日志）
- 运行环境：`target-dir = "D:/auto-game-target"`（`.cargo/config.toml`，C 盘空间迁移）；OCR 运行需 `$env:ORT_DYLIB_PATH = (Resolve-Path "libs\onnxruntime.dll").Path`
- 远程推送状态：本地未推送（按用户指示，推送由用户/MCP 决定）

---

## 六、当前状态与后续候选（M4）

### 已完成能力总览（截至 2026-09-02）

| 能力 | 里程碑 |
|---|---|
| 截图/输入/识别三后端可插拔 | M0 |
| 模板匹配（NCC）+ 找图→点击→验证 | M1 |
| TOML 配置驱动 + 流程引擎 + 文本/HTML 报告 | M2 |
| 循环 / 条件分支（编译期校验） | M3 |
| key_combo 全量按键 | 打磨轮 |
| region 区域限定匹配 | 打磨轮 |
| F9 failsafe 紧急停止 | 打磨轮 |
| template 模板采集子命令（代码内闭环） | 打磨轮 |
| 失败自动存档 + 新旧对照图 | 打磨轮 |
| jitter 拟人化点击（零依赖 PRNG） | 打磨轮 |
| 模板匹配加速（金字塔+并行 ≈88x） | 打磨轮 |
| fast→exact 确认开关（meta 全局） | 打磨轮 |
| step 级 verify_exact 覆盖 | 打磨轮 |
| 窗口级捕获 + 坐标映射 | M4 |
| 点击随机延时（click_delay） | M4 后续① |
| OCR 文字识别（ocr_text/if_text/click_text/assert_text） | M4 后续② |

### M4 候选（按用户优先级推进）

1. ~~窗口级捕获 + 坐标映射~~ ✅ 已做
2. ~~点击随机延时~~ ✅ 已做
3. ~~OCR 接入~~ ✅ 已做（paddleocr_rs_onnx + PP-OCRv4）
4. ~~GUI 录制器（egui）~~ ✅ 已做（M4 后续③）——可视化编排场景、录制点击序列、画面预览、步骤编辑、导出并复用引擎运行。
5. ~~场景静态校验（validate 子命令）~~ ✅ 已做（M4 后续④）——不实际跑场景就能检查 TOML 语法 / 模板存在 / 控制流闭合 / OCR 模型存在。
6. ~~GitHub Actions CI + 打包~~ ✅ 已做（M4 后续⑤）——云端自动构建 + 测试 + Release exe 打包，绕开本机 GitHub 443 不可达。

---

## 七、完整提交清单（按时间，90+ 次提交）

### 阶段 0 初始化
`e14362c` Initial commit · `3e09aa6` Cargo.toml · `543ec94` main.rs · `a47480c` .gitignore · `40a9830` 设计文档 · `23547b9` 依赖选型 · `8d7a6f0` 游戏适配章节

### M0 骨架
`b47f6ab` lib.rs · `27e1d37` adapter 入口 · `135afe8` input.rs · `456d69b` vision.rs · `0ab64a8` main 接入 · `c4a591a` capture.rs · `c564629` gitignore

### M1 视觉原语
`cebfabb` imageproc 依赖 · `a4e61f7` vision 模板匹配 · `7dc7316` action 原语 · `0e3ec6f` lib 注册 · `79db4c5` main 演示 · `f60f8c1` 性能风险条目 · `6ed622b` 小区域演示 · `997e947` Cargo.lock

### M2 流程引擎
`8c4b083` TOML 引擎+报告+冒烟（8 文件 +439）

### M3 控制流与 HTML
`6b69897` 总提交 · `1058fa7` engine 编译执行 · `4cb5c7a` report HTML · `b407d46` script count · `4432a03` m3_demo · `6ab5611` 文档同步

### 打磨轮（2026-09-02）
- `2b177a9` 打磨总提交（key_combo/region/failsafe/按键/单测/文档，+1120）
- `f788073` device_query 依赖 · `88d50e6` Region · `0b58f54` Key 扩展+key_combo · `4c1a285` action 增强 · `f1ed970` engine failsafe/region/combo+修复 else 跳转 · `874f647` report 单测 · `c482abd` script keys/region · `ee14161` 完整使用文档 · `0aa18bf` features 场景
- `225ea1f`/`cbddf9f` template 子命令 · `17ace69` README template
- `0cb54dd`/`96454d6`/`5c14132` 失败存档+对照图 · `e12b26b` README
- `39fa59e`/`023ead8`/`8326c6f` jitter · `ea5eac3` README
- `077cacf`/`a210036` design.md 同步打磨轮
- `39106b5`/`96420a6`/`a9c0a95`/`d4fc761`/`0cd1b30`/`87b27ac`/`cc70625` 金字塔+并行加速
- `978c30c` + 远程拆分的 6 个提交：fast→exact 确认开关
- `3fe3e8d` step 级 verify_exact 覆盖
- `c678853` Merge（远程同步合并）

---

*文档生成：2026-09-02 · 依据：git 提交历史 + docs/design.md + README.md + 开发对话决策记录。*
