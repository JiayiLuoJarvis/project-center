# P1 配置接线与启动 UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `config.rs` 死代码并接线（菜单 + CLI 管理 `config.json`，启动工具全部来自配置），同时将启动选择合并为单屏，并支持项目级默认启动方式（置顶 + `[默认]` 标注）。

**Architecture:** `launcher.rs` 合并 `LauncherKind`/`ToolKind` 为 `LaunchEnv`，以 `(env, command)` 启动；`menu.rs` 用扁平 `LaunchOption` 列表单屏选择，顶层新增「配置」入口，项目操作新增「默认启动方式」；`models.rs` 加 `default_tool` 字段；`main.rs` 补 `mod config` 并新增 `pcs config` 子命令组。

**Tech Stack:** Rust 2024, clap, dialoguer (FuzzySelect/Select/Confirm/Input), serde_json, anyhow。文件位于 `src/launcher.rs` / `src/menu.rs` / `src/models.rs` / `src/main.rs` / `src/config.rs`。

## Global Constraints

- 全部 CLI 输出与源码注释使用简体中文。
- 配置校验规则沿用 config.rs：name 非空、非保留名「终端」、command 非空且不含空白、同环境内 name 不区分大小写不重复。
- 保存采用原子写：`.tmp` → 删旧 → rename；写盘失败仅 stderr 警告，不中断。
- WSL/PS 保持当前控制台阻塞 + 抑制 Ctrl+C（`SetConsoleCtrlHandler`）；IDE/文件夹静默 spawn。禁止回退为 spawn-and-return。
- 控制台标题仍为「项目名 - 分组名」，设置失败不阻止启动。
- 旧 JSON schema 兼容：`wslPath`、`#[serde(default)]`、`defaultTool` 缺失回退。
- 项目/分组查找大小写不敏感语义不变；`@id` 引用不受影响。
- 菜单交互无法自动化测试，改动后按验证命令在真实终端手工验证。

---

### Task 1: models.rs 新增 default_tool 字段

**Files:**
- Modify: `src/models.rs`
- Test: `src/models.rs`（`#[cfg(test)] mod tests` 内新增）

**Interfaces:**
- Consumes: 现有 `Project` 结构。
- Produces: `Project` 新增 `pub default_tool: String` 字段（`#[serde(rename = "defaultTool", default)]`）。

- [ ] **Step 1: 写失败的测试**

在 `src/models.rs` 的 `mod tests` 内追加：

```rust
#[test]
fn json_default_tool_round_trip_and_missing() {
    let json = r#"{"groups":[{"name":"G","projects":[{"name":"x","defaultTool":"opencode"}]}]}"#;
    let data: ProjectData = serde_json::from_str(json).unwrap();
    assert_eq!(data.groups[0].projects[0].default_tool, "opencode");

    let legacy = r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#;
    let data: ProjectData = serde_json::from_str(legacy).unwrap();
    assert_eq!(data.groups[0].projects[0].default_tool, "");

    let out = serde_json::to_value(&data).unwrap();
    assert_eq!(out["groups"][0]["projects"][0]["defaultTool"], "");
}

#[test]
fn project_has_default_tool_helper() {
    let p = Project::new("a", r"E:\a", "");
    assert!(!p.has_default_tool());
    let mut p = Project::new("a", r"E:\a", "");
    p.default_tool = "opencode".into();
    assert!(p.has_default_tool());
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test`
Expected: 编译错误 `no field 'default_tool' on type 'Project'`。

- [ ] **Step 3: 实现字段**

在 `Project` 的 `path` / `wsl_path` 之后新增：

```rust
#[serde(rename = "defaultTool", default)]
pub default_tool: String,
```

并在 `impl Project` 中新增辅助方法 `has_default_tool(&self) -> bool`（`!self.default_tool.trim().is_empty()`）。

- [ ] **Step 4: 运行测试**

Run: `cargo test`
Expected: 全部通过，含新增两条。

### Task 2: launcher.rs 重构为 (env, command) 启动

**Files:**
- Modify: `src/launcher.rs`
- Test: `src/launcher.rs`（`#[cfg(test)] mod tests` 内改造/新增）

**Interfaces:**
- Consumes: `config::Tool`、`models::Project`。
- Produces:
  - `pub enum LaunchEnv { Wsl, PowerShell, Ide, Explorer }`，方法 `label(self) -> &'static str`（WSL / PowerShell / IDE / 文件夹）。
  - `pub fn launch(p: &Project, group_name: &str, env: LaunchEnv, command: &str) -> Result<(), String>`：`command` 为工具命令（WSL/PS 的「终端」传空串表示裸环境）。
  - `pub fn command_for(env: LaunchEnv, tool: &config::Tool) -> String`（IDE/WSL/PS 均取 `tool.command`）。
  - 删除 `LauncherKind`、`ToolKind`、`open_vs_code`、`open_cursor`、`open_wsl_tool`、`open_ps_tool`、`open_powershell`、`open_wsl` 的公开旧入口，改为内部 `fn spawn_env` 系列。

