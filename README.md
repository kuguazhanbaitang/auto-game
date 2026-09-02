# auto-game · 通用电脑游戏自动化测试框架

用 **Rust** 编写的通用电脑游戏自动化测试框架。把「截图 → 识别 → 决策 → 输入」抽象成可插拔的动作原语，用 **TOML 配置文件**描述测试流程，即可对任意 PC 游戏执行自动化冒烟测试。

> **定位**：纯视觉方案（不读内存、不注入），规避反作弊风险；复用成熟核心库，不重复造轮子。
> **注意**：仅用于个人合法自动化测试与研究，请遵守各游戏服务条款，勿用于挂机牟利或违反规则的行为。

---

## ✨ 特性

- **配置驱动**：测试场景 = TOML 文件，不写代码即可编排流程
- **模板采集**：内置 `template` 子命令——用代码截图生成模板 PNG 到 `assets/`，并自动输出坐标与可直接粘贴的 TOML 片段，无需手工截图裁图
- **控制流**：支持循环（`repeat`）与条件分支（`if_image` / `else`），可处理"遇弹窗关闭""重复刷本"等真实场景
- **区域匹配**：可限定搜索区域，规避全屏模板匹配的性能瓶颈（M1 实测全屏朴素算法极慢，区域化后大幅提速）
- **模板匹配加速**：图像金字塔粗到细 + Rayon 并行候选精匹配——合成基准实测 debug 下 **~88 倍加速**（14s→0.16s），全屏匹配不再慢
- **fast→exact 确认开关**：`[meta] verify_exact = true` 时，每个模板在金字塔定位后再做一次像素级精确确认——位置确切到像素、置信度精确，适合"点击必须落在确切像素"的场景；实测仅比 fast 多 ~16% 开销（远快于全图精确匹配）
- **紧急停止（failsafe）**：运行中随时按 `F9` 中止，防止脚本失控
- **失败现场存档**：任一步骤失败自动保存现场截图；若该步带模板，再生成「左=旧模板 / 右=现场」对照图，UI 变更一目了然
- **拟人化点击**：点击动作支持 `jitter` 随机抖动——每次点击位置在目标周围动态分布，不总点同一个像素，更接近真人操作
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

### 采集游戏模板（不用手工截图）
把游戏开成**窗口模式**并置于前台，用框架自己截图生成模板，命令会打印坐标与可粘贴的 TOML 片段：

```bash
# 方式一：先全屏截图定位元素（打开生成的 full.png，用看图工具读取元素像素坐标）
cargo run -- template --full --name full

# 方式二：精确裁剪指定屏幕区域为模板
cargo run -- template --name login_btn --x 640 --y 380 --w 120 --h 40

# 方式三：把鼠标移到元素上，以鼠标为左上角/中心截取（真实桌面最方便）
cargo run -- template --name attack_btn --at-mouse --w 80 --h 80
cargo run -- template --name attack_btn --at-mouse --center --w 80 --h 80

# 附加 --preview 可同时存一张全屏预览，便于查看元素坐标
```

---

## 🖥️ CLI

```
auto-game run <场景.toml> [--assets <资源目录>]
auto-game template [选项]   # 模板采集：截图生成模板 + 输出坐标/TOML 片段
```

| 命令 | 说明 |
|---|---|
| `run <场景.toml>` | 执行场景文件（必填） |
| `run --assets <目录>` | 模板图像根目录，默认 `assets/` |
| `template` | 模板采集子命令，见下方「模板采集」 |

### `template` 采集参数

| 参数 | 说明 |
|---|---|
| `--name <模板名>` | 必填，输出文件名（不含扩展名，如 `login_btn` → `login_btn.png`） |
| `--out <目录>` | 输出目录，默认 `assets/` |
| `--x --y` | 区域左上角坐标（区域模式） |
| `--w --h` | 区域宽高 |
| `--at-mouse` | 以鼠标当前位置为左上角截取（需 `--w --h`） |
| `--center` | 配合 `--at-mouse`：以鼠标位置为中心截取 |
| `--full` | 全屏截图（用于定位元素坐标） |
| `--preview` | 额外保存一张全屏预览到 `reports/`，便于确认坐标 |

---

## 📝 场景配置语法

场景是一个 TOML 文件：`[meta]` 声明元信息，`[[step]]` 数组按顺序定义动作步骤。

