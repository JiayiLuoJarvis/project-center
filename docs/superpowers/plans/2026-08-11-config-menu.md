# config.json 菜单内管理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `pcs` 顶级交互菜单中新增「配置管理」入口,按环境（WSL/PowerShell/IDE）增删改 `config.json` 中的工具,并支持一键恢复默认配置,改动即时生效并原子写回文件。

**Architecture:** `config.rs` 承担全部纯逻辑（环境枚举、工具增删改校验、原子保存）并提供单测；`menu.rs` 只做 dialoguer 交互编排;`main.rs` 将 `config` 改为可变并传入菜单,内存就地修改使后续工具选择立即看到新工具。

**Tech Stack:** Rust 2024, clap, dialoguer (Select/Confirm/Input), serde_json, anyhow。文件位于 `config.rs` / `menu.rs` / `main.rs`。

## Global Constraints

- 全部 CLI 输出与源码注释使用简体中文。
- 校验规则：工具 `name` 非空、不得为保留名「终端」（`RESERVED_TERMINAL`）、`command` 非空且不含空白字符、同环境内 `name` 不重复（不区分大小写）。
- 保存采用原子写：写 `config.json.tmp` → 删除旧 `config.json` → rename。
- `config.json` 位于 `pcs.exe` 同目录。
- 校验失败 → 报错并留在当前菜单层、不保存；写盘失败 → 仅 stderr 警告、本次内存修改仍生效。
- 既有 `Config::load_from_dir` 的自愈语义（缺失生成默认、损坏备份 `.bak` 并重置）不得改变。
- 交互菜单无法自动化测试，改动后按验证命令手工验证。

---

### Task 1: config.rs 工具增删改纯函数与校验

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs`（`#[cfg(test)] mod tests` 内新增）

**Interfaces:**
- Consumes: 现有 `Tool`, `AppConfig`, `RESERVED_TERMINAL`（`src/config.rs`）。
- Produces:
  - `pub enum ConfigEnv { Wsl, PowerShell, Ide }`，方法 `key(self) -> &'static str`、`label(self) -> &'static str`、`tools(self, &AppConfig) -> &[Tool]`、`tools_mut(self, &mut AppConfig) -> &mut Vec<Tool>`。
  - `fn validate_tool_fields(name: &str, command: &str) -> Result<Tool, String>`（私有，供 add/edit 与 `validate_tool` 复用）。
  - `pub fn add_tool(config: &mut AppConfig, env: ConfigEnv, name: &str, command: &str) -> Result<(), String>`
  - `pub fn edit_tool(config: &mut AppConfig, env: ConfigEnv, index: usize, name: &str, command: &str) -> Result<(), String>`
  - `pub fn remove_tool(config: &mut AppConfig, env: ConfigEnv, index: usize) -> Result<(), String>`
  - `pub fn reset_config(config: &mut AppConfig)`

- [ ] **Step 1: 写失败的测试**

在 `src/config.rs` 的 `mod tests` 内追加以下测试:

```rust
fn base_config() -> AppConfig {
    AppConfig::defaults()
}

#[test]
fn add_tool_appends_to_env() {
    let mut config = base_config();
    add_tool(&mut config, ConfigEnv::Ide, "CodeBuddy", "codebuddy").unwrap();
    assert_eq!(config.ide.last().unwrap().name, "CodeBuddy");
    assert_eq!(config.ide.last().unwrap().command, "codebuddy");
}

#[test]
fn add_tool_rejects_duplicate_name() {
    let mut config = base_config();
    assert!(add_tool(&mut config, ConfigEnv::Ide, "VS Code", "other").is_err());
    assert_eq!(config.ide.len(), 2);
}

#[test]
fn add_tool_rejects_invalid_fields() {
    let mut config = base_config();
    assert!(add_tool(&mut config, ConfigEnv::Wsl, "", "x").is_err());
    assert!(add_tool(&mut config, ConfigEnv::Wsl, "终端", "x").is_err());
    assert!(add_tool(&mut config, ConfigEnv::Wsl, "x", "").is_err());
    assert!(add_tool(&mut config, ConfigEnv::Wsl, "x", "two words").is_err());
    assert_eq!(config.wsl.len(), 2);
}

#[test]
fn edit_tool_updates_in_place() {
    let mut config = base_config();
    edit_tool(&mut config, ConfigEnv::Ide, 0, "CodeBuddy", "codebuddy").unwrap();
    assert_eq!(config.ide[0].name, "CodeBuddy");
    assert_eq!(config.ide[0].command, "codebuddy");
    assert_eq!(config.ide.len(), 2);
}

#[test]
fn edit_tool_rejects_duplicate_and_oob() {
    let mut config = base_config();
    assert!(edit_tool(&mut config, ConfigEnv::Ide, 0, "Cursor", "x").is_err());
    assert!(edit_tool(&mut config, ConfigEnv::Ide, 5, "x", "x").is_err());
}

#[test]
fn remove_tool_removes_at_index() {
    let mut config = base_config();
    remove_tool(&mut config, ConfigEnv::Ide, 0).unwrap();
    assert_eq!(config.ide.len(), 1);
    assert_eq!(config.ide[0].name, "Cursor");
    assert!(remove_tool(&mut config, ConfigEnv::Ide, 5).is_err());
}

#[test]
fn reset_config_restores_defaults() {
    let mut config = base_config();
    add_tool(&mut config, ConfigEnv::Wsl, "extra", "extra").unwrap();
    remove_tool(&mut config, ConfigEnv::Ide, 0).unwrap();
    reset_config(&mut config);
    assert_eq!(config, AppConfig::defaults());
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test`
Expected: 编译错误，如 `cannot find function 'add_tool'` / `cannot find enum 'ConfigEnv'`。