- [ ] **Step 1: 改写测试**

`console_title_uses_project_and_group` 与 `explorer_labels` 改为覆盖新枚举标签；新增命令构造测试：

```rust
#[test]
fn env_labels() {
    assert_eq!(LaunchEnv::Wsl.label(), "WSL");
    assert_eq!(LaunchEnv::Ide.label(), "IDE");
    assert_eq!(LaunchEnv::Explorer.label(), "文件夹");
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test`
Expected: 编译错误（`LauncherKind` / `ToolKind` 不存在）。

- [ ] **Step 3: 重构实现**

- 定义 `LaunchEnv`（替代 `LauncherKind`），删除 `ToolKind` 及其 `label`。
- `launch(p, group, env, command)` 分发：
  - `Wsl`：`wsl.exe --cd <linux> -- <command>`（command 空则裸 `wsl.exe --cd <linux>`）；阻塞等待 + 抑制 Ctrl+C。
  - `PowerShell`：`powershell.exe -NoExit -Command "Set-Location -LiteralPath '<escaped>'; <command>"`（command 空则省略 `; ` 段）；阻塞等待 + 抑制 Ctrl+C。
  - `Ide`：`spawn_quiet("cmd", &["/c", command, win_path])`。
  - `Explorer`：`spawn_quiet("explorer.exe", &[win_path])`。
- 错误信息统一 `"{环境} 启动失败: {e}"`。

- [ ] **Step 4: 运行测试**

Run: `cargo test`
Expected: 通过。

### Task 3: main.rs 接线 config 并新增 pcs config 子命令组

**Files:**
- Modify: `src/main.rs`
- Test: `src/main.rs`（`#[cfg(test)] mod tests` 内新增）

**Interfaces:**
- Consumes: `config::{Config, AppConfig, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config}`、`launcher::LaunchEnv`。
- Produces:
  - `mod config;`
  - `Command::Config(ConfigCommand)` 子命令组（list / add / edit / rm / reset）。
  - `fn parse_config_env(s: &str) -> Result<ConfigEnv>`（大小写不敏感；非法报错「未知环境: {s}，可选 wsl/powershell/ide」）。

- [ ] **Step 1: 更新 Command 枚举与分发**

```rust
#[command(subcommand, about = "管理启动配置")]
Config(ConfigCommand),
```

`ConfigCommand`：

```rust
#[derive(Subcommand)]
enum ConfigCommand {
    #[command(about = "列出配置工具")]
    List { #[arg(long)] env: Option<String> },
    #[command(about = "添加工具")]
    Add {
        #[arg(long)] env: String,
        #[arg(long)] name: String,
        #[arg(long)] command: String,
    },
    #[command(about = "编辑工具")]
    Edit {
        #[arg(long)] env: String,
        #[arg(long)] index: usize,
        #[arg(long)] name: Option<String>,
        #[arg(long)] command: Option<String>,
    },
    #[command(about = "删除工具")]
    Rm { #[arg(long)] env: String, #[arg(long)] index: usize },
    #[command(about = "恢复默认配置")]
    Reset,
}
```

`main()` 分发：`Some(Command::Config(cmd)) => cmd_config(cmd)`。

- [ ] **Step 2: 实现 cmd_config**

```rust
fn cmd_config(command: ConfigCommand) -> Result<()> {
    let mut config = Config::load();
    match command {
        ConfigCommand::List { env } => { /* 列出；无 --env 输出三环境 */ }
        ConfigCommand::Add { env, name, command } => {
            add_tool(&mut config, parse_config_env(&env)?, &name, &command).map_err(anyhow::Error::msg)?;
            save_config(&config);
        }
        ConfigCommand::Edit { env, index, name, command } => {
            // name/command 至少一个为 Some；用现有值填充 None
            let env = parse_config_env(&env)?;
            let (n, c) = match (name, command) {
                (Some(n), Some(c)) => (n, c),
                (Some(n), None) => {
                    let tools = env.tools(&config);
                    let cur = tools.get(index).ok_or_else(|| anyhow::anyhow!("工具索引无效"))?;
                    (n, cur.command.clone())
                }
                (None, Some(c)) => {
                    let tools = env.tools(&config);
                    let cur = tools.get(index).ok_or_else(|| anyhow::anyhow!("工具索引无效"))?;
                    (cur.name.clone(), c)
                }
                (None, None) => bail!("至少提供 --name 或 --command"),
            };
            edit_tool(&mut config, env, index, &n, &c).map_err(anyhow::Error::msg)?;
            save_config(&config);
        }
        ConfigCommand::Rm { env, index } => {
            remove_tool(&mut config, parse_config_env(&env)?, index).map_err(anyhow::Error::msg)?;
            save_config(&config);
        }
        ConfigCommand::Reset => { reset_config(&mut config); save_config(&config); }
    }
    Ok(())
}

fn save_config(config: &AppConfig) {
    if !Config::save(config) {
        eprintln!("警告：无法写入 config.json，本次修改未持久化。");
    }
}
```

