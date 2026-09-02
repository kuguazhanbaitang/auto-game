# auto-game 通用电脑游戏自动化测试框架 · 设计文档

> 定位：不针对单一游戏，而是把「截图 → 识别 → 决策 → 输入」抽象成可插拔原语，用配置文件描述测试流程，对任意 PC 游戏执行自动化测试。
> 原则：**纯视觉方案**（不读内存、不注入），规避反作弊风险；**复用成熟核心库**，不重复造轮子。

---

## 1. 生态调研结论（参考核心库）

| 能力 | 候选库 | 选择 | 理由 |
|---|---|---|---|
| 截图 | rustautogui / xcap / screenshots | **xcap**（增强）+ **rustautogui**（内置） | xcap 支持多显示器、指定窗口捕获；rustautogui 自带截图与找图同源 |
| 输入模拟 | enigo / rustautogui / autocontrol | **enigo**（抽象）+ **rustautogui**（内置） | enigo 跨平台、被 RustDesk 生产验证，trait 抽象便于替换 |
| 模板匹配找图 | imageproc / rustautogui（Segmented NCC / FFT）/ template-matching(GPU) / opencv | **imageproc**（已实现） | `template_matching`（CrossCorrelationNormalized）+ `find_extremes`，纯 Rust 无重依赖；rustautogui 保留在 Cargo.toml 作备选/预留 |
| 紧急停止（failsafe）键盘监听 | device_query | **device_query** | 轮询 `F9` 键状态，不可用时降级禁用 |
| 并行加速 | rayon | **rayon** | 金字塔候选精匹配用 `par_iter` 并行 |
| OCR 文字识别 | tesseract-rs / ddddocr / ocr-rs(MNN) / ocrs(RTen) / **paddleocr_rs_onnx** | **paddleocr_rs_onnx 0.2.7**（已实现） | 完整 DBNet 检测 + CRNN 识别、带置信度与坐标（OcrBlock 含 x/y/w/h）、MIT 协议，恰好匹配 PP-OCRv4 标准 ONNX 模型；tesseract（本机未装系统引擎+中文包）、ocr-rs/MNN（编译期须从 GitHub 下载预编译库，HTTPS 443 不可达）、ocrs/RTen（只认 .rten 私有格式 + 仅 Latin 字母）均已验证放弃，详见 §6.12 |
| 配置 | serde + TOML | **serde + toml** | Rust 标准做法 |
| 日志 / 报告 | tracing | **tracing** | 结构化、分层过滤 |

> 依赖选型结论（按已落地实现）：**xcap** 负责截图（全屏/区域），**enigo** 负责键鼠输入，**imageproc** 负责模板匹配（NCC），**device_query** 负责 F9 failsafe 键盘监听，**rayon** 负责匹配候选并行，**serde+toml** 负责配置。rustautogui 保留在依赖中以备后续换用其分段/FFT 匹配。上层价值在「脚本引擎 + 报告 + 模板采集 + 匹配加速」，这是目前开源空白。

---

## 2. 总体架构（分层）

```
┌───────────────────────────────────────────────────┐
│ Script Layer   测试脚本（TOML）                     │  场景即配置
├───────────────────────────────────────────────────┤
│ Engine Layer   流程引擎                            │
│  执行上下文 / 顺序与条件 / 超时 / 重试 / 紧急停止     │
├───────────────────────────────────────────────────┤
│ Action Layer   动作原语                            │
│  FindImage / ClickImage / KeyCombo / TypeText /   │
│  MoveMouse / WaitImage / AssertImage / Click±jitter│
│  OcrText / ClickText / AssertText / IfText(OCR)    │
├───────────────────────────────────────────────────┤
│ Adapter Layer  可插拔后端                          │
│  capture(xcap) / input(enigo) / vision(imageproc) │
│  / ocr(paddleocr_rs_onnx)                          │
├───────────────────────────────────────────────────┤
│ 核心库  xcap · enigo · imageproc · paddleocr_rs_onnx · device_query · rayon │
└───────────────────────────────────────────────────┘
```

