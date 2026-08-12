# P1 配置接线与启动 UX — 设计规格

日期：2026-08-12
状态：已批准
关联：P1 of 「产品功能/优化」批次（P2 最近/快速启动、P3 数据安全、P4 项目自定义命令、P5 录入效率）

## 1. 背景与目标

`src/config.rs`（commit 3365980 加入）是未被任何模块引用的死代码：`main.rs` 无 `mod config`，菜单无配置入口，`launcher.rs` 仍硬编码 opencode/cursor-agent/code/cursor。用户修改 `config.json` 完全不生效。

P1 目标：

1. 接线 config：菜单与 CLI 均可管理 `config.json`，启动工具列表与命令全部来自配置。
2. 启动 UX：两级选择（环境 → 工具）合并为彻底单屏；项目可设默认启动方式，命中时该选项置顶并标 `[默认]`。

非目标：最近打开 / `pcs go` 全局快速启动（P2）、数据备份与回收站（P3）、项目自定义命令（P4）、批量录入（P5）。

## 2. 架构与模块变更

```
main.rs     mod config; 新增 `pcs config` 子命令组
models.rs   Project 新增 default_tool 字段（JSON: defaultTool）
launcher.rs 以 (env, command) 启动，命令全部来自 config；删除 ToolKind 硬编码路径
menu.rs     单屏启动选择（LaunchOption 扁平列表）、顶层「配置」入口、项目「默认启动方式」操作
```

### 2.1 启动模型（launcher.rs）

`LauncherKind` 与 `ToolKind` 合并为单一枚举 `LaunchEnv`：

| 环境 | 工具来源 | 启动命令 |
|---|---|---|
| WSL | 固定「终端」+ `config.wsl` | `wsl.exe --cd <linux> -- <command>` |
| PowerShell | 固定「终端」+ `config.powershell` | `powershell.exe -NoExit -Command "Set-Location -LiteralPath '<win>'; <command>"` |
| IDE | `config.ide` | `cmd /c <command> <win>`（静默） |
| 文件夹 | 固定一项 | `explorer.exe <win>`（静默） |

- 每个可选项 = `(env, tool_name, command)`，其中 `tool_name` 为「终端」或 config 工具名。
- 保留现有阻塞行为：WSL/PS 在当前控制台运行并等待子进程，期间抑制 Ctrl+C；IDE/文件夹静默 spawn。控制台标题仍为「项目名 - 分组名」。
- `LauncherKind` 重命名语义保持 CLI 兼容：`pcs wsl`/`ps` → 对应环境「终端」；`pcs code` → `config.ide` 第一个工具（默认 VS Code）；`open -w|-p|-c` 同理。`config.ide` 为空时 `pcs code` 报错并提示。
- 删除 `open_ps_tool` 与 `open_wsl_tool` 的 tool 参数版本，统一为 `launch(project, group, env, command)`。

### 2.2 单屏启动选择（menu.rs）

`choose_launcher` + `choose_tool` 合并为 `choose_launch_option(project, config)`：

- 构建扁平选项列表（label 形如 `WSL · 终端`、`WSL · opencode`、`IDE · VS Code`、`文件夹`）：
  - WSL：`终端` + `config.wsl` 各工具
  - 有 `has_windows_path()` 时追加：PS（`终端` + `config.powershell`）、IDE（`config.ide`）、`文件夹`
- 项目 `default_tool` 命中某选项（按工具名，不区分大小写）时：该选项移到列表首位，label 前缀 `[默认] `，默认光标落于其上；未命中则光标默认 0，不标注。
- FuzzySelect 单屏交互；取消返回 None。

### 2.3 默认启动方式（models.rs / menu.rs）

- `Project` 新增 `#[serde(rename = "defaultTool", default)] pub default_tool: String`，旧数据回退为空串。
- 项目操作菜单新增「默认启动方式」：复用启动选项列表（追加「不设默认」），选中即存 `default_tool`（存工具名，如 `opencode`），立即保存。
- 工具在 config 中被改名/删除后，default_tool 静默失效（列表不置顶不标注），不报错。

### 2.4 配置入口

菜单：顶层列表追加「配置」：

```
[分组…] | 新增分组 | 配置 | 退出
```

配置子菜单：`WSL 工具 / PowerShell 工具 / IDE 工具 / 恢复默认配置 / 返回`。选环境后列出工具（`名称 (command)`）+「新增工具」+「返回」；选工具 → 「编辑 / 删除 / 返回」。校验、报错、保存语义沿用 config.rs 现有实现（`add_tool` / `edit_tool` / `remove_tool` / `reset_config` / `Config::save`）。

CLI：新增子命令组 `pcs config`：

```text
pcs config list [--env wsl|powershell|ide]   # 列出工具；不带 --env 时输出全部
pcs config add --env <env> --name <名称> --command <命令>
pcs config edit --env <env> --index <索引> [--name <名称>] [--command <命令>]
pcs config rm --env <env> --index <索引>
pcs config reset                              # 恢复默认配置
```

- `--env` 解析为 `ConfigEnv`（大小写不敏感）；非法时报错。
- 校验失败（空名、保留名「终端」、命令含空白、重名）与 config.rs 现有 `validate_tool_fields` 规则一致，CLI 报错退出。
- 写盘失败（exe 目录只读）→ stderr 警告，CLI 仍以 0 退出（延续「仅警告」哲学，与菜单一致）。

## 3. 数据契约

- `config.json` 格式不变（`wsl`/`powershell`/`ide` 三个数组）。加载/自愈语义（缺失生成默认、损坏备份 `.bak`）不变。
- `projects.json` 项目可含 `defaultTool` 字符串字段，缺失回退；其他字段不变。保持旧 JSON schema 兼容（`wslPath`、`#[serde(default)]`）。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| `config.ide` 为空时 `pcs code` | 报错「IDE 工具列表为空，请配置或恢复默认」 |
| config 校验失败（CLI/菜单） | 报错并终止该操作，不保存 |
| 写盘失败 | stderr 警告，CLI 退出码 0，菜单继续 |
| default_tool 指向不存在的工具 | 静默忽略，不置顶不报错 |
| 仅 WSL 项目选择 PS/IDE/文件夹 | 选项不出现（沿用 `has_windows_path()`） |

## 5. 测试

- `launcher.rs`：启动命令构造（WSL/PS/IDE/Explorer 四环境 × 自定义命令）纯函数化后单测；标题不变。
- `menu.rs`：选项列表构建与 [默认] 置顶逻辑抽为纯函数单测（WSL-only 项目、无命中、命中、大小写不敏感）。
- `models.rs`：`defaultTool` 序列化往返 + 缺失回退 + 旧数据兼容。
- `main.rs`：`--env` 解析、`pcs config add` 校验失败路径（config.rs 已有单测，CLI 仅接线）。
- `cargo test` / `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo build --release`。
- 菜单交互按 AGENTS.md 在真实终端手工验证。

## 6. 验证命令

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

部署到 `E:\dev_tool\pcs\pcs.exe`（需先结束运行中的 pcs.exe 进程）。