`List` 输出格式：`[WSL]` / `[PowerShell]` / `[IDE]` 段，每工具 `名称 (command)`，空环境输出「（暂无工具）」。

- [ ] **Step 3: 更新 find_selected / cmd_open 调用点**

`cmd_direct` 与 `cmd_open` 使用 `launcher::LaunchEnv`：

- `pcs wsl` → `(LaunchEnv::Wsl, "")`（终端）。
- `pcs ps` → `(LaunchEnv::PowerShell, "")`。
- `pcs code` → `config.ide` 第一个工具；空列表报错 `bail!("IDE 工具列表为空，请用 pcs config 添加或恢复默认配置")`；无 Windows 路径项目沿用 `launch_direct` 检查。
- `open -w|-p|-c` 同上映射。

- [ ] **Step 4: 补测试**

```rust
#[test]
fn parse_env_case_insensitive() {
    assert_eq!(parse_config_env("WSL").unwrap(), ConfigEnv::Wsl);
    assert_eq!(parse_config_env("powershell").unwrap(), ConfigEnv::PowerShell);
    assert!(parse_config_env("bash").is_err());
}
```

- [ ] **Step 5: 运行测试**

Run: `cargo test`
Expected: 通过。

### Task 4: menu.rs 单屏选择 + 配置入口 + 默认启动方式

**Files:**
- Modify: `src/menu.rs`
- Test: `src/menu.rs`（`#[cfg(test)] mod tests` 内新增，新增模块）

**Interfaces:**
- Consumes: `config::{Config, AppConfig, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config}`、`launcher::{LaunchEnv, launch}`。
- Produces:
  - `pub struct LaunchOption { pub env: LaunchEnv, pub tool_name: String, pub command: String }` + `fn label(&self) -> String`（`{env_label} · {tool_name}`，Explorer 为 `文件夹`）。
  - `pub fn build_launch_options(project: &Project, config: &AppConfig) -> Vec<LaunchOption>`：WSL 固定「终端」+ `config.wsl`；`has_windows_path()` 时追加 PS（终端 + `config.powershell`）、IDE（`config.ide`）、文件夹。
  - `pub fn default_option_index(options: &[LaunchOption], project: &Project) -> Option<usize>`：`default_tool` 大小写不敏感匹配 `tool_name`。
  - `pub fn choose_launch_option(project: &Project, config: &AppConfig) -> Result<Option<LaunchOption>>`：命中默认时该选项置顶并标 `[默认] ` 前缀，FuzzySelect 单屏选择。

- [ ] **Step 1: 写纯函数与测试**

新增 `#[cfg(test)] mod tests`：

```rust
fn cfg() -> AppConfig { AppConfig::defaults() } // wsl:[opencode,cursor-agent] ps:[...] ide:[VS Code,Cursor]

#[test]
fn build_options_windows_project() {
    let p = Project::new("a", r"E:\a", "");
    let opts = build_launch_options(&p, &cfg());
    // 顺序: WSL·终端, WSL·opencode, WSL·cursor-agent,
    //       PowerShell·终端, PowerShell·opencode, PowerShell·cursor-agent,
    //       IDE·VS Code, IDE·Cursor, 文件夹
    assert_eq!(opts.len(), 9);
    assert_eq!(opts[0].tool_name, "终端");
    assert_eq!(opts[6].tool_name, "VS Code");
    assert_eq!(opts[8].env, LaunchEnv::Explorer);
}

#[test]
fn build_options_wsl_only() {
    let p = Project::new("a", "", "/mnt/e/a");
    let opts = build_launch_options(&p, &cfg());
    assert_eq!(opts.len(), 3); // 仅 WSL 三工具
}

#[test]
fn default_option_match_case_insensitive() {
    let p = Project { default_tool: "OPENCODE".into(), ..Project::new("a", r"E:\a", "") };
    let opts = build_launch_options(&p, &cfg());
    assert_eq!(default_option_index(&opts, &p), Some(1));
}

#[test]
fn default_option_missing_returns_none() {
    let p = Project { default_tool: "ghost".into(), ..Project::new("a", r"E:\a", "") };
    let opts = build_launch_options(&p, &cfg());
    assert_eq!(default_option_index(&opts, &p), None);
}

#[test]
fn reorder_puts_default_first() {
    let p = Project { default_tool: "Cursor".into(), ..Project::new("a", r"E:\a", "") };
    let opts = build_launch_options(&p, &cfg());
    let (head, tail) = split_default(&opts, &p); // 返回 (默认项置顶后的列表, 默认索引)
    assert_eq!(head[0].tool_name, "Cursor");
    assert_eq!(head[0].label(), "[默认] IDE · Cursor");
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test`
Expected: 编译错误（`LaunchOption` 等不存在）。