设计要点：
- **向下可插拔**：底层库全部封装在 trait 后，替换实现不影响上层；
- **向上配置驱动**：测试场景 = TOML 文件，不写代码也能编排流程；
- **循环本质**：每个动作都是「采集 → 识别 → 决策 → 输入」四步之一或组合。

---

## 3. 模块结构（src 规划）

```
src/
├── main.rs          # CLI 入口：run <场景.toml> + template 模板采集子命令
├── lib.rs           # 对外库 API（供二次开发/将来 GUI 调用）
├── adapter/
│   ├── mod.rs       # Region 结构 + Key 枚举导出 + 适配层汇总
│   ├── capture.rs   # 截图后端：xcap 实现（全屏/区域截图）
│   ├── input.rs     # 输入后端：enigo 实现 + Key 全量枚举 / key_from_str / key_combo
│   ├── vision.rs    # 识别后端：imageproc 模板匹配（NCC）+ Match + 金字塔/并行加速
│   └── ocr.rs       # OCR 后端：paddleocr_rs_onnx（PP-OCRv4 检测+识别）+ OcrLine/OcrTrait |
├── action.rs        # 动作原语：Actions 结构体（截图/移动/点击/按键/找图/区域截图）
├── engine.rs        # 流程引擎：指令编译（循环/分支）、执行、failsafe、失败自动存档
├── script.rs        # TOML 解析 → 动作序列（serde deserialize；Step 含 region/jitter）
└── report.rs        # 报告：文本 + HTML（转义防注入）、通过/失败/耗时汇总

根目录：
├── scenarios/       # 场景配置（demo / m3_demo / 特性演示）
├── assets/          # 模板图像（template 子命令产出，按游戏分目录）+ ocr/（PP-OCRv4 模型）
├── libs/            # 运行时动态库（onnxruntime.dll，OCR 需要）
├── vendor/          # 本地 vendor 的第三方库（paddleocr_rs_onnx，含本地补丁）
└── reports/         # 运行产物（HTML 报告 + 步骤/失败截图，gitignore 内）
```

---

## 4. 核心抽象（动作原语 + 适配层）

实际实现以 `Actions` 结构体承载动作原语，底层后端在 `adapter/` 下隔离（xcap / enigo / imageproc），设计上保留可插拔意图；若需严格 trait 契约可在此基础上抽取。

```rust
// 动作原语（src/action.rs，Actions 结构体方法）
capture_full()  -> Result<RgbaImage>             // xcap 全屏截图
capture_region(x, y, w, h) -> Result<RgbaImage>  // xcap 区域截图
move_mouse(x, y) -> Result<()>                   // enigo 移动鼠标
click() -> Result<()>                            // enigo 左键点击
key_press(key) -> Result<()>                     // enigo 按键
key_combo(&[Key]) -> Result<()>                  // 组合键：先全按、再逆序释放
type_text(&str) -> Result<()>                    // enigo 输入文本
find_image(tpl, precision, verify_exact) -> Result<Option<Match>>             // 全屏 NCC 匹配
find_image_region(tpl, precision, Region, verify_exact) -> Result<Option<Match>> // 区域限定匹配

// 辅助结构（src/adapter/）
Region { x, y, w, h }          // 搜索区域（serde Deserialize）
Match { x, y, width, height, confidence } + center()  // 命中结果，可求中心点
Key 枚举 + key_from_str(&str)  // 全量游戏键位，大小写/别名兼容（esc/return/pgup…）
```

---

## 5. 脚本格式（TOML 场景示例）

```toml
# 场景：登录主菜单并验证
[meta]
name = "登录主菜单冒烟"
window = "MyGame"          # 可选：限定窗口（xcap 捕获）
verify_exact = true        # 可选：fast→exact 确认开关，见 §6.10（默认 false）

[[step]]
action = "wait_image"
image = "login_btn.png"
precision = 0.9
timeout = 15

[[step]]
action = "click_image"
image = "login_btn.png"
precision = 0.9
timeout = 10

[[step]]
action = "type_text"
text = "test_account"

[[step]]
action = "key_press"
key = "enter"

[[step]]
action = "assert_image"     # 断言成功 → 步骤通过；失败 → 场景失败
image = "main_menu.png"
timeout = 20
```

