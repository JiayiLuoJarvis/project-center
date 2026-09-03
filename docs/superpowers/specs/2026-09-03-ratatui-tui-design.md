# ratatui 全屏 TUI 替换 dialoguer — 设计规格

日期：2026-09-03  
状态：已批准（修订：补 Windows 按键、ListPicker、启动后去留、IME、回收站/配置/Filter IA）  
关联：交互层现代化；取代 `docs/spec.md` 中「不提供全屏 TUI」的非目标

## 1. 背景与目标

`pcs` 交互菜单目前基于 `dialoguer`（FuzzySelect / Select / Input / Confirm）：短时行内提示，无备用屏幕，层级为「分组 → 项目 → 操作 → 启动」。功能完整，但视觉与信息密度普通，且与「项目管理器」心智不完全匹配。

**目标：**

1. 用 **ratatui 0.30 + crossterm（ratatui 默认 backend）** 全量替换所有交互 UI。
2. 视觉采用 **方向 1「Lazygit 工作台」**：左右分栏、焦点强对比、Tokyo Night 系 **aurora** 主题。
3. 去掉 `dialoguer` 依赖；脚本化 / 非交互 CLI 子命令行为不变。
4. 业务契约不变：`ops` / `models` / `store` / `launcher` / `config` 的数据与启动语义保持。

**非目标（本期）：**

- 可配置多主题 / 外部皮肤文件
- 鼠标支持
- 三栏预览（Yazi 方向）或 `:` 命令面板（k9s 方向）
- Linux/macOS；仍仅 Windows

## 2. 依赖

| 包 | 版本 | 说明 |
|---|---|---|
| `ratatui` | `0.30`（实现时锁定 crates.io 0.30.x，当前参考 0.30.2） | 默认启用 crossterm 0.29；可用 `ratatui::run` / `init` / `restore` |
| `crossterm` | 经 ratatui 再导出即可 | 事件用 `ratatui::crossterm::event`；一般不必单独依赖 |
| `dialoguer` | **删除** | 交互全部由 TUI 承担；`main` 中 `pcs trash empty` 的 Confirm 一并去掉 |
| `rfd` | 保留 | 选择 Windows 文件夹；调用前后处理好 raw mode / 备用屏幕 |

`Cargo.toml` 变更要点：

```toml
ratatui = "0.30"
# 移除 dialoguer = { ... }
```

## 3. 架构与模块

### 3.1 目录

```
src/
├── main.rs           # clap 分发；mod tui；清除 dialoguer
├── menu.rs           # 瘦身：LaunchOption / build_launch_options / default_first 等纯逻辑
├── tui/
│   ├── mod.rs        # run(data, config)；终端生命周期；事件循环；Launch 后处理
│   ├── app.rs        # App 状态、Mode、Focus、按键分发（无终端依赖，可单测）
│   ├── ui.rs         # 帧布局：顶栏 / 双栏 / 底栏 / 模态
│   ├── theme.rs      # aurora 色板与 Style 助手
│   ├── widgets.rs    # 列表行、模态卡片、底栏 input/confirm、帮助浮层
│   └── actions.rs    # 调用 ops/config/store；返回 ActionOutcome（含请求启动）
├── ops.rs / models.rs / store.rs / launcher.rs / config.rs  # 契约不变
```

可选：若 `menu.rs` 仅剩启动选项纯函数，可再抽 `launch_options.rs`；不强制，避免无意义搬文件。

### 3.2 终端生命周期

```
ratatui::init() 或等价 enter raw + alternate screen（含 panic hook）
loop {
  terminal.draw(|f| ui::render(f, &app))
  match next_key_event() {  // 见 §3.5：仅 KeyEventKind::Press
    None => continue,
    Some(key) => match app.handle(key) {
      Continue => {}
      Quit => break,
      // 控制台启动：先 restore，spawn+wait，再 init 回 TUI
      LaunchConsole(project, group, option) => {
        ratatui::restore();
        spawn_and_wait(...);
        ratatui::init();  // 回到 Browse，不退出进程
      }
      // IDE / explorer：restore 非必须但可保持一致；spawn 后立刻回 Browse
      LaunchDetached(project, group, option) => { spawn_no_wait(...); }
      // rfd：见下
      PickFolder { .. } => { restore → rfd → init → 把路径灌回 InlineInput }
    }
  }
}
ratatui::restore()  // 仅 q / Ctrl+C / 致命错误时离开进程
```