- [ ] **Step 3: 实现单屏选择**

- 新增 `split_default(options: &[LaunchOption], project: &Project) -> (Vec<LaunchOption>, usize)`：命中的项移到首位并 label 前缀 `[默认] `，返回置顶后的列表与默认索引（未命中 → 原列表 + 0）。
- `choose_launch_option`：构建 → split_default → FuzzySelect（prompt「选择启动方式」，default 置顶索引）→ 返回选中的 LaunchOption。
- 删除 `choose_launcher` 与 `choose_tool`；`project_actions` 的「打开」改为：

```rust
let Some(option) = choose_launch_option(&project, &config)? else { continue; };
launcher::launch(&project, group_name, option.env, &option.command)
    .map_err(|error| anyhow::anyhow!(error))?;
```

- `run` / `project_menu` / `project_actions` / `edit_project` 需要访问 `AppConfig`：`menu::run(data)` 改为 `menu::run(data, &mut config)`（`main.rs` 同步更新：`let mut config = Config::load(); menu::run(&mut data, &mut config)`）。菜单就地修改配置（配置菜单 save 时 `Config::save`）。

- [ ] **Step 4: 配置入口**

- `run` 的顶层 labels 变为 `[分组…] | 新增分组 | 配置 | 退出`，新增 `config_menu(config, theme)?`：
  - 菜单项：`WSL 工具` / `PowerShell 工具` / `IDE 工具` / `恢复默认配置` / `返回`。
  - 选环境 → 列出 `名称 (command)`（空环境显示「（暂无工具）」）+「新增工具」+「返回」；选工具 → 「编辑 / 删除 / 返回」。
  - 新增：`Input` 输入 name 与 command；校验失败 eprintln 报错并留在当前层。
  - 编辑：name/command 默认填充当前值，回车保留。
  - 删除：`Confirm` 确认。
  - 恢复默认：`Confirm` 确认后 `reset_config`。
  - 每次成功改动：`Config::save(config)`，失败 eprintln 警告。

- [ ] **Step 5: 默认启动方式操作**

- `project_actions` 的 actions 变为 `["打开", "默认启动方式", "编辑", "删除", "移动到其他分组", "返回项目列表"]`。
- 「默认启动方式」：`build_launch_options` + split_default 列表 + 追加「不设默认」；选中后：

```rust
ops::set_default_tool(data, &project_id, Some(group_name), Some(tool_name))?; // 或 None 清除
save(data)?;
```

- `ops.rs` 新增 `set_default_tool(data, id, group, tool: Option<&str>) -> Result<()>`：按 id 定位后设置/清空 `default_tool`；项目名重复时按现有 `find_project_by_id` 语义（menu 内已按 id 定位，无歧义）。

- [ ] **Step 6: 更新 launch_direct**

`launch_direct(project, group_name, env, command)`：PS/IDE/Explorer 无 Windows 路径时报错（沿用现有检查）；WSL 传 `""` 命令，IDE 从 `Config::load()` 取第一工具（`pcs code` CLI 路径用 main.rs 传入的命令参数，menu 不调用此函数）。

- [ ] **Step 7: 运行测试**

Run: `cargo test`
Expected: 通过（含新增纯函数测试）。

### Task 5: 全量验证与部署

- [ ] **Step 1: 全量验证**

Run: `cargo test`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo build --release`。全部通过。

- [ ] **Step 2: 非交互冒烟**

Run（无对话框命令）：

```text
pcs config list
pcs config add --env wsl --name hello --command hello && pcs config list --env wsl && pcs config rm --env wsl --index <索引> && pcs config list --env wsl
pcs code 不存在项目   # 期望报错
```

- [ ] **Step 3: 部署**

结束运行中的 `pcs.exe` 进程（taskkill），复制 `target\release\pcs.exe` 到 `E:\dev_tool\pcs\pcs.exe`。

- [ ] **Step 4: 交互验证（真实终端）**

手工验证：单屏启动选择、`[默认]` 置顶、配置菜单增删改、`pcs code` 跟随 `config.ide[0]`、仅 WSL 项目只显示 WSL 选项。