> `image` 路径**相对 `--assets` 目录**（默认 `assets/`），不要写 `assets/` 前缀，否则会双重拼接成 `assets/assets/...`。

动作原语一览（M3 + 打磨轮 + M4 已实现）：
`wait_image` / `find_image` / `click_image` / `click`(坐标) / `move_mouse` / `key_press` / `key_combo`(组合键) / `type_text` / `assert_image` / `wait`(固定延时) / `screenshot`(存档证据) / `repeat`+`end_repeat`(循环) / `if_image`+`else`+`end_if`(条件分支) / `ocr_text`(识别输出文字) / `if_text`+`else`+`end_if`(文字条件分支) / `click_text`(按文字点击) / `assert_text`(等待文字出现)

OCR 文字类动作（M4 后续，与模板匹配互补）：
- `ocr_text`：识别 `region`（默认全屏）内文字并输出到报告——回答「这片区域写了什么」；
- `if_text`：`text` 为期望包含子串，OCR 识别后命中走 then 分支、未命中走 else/跳过（与 `if_image` 同属条件分支编译）；
- `click_text`：识别到包含 `text` 的行后点击其中心（`jitter`/`click_delay` 同样生效，偏移限制在文字行范围内）；
- `assert_text`：轮询（200ms）直到识别到包含 `text` 的文字，超时失败。

通用参数（`[[step]]`）：
- `precision`：匹配置信度阈值 0.0~1.0，默认 `0.85`（图像类）
- `timeout`：超时秒数，默认 `15`（等待/断言类）
- `region`：限定搜索区域 `{ x, y, w, h }`，**强烈建议指定**以规避全屏匹配的性能瓶颈（图像类）
- `jitter`：点击随机抖动像素，每次点击在目标 ±N 内动态分布，拟人化（仅 `click` / `click_image`）
- `click_delay`：点击前随机延时（秒）——每次点击在 `[0, click_delay]` 内随机等待，拟人化（仅 `click` / `click_image`；缺省/≤0 不延时）
- `click_delay_min`：点击随机延时下限（秒），配合 `click_delay` 组成 `[min, max]` 区间（可选；缺省 0）
- `verify_exact`：按步骤覆盖全局开关——显式 `true` / `false` 时覆盖 `[meta] verify_exact`，缺省时回退全局值（图像类）
- `text`：type_text 的输入文本；OCR 文字类动作（`if_text` / `click_text` / `assert_text`）用作**期望包含子串**（大小写敏感 contains 匹配）

全局开关（`[meta]`）：
- `verify_exact`：模板匹配「fast→exact 确认」开关，默认 `false`。开启后每个模板在金字塔定位后再做一次像素级精确确认（返回确切位置与准确置信度，适合"点击必须落在确切像素"的极端需求），金字塔漏检时自动回退全图精确匹配兜底；实测仅比 fast 多 ~16% 开销，详见 §6.10。**优先级低于步骤级**：某步骤若显式声明 `[[step]] verify_exact`，以该步骤为准。

拟人化点击（jitter）示例：
```toml
[[step]]
action = "click"
x = 500
y = 400
jitter = 10      # 每次在 (500,400) ±10px 内随机点

[[step]]
action = "click_image"
image = "attack_btn.png"
region = { x = 800, y = 600, w = 120, h = 60 }
jitter = 8       # 以模板中心为基座，偏移自动限制在模板范围内
```

控制流语法（TOML 扁平步骤 + 显式结束标记）：
```toml
[[step]]                          # 循环 3 次
action = "repeat"
count = 3

[[step]]
action = "click_image"
image = "attack_btn.png"

[[step]]                          # 条件：弹窗出现则关闭
action = "if_image"
image = "close_btn.png"
precision = 0.8

[[step]]
action = "click_image"
image = "close_btn.png"

[[step]]
action = "else"

[[step]]
action = "wait"
seconds = 1

[[step]]
action = "end_if"

[[step]]
action = "end_repeat"
```
控制结构由引擎编译成带跳转的指令序列（`repeat`/`end_repeat`、`if_image`/`else`/`end_if` 必须配对，未闭合在编译期报错）。

---

## 6. 关键机制

