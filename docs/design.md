# auto-game 通用电脑游戏自动化测试框架 · 设计文档

> 定位：不针对单一游戏，而是把「截图 → 识别 → 决策 → 输入」抽象成可插拔原语，用配置文件描述测试流程，对任意 PC 游戏执行自动化测试。
> 原则：**纯视觉方案**（不读内存、不注入），规避反作弊风险；**复用成熟核心库**，不重复造轮子。

---

## 1. 生态调研结论（参考核心库）

| 能力 | 候选库 | 选择 | 理由 |
|---|---|---|---|
| 截图 | rustautogui / xcap / screenshots | **xcap**（增强）+ **rustautogui**（内置） | xcap 支持多显示器、指定窗口捕获；rustautogui 自带截图与找图同源 |
| 输入模拟 | enigo / rustautogui / autocontrol | **enigo**（抽象）+ **rustautogui**（内置） | enigo 跨平台、被 RustDesk 生产验证，trait 抽象便于替换 |
| 模板匹配找图 | rustautogui（Segmented NCC / FFT）/ imageproc / template-matching(GPU) / opencv | **rustautogui**（主） | 自带多线程 + 可选 OpenCL，免 OpenCV 重依赖；`imageproc::template_matching` 做纯 Rust 兜底 |
| OCR（可选，预留） | tesseract-rs / ddddocr | 预留接口 | 按需接入，不进 MVP |
| 配置 | serde + TOML | **serde + toml** | Rust 标准做法 |
| 日志 / 报告 | tracing | **tracing** | 结构化、分层过滤 |

