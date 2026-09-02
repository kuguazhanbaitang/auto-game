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
- **OCR 文字识别**：`ocr_text` / `if_text` / `click_text` / `assert_text`——识别游戏界面文字（血量/数值/标题/弹窗文案），与模板匹配互补（模板回答"某图在不在/在哪"，OCR 回答"这里写了什么"）
- **组合键**：`key_combo` 支持 Ctrl+A 等组合键；按键覆盖游戏常用键位（WASD/数字/F1-F12/方向/修饰键）
- **GUI 录制器**：`gui` 子命令打开桌面界面——实时画面预览、录制手动点击/按键自动生成步骤、框选即存模板、步骤可视化编辑、一键导出 TOML 并复用引擎运行（录制自动生成 `click`/`key_press`，坐标点击可一键转 `click_image` 模板）
- **报告**：文本报告 + HTML 报告（状态着色、耗时统计、注入转义）
- **可插拔后端**：截图 / 输入 / 识别 / OCR 全部封装在 trait 之后，可替换底层库（xcap / enigo / rustautogui / paddleocr_rs_onnx）

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
auto-game gui [--assets <资源目录>]   # GUI 录制器：可视化录制/预览/编辑/运行
```

| 命令 | 说明 |
|---|---|
| `run <场景.toml>` | 执行场景文件（必填） |
| `run --assets <目录>` | 模板图像根目录，默认 `assets/` |
| `template` | 模板采集子命令，见下方「模板采集」 |
| `gui` | 打开 GUI 录制器（可视化录制点击/按键、框选模板、编辑步骤、导出并运行），见「GUI 录制器」 |

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

## 🎮 GUI 录制器（egui）

可视化编排场景：手动操作游戏录一遍 → 自动生成步骤 → 预览/编辑 → 导出 TOML → 复用引擎运行。

```
auto-game gui            # 启动（默认 assets/）
auto-game gui --assets D:\my-assets
```

### 基本流程

1. **启动**：运行 `auto-game gui`，窗口左侧为步骤编辑区，中央为画面预览。
2. **画面**：点「刷新」看全屏预览；勾选「窗口模式」并填窗口标题关键字（如 `龙之谷` / `MuMu`），按游戏窗口捕获（同 `[meta] window` 逻辑）。
3. **录制**：点「⏺ 开始录制」，把焦点切到游戏窗口手动操作——左键点击生成 `click` 步骤、按键生成 `key_press` 步骤（实时追加在左侧列表）。录完点「⏹ 停止录制」。
4. **框选模板**：在预览图上拖动框选 UI 元素 → 上方显示真实像素坐标/尺寸 → 「保存模板」存到 `assets/<name>.png`；或「插入 region」生成带区域的等待步骤。
5. **坐标点击转模板**：选中某条 `click` 步骤点「→模板」，自动以该坐标为中心截 64×64 模板并转为 `click_image`（保留 jitter/click_delay 拟人化参数）——比纯坐标点击更抗 UI 微移。
6. **编辑**：每步可改动作（14 种）与参数（x/y/jitter/precision/timeout/image/text/key/region…），支持上移/下移/删除/清空，随时插入 `wait` / `screenshot` 步骤。
7. **导出**：填「场景名」→「💾 导出 TOML」，生成 `scenarios/<场景名>.toml`（与手写场景完全等价，可直接 `auto-game run` 运行）。
8. **运行**：点「▶ 运行场景」后台复用引擎执行，结果回状态栏；运行中引擎接管鼠标键盘，**F9 可中止**。

### 注意事项

- 录制期间请把焦点放在**游戏窗口**上操作；在 GUI 窗口里按字母/数字键会被当作录制输入（白名单外的 F13+、小键盘、标点不录）。
- 预览/录制与引擎运行约束相同：需要真实桌面会话与前台窗口。
- 若场景用到 OCR 动作，运行前需 `$env:ORT_DYLIB_PATH = (Resolve-Path "libs\onnxruntime.dll").Path`（与 CLI 一致）。

---

## 📝 场景配置语法

场景是一个 TOML 文件：`[meta]` 声明元信息，`[[step]]` 数组按顺序定义动作步骤。

```toml
[meta]
name = "登录冒烟"        # 场景名（用于报告目录与展示）
window = "MyGame"       # 可选：窗口模式——按标题关键字捕获该窗口（M4 已实现，见「窗口模式」）
verify_exact = true     # 可选：fast→exact 精确确认全局默认（false；可被步骤级覆盖）