1. **区域限定匹配**：图像动作加 `region` 参数，只 `capture_region` 目标区域再匹配（坐标带区域偏移换算），规避全屏 NCC 的性能瓶颈（M1 实测 500×400 debug 约 78s、全屏更久；限定区域 + `--release` 后可用）；
2. **窗口捕获 vs 全屏（M4 已实现）**：默认全屏截图；`[meta] window = "<标题关键字>"` 时进入窗口模式——xcap `Window::all()` 枚举窗口（已按 z 序返回），忽略最小化、按标题关键字 contains 匹配；多开命中多个时取 z 序最前者并告警。窗口捕获规避遮挡、支持多开（每个实例独立场景文件）；
3. **超时与轮询**：每个步骤独立 `timeout`；`wait_image` / `assert_image` 内部 200ms 轮询直至超时；
4. **紧急停止（failsafe）**：已实现——`device_query` 轮询 `F9` 键，运行中随时中止并记一条 FAIL；非交互会话读不到键盘时自动降级禁用并告警，不影响场景执行；
5. **失败自动存档（证据链）**：任一步骤 FAIL 自动保存现场截图（有 `region` 截区域、否则全屏 `fail_step_<n>.png`）；若该步带模板，再生成「左=旧模板 / 右=现场」对照图 `diff_step_<n>.png`，UI 变更一目了然；路径写入报告详情；
6. **结果报告**：文本报告（控制台）+ HTML 报告（`reports/<场景名>/index.html`，状态着色、耗时统计、HTML 转义防注入）；
7. **拟人化点击（jitter + 随机延时）**：零依赖 xorshift64* 随机源（启动时按时间播种），`click` / `click_image` 坐标在 ±N 内随机分布（`click_image` 偏移自动限制在模板范围内避免点出元素）；配 `click_delay`（可选 `click_delay_min`）时，每次点击在鼠标到位后、按下前随机等待 `[min, max]`（纳秒精度采样）——「位置 + 时机」都不固定，更接近真人操作节奏。报告显示实际点击坐标与基座、偏移量及延时；
8. **模板采集（template 子命令）**：用代码截图生成模板 PNG 到 `assets/`，同时打印坐标与可直接粘贴的 TOML 片段（`image` + `region`），「截图→模板→坐标→场景片段」闭环，无需手工截图裁图；
9. **模板匹配加速（金字塔 + 并行）**：图像金字塔粗到细——先在低分辨率全图粗定位 top-K 候选，再逐层在候选邻域（窗口 = 模板尺寸 + 2×radius）并行精匹配，把「全屏大匹配」变为「若干小窗口匹配」；语义不变（仍返回全局最高置信度位置）。合成基准实测 debug 下 400×300 + 50×40 模板：**~14s → ~0.16s（≈88x）**；小图/小模板（<128px 或模板 <16px）自动回退精确匹配。
10. **fast→exact 确认开关（`[meta] verify_exact`，可被步骤级覆盖）**：默认金字塔加速返回近似位置与置信度（±1px 内），适合绝大多数点击；当需求升级为"点击必须落在确切像素"时，在 `[meta]` 开 `verify_exact = true`，引擎对每个模板走「fast 粗定位 → 最终位置精确确认」：确认复用「模板尺寸 + 2×radius」邻域做一次精确 NCC（抹掉降采样累积误差、给出准确置信度），若金字塔粗层无候选（极端漏检）则自动回退全图精确匹配兜底，保证不漏检。**粒度：全局 + 步骤级双重控制**——`[meta] verify_exact` 设默认值；某步骤 `[[step]] verify_exact = true/false` 显式声明时覆盖全局（适合"大部分快路径、个别关键步骤要像素级精确"的场景）。合成基准（400×300 + 50×40 模板，debug）：fast=142ms vs fast→exact 确认=166ms（仅多 ~24ms）vs 全图精确=11.5s（≈69x）——"确认"不牺牲加速收益。
11. **窗口级捕获 + 坐标映射（M4）**：窗口模式的识别结果统一映射回**屏幕坐标**供输入——`capture_target` 返回（窗口图, 窗口左上角屏幕偏移 ox, oy）；`find_image` 匹配到窗口内坐标后经 `offset_match` 加窗口原点得屏幕坐标；`[[step]] region` 在窗口模式下语义为**窗口内坐标**（相对窗口左上角），经 `region_match_to_screen` 映射回屏幕；`snapshot` / `snapshot_region` 为窗口感知接口（`screenshot` 动作与失败存档均走窗口图，存档/对照图只含窗口内容）。
12. **OCR 文字识别（PP-OCRv4，与模板匹配互补）**：模板匹配回答「某图像在不在/在哪」，OCR 回答「这片区域写了什么」——识别动态文本（血量/数值/标题/弹窗文案）是模板匹配做不到的。后端 `paddleocr_rs_onnx`（ONNX Runtime + PaddleOCR 模型）：**det 检测（DBNet，定位文字区域）→ rec 识别（CRNN，区域转字符序列）→ ctc_decode**，输出带置信度与坐标（`OcrBlock`）。模型为 PP-OCRv4 中文（`assets/ocr/`：det/rec ONNX + ppocr_keys_v1.txt 字符集），从 ModelScope RapidAI/RapidOCR 仓库下载；运行时依赖 `libs/onnxruntime.dll`（`ORT_DYLIB_PATH` 环境变量指向）。引擎**懒加载**（首次用到 OCR 动作才加载模型，纯图像场景零开销）。
    - **选型三连败（为何弃）**：tesseract（本机未装系统引擎+中文包，需用户装系统依赖）；ocr-rs/MNN（编译期须从 GitHub releases 下载预编译 MNN 库，HTTPS 443 不可达）；ocrs/RTen（`Model::load` 只认 `.rten` 私有 flatbuffers 格式、不认标准 ONNX，且仅 Latin 字母、预处理硬编码灰度，与 PP-OCRv4 中文 RGB 不匹配）。paddleocr_rs_onnx 是全 Rust ONNX Runtime 绑定 + 标准 ONNX 模型 + 中文 + 置信度/坐标 + MIT，最终命中。
    - **vendor patch（两处本地修改）**：① 上游 `configure_session_builder` 无条件引用 `ort::ep::{DirectML,CUDA,OpenVINO,NNAPI,CoreML,CANN}`，被 ort 2.0.0-rc 的 feature gate 掉 → 默认 features 下 6 个 E0433 编译失败；本地为各 EP 分支加 `#[cfg(feature = "...")]` 门控，未启用则落回 CPU 分支。② rec 模型输入为**全动态 shape** `[-1,3,-1,-1]`，上游 `(-1).max(1)=1` 把高度算成 1 → rec 输入被压成 1px 高、识别全空（实测 0 行）；修正为动态维度回退 **PP-OCRv4 标准高度 48**，实测中文识别准确（"龙之谷阴阳师12345" 等全部命中，置信度 1.0）。
    - **文字匹配语义**：`if_text` / `click_text` / `assert_text` 按 `text` 子串 contains 匹配（大小写敏感），多个命中行取**置信度最高**者；`click_text` 点击命中行中心，`jitter` 偏移限制在文字行范围内。

