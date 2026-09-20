# SSH 项目自定义命令 — 设计文档

日期：2026-09-20  
状态：已实现  
关联：`docs/superpowers/specs/2026-09-20-product-backlog-and-recent-opens.md`（backlog #2）、`docs/superpowers/specs/2026-09-08-ssh-projects-design.md`、`docs/spec.md`

## 1. 目标与非目标

### 目标

- SSH 项目可配置、启动项目级自定义命令，对齐本地：`LaunchPicker` / `pcs run` / 启动成功后记入最近打开。
- 执行：一锤子——远程跑完命令后 SSH 结束。
- SSH 项目上命令环境固定为 `ssh`（表单不选 WSL/PowerShell/IDE）。
- 远程 `path` 非空时：`cd` 失败则不执行命令，SSH 非零退出。

### 非目标

- 独立 `pcs connection` CLI
- Remote IDE（`code` / `cursor` remote）
- 跑完留 shell、端口转发、ProxyJump、改 `~/.ssh/config`
- 给 SSH 项目挂 config 全局工具（`config.wsl` 等）
- 改变现有「SSH · 终端」的静默 `cd` 降级行为（仅自定义命令走严格 `cd`）

### 成功标准

- SSH 项目 LaunchPicker：至少「SSH · 终端」；有自定义命令时追加在后。
- `pcs run <项目> <命令名>` 对 SSH 项目可用。
- 本地项目自定义命令行为不变。

## 2. 数据与启动

### 数据

- 继续使用 `Project.commands[]`（`name` / `env` / `command`），**不改 JSON schema**。
- `env` 合法值扩展：`wsl` | `powershell` | `ide` | `ssh`（大小写不敏感）。
- SSH 项目新增/编辑命令时 **强制写入 `env=ssh`**。
- 脏数据（SSH 项目上残留的非 `ssh` 命令）：实现时用最简单、不破坏现网的方式处理（例如 LaunchPicker 只展示 `env=ssh`；不必单独做迁移）。

### LaunchPicker（`build_launch_options`）

- SSH 项目：
  1. `SSH · 终端`（`command` 空，现有行为）
  2. 追加该项目自定义命令（标签仍为 `⚙ 名称 (ssh)`）
- 仍不生成 WSL / PowerShell / IDE / 文件夹 / config 全局工具。
- `default_tool` 可命中自定义命令名（与本地一致）。

### 远程执行

- `spawn_direct(..., LaunchEnv::Ssh, command)`：**不再忽略 `command`**。
- `command` 为空 → 现有交互终端（`cd …; exec $SHELL`，静默降级不变）。
- `command` 非空 → 一锤子：
  - `path` 空：远程直接执行用户命令（无 `cd`）。
  - `path` 非空：`cd '<escaped_path>' && <user_command>`（`&&`；`cd` 失败即非零退出）。
- 自定义命令同样分配 TTY（`-t`），认证 / askpass / 临时密钥与现 `spawn_ssh` 共用。
- 用户命令当作远程 shell 片段原样拼接；path 单引号转义保持现状。用户自行负责命令内 quoting。

### CLI

- `pcs run` / `pcs run --list`：SSH 项目可用。
- 不新增 connection 子命令；`pcs ssh` 仍只开终端。

## 3. TUI

- SSH 项目沿用现有「自定义命令」管理入口（增删改）。
- 新增/编辑：**不弹运行环境**，环境固定 `ssh`；表单只需名称 + 命令内容。
- LaunchPicker / 最近打开分叉行为不变；「SSH · 终端」本身不改。

## 4. 验证

- `build_launch_options`：SSH 含终端 + 自定义命令；本地行为回归。
- SSH 参数构造单测：空 path / 有 path 的 `cd &&` 形态（不连真机）。
- TUI：SSH 项目加命令不经过环境选择。
- `cargo test`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`。

## 5. 实现触及面（参考）

| 区域 | 变更要点 |
|------|----------|
| `domain/command.rs` | `canonical_env` 认 `ssh`；SSH 项目强制 `env=ssh` |
| `launch/options.rs` | SSH 分支追加自定义命令后返回 |
| `launch/spawn.rs` | `spawn_ssh` / `ssh_args` 支持一锤子命令 + 严格 `cd` |
| `tui/app/*` | SSH 项目跳过 `CommandEnv` 选择 |
| `cli/project.rs` | `pcs run` 走通 SSH（随 spawn 修复自然生效） |
| `docs/spec.md` | 补 SSH 自定义命令一小节 |

## 6. 已锁定取舍摘要

| 议题 | 结论 |
|------|------|
| 痛点 | 一键跑远程命令（对标本地自定义命令） |
| 执行模型 | 一锤子，跑完断连 |
| 运行环境 | SSH 项目锁死 `ssh` |
| path / `cd` | 有 path 则必须成功，否则失败退出 |
| 本轮范围 | 仅自定义命令；connection CLI / Remote IDE 仍 backlog |