- [ ] **Step 3: 实现纯函数与重构 `validate_tool`**

在 `Tool` / `AppConfig` 之后新增 `ConfigEnv`:

```rust
/// 可配置工具的环境，与 config.json 顶层键一一对应。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigEnv {
    Wsl,
    PowerShell,
    Ide,
}

impl ConfigEnv {
    pub fn key(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "powershell",
            Self::Ide => "ide",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Wsl => "WSL",
            Self::PowerShell => "PowerShell",
            Self::Ide => "IDE",
        }
    }

    pub fn tools(self, config: &AppConfig) -> &[Tool] {
        match self {
            Self::Wsl => &config.wsl,
            Self::PowerShell => &config.powershell,
            Self::Ide => &config.ide,
        }
    }

    pub fn tools_mut(self, config: &mut AppConfig) -> &mut Vec<Tool> {
        match self {
            Self::Wsl => &mut config.wsl,
            Self::PowerShell => &mut config.powershell,
            Self::Ide => &mut config.ide,
        }
    }
}
```

在 `validate_tool` 前新增字段级校验函数:

```rust
/// 校验 name / command 组合并构造 Tool；调用方需自行 trim。
fn validate_tool_fields(name: &str, command: &str) -> Result<Tool, String> {
    if name.is_empty() {
        return Err("缺少 name 或 name 为空".into());
    }
    if name == RESERVED_TERMINAL {
        return Err(format!("name 与保留名「{RESERVED_TERMINAL}」冲突"));
    }
    if command.is_empty() {
        return Err("缺少 command 或 command 为空".into());
    }
    if command.chars().any(|c| c.is_whitespace()) {
        return Err("command 含空白字符".into());
    }
    Ok(Tool::new(name, command))
}
```

重构 `validate_tool` 以复用上述函数（返回类型由 `Result<Tool, &'static str>` 改为 `Result<Tool, String>`）:

```rust
fn validate_tool(value: &serde_json::Value) -> Result<Tool, String> {
    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let command = value.get("command").and_then(|v| v.as_str()).unwrap_or("");
    validate_tool_fields(name.trim(), command.trim())
}
```

`parse_tools` 内对 `Err(reason)` 的 `eprintln!("config.json: {key}[{index}] 无效（{reason}），已跳过。")` 无需改动（`String` 可直接格式化）。

在 `Config` impl 之外（自由函数，靠近 `parse_config`）新增增删改与重置:

```rust
pub fn add_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    name: &str,
    command: &str,
) -> Result<(), String> {
    let tool = validate_tool_fields(name.trim(), command.trim())?;
    let tools = env.tools_mut(config);
    if tools.iter().any(|t| t.name.eq_ignore_ascii_case(&tool.name)) {
        return Err(format!("名称「{}」已存在", tool.name));
    }
    tools.push(tool);
    Ok(())
}

pub fn edit_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
    name: &str,
    command: &str,
) -> Result<(), String> {
    let tool = validate_tool_fields(name.trim(), command.trim())?;
    let tools = env.tools_mut(config);
    if index >= tools.len() {
        return Err("工具索引无效".into());
    }
    if tools
        .iter()
        .enumerate()
        .any(|(i, t)| i != index && t.name.eq_ignore_ascii_case(&tool.name))
    {
        return Err(format!("名称「{}」已存在", tool.name));
    }
    tools[index] = tool;
    Ok(())
}

pub fn remove_tool(config: &mut AppConfig, env: ConfigEnv, index: usize) -> Result<(), String> {
    let tools = env.tools_mut(config);
    if index >= tools.len() {
        return Err("工具索引无效".into());
    }
    tools.remove(index);
    Ok(())
}

pub fn reset_config(config: &mut AppConfig) {
    *config = AppConfig::defaults();
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: PASS,现有 44 个测试 + 新增 7 个（`add_tool_appends_to_env`、`add_tool_rejects_duplicate_name`、`add_tool_rejects_invalid_fields`、`edit_tool_updates_in_place`、`edit_tool_rejects_duplicate_and_oob`、`remove_tool_removes_at_index`、`reset_config_restores_defaults`）。

Run: `cargo fmt --check`
Run: `cargo clippy --all-targets -- -D warnings`
Expected: 均通过。

- [ ] **Step 5: 提交**

```bash
git add src/config.rs
git commit -m "pcs: 配置工具增删改纯函数与字段级校验"
```

---

### Task 2: config.rs 原子保存能力

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs`（`mod tests` 内新增）

**Interfaces:**
- Consumes: Task 1 的 `ConfigEnv` / `add_tool`。
- Produces:
  - `pub fn Config::save(config: &AppConfig) -> bool` — 写 exe 同目录。
  - `pub(crate) fn Config::save_to_dir(dir: &Path, config: &AppConfig) -> bool` — 供测试与复用。
  - `Tool` 与 `AppConfig` 派生 `serde::Serialize`。

- [ ] **Step 1: 写失败的测试**

在 `src/config.rs` 的 `mod tests` 内追加:

```rust
#[test]
fn save_then_load_roundtrip() {
    let dir = temp_dir();
    let mut config = AppConfig::defaults();
    add_tool(&mut config, ConfigEnv::Ide, "CodeBuddy", "codebuddy").unwrap();
    assert!(Config::save_to_dir(&dir, &config));
    let loaded = Config::load_from_dir(&dir);
    assert_eq!(loaded, config);
    assert!(!dir.join("config.json.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_serializes_all_env_keys() {
    let dir = temp_dir();
    let config = AppConfig::defaults();
    assert!(Config::save_to_dir(&dir, &config));
    let text = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(value.get("wsl").is_some());
    assert!(value.get("powershell").is_some());
    assert!(value.get("ide").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_then_load_empty_envs() {
    let dir = temp_dir();
    let config = AppConfig {
        wsl: Vec::new(),
        powershell: Vec::new(),
        ide: Vec::new(),
    };
    assert!(Config::save_to_dir(&dir, &config));
    assert_eq!(Config::load_from_dir(&dir), config);
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test`
Expected: 编译错误，如 `cannot find function 'save_to_dir'` 或 `the trait bound 'AppConfig: Serialize' is not satisfied`。

- [ ] **Step 3: 实现保存**

给两个结构体派生序列化:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Tool {
    pub name: String,
    pub command: String,
}
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppConfig {
    pub wsl: Vec<Tool>,
    pub powershell: Vec<Tool>,
    pub ide: Vec<Tool>,
}
```

在 `Config` impl 内 `load()` 附近新增:

```rust
pub fn save(config: &AppConfig) -> bool {
    Self::save_to_dir(&Self::exe_dir(), config)
}