---

## 7. 迭代路线

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **M0 骨架** | 接入 xcap + enigo + rustautogui，实现截图保存 PNG、模拟一次键鼠 | 本机跑通采集与输入链路 |
| **M1 原语** | Vision trait 落地模板匹配，实现 click_image / wait_image / assert_image | 可对一个游戏完成「找图→点击→验证」手写调用 |
| **M2 引擎** | TOML 脚本解析 + 流程引擎 + 报告 | 纯配置文件能跑通一个冒烟场景 |
| **M3 增强** | ✅ 条件分支/循环、✅ HTML 报告 | 覆盖多场景，报告可读（HTML 已可读） |
| **打磨轮** | ✅ key_combo 全量按键、✅ region 区域限定匹配、✅ F9 failsafe、✅ template 模板采集子命令、✅ 失败自动存档+新旧对照图、✅ jitter 拟人化点击、✅ 模板匹配加速（金字塔+并行，实测 ~88x）、✅ fast→exact 确认开关（[meta] 全局 + [[step]] 步骤级覆盖，实测仅多 ~16% 开销） | 26 个单测全绿（另 3 个 ignored 基准/真实截图），实跑闭环验证 |
| **M4 窗口捕获** | ✅ 窗口级捕获（`[meta] window` 标题匹配 + xcap `capture_window` + 窗口→屏幕坐标映射，region 窗口内语义、snapshot 窗口感知、失败存档只含窗口） | 33 个单测全绿（另 3 个 ignored 基准/真实截图） |
| **M4 后续①** | ✅ 点击随机延时（`click_delay` + `click_delay_min`，复用 xorshift64*，点击前随机等待 `[min, max]`，报告显示延时） | 37 个单测全绿（另 3 个 ignored 基准/真实截图） |
| **M4 后续②** | ✅ OCR 文字识别（`ocr_text`/`if_text`/`click_text`/`assert_text`；paddleocr_rs_onnx + PP-OCRv4 中文模型；vendor patch 修复动态 shape bug；实测识别中文准确） | 39 个单测全绿（另 3 个 ignored 基准/真实截图） |
| **M4 后续候选** | GUI 录制器（egui）、场景静态校验、GitHub Actions CI + 打包 | 按用户优先级推进 |