[[step]]
action = "wait_image"   # 等待模板出现
image = "login_btn.png"
precision = 0.9
timeout = 15

[[step]]
action = "click_image"  # 找到模板并点击其中心
image = "login_btn.png"
precision = 0.9
verify_exact = false    # 步骤级覆盖：此步关闭精确确认（其余步仍走全局 true）

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
| `click` | `x`, `y`, `jitter`, `click_delay` | 移动到坐标并左键点击；`jitter` 可在 ±N 像素内随机偏移，`click_delay` 点击前随机等待 |
| `key_press` | `key` | 按下单个按键 |
| `key_combo` | `keys` | 组合键，如 `keys = ["ctrl", "a"]` |
| `type_text` | `text` | 输入一段文本 |

### 图像识别动作

| action | 参数 | 说明 |
|---|---|---|
| `find_image` | `image`, `precision`, `region` | 查找模板，输出位置与置信度（未找到不算失败） |
| `wait_image` | `image`, `precision`, `timeout`, `region` | 轮询等待模板出现（200ms 间隔） |
| `click_image` | `image`, `precision`, `region`, `jitter`, `click_delay` | 找到模板并点击其中心；未找到则失败 |
| `assert_image` | `image`, `precision`, `timeout`, `region` | 断言模板在超时内出现；否则失败 |
| `if_image` | `image`, `precision`, `region` | 条件判断：命中走 then 分支，未命中走 else/跳过 |

### OCR 文字识别动作（与模板匹配互补）

| action | 参数 | 说明 |
|---|---|---|
| `ocr_text` | `region` | 识别区域内文字并输出到报告（回答"这片区域写了什么"） |
| `if_text` | `text`, `region` | 条件判断：识别到包含 `text` 的文字走 then 分支，否则走 else/跳过 |
| `click_text` | `text`, `region`, `jitter`, `click_delay` | 识别到包含 `text` 的文字行后点击其中心；未识别到则失败 |
| `assert_text` | `text`, `region`, `timeout` | 断言在超时内识别到包含 `text` 的文字；否则失败 |

**OCR 示例**（识别血量 + 按文字点击 + 文案断言）：
```toml
[[step]]
action = "ocr_text"
region = { x = 20, y = 20, w = 300, h = 80 }   # 读取左上角数值区

[[step]]
action = "click_text"                            # 点击「开始战斗」按钮
text = "开始战斗"
region = { x = 800, y = 600, w = 400, h = 300 }

[[step]]
action = "assert_text"                           # 等待结算文案出现
text = "胜利"
timeout = 30
```

> `text` 为**包含子串**匹配（大小写敏感），命中多行时取置信度最高者；`click_text` 的 `jitter` 偏移限制在文字行范围内。

### 通用参数

| 参数 | 适用动作 | 说明 |
|---|---|---|
| `precision` | 图像类 | 匹配置信度阈值 0.0~1.0，默认 `0.85` |
| `timeout` | 等待/断言类 | 超时秒数，默认 `15` |
| `region` | 图像类 | 限定搜索区域 `{ x, y, w, h }`，**强烈建议指定**以提升性能 |
| `jitter` | `click` / `click_image` | 点击随机抖动像素，每次点击在目标 ±N 内动态分布，默认 `0`（精确点击） |
| `click_delay` | `click` / `click_image` | 点击前随机延时（秒）：每次点击前随机等待 `[0, click_delay]`，拟人化；缺省/`≤0` 不延时 |
| `click_delay_min` | `click` / `click_image` | 点击随机延时下限（秒），配合 `click_delay` 组成 `[min, max]` 区间（可选） |
| `text` | `type_text` / OCR 文字类 | 输入文本；OCR 类动作用作「期望包含子串」匹配 |
| `verify_exact` | 图像类 | 按步骤覆盖全局开关：显式 `true` / `false` 覆盖 `[meta] verify_exact`，缺省回退全局值 |

**全局开关（`[meta]`）**：

| 参数 | 说明 |
|---|---|
| `verify_exact` | 默认 `false`。设为 `true` 时，所有模板匹配走「fast 粗定位 → exact 精确确认」，返回像素级精确位置与准确置信度；金字塔漏检自动回退全图精确匹配兜底。某步骤可写 `[[step]] verify_exact = false` 单独关闭（见「通用参数」） |

### 控制流