约束：

- **`App` 与 terminal 解耦**：`app.handle(KeyEvent) -> AppOutcome`；`tui::mod` 持有 `Terminal`，按 outcome 做 restore/init/spawn。rfd 往返只动终端，不重建 `App` 状态。
- 任何**进程退出**路径（含 Ctrl+C、错误返回、panic hook）必须 `restore`，避免终端残留 raw mode。
- 调用 `rfd` 选文件夹前：`restore`；结束后再 `init` 回到 TUI（若实测 rfd 不干扰 console 可简化，以 WT 手测为准）。
- WSL/PowerShell 启动子进程前必须已离开备用屏幕，否则子进程与 TUI 争用控制台。
- 会话内始终使用**内存中的** `data: &mut ProjectData` 与 `config: &mut AppConfig`；禁止像旧 `set_default_tool` 那样在菜单路径上再 `Config::load()` 一份旁路副本。

### 3.3 与 main 的衔接

| 入口 | 行为 |
|---|---|
| `pcs` / `pcs menu` | `tui::run(&mut data, &mut config)` |
| `pcs open <名>` 无 `-w/-p/-c` | TUI 短会话：仅 LaunchPicker（或主界面预选项目后打开 LaunchPicker）。实现任选，优先代码简单。规格要求：选项与 `build_launch_options` + `default_first` 一致；确认后按 §5.1 启动语义执行；取消则进程退出且 exit code 0 |
| `pcs open` 带标志 / `wsl`/`ps`/`code` 等 | 不变，无 TUI |
| 其它 CRUD CLI | 不变 |
| `pcs trash empty` | **删除** `dialoguer::Confirm`：无 `--force` 时打印提示并拒绝执行（或要求 `--force`）；有 `--force` 才清空。禁止残留 dialoguer |

### 3.4 actions 边界

- `actions.rs` 只做：查改 `ProjectData`/`AppConfig`、`Store::save` / config save、组装 `LaunchOption`。
- 不在 actions 里 `spawn`；由 `tui::mod` 在合适的终端状态下统一启动。
- UI 不直接解析 JSON、不复制 `ops` 校验逻辑。
- 校验失败：底栏 flash 错误文案（对应旧 `eprintln!`），不崩、不静默。

### 3.5 Windows 按键硬约束（Critical）

crossterm 在 Windows 上对同一物理键会送达 **Press + Release**（有时还有 Repeat）。

**硬性规则：**

1. 事件循环只处理 `Event::Key`，且 **`key.kind == KeyEventKind::Press`**（`Repeat` 是否当作移动由实现定，默认忽略 Repeat，避免长按连跳失控；若手感需要可对 `j/k` 等导航键单独接受 Repeat）。
2. `KeyEventKind::Release` **一律忽略**。
3. 修饰键：`Ctrl+C` 视为退出（与 `q` 相同 outcome），在 raw mode 下自行处理，不依赖进程默认 SIGINT 行为。
4. 本条写入实现检查清单；漏做会导致 `j`/`q`/`Enter` 双触发，验收不合格。

## 4. 视觉设计（方向 1 · Lazygit 工作台）

### 4.1 布局

```
╭─ pcs · Project Center ─────────────── <分组> · N 项目 ─╮
│ GROUPS             │ PROJECTS · <分组>                  │
│┌─────────────────┐ │┌──────────────────────────────────┐│
││ ● dev        12 │ ││ ▶ name  ★default   摘要启动方式   ││
││   tools       4 │ ││   name             次要路径行      ││
││   ───────────── │ ││                                  ││
││  回收站       2 │ │└──────────────────────────────────┘│
││  配置           │ │                                    │
│└─────────────────┘ │                                    │
├────────────────────┴────────────────────────────────────┤
│ enter 打开  o 操作  a 新增  d 删除  / 过滤  ? 帮助  q 退出 │
╰─────────────────────────────────────────────────────────╯
```

- **顶栏**：应用名 + 当前上下文（分组名、项目数）。
- **左栏**：分组列表；底部分区固定「回收站」「配置」（类似 lazygit 固定区）。
- **右栏**：当前上下文列表（见 §5.8 / §5.9）；项目为 **双行**：上行名称 + 默认/启动摘要；下行 muted 路径（Windows 或 linux_path）。
- **底栏**：上下文相关快捷键（muted）；Filter / InlineInput / Confirm 时切换为 prompt。
- **模态**（LaunchPicker、ActionMenu、ListPicker、Help）：居中卡片；非焦点区可保持暗色边框。

