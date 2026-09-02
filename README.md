# auto-game · 通用电脑游戏自动化测试框架

用 **Rust** 编写的通用电脑游戏自动化测试框架。把「截图 → 识别 → 决策 → 输入」抽象成可插拔的动作原语，用 **TOML 配置文件**描述测试流程，即可对任意 PC 游戏执行自动化冒烟测试。

> **定位**：纯视觉方案（不读内存、不注入），规避反作弊风险；复用成熟核心库，不重复造轮子。
> **注意**：仅用于个人合法自动化测试与研究，请遵守各游戏服务条款，勿用于挂机牟利或违反规则的行为。

---

## ✨ 特性

- **配置驱动**：测试场景 = TOML 文件，不写代码即可编排流程
- **控制流**：支持循环（`repeat`）与条件分支（`if_image` / `else`），可处理"遇弹窗关闭""重复刷本"等真实场景
- **区域匹配**：可限定搜索区域，规避全屏模板匹配的性能瓶颈（M1 实测全屏朴素算法极慢，区域化后大幅提速）
- **紧急停止（failsafe）**：运行中随时按 `F9` 中止，防止脚本失控
- **组合键**：`key_combo` 支持 Ctrl+A 等组合键；按键覆盖游戏常用键位（WASD/数字/F1-F12/方向/修饰键）
- **报告**：文本报告 + HTML 报告（状态着色、耗时统计、注入转义）
- **可插拔后端**：截图 / 输入 / 识别全部封装在 trait 之后，可替换底层库（xcap / enigo / rustautogui）

---

## 🚀 快速开始

### 环境要求
- Rust 工具链（稳定版即可）：<https://rustup.rs>
- Windows / macOS / Linux

### 构建
```bash
cd auto-game
cargo build --release
```

### 运行一个场景
```bash
# 运行自带冒烟场景（含截图/等待/移动/点击/按键）
cargo run -- run scenarios/demo.toml

# 运行 M3 控制流演示（循环 + 条件分支）
cargo run -- run scenarios/m3_demo.toml

# 指定模板资源目录（默认 assets/）
cargo run -- run scenarios/xxx.toml --assets assets/dn
```

运行结束后，报告输出到控制台，并生成 `reports/<场景名>/index.html`（HTML 报告）与步骤截图。

---

## 🖥️ CLI

```
auto-game run <场景.toml> [--assets <资源目录>]
```

| 参数 | 说明 |
|---|---|
| `run <场景.toml>` | 执行场景文件（必填） |
| `--assets <目录>` | 模板图像根目录，默认 `assets/` |

---

## 📝 场景配置语法

场景是一个 TOML 文件：`[meta]` 声明元信息，`[[step]]` 数组按顺序定义动作步骤。

```toml
[meta]
name = "登录冒烟"        # 场景名（用于报告目录与展示）
window = "MyGame"       # 可选：限定窗口标题（预留，xcap 窗口捕获）

[[step]]
action = "wait_image"   # 等待模板出现
image = "login_btn.png"
precision = 0.9
timeout = 15

[[step]]
action = "click_image"  # 找到模板并点击其中心
image = "login_btn.png"
precision = 0.9

[[step]]
action = "type_text"    # 输入文本
text = "test_account"

[[step]]
action = "key_press"    # 按键
key = "enter"

[[step]]
action = "assert_image" # 断言模板在超时内出现
image = "main_menu.png"
timeout = 20
```

> `image` 路径是**相对 `--assets` 目录**的，不要写 `assets/` 前缀。

---

## 🎮 动作参考

### 基础动作

| action | 参数 | 说明 |
|---|---|---|
| `screenshot` | — | 全屏截图存档到 `reports/<场景>/step_<n>.png` |
| `wait` | `seconds` | 固定延时（秒） |
| `move_mouse` | `x`, `y` | 移动鼠标到屏幕坐标 |
| `click` | `x`, `y` | 移动到坐标并左键点击 |
| `key_press` | `key` | 按下单个按键 |
| `key_combo` | `keys` | 组合键，如 `keys = ["ctrl", "a"]` |
| `type_text` | `text` | 输入一段文本 |

### 图像识别动作

| action | 参数 | 说明 |
|---|---|---|
| `find_image` | `image`, `precision`, `region` | 查找模板，输出位置与置信度（未找到不算失败） |
| `wait_image` | `image`, `precision`, `timeout`, `region` | 轮询等待模板出现（200ms 间隔） |
| `click_image` | `image`, `precision`, `region` | 找到模板并点击其中心；未找到则失败 |
| `assert_image` | `image`, `precision`, `timeout`, `region` | 断言模板在超时内出现；否则失败 |
| `if_image` | `image`, `precision`, `region` | 条件判断：命中走 then 分支，未命中走 else/跳过 |

### 通用参数

| 参数 | 适用动作 | 说明 |
|---|---|---|
| `precision` | 图像类 | 匹配置信度阈值 0.0~1.0，默认 `0.85` |
| `timeout` | 等待/断言类 | 超时秒数，默认 `15` |
| `region` | 图像类 | 限定搜索区域 `{ x, y, w, h }`，**强烈建议指定**以提升性能 |

### 控制流

| action | 参数 | 说明 |
|---|---|---|
| `repeat` | `count` | 开始循环（`count` 次），与 `end_repeat` 配对 |
| `end_repeat` | — | 结束循环 |
| `if_image` | `image` 等 | 开始条件分支，与 `end_if` 配对 |
| `else` | — | 否则分支（可选） |
| `end_if` | — | 结束条件 |

