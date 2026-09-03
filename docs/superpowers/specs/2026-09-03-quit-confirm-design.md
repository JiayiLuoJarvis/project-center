# TUI 退出二次确认 — 设计规格

日期：2026-09-03  
状态：已批准  
关联：`2026-09-03-ratatui-tui-design.md`（修订 q / Ctrl+C 退出路径）

## 1. 背景与目标

全屏 TUI 下 `q` 与 `Ctrl+C` 立刻 `Outcome::Quit`，容易误触退出。目标：退出前二次确认，且**不破坏**当前模式状态（表单输入、LaunchPicker、删除 Confirm、短会话等）。

### 非目标

- launcher 子进程期间的 `SetConsoleCtrlHandler` 抑制逻辑
- 非 TUI / 脚本化 CLI 命令
- 可配置「是否需要确认」开关
- 用 `Enter` 确认退出（保持与现有破坏性 Confirm 的 `[y/N]` 一致）

## 2. 方案选择

| 方案 | 结论 |
|---|---|
| `ConfirmKind::Quit` 换入 `Mode::Confirm` | **否决**：`back_to_browse()` 会丢掉当前模态；短会话取消后会落到完整 Browse |
| 双击 Ctrl+C 时间窗 | 不采用（用户已选二次确认） |
| **`quit_confirm: bool` 覆盖层** | **采用**：不改 `Mode`，取消后留在原模式 |

## 3. 行为规格

### 3.1 进入退出确认

| 触发 | 条件 | 结果 |
|---|---|---|
| `q` / `Q` | `Mode::Browse` 且 `!quit_confirm` | `quit_confirm = true` |
| `Ctrl+C` / `Ctrl+C`（大小写 c） | 任意模式且 `!quit_confirm` | `quit_confirm = true` |

不进入退出确认的情况：

- Help 中的 `q`：仍只关闭 Help（与现网一致）
- Filter / Form / LaunchPicker 等文本输入路径中的 `q`：仍作为字符
- 短会话 LaunchPicker 的 `Esc`：仍直接 `Outcome::Quit`（明确取消，非误触退出）

### 3.2 已在退出确认时

`handle` **优先**处理 `quit_confirm`，不再把按键分发给当前 `Mode`。

| 按键 | 结果 |
|---|---|
| `y` / `Y` | `quit_confirm = false`；`Outcome::Quit` |
| `q` / `Q` | 同上（二次按退出键即确认） |
| 再次 `Ctrl+C` | 同上 |
| `Esc` / `n` / `N` / 其它任意键 | `quit_confirm = false`；`Outcome::Continue`；**Mode 不变** |

`Enter` 视为取消（与现有删除 Confirm「非 y 即否」一致，UI 仍标 `[y/N]`）。

### 3.3 与破坏性 Confirm 叠加

若当前已是 `Mode::Confirm`（删除等），再按 `Ctrl+C`：

1. `quit_confirm = true`，居中弹窗改画退出确认文案（叠在删除确认之上）
2. 取消退出确认后：`Mode::Confirm` **仍在**，可继续 y/N 完成原操作

### 3.4 短会话（`pcs open <名>` 无标志）

- LaunchPicker 内 `Ctrl+C` → 退出确认；取消后仍留在 LaunchPicker
- 确认退出 → 进程退出
- `Esc` 无过滤时直接退出：**不变**

### 3.5 绘制

- `App` 增加 `quit_confirm: bool`（默认 `false`）；`new` / `short_launch` 均为 false
- `ui::render`：在现有 `match app.mode` **之后**，若 `quit_confirm`，对全屏 `area` 调用与删除确认相同的 `render_confirm(..., "确认退出？")`
- 确认 UI 为**居中小弹窗**（`Clear` + 危险色边框 + 标题「确认」+ 正文 + `[y/N]`），与 Form/List 的大模态区分；删除 `Mode::Confirm` 与退出确认共用此绘制
- 若下层已是删除 Confirm 弹窗，退出确认叠在其上；取消退出后删除弹窗仍在
- 帮助文案：`q/Ctrl+C  退出` → `q/Ctrl+C  退出（确认）`
- 底栏快捷提示中的 `q 退出` 可保持简写（详细说明在 `?` 帮助）

### 3.6 文案

- 退出确认：`确认退出？` + `[y/N]`（弹窗样式与删除确认一致）

## 4. 实现边界

| 文件 | 变更 |
|---|---|
| `src/tui/app.rs` | 字段；`handle` 早退；Browse `q`；单测 |
| `src/tui/ui.rs` | `quit_confirm` 覆盖绘制 |
| `src/tui/widgets.rs` | 帮助一行 |
| `AGENTS.md` | 运行时说明：`q`/`Ctrl+C` 需确认 |
| `docs/superpowers/specs/2026-09-03-ratatui-tui-design.md` | 按键表与状态机改为「先确认再 Quit」 |

不改：`launcher.rs`、`main.rs` CLI 分发、`Mode` / `ConfirmKind` 枚举、`docs/spec.md`（旧 dialoguer 文档）。

## 5. 测试

单元测试（`app.rs`）：

1. Browse `q` → `quit_confirm`，非 Quit；再 `y` → Quit
2. 任意模式 `Ctrl+C` → `quit_confirm`；再 `Ctrl+C` → Quit
3. 退出确认下 `n` / `Esc` → 取消，`mode` 与焦点不变
4. Help 下 `q` → 关 Help，不进入 `quit_confirm`
5. Filter 下输入 `q` → 过滤字符含 `q`，不退出
6. `Mode::Confirm`（如 EmptyTrash）下 `Ctrl+C` 再 `Esc` → 仍为原 Confirm

验证命令：

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

## 6. 修订后的状态机（摘录）

```
任意 Mode
  ├─ Ctrl+C（且 !quit_confirm）     → quit_confirm = true
  └─ （Browse）q（且 !quit_confirm） → quit_confirm = true

quit_confirm == true
  ├─ y / q / Ctrl+C  → Quit（restore 后进程退出）
  └─ 其它            → quit_confirm = false（Mode 不变）

Browse
  ├─ …（其余按键同原规格）
  └─ 不再：q → 直接 Quit

Help
  └─ q / Esc / ?     → 关 Help（不变）

短会话 LaunchPicker
  └─ Esc（无过滤）   → 直接 Quit（不变）
```