### 4.2 主题 aurora（Tokyo Night 系 truecolor）

| 角色 | 色值 | 用途 |
|---|---|---|
| 背景 | `#1a1b26` | 主背景 |
| 面板底 | `#16161e` | 列表内底 |
| 焦点边框 | `#7aa2f7` | 当前焦点栏 Block 边框 |
| 非焦点边框 | `#3b4261` | 另一栏 |
| 选中行 bg | `#283457` | List 高亮 |
| 选中行 fg | `#c0caf5` | |
| 标题/分组强调 | `#bb9af7` | 栏目标题 |
| 默认工具 / 成功 | `#9ece6a` | `[默认]`、★ |
| 仅 WSL / 警告 | `#e0af68` | 无 Windows 路径标记 |
| 次要文字 | `#565f89` | 路径、底栏说明 |
| 危险 | `#f7768e` | 删除确认 |
| 快捷键强调 | `#7dcfff` | 底栏按键字符 |

实现：`theme.rs` 集中 `Color::Rgb(r,g,b)` 与常用 `Style`。目标平台为 Windows Terminal / 新 conhost，**直接使用 RGB**；不必做 truecolor 探测。若极端环境无 truecolor，允许观感异常，不为此加命名色 fallback 分支（简化实现）。

### 4.3 其它视觉规则

- 焦点栏：亮边框 + 标题用强调色；非焦点栏变暗。
- 默认启动方式：名称旁绿 `★` 或摘要中带工具名。
- 自定义命令在 LaunchPicker 中保持现有 `⚙ 名称 (env)` 风格标签（颜色可用强调色）。
- 宽字符：截断路径时按**显示宽度**（unicode width），尾部 `…`。
- 少用 emoji；需要符号时优先 `★` `⚙` 等兼容 WT 的字符。
- **双行项目行**：使用单个 `ListItem` 内多行 `Text`/`Line`（上行名称、下行路径），**不要**把一个项目拆成两个可选中项。选中高亮覆盖两行。

## 5. 状态机与按键

### 5.1 模式

```
Browse
  ├─ Enter(项目)           → LaunchPicker → Enter → Launch*
  ├─ o                     → ActionMenu
  ├─ /                     → Filter（当前栏，见 §5.10）
  ├─ a / e / 表单类文本    → InlineInput
  ├─ 需要单选列表的步骤    → ListPicker（见 §5.8）
  ├─ d 等破坏操作          → Confirm
  ├─ ?                     → Help
  ├─ q / Ctrl+C            → Quit
  └─ Esc                   → 清过滤 / 关 Help；不作退出

左栏选中「回收站」「配置」时仍为 Browse，右栏数据源与标题切换（§5.9 / §5.11）。
```

**启动后去留（修订，相对初版）：**

| 启动类型 | TUI 行为 | 进程 |
|---|---|---|
| WSL / PowerShell（有 child，需 wait） | `restore` → `spawn_direct` → `wait_direct` → **`init` 回到 Browse** | **不退出** |
| IDE / explorer（detached，无 wait） | `spawn_direct`（可先 restore 再 init，或保持 raw 若实测无干扰）→ **留在 Browse** | **不退出** |
| `pcs open` 无标志短会话确认启动 | 与上表相同类型语义；短会话在 wait/spawn 完成后 **进程退出**（短会话本就无 Browse 可回） | 短会话退出 |
| `q` / `Ctrl+C` | `restore` | 退出 |

说明：

- 相对旧 `menu.rs`（启动后 `continue`）对 **IDE/explorer** 行为一致（留在菜单）。
- 对 **WSL/PS**：旧菜单 wait 完也 `continue`；本规格同样 wait 完回 TUI，而不是「进程退出」。初版「Launch 后进程退出」**撤销**，避免连开多个项目时反复启动 `pcs`。
- 若未来需要「启动后退出 pcs、把控制台留给子 shell」的模式，单开后续需求；本期以可继续浏览为准。

### 5.2 焦点

- `Focus::Groups | Focus::Projects`（Browse 下；右栏在回收站/配置上下文中仍用 `Focus::Projects` 语义表示「右栏」）。
- 模态打开时按键只进模态；`Esc` 关闭模态并恢复原焦点。

### 5.3 Browse 按键