**示例：重复刷本 10 次，遇弹窗就关闭**
```toml
[[step]]
action = "repeat"
count = 10

[[step]]
action = "if_image"
image = "close_btn.png"
precision = 0.8

[[step]]
action = "click_image"
image = "close_btn.png"

[[step]]
action = "end_if"

[[step]]
action = "click_image"
image = "attack_btn.png"
region = { x = 800, y = 600, w = 400, h = 300 }

[[step]]
action = "end_repeat"
```

> 控制结构必须配对闭合（`repeat`/`end_repeat`、`if_image`/`else`/`end_if`），未闭合会在**编译期**报错，不会带病执行。

---

## ⌨️ 按键对照

`key_press` 与 `key_combo` 的 `key` / `keys` 取值（不区分大小写）：

| 类别 | 取值 |
|---|---|
| 字母 | `a` ~ `z` |
| 数字 | `0` ~ `9` |
| 功能键 | `f1` ~ `f12` |
| 方向键 | `up` / `down` / `left` / `right` |
| 编辑键 | `enter`(return) / `escape`(esc) / `space` / `tab` / `backspace` / `delete`(del) / `insert`(ins) / `home` / `end` / `pageup`(pgup) / `pagedown`(pgdn) |
| 修饰键 | `ctrl`(control) / `shift` / `alt` / `meta`(win) |

---

## 🖼️ 区域匹配（性能要点）

**背景**：默认的模板匹配是"全屏截取 → 朴素算法扫描"，在 debug 模式下 500×400 区域就实测约 78 秒，全屏更久。

**做法**：给图像类动作加 `region` 参数，只在目标区域搜索：
```toml
[[step]]
action = "click_image"
image = "skill_btn.png"
region = { x = 1600, y = 900, w = 300, h = 150 }
```
配合 `cargo build --release` 运行，性能可接受。建议所有图像动作都尽量指定 `region`。

---

## 🛑 紧急停止（failsafe）

场景运行中，**随时按 `F9`** 可立即中止并记录一条 FAIL 步骤。适用于脚本失控、需要紧急接管鼠标键盘时。

> 非交互会话（如无桌面环境）下可能无法读取键盘状态，此时 failsafe 自动禁用并打印警告，不影响场景执行。

---

## 📊 报告

- **控制台**：文本报告（每步状态/耗时/详情 + 汇总）
- **HTML**：`reports/<场景名>/index.html`，含状态着色、耗时统计；所有文本均做 HTML 转义，防注入
- **截图存档**：`screenshot` 动作与失败步骤的截图保存于 `reports/<场景名>/`

---

## 🎯 目标游戏适配要点

详见 [`docs/design.md`](docs/design.md) 第 10 节，摘要：

| 游戏 | 运行模式 | 适配要点 |
|---|---|---|
| **龙之谷**（PC 端游） | 固定**窗口模式** | 模板只针对静态 UI（按钮/图标/菜单）；WASD 移动用 `key_press`，UI 点击用 `click_image`；等待步骤设足 timeout |
| **阴阳师**（手游·安卓模拟器） | 模拟器窗口 | 捕获目标是模拟器窗口（多开须指定标题）；点击类操作为主；随机弹窗频繁 → 用 `if_image` 关闭弹窗 |

模板资源按 `assets/<game>/` 分目录，命名 `界面_元素.png`（如 `dn_login_btn.png` / `yyj_garden.png`）。

---

## 🧪 测试

```bash
cargo test          # 运行单元测试（脚本解析 / 控制流编译 / 报告 / 按键映射）
```
自带 17 个单元测试，覆盖：TOML 解析、`repeat`/`if`/`else` 配对与跳转填充、未闭合报错、嵌套控制流、报告汇总与 HTML 转义、按键名解析与映射。

`scenarios/` 下还提供了可执行演示场景：
- `demo.toml` — M2 冒烟（全链路）
- `m3_demo.toml` — M3 控制流（循环 + 分支）
- `features.toml` — 新特性（组合键 / 区域匹配 / 控制流组合）

---

## 📁 目录结构

```
auto-game/
├── Cargo.toml
├── README.md                 # 本文件
├── docs/design.md            # 设计文档（架构/选型/游戏适配）
├── src/
│   ├── main.rs               # CLI 入口：auto-game run <场景>
│   ├── lib.rs                # 库入口（供二次开发）
│   ├── adapter/              # 可插拔后端：capture / input / vision + Region
│   ├── action.rs             # 动作原语（截图/移动/点击/按键/找图）
│   ├── engine.rs             # 流程引擎（编译指令 + 循环/分支 + failsafe）
│   ├── script.rs             # TOML 场景解析
│   └── report.rs             # 文本 + HTML 报告
├── scenarios/                # 场景配置（演示/测试用例）
└── assets/                   # 模板图像（按游戏分目录）
```

---

## ❓ FAQ

**Q：模板匹配很慢怎么办？**
指定 `region` 缩小搜索范围，并用 `--release` 构建运行。

**Q：点击/按键没生效？**
输入模拟可能被系统/杀软拦截（UIPI）。请以管理员/前台窗口运行，或在杀软中添加白名单。

**Q：游戏窗口被遮挡截不到图？**
将游戏设为窗口模式并置于前台；窗口捕获（xcap `capture_window`）为预留能力，后续版本支持。

**Q：会不会被反作弊封号？**
本框架坚持纯视觉方案（只截图 + 模拟键鼠），不读内存、不注入，风险远低于内存类工具；仍请遵守游戏条款。