| action | 参数 | 说明 |
|---|---|---|
| `repeat` | `count` | 开始循环（`count` 次），与 `end_repeat` 配对 |
| `end_repeat` | — | 结束循环 |
| `if_image` | `image` 等 | 开始条件分支（模板匹配），与 `end_if` 配对 |
| `if_text` | `text` 等 | 开始条件分支（OCR 文字匹配），与 `end_if` 配对 |
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

## 🖱️ 拟人化点击（jitter + 随机延时）

默认点击总是落在模板中心/指定坐标（同一像素点）、时机固定。拟人化有两层——**位置**用 `jitter` 随机，**时机**用 `click_delay` 随机：

```toml
# 坐标点击：以 (500, 400) 为中心，每次在 ±10px 内随机偏移，
#             且点击前随机等待 0.1~0.3s（位置 + 时机都不固定）
[[step]]
action = "click"
x = 500
y = 400
jitter = 10
click_delay = 0.3
click_delay_min = 0.1

# 模板点击：以模板中心为基座，偏移被限制在模板范围内（不会点出元素）
[[step]]
action = "click_image"
image = "attack_btn.png"
region = { x = 800, y = 600, w = 120, h = 60 }
jitter = 8
click_delay = 0.2
```

要点：
- `jitter = 0`（默认）→ 精确点击，行为与之前完全一致
- `click` 的偏移：基座坐标 ± jitter；`click_image` 的偏移以模板中心为基座，且**自动限制在模板范围内**，保证不会点偏到目标外
- `click_delay = 0.3` → 每次点击前随机等待 `[0, 0.3]s`；再配 `click_delay_min = 0.1` → 随机等待 `[0.1, 0.3]s`（真实玩家连点从不精确等长）
- `click_delay` 缺省 / `≤ 0`，或 `min ≥ max` → 不延时（向后兼容）
- 报告会显示实际点击坐标与基座、偏移量及延时，方便追溯：
  ```
  点击坐标 (504, 397)（基座 (500, 400) + jitter (4, -3)），点击前随机延时 0.187s
  ```

> 两者都是「零依赖 xorshift64\*」随机源（启动按时间播种），不引入额外依赖。

---

## 🪟 窗口模式（M4）

默认捕获**全屏**；若目标游戏运行在独立窗口（龙之谷窗口模式、阴阳师模拟器），可在 `[meta]` 指定窗口标题关键字，进入窗口模式——只捕获该窗口内容，规避桌面遮挡、支持多开：

```toml
[meta]
name = "阴阳师-刷本"
window = "MuMu模拟器"   # 按标题关键字匹配窗口（contains，不区分大小写）
```

行为约定：

- **窗口匹配规则**：枚举当前所有窗口（xcap `Window::all()`，按 z 序返回），忽略最小化窗口，按标题关键字 `contains` 匹配；多开命中多个实例时取 z 序最前者（最顶层）并打印告警——每个实例用各自独立的场景文件 + 各自标题关键字即可精确分流。
- **坐标语义**：窗口模式下，`[[step]] region` 与匹配到的模板坐标均为**窗口内坐标**（相对窗口左上角）；点击/移动等输入统一由引擎映射回**屏幕坐标**执行，无需手工换算。
- **截图动作**：`screenshot` / `snapshot` 与失败自动存档只保存窗口内容（不含桌面其他区域），对照图更干净。
- **找不到窗口**：场景启动时报错并列出当前可捕获的窗口标题，便于修正关键字。

> 需要全屏时留空或不写 `window` 即可，行为与旧版一致。

---

## 🔤 OCR 文字识别（PP-OCRv4 中文）

模板匹配只能回答「某图像在不在/在哪」，**读不出动态文字**（血量、数值、标题、弹窗文案）。OCR 补齐这块：识别区域内的文字并输出坐标/置信度，可做条件分支、按文字点击、文字断言。

**模型与运行时已随仓库提供**（克隆即用，无需手工下载）：
- `assets/ocr/` — PP-OCRv4 中文模型（检测 `ch_PP-OCRv4_det_mobile.onnx` + 识别 `ch_PP-OCRv4_rec_mobile.onnx` + 字符集 `ppocr_keys_v1.txt`，来自 ModelScope RapidAI/RapidOCR）
- `libs/` — ONNX Runtime 动态库（`onnxruntime.dll` + `onnxruntime_providers_shared.dll`）

