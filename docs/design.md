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

动作原语一览（MVP 范围）：
`wait_image` / `find_image` / `click_image` / `click`(坐标) / `move_mouse` / `key_press` / `key_combo` / `type_text` / `assert_image` / `wait`(固定延时) / `screenshot`(存档证据)

---

## 6. 关键机制

1. **DPI 感知**：全局换算逻辑坐标 → 物理像素，避免高 DPI 下找图与点击坐标错位；
2. **窗口捕获 vs 全屏**：默认全屏；指定 `window` 时用 xcap 按窗口捕获，规避遮挡；
3. **超时与重试**：每个步骤独立 `timeout`；`wait_image` 内部轮询（建议间隔 0.2s）；失败可配 `retry` 次数；
4. **紧急停止（failsafe）**：默认绑定 `F9` 热键，脚本运行中可随时中止（参考 useHID 的 Failsafe 设计）；
5. **证据链**：每个步骤执行前后自动截图存档到 `reports/<场景名>/<时间戳>/`，失败必有图；
6. **结果报告**：汇总各步骤 通过/失败/耗时，输出文本报告（后续可扩展 HTML/JSON）。

---

## 7. 迭代路线

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **M0 骨架** | 接入 xcap + enigo + rustautogui，实现截图保存 PNG、模拟一次键鼠 | 本机跑通采集与输入链路 |
| **M1 原语** | Vision trait 落地模板匹配，实现 click_image / wait_image / assert_image | 可对一个游戏完成「找图→点击→验证」手写调用 |
| **M2 引擎** | TOML 脚本解析 + 流程引擎 + 报告 | 纯配置文件能跑通一个冒烟场景 |
| **M3 增强** | 条件分支/循环、OCR 接入、HTML 报告、可选 GUI 录制器 | 覆盖多场景，报告可读 |

---

## 8. 风险与对策

| 风险 | 对策 |
|---|---|
| 模板匹配对分辨率/画质敏感 | 限定窗口/区域、多分辨率模板、可调 precision 容差 |
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