---

## 8. 风险与对策

| 风险 | 对策 |
|---|---|
| 模板匹配对分辨率/画质敏感 | 限定窗口/区域、多分辨率模板、可调 precision 容差 |
| 模板匹配计算量大（M1 实测：imageproc 朴素算法，500×400 区域 debug 约 78s，全屏更久） | **已实现**图像金字塔 + Rayon 并行加速（合成基准 debug 400×300 约 14s→0.16s，≈88x）；配合 `region` 限定 + `--release` 效果最佳 |
| 窗口被遮挡截不到 | xcap 窗口捕获；必要时升级 Desktop Duplication |
| 反作弊封号 | 坚持纯视觉方案，明确不做内存读取/注入 |
| 杀软拦截输入模拟 | 文档说明白名单/签名建议；失败提示清晰 |
| 核心库 API 变动 | 全部隔离在 adapter 层，trait 为契约 |
| 纯色/无纹理区域 NCC 匹配「处处命中」（100×50 纯色模板在错误区域也 PASS） | 采集模板选**有纹理**的 UI 元素（按钮文字、图标边缘），避开纯色大块背景 |
| 模板路径带 `assets/` 前缀导致双重拼接 | `image` 路径相对 `--assets` 目录，直接写文件名 |
| 非交互会话输入被系统拦截（UIPI："not all input events were sent"） | 真实桌面 + 前台窗口 + 管理员/白名单运行；`move_mouse` 不受限时可先用其验证坐标 |
| OCR 模型/运行时体积大（模型 ~15MB + onnxruntime.dll ~18MB） | 随仓库提交 `assets/ocr/` + `libs/`，克隆即用；运行时设 `ORT_DYLIB_PATH` 指向 dll |
| OCR 纯 CPU 推理耗时 | 模型**懒加载**（纯图像场景零开销）；识别区域用 `region` 收窄；`--release` 构建；仅对需要文字的步骤启用 |
| OCR 误识别 / 动态文本（血量、数值、倒计时） | `ocr_text` 输出带置信度；条件/点击/断言取置信度最高命中行；关键断言可配合 `assert_image` 双保险 |

---

## 9. 不做的事（边界）

- 不做内存读取、DLL 注入、反作弊绕过；
- 不做**逐帧实时 OCR / 大模型端侧部署**：当前为 PP-OCRv4 CPU 推理（单帧识别足够，不追求视频流实时）；
- 不绑定任何具体游戏（配置层隔离差异）；
- 不作为违规外挂/挂机工具发布（仅用于个人合法自动化测试与研究）。

---

## 10. 目标游戏适配

> 首批目标：**龙之谷**（PC 端游）与 **阴阳师**（手游，PC 端经安卓模拟器运行）。两条适配基线如下，具体场景在 `scenarios/` 下按游戏分目录管理。

### 10.1 龙之谷（PC 端游）