> 依赖选型结论：**rustautogui = 底层主力**（它把 PyAutoGUI 的截图+找图+键鼠一次性提供），xcap 补多显示器/窗口场景，enigo 提供可替换的输入抽象。上层价值在「脚本引擎 + 报告」，这是目前开源空白。

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
│  FindImage / ClickImage / KeyPress / TypeText /   │
│  MoveMouse / WaitImage / AssertImage              │
├───────────────────────────────────────────────────┤
│ Adapter Layer  可插拔后端（trait）                  │
│  CaptureTrait / InputTrait / VisionTrait          │
├───────────────────────────────────────────────────┤
│ 核心库  xcap · enigo · rustautogui（image/serde…） │
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
├── main.rs          # CLI 入口：auto-game run <场景.toml>
├── lib.rs           # 对外库 API（供二次开发/将来 GUI 调用）
├── adapter/
│   ├── mod.rs       # 抽象 trait 定义（Capture/Input/Vision）
│   ├── capture.rs   # 截图后端：xcap 实现 + rustautogui 实现
│   ├── input.rs     # 输入后端：enigo 实现 + rustautogui 实现
│   └── vision.rs    # 识别后端：rustautogui 模板匹配实现
├── action.rs        # 动作原语枚举 + 各自执行逻辑
├── engine.rs        # 流程引擎：上下文、条件等待、重试、failsafe
├── script.rs        # TOML 解析 → 动作序列（serde deserialize）
├── config.rs        # 全局配置：DPI、全局超时、fail-safe 热键、日志级别
└── report.rs        # 报告：步骤日志、截图存档、通过/失败/耗时汇总
```

---

## 4. 核心抽象（trait 设计）

```rust
// 截图
pub trait CaptureTrait {
    fn capture_full(&self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>;
    fn capture_region(&self, rect: Rect) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>;
    fn capture_window(&self, title: &str) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>; // xcap 增强
}

// 输入
pub trait InputTrait {
    fn move_mouse(&self, x: i32, y: i32) -> Result<()>;
    fn click(&self, button: MouseButton) -> Result<()>;
    fn double_click(&self) -> Result<()>;
    fn scroll(&self, dy: i32) -> Result<()>;
    fn key_press(&self, key: Key) -> Result<()>;
    fn key_combo(&self, keys: &[Key]) -> Result<()>;
    fn type_text(&self, text: &str) -> Result<()>;
}

// 识别
pub trait VisionTrait {
    fn find_template(&self, image: &Path, precision: f64) -> Result<Option<Match>>;
    fn wait_until_found(&self, image: &Path, precision: f64, timeout: Duration)
        -> Result<Option<Match>>;
}
```

---

## 5. 脚本格式（TOML 场景示例）

```toml
# 场景：登录主菜单并验证
[meta]
name = "登录主菜单冒烟"
window = "MyGame"          # 可选：限定窗口（xcap 捕获）

[[step]]
action = "wait_image"
image = "assets/login_btn.png"
precision = 0.9
timeout = 15

[[step]]
action = "click_image"
image = "assets/login_btn.png"
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
image = "assets/main_menu.png"
timeout = 20
```

动作原语一览（M3 已实现）：
`wait_image` / `find_image` / `click_image` / `click`(坐标) / `move_mouse` / `key_press` / `type_text` / `assert_image` / `wait`(固定延时) / `screenshot`(存档证据) / `repeat`+`end_repeat`(循环) / `if_image`+`else`+`end_if`(条件分支)

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

1. **DPI 感知**：全局换算逻辑坐标 → 物理像素，避免高 DPI 下找图与点击坐标错位；
2. **窗口捕获 vs 全屏**：默认全屏；指定 `window` 时用 xcap 按窗口捕获，规避遮挡；
3. **超时与重试**：每个步骤独立 `timeout`；`wait_image` 内部轮询（建议间隔 0.2s）；失败可配 `retry` 次数；
4. **紧急停止（failsafe）**：默认绑定 `F9` 热键，脚本运行中可随时中止（参考 useHID 的 Failsafe 设计）；
5. **证据链**：每个步骤执行前后自动截图存档到 `reports/<场景名>/<时间戳>/`，失败必有图；
6. **结果报告**：汇总各步骤 通过/失败/耗时，输出文本报告 + HTML 报告。

---

## 7. 迭代路线

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **M0 骨架** | 接入 xcap + enigo + rustautogui，实现截图保存 PNG、模拟一次键鼠 | 本机跑通采集与输入链路 |
| **M1 原语** | Vision trait 落地模板匹配，实现 click_image / wait_image / assert_image | 可对一个游戏完成「找图→点击→验证」手写调用 |
| **M2 引擎** | TOML 脚本解析 + 流程引擎 + 报告 | 纯配置文件能跑通一个冒烟场景 |
| **M3 增强** | ✅ 条件分支/循环、✅ HTML 报告；OCR 接入 / GUI 录制器预留 | 覆盖多场景，报告可读（HTML 已可读） |

---

## 8. 风险与对策

| 风险 | 对策 |
|---|---|
| 模板匹配对分辨率/画质敏感 | 限定窗口/区域、多分辨率模板、可调 precision 容差 |
| 模板匹配计算量大（M1 实测：imageproc 朴素算法，500×400 区域 debug 约 78s，全屏更久） | 限定搜索区域（capture_region）、优先 release 模式运行、后续可换 rustautogui 分段匹配或 FFT 加速 |
| 窗口被遮挡截不到 | xcap 窗口捕获；必要时升级 Desktop Duplication |
| 反作弊封号 | 坚持纯视觉方案，明确不做内存读取/注入 |
| 杀软拦截输入模拟 | 文档说明白名单/签名建议；失败提示清晰 |
| 核心库 API 变动 | 全部隔离在 adapter 层，trait 为契约 |

---

## 9. 不做的事（边界）

- 不做内存读取、DLL 注入、反作弊绕过；
- 不做通用 OCR 进 MVP（预留接口）；
- 不绑定任何具体游戏（配置层隔离差异）；
- 不作为违规外挂/挂机工具发布（仅用于个人合法自动化测试与研究）。

---

## 10. 目标游戏适配

> 首批目标：**龙之谷**（PC 端游）与 **阴阳师**（手游，PC 端经安卓模拟器运行）。两条适配基线如下，具体场景在 `scenarios/` 下按游戏分目录管理。

### 10.1 龙之谷（PC 端游）

| 维度 | 适配要点 |
|---|---|
| 运行模式 | 测试时固定用**窗口模式**（非全屏），配合 xcap `capture_window` 捕获，避免被遮挡、截图更稳定 |
| 分辨率 | 设置固定分辨率（如 1920×1080）并关闭动态分辨率，保证模板不随画质浮动失效 |
| 画面特征 | 3D 场景光照/特效动态变化大 → 模板匹配**只针对静态 UI**：按钮、图标、加载提示、任务栏、选项菜单；不做动态画面识别 |
| 可测场景 | 启动器 → 选区/选服 → 角色选择 → 进入主城 → 打开背包/任务面板等「UI 密集」流程，适合做冒烟测试 |
| 输入 | 移动（WASD）、技能快捷键、背包/菜单键用 `key_press` / `key_combo`；点击 UI 用 `click_image` |
| 注意 | 网络/登录态波动会导致加载时间变化 → 所有「等待」步骤要设足够 `timeout`，`wait_image` 轮询兜底 |

### 10.2 阴阳师（手游 · 安卓模拟器）

| 维度 | 适配要点 |
|---|---|
| 运行模式 | 经安卓模拟器（MuMu / 雷电 / 夜神等）运行 → 捕获目标是**模拟器窗口** |
| 窗口定位 | 模拟器窗口标题固定、分辨率可设固定值 → 模板稳定性好；**多开时必须指定准确窗口标题**，避免截错实例 |
| 画面特征 | 阴阳师 UI 以静态 HUD 为主（按钮、式神图标、体力/勾玉数值区、公告面板）→ 高度适合模板匹配 |
| 可测场景 | 登录 → 庭院主界面 → 探索副本入口 → 战斗结算 → 商店/式神录等，均以「找图→点击→断言」原语即可覆盖 |
| 输入 | 点击类操作为主；模拟器若拦截输入，需在模拟器设置中开启**鼠标直通/触摸模拟**，必要时用 `click` 坐标直发 |
| 注意 | 游戏内**随机弹窗/公告/活动界面**频繁 → 场景需加「关闭弹窗」容错步骤，或对非关键弹窗做跳过处理 |

### 10.3 通用约束（两游戏共同）

- 模板资源统一放 `assets/<game>/`，命名按「界面_元素」约定（如 `dn_login_btn.png` / `yyj_garden.png`）；
- 每个游戏建独立场景配置目录 `scenarios/<game>/`，互不干扰；
- 首次适配某游戏时，先用 `screenshot` 原语采集真实界面截图，再裁剪生成模板，确保模板与实机一致；
- 两游戏均为运营中网游，仅用于**个人合法自动化测试与研究**，遵守各游戏服务条款，不用于挂机牟利或违反规则的行为。