```toml
[meta]
name = "登录冒烟"        # 场景名（用于报告目录与展示）
window = "MyGame"       # 可选：限定窗口标题（预留，xcap 窗口捕获）
<<<<<<< HEAD
verify_exact = true     # 可选：fast→exact 精确确认全局默认（false；可被步骤级覆盖）
=======
verify_exact = true     # 可选：模板匹配 fast→exact 精确确认（默认 false，见「区域匹配与性能」）
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

[[step]]
action = "wait_image"   # 等待模板出现
image = "login_btn.png"
precision = 0.9
timeout = 15

[[step]]
action = "click_image"  # 找到模板并点击其中心
image = "login_btn.png"
precision = 0.9
<<<<<<< HEAD
verify_exact = false    # 步骤级覆盖：此步关闭精确确认（其余步仍走全局 true）
=======
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

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
| `click` | `x`, `y`, `jitter` | 移动到坐标并左键点击；`jitter` 可在 ±N 像素内随机偏移 |
| `key_press` | `key` | 按下单个按键 |
| `key_combo` | `keys` | 组合键，如 `keys = ["ctrl", "a"]` |
| `type_text` | `text` | 输入一段文本 |

### 图像识别动作

| action | 参数 | 说明 |
|---|---|---|
| `find_image` | `image`, `precision`, `region` | 查找模板，输出位置与置信度（未找到不算失败） |
| `wait_image` | `image`, `precision`, `timeout`, `region` | 轮询等待模板出现（200ms 间隔） |
| `click_image` | `image`, `precision`, `region`, `jitter` | 找到模板并点击其中心；未找到则失败 |
| `assert_image` | `image`, `precision`, `timeout`, `region` | 断言模板在超时内出现；否则失败 |
| `if_image` | `image`, `precision`, `region` | 条件判断：命中走 then 分支，未命中走 else/跳过 |

### 通用参数

| 参数 | 适用动作 | 说明 |
|---|---|---|
| `precision` | 图像类 | 匹配置信度阈值 0.0~1.0，默认 `0.85` |
| `timeout` | 等待/断言类 | 超时秒数，默认 `15` |
| `region` | 图像类 | 限定搜索区域 `{ x, y, w, h }`，**强烈建议指定**以提升性能 |
| `jitter` | `click` / `click_image` | 点击随机抖动像素，每次点击在目标 ±N 内动态分布，默认 `0`（精确点击） |
<<<<<<< HEAD
| `verify_exact` | 图像类 | 按步骤覆盖全局开关：显式 `true` / `false` 覆盖 `[meta] verify_exact`，缺省回退全局值 |
=======
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

**全局开关（`[meta]`）**：

| 参数 | 说明 |
|---|---|
<<<<<<< HEAD
| `verify_exact` | 默认 `false`。设为 `true` 时，所有模板匹配走「fast 粗定位 → exact 精确确认」，返回像素级精确位置与准确置信度；金字塔漏检自动回退全图精确匹配兜底。某步骤可写 `[[step]] verify_exact = false` 单独关闭（见「通用参数」） |
=======
| `verify_exact` | 默认 `false`。设为 `true` 时，所有模板匹配走「fast 粗定位 → exact 精确确认」，返回像素级精确位置与准确置信度；金字塔漏检自动回退全图精确匹配兜底。见「区域匹配与性能」 |
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

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

## 📸 模板采集（不手工截图）

框架内置 `template` 子命令，**用代码截图生成模板**，同时给出它的屏幕坐标——「截图 → 模板 → 坐标 → 场景片段」全链路闭环，不需要任何外部截图工具。

**完整流程（以龙之谷登录按钮为例）：**

1. **定位**：游戏窗口模式放前台，先全屏截一张定位图
   ```bash
   cargo run -- template --full --name full
   # 打开 assets/full.png，找到「登录」按钮的像素坐标（如 640, 380）
   ```

2. **裁剪模板**：按坐标精确截取按钮区域
   ```bash
   cargo run -- template --name dn_login_btn --x 640 --y 380 --w 120 --h 40
   ```
   命令输出（保存路径 + 可直接粘贴的场景片段）：
   ```
   ✅ 模板已保存: assets\dn_login_btn.png  (120x40)

   在场景中直接使用（复制以下片段）：
   [[step]]
   action = "click_image"
   image = "dn_login_btn.png"
   precision = 0.85
   region = { x = 640, y = 380, w = 120, h = 40 }
   ```

3. **粘贴到场景**：把输出的 `image` + `region` 片段并入你的 TOML 场景即可。`region` 既描述了模板所在位置，又用来**限定搜索范围**（匹配更快更准）。

**鼠标快捷流**：不想看图查坐标时，把鼠标移到元素上直接截：
```bash
cargo run -- template --name attack_btn --at-mouse --center --w 80 --h 80
```
（以鼠标为中心截 80×80；`--at-mouse` 不带 `--center` 则以鼠标为左上角。）

> 提示：截图坐标以**屏幕左上角 (0,0)** 为原点。游戏请用**固定窗口模式**，避免窗口尺寸/缩放变化导致坐标漂移。

---

## 🖱️ 拟人化点击（jitter）

默认点击总是落在模板中心/指定坐标（同一像素点）。开启 `jitter` 后，每次点击位置在目标周围 **±N 像素**内随机分布，更接近真人操作，也避免长期机械重复同一坐标：

```toml
# 坐标点击：以 (500, 400) 为中心，每次在 ±10px 内随机偏移
[[step]]
action = "click"
x = 500
y = 400
jitter = 10