| 键 | 行为 |
|---|---|
| `j` / `k` / `↑` / `↓` | 当前栏移动 |
| `Tab` / `h` / `l` / `←` / `→` | 切换左右栏 |
| `Enter` | 左栏分组：聚焦右栏；左栏回收站/配置：聚焦右栏；右栏项目：打开 LaunchPicker；右栏回收站项 / 配置项：打开对应 ActionMenu 或进入下钻（§5.9 / §5.11） |
| `o` | 当前右栏选中项的 ActionMenu（项目 / 回收站项 / 配置工具） |
| `a` | 上下文新增（左栏在分组区：新分组；右栏在项目：新项目；配置工具列表：新工具；自定义命令列表：新命令） |
| `e` | 编辑选中 |
| `d` | 删除（Confirm） |
| `m` | 移动项目（ListPicker 选目标分组） |
| `/` | 过滤当前栏（§5.10） |
| `g` / `G` | 到顶 / 到底 |
| `?` | 帮助 |
| `q` / `Ctrl+C` | 退出 |
| `Esc` | 若有过滤：清除过滤；若 Help 开：关闭；**不**作为退出键 |

### 5.4 LaunchPicker

- 数据：`build_launch_options` + `default_first`（公开纯函数，见 §7）置顶与 `[默认]` 前缀。
- `j/k` 移动，`Enter` 确认 → 按 §5.1 启动语义，`Esc` 取消。

### 5.5 InlineInput

- 底栏单行：`提示` + 输入缓冲；多字段串行（名称 → …）。
- `Enter` 提交当前字段或完成表单，`Esc` 取消整次编辑。
- 编辑时字段 default 预填当前值（对齐旧 menu）。
- Windows 路径选取：走 `rfd`（见 §3.2）；也可在路径类型 ListPicker 中选「Windows 文件夹」后触发 rfd。
- **中文 IME**：见 §10；实现阶段在 WT 手测组字；若失败，fallback 策略写进实现说明并记入风险关闭条件。

### 5.6 Confirm

- 底栏或小卡片：`确认…？ y/N`，仅 `y`/`Y` 执行；其它键（含 `n`/`N`/`Esc`）取消。
- 危险操作文案用危险色。

### 5.7 ActionMenu

项目操作与现菜单对齐，至少包括：

- 打开（进 LaunchPicker）
- 默认启动方式（→ ListPicker，含「不设默认」）
- 自定义命令（→ 命令列表 Browse 子态或二级列表 + a/e/d）
- 编辑、删除、移动、返回

配置/回收站操作见 §5.9 / §5.11。交互为列表 + Confirm + InlineInput + ListPicker，**不削减**现 `menu.rs` 功能。

### 5.8 ListPicker（Critical 补全）

通用单选列表模态，与 ActionMenu 可共用渲染，但语义独立：有明确「选项集合 + 选中回调」，不是固定操作表。

**必须覆盖的现有 Select 场景：**

| 场景 | 选项来源 |
|---|---|
| 新增项目：路径类型 | `Windows 路径（选择文件夹）` / `仅 WSL 路径（手动输入）` |
| 自定义命令：运行环境 | WSL / PowerShell / IDE |
| 移动项目：目标分组 | 其它分组名 |
| 默认启动方式 | `build_launch_options` 标签 + `不设默认` |
| 配置：选择环境（若采用先选环境再进工具列表） | WSL / PowerShell / IDE 工具 |

按键：`j/k` 移动，`Enter` 确认，`Esc` 取消。  
实现上可用同一 `Mode::ListPicker { title, items, selected, on_confirm }`（或等价枚举），避免为每个场景再长一种 Mode。

### 5.9 回收站 IA

左栏选中「回收站」→ 右栏列出 `data.trash`（标签对齐现 `trash_label`：`[分组]` / `[项目]` + 删除日期等）。

| 键 | 行为 |
|---|---|
| `Enter` / `o` | 单项 ActionMenu：恢复 / 彻底删除 / 返回 |
| `r` | 快捷：恢复选中项（失败 flash） |
| `d` | 彻底删除（Confirm，不可恢复文案） |
| `D` 或底栏「清空」入口 | 清空回收站（Confirm） |
| `a` / `e` / `m` | 无效（可 no-op 或 flash「回收站不支持」） |

空回收站：右栏显示占位「回收站为空」。

### 5.10 Filter IA