/// 原子写：写 `config.json.tmp` -> 删除旧文件 -> rename。
pub(crate) fn save_to_dir(dir: &Path, config: &AppConfig) -> bool {
    let path = dir.join("config.json");
    let Ok(json) = serde_json::to_string_pretty(config) else {
        return false;
    };
    let tmp = dir.join("config.json.tmp");
    if std::fs::write(&tmp, json).is_err() {
        return false;
    }
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&tmp, &path).is_ok()
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`
Expected: PASS,新增 3 个保存测试全部通过。

Run: `cargo fmt --check`
Run: `cargo clippy --all-targets -- -D warnings`
Expected: 均通过。

- [ ] **Step 5: 提交**

```bash
git add src/config.rs
git commit -m "pcs: config.json 原子保存能力"
```

---

### Task 3: 菜单接线可变配置

**Files:**
- Modify: `src/main.rs:132`（`let config = Config::load();`）
- Modify: `src/menu.rs:57`（`pub fn run(data: &mut ProjectData, config: &AppConfig)`）

**Interfaces:**
- Consumes: 现有 `menu::run`。
- Produces: `pub fn run(data: &mut ProjectData, config: &mut AppConfig) -> Result<()>`；`main.rs` 中 `let mut config = Config::load();` 并传 `&mut config`。

- [ ] **Step 1: 实现引用改动**

`src/main.rs`:

```rust
let mut config = Config::load();
...
None | Some(Command::Menu) => {
    let mut data = Store::load();
    menu::run(&mut data, &mut config)
}
```

`src/menu.rs`:

```rust
pub fn run(data: &mut ProjectData, config: &mut AppConfig) -> Result<()> {
```

（`run` 内部对 `config` 的所有 `&AppConfig` 传参调用点——`project_menu(data, group_name, config, &theme)`——依赖 `&mut T` 到 `&T` 的自动重借用，无需改动。）

- [ ] **Step 2: 验证现有行为未回归**

Run: `cargo test`
Expected: PASS（数量与 Task 2 结束时一致，共 54 个）。

Run: `cargo fmt --check`
Run: `cargo clippy --all-targets -- -D warnings`
Expected: 均通过。

- [ ] **Step 3: 提交**

```bash
git add src/main.rs src/menu.rs
git commit -m "pcs: 菜单改用可变配置引用"
```

---

### Task 4: 菜单内配置管理界面

**Files:**
- Modify: `src/menu.rs`

**Interfaces:**
- Consumes: Task 1 的 `ConfigEnv` / `add_tool` / `edit_tool` / `remove_tool` / `reset_config`,Task 2 的 `Config::save`。现有 `run` 已接收 `&mut AppConfig`（Task 3）。
- Produces: 私有函数 `config_menu`、`env_menu`、`tool_actions`、`add_tool_interactive`、`save_config`。顶级菜单新增「配置管理」入口（第 4 项，位于「新增分组」之后、「退出」之前）。

- [ ] **Step 1: 更新导入**

`src/menu.rs` 顶部:

```rust
use crate::config::{AppConfig, Config, ConfigEnv, RESERVED_TERMINAL, Tool};
use crate::config::{add_tool, edit_tool, remove_tool, reset_config};
```

- [ ] **Step 2: 顶级菜单加入口**

在 `run` 的顶级循环中（`labels.push("新增分组".into());` 之后）:

```rust
labels.push("配置管理".into());
labels.push("退出".into());
```

并调整分支判断:

```rust
if index == group_count {
    add_group(data, &theme)?;
    continue;
}
if index == group_count + 1 {
    config_menu(config, &theme)?;
    continue;
}
if index == group_count + 2 {
    return Ok(());
}
```

- [ ] **Step 3: 实现配置管理子菜单**

在 `run` 之后新增:

```rust
fn config_menu(config: &mut AppConfig, theme: &ColorfulTheme) -> Result<()> {
    let labels = ["WSL 工具", "PowerShell 工具", "IDE 工具", "恢复默认配置", "返回"];
    loop {
        let Some(index) = Select::with_theme(theme)
            .with_prompt("配置管理")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        match index {
            0 => env_menu(config, ConfigEnv::Wsl, theme)?,
            1 => env_menu(config, ConfigEnv::PowerShell, theme)?,
            2 => env_menu(config, ConfigEnv::Ide, theme)?,
            3 => {
                if Confirm::with_theme(theme)
                    .with_prompt("确认恢复默认配置？")
                    .default(false)
                    .interact()?
                {
                    reset_config(config);
                    save_config(config);
                    println!("已恢复默认配置。");
                }
            }
            _ => return Ok(()),
        }
    }
}

fn env_menu(config: &mut AppConfig, env: ConfigEnv, theme: &ColorfulTheme) -> Result<()> {
    loop {
        let tools = env.tools(config);
        let mut labels: Vec<String> = tools
            .iter()
            .map(|tool| format!("{} ({})", tool.name, tool.command))
            .collect();
        let tool_count = labels.len();
        labels.push("新增工具".into());
        labels.push("返回".into());
        let Some(index) = Select::with_theme(theme)
            .with_prompt(format!("{}：工具", env.label()))
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == tool_count {
            add_tool_interactive(config, env, theme)?;
            continue;
        }
        if index == tool_count + 1 {
            return Ok(());
        }
        if tool_actions(config, env, index, theme)? {
            return Ok(());
        }
    }
}

fn add_tool_interactive(config: &mut AppConfig, env: ConfigEnv, theme: &ColorfulTheme) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("名称")
        .interact_text()?;
    let command: String = Input::with_theme(theme)
        .with_prompt("命令")
        .interact_text()?;
    match add_tool(config, env, &name, &command) {
        Ok(()) => {
            save_config(config);
            println!("工具已新增: {}", name.trim());
        }
        Err(message) => eprintln!("{message}"),
    }
    Ok(())
}