# 模板点击：以模板中心为基座，偏移被限制在模板范围内（不会点出元素）
[[step]]
action = "click_image"
image = "attack_btn.png"
region = { x = 800, y = 600, w = 120, h = 60 }
jitter = 8
```

要点：
- `jitter = 0`（默认）→ 精确点击，行为与之前完全一致
- `click` 的偏移：基座坐标 ± jitter
- `click_image` 的偏移：以模板中心为基座，且**自动限制在模板范围内**，保证不会点偏到目标外
- 报告会显示实际点击坐标与基座、偏移量，方便追溯：
  ```
  点击坐标 (504, 397)（基座 (500, 400) + jitter (4, -3)）
  ```

---

## 🖼️ 区域匹配与性能（性能要点）

**背景**：默认的模板匹配是"全屏截取 → 算法扫描"，M1 实测 imageproc 朴素 NCC 在 debug 模式下 500×400 区域约 78 秒，全屏更久。

**已内置加速**：`find_template` 默认走**图像金字塔 + 并行**路径——先在低分辨率全图粗定位 top-K 候选，再逐层在候选邻域并行精匹配，把「全屏大匹配」变为「若干小窗口匹配」，语义不变（仍返回全局最高置信度位置）。合成基准实测（debug，400×300 + 50×40 模板）**约 14s → 0.16s（≈88 倍）**。小图/小模板自动回退精确匹配。

**需要确切像素？开 `verify_exact`**：金字塔加速返回的位置精确到 ±1px、置信度为近似值，对绝大多数点击足够。若需求是"点击必须落在确切像素"（如精确像素断言、对极小 UI 区域点击），在 `[meta]` 设置 `verify_exact = true`，每个模板走「fast 粗定位 → 最终位置精确确认」：在模板 + 2×radius 邻域内做一次精确 NCC，抹掉降采样误差、给出准确置信度；金字塔漏检时自动回退全图精确匹配兜底（保证不漏检）。合成基准实测（debug，400×300 + 50×40 模板）：fast=142ms vs 确认后=166ms vs 全图精确=11.5s——确认几乎不增加开销（≈69 倍 vs 全图精确）。

<<<<<<< HEAD
**粒度控制**：全局开启后，若个别步骤想走快路径，用步骤级覆盖单独关闭：
```toml
[meta]
verify_exact = true      # 全局默认开启

[[step]]
action = "wait_image"    # 关键步骤：保持像素级精确
image = "boss_hp.png"

[[step]]
action = "click_image"   # 高频点击：此步关闭确认，恢复 ~88x 快路径
image = "attack_btn.png"
verify_exact = false
```
未声明 `verify_exact` 的步骤自动回退全局值；步骤级显式声明优先于全局。

=======
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec
**仍建议**：给图像类动作加 `region` 参数进一步收窄搜索范围：
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

### 失败自动存档（UI 变更排查）

任一步骤 **FAIL** 时，框架自动存档现场画面，方便核对「UI 是否更新」：

| 文件 | 内容 |
|---|---|
| `fail_step_<n>.png` | 失败时的现场截图（该步有 `region` 则只截该区域，否则全屏） |
| `diff_step_<n>.png` | 新旧对照图：**左=旧模板**（`assets/` 里那版）\| **右=现场**（最新画面） |

报告详情会给出这些文件的路径，例如：
```
[ 3] FAIL assert_image   断言失败：模板未出现
  失败存档: 现场截图: reports\登录冒烟\fail_step_3.png
  失败存档: 新旧对照(左=模板 login_btn.png / 右=现场): reports\登录冒烟\diff_step_3.png