- `/` 进入过滤：底栏显示 `/` + 缓冲；**过滤字符串只匹配当前焦点栏**。
- **边滤边导航（Lazygit 风格）**：输入字符追加到过滤串并立即收窄列表；`j`/`k`/`↑`/`↓` 仍在**过滤后的列表**上移动；退格删过滤字符。
- `Enter`：结束过滤编辑（保留当前过滤串，回到 Browse，列表仍为过滤视图）。
- `Esc`：若正在输入过滤 → 清空过滤串并退出 Filter 模式；若已在 Browse 且仍有过滤 → 再按 `Esc` 清空过滤（与 §5.3 一致）。
- 过滤匹配：建议名称子串、大小写不敏感；项目可同时匹配 path / linux_path（实现可简化为仅名称，但需在代码注释标明）。

### 5.11 配置 IA

左栏选中「配置」→ 右栏先显示环境级条目：`WSL 工具` / `PowerShell 工具` / `IDE 工具` / `恢复默认配置`。

| 键 | 行为 |
|---|---|
| `Enter` 在环境行 | 右栏下钻为该环境工具列表（名称 + command），或打开 ListPicker/子列表 |
| `Enter` / `o` 在工具行 | ActionMenu：编辑 / 删除 / 返回 |
| `a` 在工具列表 | 新增工具（InlineInput：名称 → 命令） |
| `e` | 编辑工具 |
| `d` | 删除工具（Confirm） |
| `Enter` 在「恢复默认配置」 | Confirm → `reset_config` + save |

下钻后 `Esc` 或底栏「返回」回到环境列表。  
全程使用会话内 `&mut AppConfig`，变更立即 `Config::save`。

### 5.12 分组管理

- 左栏焦点在某分组：`e` 重命名（InlineInput）；`d` 删除分组（Confirm，文案含项目数，进回收站，对齐 `remove_group(..., false)`）。
- `a` 在左栏分组区：新增分组。
- 不强制单独「管理本分组」菜单；快捷键直达即可。若实现想保留 ActionMenu 亦可。

## 6. 数据与启动语义（不变）

- 数据文件、UUID、`wslPath`、原子保存、大小写不敏感查找、重名需 `--group`：均不变。
- `Project::linux_path` / `has_windows_path`：不变；无 Windows 路径时 LaunchPicker 不出现 PS/IDE/文件夹；自定义命令仍按环境门禁过滤。
- `launcher`：WSL/PS 当前控制台阻塞 wait + Ctrl+C 抑制；IDE/Cursor/explorer 静默；标题「项目名 - 分组名」。`spawn_direct` / `wait_direct` API 不改。
- 菜单内变更仍 **立即 save**。

## 7. 从 menu.rs 的迁移

| 保留 | 处理 |
|---|---|
| `LaunchOption`、`build_launch_options`、`default_option_index`、`default_first`、标签 | **公开**为纯函数（`default_first` 现为私有，需 `pub`），供 TUI 与 CLI 共用；单测随纯函数保留在 `menu.rs`（或迁出后的模块） |
| `run` 及所有 dialoguer 循环 | 删除，由 `tui::run` 替代 |
| `choose_launch_option` | 改为 TUI LaunchPicker；`pcs open` 无标志走 TUI 短会话 |
| println 状态提示 | TUI 内用底栏 flash；CLI 路径仍可 println |

## 8. 实现顺序

1. 加 `ratatui`，建 `tui` 空壳：静态分栏 + `q`/`Ctrl+C` 退出；**验证 Windows 仅 Press 触发一次**；验证 WT 备用屏幕与 restore。
2. Browse 绑定真实 `ProjectData`：左右列表、j/k/Tab；`App::handle` 可单测。
3. `theme` aurora + 双行项目 `ListItem` + 焦点边框。
4. LaunchPicker 模态 + §5.1 启动语义接 `launcher`（console wait 后回 TUI；detached 留 TUI）。
5. ListPicker + ActionMenu + Confirm；删除/移动/默认工具/路径类型等。
6. InlineInput 完成分组/项目/命令/配置工具 CRUD；rfd 路径；**WT 中文 IME 手测**（失败则定 fallback）。
7. 回收站、配置右栏全流程（§5.9 / §5.11）。
8. Filter（§5.10）、`?` 帮助。
9. `main` 去掉 dialoguer（含 `trash empty`）；`open` 无标志接 TUI；删除 `dialoguer` 依赖。
10. 更新 `docs/spec.md`、`AGENTS.md`（TUI 取代「非全屏 / dialoguer」；并修正 spec 中过时的「两级选择 / 发起后即退出不等待」表述，与代码及本规格对齐）。
11. `cargo test` / `fmt` / `clippy -D warnings` / `build --release`；Windows Terminal 手测清单见 §9。