fn tool_actions(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
    theme: &ColorfulTheme,
) -> Result<bool> {
    let tool = env.tools(config)[index].clone();
    let actions = ["编辑", "删除", "返回"];
    let Some(action) = Select::with_theme(theme)
        .with_prompt(format!("{}：选择操作", tool.name))
        .items(&actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(false);
    };
    match action {
        0 => {
            let name: String = Input::with_theme(theme)
                .with_prompt("名称")
                .default(tool.name.clone())
                .interact_text()?;
            let command: String = Input::with_theme(theme)
                .with_prompt("命令")
                .default(tool.command.clone())
                .interact_text()?;
            match edit_tool(config, env, index, name.trim(), command.trim()) {
                Ok(()) => {
                    save_config(config);
                    println!("工具已更新。");
                }
                Err(message) => eprintln!("{message}"),
            }
            Ok(false)
        }
        1 => {
            if Confirm::with_theme(theme)
                .with_prompt(format!("确认删除工具 `{}`？", tool.name))
                .default(false)
                .interact()?
            {
                remove_tool(config, env, index).map_err(anyhow::anyhow)?;
                save_config(config);
                println!("已删除工具: {}", tool.name);
                return Ok(true);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn save_config(config: &AppConfig) {
    if !Config::save(config) {
        eprintln!("无法写入 config.json，本次修改仅在本次运行生效。");
    }
}
```

- [ ] **Step 4: 编译与静态检查**

Run: `cargo test`
Expected: PASS（54 个测试，与 Task 3 一致；无新增自动化测试，交互逻辑不纳入单测）。

Run: `cargo fmt --check`
Run: `cargo clippy --all-targets -- -D warnings`
Run: `cargo build --release`
Expected: 全部通过，生成 `target\release\pcs.exe`。

- [ ] **Step 5: 真实终端手工验证**

在 Windows Terminal / conhost 中验证：

1. 运行 `pcs`,顶级菜单出现「配置管理」第 4 项;选中进入子菜单,含 WSL / PowerShell / IDE 工具、「恢复默认配置」、「返回」。
2. IDE 环境新增 `CodeBuddy`/`codebuddy`:成功后 `config.json` 出现该项,随后打开项目两级选择里能看到新工具,无需重启。
3. 编辑已有工具名称/命令,默认值预填;回车保留原值。
4. 新增空名称、名称「终端」、命令含空白、与现有工具重名 → 均报错并留在当前层,`config.json` 未被改动。
5. 删除工具需确认;删除后立即生效。
6. 「恢复默认配置」确认后三环境回到 `AppConfig::defaults()`,`config.json` 同步。
7. 手动把 `config.json` 权限设为只读（或改用只读目录副本部署）后再改动 → stderr 出现无法写入警告,菜单继续运行。

- [ ] **Step 6: 提交**

```bash
git add src/menu.rs
git commit -m "pcs: 菜单内配置管理入口"
```

---

## 自审记录

- **Spec 覆盖**：架构（Task 1/2/3/4 模块分工）、菜单流程（Task 4 顶级入口与子菜单）、错误处理（校验失败留当前层+写盘失败仅警告,Task 4 Step 3 的 `save_config` 与 `eprintln`）、恢复默认（Task 1 `reset_config` + Task 4 入口）、保存写盘（Task 2 原子写）、测试（Task 1/2 单测 + Task 4 手工清单）均已落实。
- **占位符检查**：所有步骤含具体代码或明确命令,无 TBD/TODO。
- **类型一致性**：`ConfigEnv::key/label/tools/tools_mut`、`add_tool`/`edit_tool`/`remove_tool`/`reset_config` 签名在 Task 1 定义后于 Task 2/4 一致引用;`Config::save`/`save_to_dir` 在 Task 2 定义后于 Task 4 引用;`menu::run(&mut data, &mut config)` 与 Task 3 签名一致。