```
打开 `diff_step_*.png`，一眼就能看出旧按钮长什么样、现在变成了什么，随后用 `template` 命令重新采集即可。

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
<<<<<<< HEAD
自带 26 个单元测试，覆盖：TOML 解析（含 `meta.verify_exact` 默认值与 `[[step]] verify_exact` 步骤级覆盖）、`repeat`/`if`/`else` 配对与跳转填充、未闭合报错、嵌套控制流、报告汇总与 HTML 转义、按键名解析与映射、失败对照图拼接、点击 jitter 随机偏移、金字塔加速匹配与精确匹配一致性/误报拒绝、fast→exact 确认路径与精确匹配一致、确认路径兜底不误报（另有 3 个 ignored：耗时基准×2、真实截图自匹配）。
=======
自带 25 个单元测试，覆盖：TOML 解析（含 `meta.verify_exact` 默认值）、`repeat`/`if`/`else` 配对与跳转填充、未闭合报错、嵌套控制流、报告汇总与 HTML 转义、按键名解析与映射、失败对照图拼接、点击 jitter 随机偏移、金字塔加速匹配与精确匹配一致性/误报拒绝、fast→exact 确认路径与精确匹配一致、确认路径兜底不误报（另有 3 个 ignored：耗时基准×2、真实截图自匹配）。
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

`scenarios/` 下还提供了可执行演示场景：
- `demo.toml` — M2 冒烟（全链路）
- `m3_demo.toml` — M3 控制流（循环 + 分支）

---

## 📁 目录结构

```
auto-game/
├── Cargo.toml
├── README.md                 # 本文件
├── docs/design.md            # 设计文档（架构/选型/游戏适配）
├── src/
│   ├── main.rs               # CLI 入口：run <场景> / template 模板采集
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
已内置加速：默认走图像金字塔 + 并行路径（合成基准实测 debug 约 88 倍加速）。仍建议给图像动作指定 `region` 收窄范围，并用 `--release` 构建运行。

**Q：需要点击精确到某个像素，金字塔加速的 ±1px 够吗？**
<<<<<<< HEAD
给 `[meta]` 加 `verify_exact = true`：每个模板在金字塔定位后再做一次精确确认，返回像素级精确位置与准确置信度；金字塔漏检会自动回退全图精确匹配兜底。实测仅比 fast 多 ~16% 开销（400×300 模板确认后约 166ms，而全图精确约 11.5s）。若只需个别关键步骤精确，可在 `[meta]` 全局开启后，对高频步骤写 `[[step]] verify_exact = false` 单独关掉确认（步骤级优先于全局）。
=======
给 `[meta]` 加 `verify_exact = true`：每个模板在金字塔定位后再做一次精确确认，返回像素级精确位置与准确置信度；金字塔漏检会自动回退全图精确匹配兜底。实测仅比 fast 多 ~16% 开销（400×300 模板确认后约 166ms，而全图精确约 11.5s）。
>>>>>>> a352e497aaa5e88d41c3d0baae4d2d9cd60a1dec

**Q：怎么采集模板图像？还要自己截图裁剪吗？**
不用。用内置 `template` 子命令：`auto-game template --name <模板> --x --y --w --h`（或 `--at-mouse` 以鼠标定位）。它用代码截图生成模板 PNG 到 `assets/`，并打印坐标与可直接粘贴的 TOML 片段。详见「模板采集」一节。

**Q：模板需要很精确吗？**
模板即目标区域的原样截图即可（按钮/图标/静态 UI）。运行匹配用 `precision`（默认 0.85）容忍轻微差异；背景会动的区域请把模板裁小一点（只含静态部分），并配合 `region` 限定。

**Q：游戏更新、UI 变了，模板匹配不上了怎么办？**
这是预期内的正常情况，不会导致脚本乱点。失败步骤会自动保存现场截图与「新旧对照图」（`reports/<场景>/diff_step_*.png`，左=旧模板，右=现场），打开对照即可确认界面差异；再用 `template` 命令重新采集新模板。详见「失败自动存档」。

**Q：点击/按键没生效？**
输入模拟可能被系统/杀软拦截（UIPI）。请以管理员/前台窗口运行，或在杀软中添加白名单。

**Q：游戏窗口被遮挡截不到图？**
将游戏设为窗口模式并置于前台；窗口捕获（xcap `capture_window`）为预留能力，后续版本支持。

**Q：会不会被反作弊封号？**
本框架坚持纯视觉方案（只截图 + 模拟键鼠），不读内存、不注入，风险远低于内存类工具；仍请遵守游戏条款。