**运行前设置动态库路径**（PowerShell）：
```powershell
$env:ORT_DYLIB_PATH = (Resolve-Path "libs\onnxruntime.dll").Path
```

模型在**首次用到 OCR 动作时懒加载**（纯图像场景零开销）；识别区域尽量用 `region` 收窄以提速。

**动作**：`ocr_text`（识别输出）/ `if_text`（文字条件分支）/ `click_text`（按文字点击）/ `assert_text`（等待文字出现），详见「动作参考」。

**典型场景**（阴阳师刷本，识别 + 按文字操作 + 文案断言）：
```toml
[meta]
name = "阴阳师-刷本"
window = "MuMu模拟器"

[[step]]
action = "wait_image"          # 模板等待：进入副本按钮
image = "yyj_dungeon_btn.png"
region = { x = 300, y = 700, w = 400, h = 200 }

[[step]]
action = "click_image"
image = "yyj_dungeon_btn.png"

[[step]]
action = "click_text"          # 文字点击：「开始战斗」（无需为文字做模板）
text = "开始战斗"
region = { x = 500, y = 500, w = 500, h = 300 }

[[step]]
action = "repeat"              # 重复刷本，直到胜利/失败文案出现
count = 50

[[step]]
action = "if_text"             # 检测到「胜利」→ 结算处理
text = "胜利"

[[step]]
action = "click_image"
image = "yyj_confirm_btn.png"

[[step]]
action = "end_if"

[[step]]
action = "end_repeat"
```

---

## 🖼️ 区域匹配与性能（性能要点）

**背景**：默认的模板匹配是"全屏截取 → 算法扫描"，M1 实测 imageproc 朴素 NCC 在 debug 模式下 500×400 区域约 78 秒，全屏更久。

**已内置加速**：`find_template` 默认走**图像金字塔 + 并行**路径——先在低分辨率全图粗定位 top-K 候选，再逐层在候选邻域并行精匹配，把「全屏大匹配」变为「若干小窗口匹配」，语义不变（仍返回全局最高置信度位置）。合成基准实测（debug，400×300 + 50×40 模板）**约 14s → 0.16s（≈88 倍）**。小图/小模板自动回退精确匹配。

**需要确切像素？开 `verify_exact`**：金字塔加速返回的位置精确到 ±1px、置信度为近似值，对绝大多数点击足够。若需求是"点击必须落在确切像素"（如精确像素断言、对极小 UI 区域点击），在 `[meta]` 设置 `verify_exact = true`，每个模板走「fast 粗定位 → 最终位置精确确认」：在模板 + 2×radius 邻域内做一次精确 NCC，抹掉降采样误差、给出准确置信度；金字塔漏检时自动回退全图精确匹配兜底（保证不漏检）。合成基准实测（debug，400×300 + 50×40 模板）：fast=142ms vs 确认后=166ms vs 全图精确=11.5s——确认几乎不增加开销（≈69 倍 vs 全图精确）。

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
| **龙之谷**（PC 端游） | 固定**窗口模式** | 模板只针对静态 UI（按钮/图标/菜单）；WASD 移动用 `key_press`，UI 点击用 `click_image`；血量/等级/伤害等动态文本用 `ocr_text`；等待步骤设足 timeout |
| **阴阳师**（手游·安卓模拟器） | 模拟器窗口 | 捕获目标是模拟器窗口（多开须指定标题）；点击类操作为主 → 文字按钮可直接 `click_text`；随机弹窗用 `if_image` 关闭、"胜利/失败"结算用 `assert_text`/`if_text` 判断 |

模板资源按 `assets/<game>/` 分目录，命名 `界面_元素.png`（如 `dn_login_btn.png` / `yyj_garden.png`）。

---

## 🧪 测试