| 维度 | 适配要点 |
|---|---|
| 运行模式 | 测试时固定用**窗口模式**（非全屏），`[meta] window = "龙之谷"`（或游戏窗口标题关键字）即启用 M4 窗口级捕获，规避遮挡、截图更稳定 |
| 分辨率 | 设置固定分辨率（如 1920×1080）并关闭动态分辨率，保证模板不随画质浮动失效 |
| 画面特征 | 3D 场景光照/特效动态变化大 → 模板匹配**只针对静态 UI**：按钮、图标、加载提示、任务栏、选项菜单；不做动态画面识别 |
| 可测场景 | 启动器 → 选区/选服 → 角色选择 → 进入主城 → 打开背包/任务面板等「UI 密集」流程，适合做冒烟测试 |
| 输入 | 移动（WASD）、技能快捷键、背包/菜单键用 `key_press` / `key_combo`；点击 UI 用 `click_image` |
| 拟人化操作 | 连击/重复刷本等反复点击时给 `click_image` 配 `jitter`（如 6~10px）+ `click_delay`（如 0.1~0.3s），每次落点与点击时机都动态分布、更接近真人，也降低机械重复特征 |
| OCR 适配 | 血量/等级/技能名/伤害数字等**动态文本**用 `ocr_text` 读取（模板匹配读不了变化文字）；"连接中断/更新公告"等关键文案用 `if_text` 兜底判断 |
| 注意 | 网络/登录态波动会导致加载时间变化 → 所有「等待」步骤要设足够 `timeout`，`wait_image` 轮询兜底 |

### 10.2 阴阳师（手游 · 安卓模拟器）

| 维度 | 适配要点 |
|---|---|
| 运行模式 | 经安卓模拟器（MuMu / 雷电 / 夜神等）运行 → 捕获目标是**模拟器窗口** |
| 窗口定位 | 模拟器窗口标题固定、分辨率可设固定值 → 模板稳定性好；**多开时必须指定准确窗口标题**（`[meta] window`）：框架按标题关键字 contains 匹配、忽略最小化窗口，多个实例命中时取 z 序最前者并告警——每个模拟器实例一个独立场景文件，避免截错实例 |
| 画面特征 | 阴阳师 UI 以静态 HUD 为主（按钮、式神图标、体力/勾玉数值区、公告面板）→ 高度适合模板匹配 |
| 可测场景 | 登录 → 庭院主界面 → 探索副本入口 → 战斗结算 → 商店/式神录等，均以「找图→点击→断言」原语即可覆盖 |
| 输入 | 点击类操作为主；模拟器若拦截输入，需在模拟器设置中开启**鼠标直通/触摸模拟**，必要时用 `click` 坐标直发 |
| 拟人化操作 | 阴阳师以点击类操作为主 → 关键按钮点击配 `jitter` + `click_delay`；随机弹窗关闭用 `if_image` + `click_image`（可配 jitter/click_delay）兜底 |
| OCR 适配 | 式神名/材料数量/剧情文案/公告标题等用 `ocr_text`；**文字按钮**直接 `click_text` 点按（如 `text = "开始战斗"`），免去为每个文字做模板；"胜利/失败/结算"等文案用 `assert_text` 断言 |
| 注意 | 游戏内**随机弹窗/公告/活动界面**频繁 → 场景需加「关闭弹窗」容错步骤，或对非关键弹窗做跳过处理 |

### 10.3 通用约束（两游戏共同）

- 模板资源统一放 `assets/<game>/`，命名按「界面_元素」约定（如 `dn_login_btn.png` / `yyj_garden.png`）；
- 每个游戏建独立场景配置目录 `scenarios/<game>/`，互不干扰；
- 首次适配某游戏时，用 `template` 子命令（`--at-mouse` / `--x --y` / `--full`）直接采集真实界面模板到 `assets/<game>/`，命令同时输出坐标与可直接粘贴的场景片段，无需手工截图裁图；
- **OCR 与模板匹配互补使用**：模板定位静态 UI（按钮/图标），OCR 读动态文字（血量/数值/文案）。OCR 模型与运行时随仓库提供（`assets/ocr/` + `libs/`），无需手工下载，运行时设 `ORT_DYLIB_PATH` 指向 `libs/onnxruntime.dll`；
- 两游戏均为运营中网游，仅用于**个人合法自动化测试与研究**，遵守各游戏服务条款，不用于挂机牟利或违反规则的行为。