## 9. 测试与验收

### 9.1 自动

- 现有 `models` / `ops` / `store` / `launcher` / `main` / `menu` 纯函数单测保持通过。
- 启动选项纯函数单测保留或略扩（默认置顶、无 Windows 路径过滤、自定义命令门禁）。
- **建议**：`App::handle(KeyEvent)` 对模式切换、过滤、Confirm 的单元测试（构造 KeyEvent，无终端）。
- **不**强制像素级 UI 测试或 crossterm 集成测试。

### 9.2 手工（Windows Terminal / conhost）

1. 分栏浏览、焦点色、双行路径显示正常。
2. **每个按键只响应一次**（Press 过滤）。
3. Enter 启动 WSL/PS：离开备用屏幕，占用控制台并 wait，结束后 **回到 TUI Browse**；终端不卡 raw mode。
4. IDE/文件夹：静默启动后 **仍在 TUI**。
5. 分组/项目增删改、移动、默认工具、自定义命令、回收站恢复与清空、配置工具增删改与恢复默认。
6. ListPicker 各场景（路径类型、环境、目标分组）。
7. `/` 过滤（边滤边 j/k、Esc 清除）、`?` 帮助、`q`/`Ctrl+C` restore 正常。
8. **中文 IME**：InlineInput 输入中文分组名/项目名可组字提交（或已文档化的 fallback 可用）。
9. rfd 选文件夹后 TUI 恢复正常。
10. `pcs ls`、`pcs path`、带 flag 的 `open`、`pcs trash empty`（无 dialoguer）等仍无 TUI、输出可用。

## 10. 风险与对策

| 风险 | 对策 |
|---|---|
| Windows 按键双触发 | **只处理 `KeyEventKind::Press`**（§3.5）；步骤 1 手测 |
| rfd 与 raw mode 冲突 | 调用前后 restore/init；App 状态不丢 |
| 启动后终端状态损坏 | console 启动一律先 restore 再 spawn；panic hook + 退出路径 restore；wait 后再 init |
| 中文 IME 组字失败 | 步骤 6 WT 手测；失败则采用可文档化的 fallback（例如临时 restore 用外部输入、或提示用 CLI `pcs add`），**不得 silently 丢字** |
| 中文宽度截断错误 | ratatui/unicode 显示宽度截断 |
| 状态机膨胀 | 强制复用 ListPicker，禁止为每个 Select 场景新 Mode |
| 会话内 config 被旁路 load 覆盖 | 禁止菜单路径 `Config::load()`；只用 `tui::run` 传入的 `&mut AppConfig` |

## 11. 文档同步（实现阶段）

- `docs/spec.md`：技术选型 dialoguer → ratatui；§6 菜单流程改为 TUI 分栏；§7 与 launcher 实际 wait 语义对齐（删除过时「发起后即退出、不等待」）；§9 删除「不提供全屏 TUI」。
- `AGENTS.md`：交互描述改为 ratatui TUI；验证说明保留 WT 手测；模块边界增加 `src/tui/`；注明 Windows Press 过滤与启动后回 TUI。
- 本文件为设计权威；实现若有小幅偏差（如 `open` 短会话 vs 主界面预选）在 PR/提交说明中注明，不削弱 §3.5、§5.1、§5.8–§5.11 硬约束。

## 12. 决策摘要

| 项 | 决定 |
|---|---|
| 范围 | 全量交互替换，删除 dialoguer（含 CLI Confirm） |
| 架构 | `src/tui/` 模块化；App 与 Terminal 解耦 |
| 视觉 | Lazygit 工作台 + aurora（Tokyo Night RGB） |
| 导航 | 左右分栏；启动/操作/单选居中模态 |
| Windows 按键 | **仅 `KeyEventKind::Press`** |
| 通用单选 | **ListPicker** 覆盖一切原 Select 场景 |
| 启动后 | console：restore → wait → **init 回 Browse**；detached：**留 Browse**；进程不因启动而退出 |
| 文本输入 | 底栏 InlineInput；目录 rfd；IME 必测 |
| 过滤 | `/` 边滤边 j/k；Esc 清过滤 |
| 业务层 | 契约不变；会话内单一 config/data 可变借用 |