```bash
cargo test          # 运行单元测试（脚本解析 / 控制流编译 / 报告 / 按键映射）
```
自带 39 个单元测试，覆盖：TOML 解析（含 `meta.verify_exact` 默认值与 `[[step]] verify_exact` 步骤级覆盖、`click_delay` 区间）、`repeat`/`if`/`else` 配对与跳转填充、`if_text` 与 `if_image` 共用条件分支编译、未闭合报错、嵌套控制流、报告汇总与 HTML 转义、按键名解析与映射、失败对照图拼接、点击 jitter 随机偏移、随机延时区间、金字塔加速匹配与精确匹配一致性/误报拒绝、fast→exact 确认路径与精确匹配一致、确认路径兜底不误报、OCR 模型加载与识别流程 smoke（另有 3 个 ignored：耗时基准×2、真实截图自匹配）。

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
│   ├── main.rs               # CLI 入口：run <场景> / template 模板采集 / gui 录制器
│   ├── lib.rs                # 库入口（供二次开发）
│   ├── gui.rs                # GUI 录制器（egui/eframe 桌面界面）
│   ├── adapter/              # 可插拔后端：capture / input / vision / ocr + Region
│   ├── action.rs             # 动作原语（截图/移动/点击/按键/找图）
│   ├── engine.rs             # 流程引擎（编译指令 + 循环/分支 + failsafe + OCR 动作）
│   ├── script.rs             # TOML 场景解析
│   └── report.rs             # 文本 + HTML 报告
├── scenarios/                # 场景配置（演示/测试用例）
├── assets/                   # 模板图像（按游戏分目录）+ ocr/（PP-OCRv4 模型）
├── libs/                     # 运行时动态库（onnxruntime.dll，OCR 需要）
└── vendor/                   # 本地 vendor 库（paddleocr_rs_onnx，含本地补丁）
```

---

## ❓ FAQ

**Q：模板匹配很慢怎么办？**
已内置加速：默认走图像金字塔 + 并行路径（合成基准实测 debug 约 88 倍加速）。仍建议给图像动作指定 `region` 收窄范围，并用 `--release` 构建运行。

**Q：需要点击精确到某个像素，金字塔加速的 ±1px 够吗？**
给 `[meta]` 加 `verify_exact = true`：每个模板在金字塔定位后再做一次精确确认，返回像素级精确位置与准确置信度；金字塔漏检会自动回退全图精确匹配兜底。实测仅比 fast 多 ~16% 开销（400×300 模板确认后约 166ms，而全图精确约 11.5s）。若只需个别关键步骤精确，可在 `[meta]` 全局开启后，对高频步骤写 `[[step]] verify_exact = false` 单独关掉确认（步骤级优先于全局）。

**Q：怎么采集模板图像？还要自己截图裁剪吗？**
不用。用内置 `template` 子命令：`auto-game template --name <模板> --x --y --w --h`（或 `--at-mouse` 以鼠标定位）。它用代码截图生成模板 PNG 到 `assets/`，并打印坐标与可直接粘贴的 TOML 片段。详见「模板采集」一节。

**Q：模板需要很精确吗？**
模板即目标区域的原样截图即可（按钮/图标/静态 UI）。运行匹配用 `precision`（默认 0.85）容忍轻微差异；背景会动的区域请把模板裁小一点（只含静态部分），并配合 `region` 限定。

**Q：血量、数值、标题这些动态文字读不了怎么办？**
模板匹配读不了文字——用 OCR：`ocr_text` 识别区域文字并输出（带坐标与置信度），`if_text` 做文字条件分支，`click_text` 按文字点击，`assert_text` 等待文字出现。模型与运行时（`assets/ocr/` + `libs/`）已随仓库提供，运行前设 `$env:ORT_DYLIB_PATH = (Resolve-Path "libs\onnxruntime.dll").Path` 即可。

**Q：OCR 会不会很慢？**
模型**懒加载**（首次用到才加载，纯图像场景零开销）；识别用 `region` 收窄区域 + `--release` 构建可明显提速。PP-OCRv4 是轻量移动模型，单帧识别毫秒~百毫秒级，满足"逐步骤识别"而非"逐帧实时"的需求。

**Q：游戏更新、UI 变了，模板匹配不上了怎么办？**
这是预期内的正常情况，不会导致脚本乱点。失败步骤会自动保存现场截图与「新旧对照图」（`reports/<场景>/diff_step_*.png`，左=旧模板，右=现场），打开对照即可确认界面差异；再用 `template` 命令重新采集新模板。详见「失败自动存档」。

**Q：点击/按键没生效？**
输入模拟可能被系统/杀软拦截（UIPI）。请以管理员/前台窗口运行，或在杀软中添加白名单。

**Q：游戏窗口被遮挡截不到图？**
将游戏设为窗口模式并置于前台；窗口捕获（xcap `capture_window`）为预留能力，后续版本支持。

**Q：会不会被反作弊封号？**
本框架坚持纯视觉方案（只截图 + 模拟键鼠），不读内存、不注入，风险远低于内存类工具；仍请遵守游戏条款。
