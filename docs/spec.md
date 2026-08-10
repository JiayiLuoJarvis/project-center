# Project Center — Rust CLI 规格说明

版本：v2.4
日期：2026-08-10
状态：已实现

## 1. 目标

`pcs` 是 Windows 原生 Rust CLI，用于按分组管理开发项目，并在 WSL、PowerShell、IDE（VS Code / Cursor）之间快速启动。工具不使用全屏 TUI，不进入备用屏幕，也不维护常驻终端状态机。

默认运行 `pcs` 时使用轻量选择菜单：分组 → 项目 → 项目操作。菜单支持新增、查看、编辑、删除和移动项目，也支持新增、重命名和删除分组。CLI 子命令仍用于脚本和快捷操作。

## 2. 技术选型

| 项 | 选择 |
|---|---|
| 语言 | Rust 2024 |
| CLI 参数 | clap derive |
| 选择菜单 | dialoguer（短时 ↑↓ / Enter 交互） |
| 数据持久化 | JSON + serde / serde_json |
| 错误处理 | anyhow |
| 原生文件夹对话框 | rfd（Windows native） |
| 产物 | `pcs.exe` 单二进制 |

## 3. 项目结构

```
src/
├── main.rs       # clap 命令定义与分发
├── menu.rs       # 分组/项目/项目操作与两级启动选择菜单
├── ops.rs        # 项目与分组的纯数据操作、同名解析
├── models.rs     # 数据模型、JSON 序列化、路径互转
├── store.rs      # JSON 读取与原子写入
└── launcher.rs   # WSL、PowerShell、opencode、cursor-agent、VS Code、Cursor 启动
```

## 4. 数据与路径

数据文件：`%APPDATA%\project_center\projects.json`；`APPDATA` 缺失时回退到 `%USERPROFILE%\.project_center\projects.json`。

JSON schema 与旧 Flutter 应用兼容，WSL 字段仍为 `wslPath`，可选字段使用默认值。每个项目带 `id` 字段（UUID v4），作为重命名后仍稳定的标识；旧数据缺 `id` 时加载自动补生成。文件不存在或损坏时读取为空数据。保存流程保持原子写：`.json.tmp` → 删除旧文件 → rename。

`Project::linux_path()` 优先使用手填 `wsl_path`，否则把 Windows 路径转换为 `/mnt/<drive>/...`。Linux 路径直接透传。`has_windows_path()` 用于禁用 PowerShell / IDE 启动。

## 5. 命令

```text
pcs                                      # 打开轻量菜单
pcs menu                                 # 打开轻量菜单
pcs ls [--group <分组>] [--json]          # 列出项目
pcs open <项目> [--group <分组>] [-w|-p|-c] # 选择启动方式并打开；带标志时直接启动
pcs wsl <项目> [--group <分组>]           # 直接使用 WSL
pcs ps <项目> [--group <分组>]            # 直接使用 PowerShell
pcs code <项目> [--group <分组>]          # 直接使用 VS Code
pcs path <项目> [--group <分组>]          # 打印 Windows 路径
pcs wslpath <项目> [--group <分组>]       # 打印 WSL 路径
pcs add <项目> --group <分组> [--dir <Windows路径>] [--wsl-path <WSL路径>]
pcs edit <项目> [--group <分组>] [--new-name <名称>] [--dir <路径>] [--wsl-path <路径>]
pcs rm <项目> [--group <分组>]
pcs mv <项目> --to <目标分组> [--group <源分组>]
pcs group add <分组>
pcs group rename <旧名称> <新名称>
pcs group rm <分组> [--force]
```

项目名跨分组不唯一时，所有按名称操作都必须提供 `--group`，不会默默操作第一个匹配项。任意 `<项目>` 位置也可使用 `@<id>` 按项目 id 选择（支持唯一前缀），id 不受重命名影响；因此以 `@` 开头的项目名无法再按名字选择。旧数据首次加载时自动补齐 id 并立即写回，保证 id 稳定可用。

## 6. 菜单流程

1. 使用 ↑↓ 选择分组，也可以在此处新增分组。
2. 进入分组后选择项目，也可以新增项目或管理当前分组。新增项目可选择 Windows 路径，或只输入 WSL 路径。
3. 选择项目后可打开、编辑、删除、移动或返回项目列表。
4. 编辑项目时各字段默认填充当前值，直接回车即可保留。
5. 打开项目时先选择启动环境（WSL / PowerShell / IDE 启动，仅展示当前项目可用的环境），再选择具体工具，全程无启动确认。
6. 启动成功后退出菜单；取消任一级选择则回到项目操作菜单。

菜单内的增删改操作会立即保存到 JSON 文件。仅 WSL 项目保存为空的 `path` 和独立的 `wslPath`，因此只显示 WSL 环境。

## 7. 启动

打开项目时采用两级选择：先选启动环境，再选工具。各组合的启动方式如下：

| 环境 | 工具 | 启动方式 |
|---|---|---|
| WSL | 终端 | 当前控制台运行 `wsl.exe --cd <linuxPath>`，立即返回 |
| WSL | opencode | 当前控制台运行 `wsl.exe --cd <linuxPath> -- opencode`，立即返回 |
| WSL | cursor-agent | 当前控制台运行 `wsl.exe --cd <linuxPath> -- cursor-agent`，立即返回 |
| PowerShell | 终端 | 当前控制台运行 `powershell.exe -NoExit -Command "Set-Location -LiteralPath '<winPath>'"`，立即返回 |
| PowerShell | opencode | 当前控制台运行 `powershell.exe -NoExit -Command "Set-Location -LiteralPath '<winPath>'; opencode"`，立即返回 |
| PowerShell | cursor-agent | 当前控制台运行 `powershell.exe -NoExit -Command "Set-Location -LiteralPath '<winPath>'; cursor-agent"`，立即返回 |
| IDE 启动 | VS Code | 后台运行 `cmd /c code <winPath>`，不创建新控制台 |
| IDE 启动 | Cursor | 后台运行 `cmd /c cursor <winPath>`，不创建新控制台 |

WSL 和 PowerShell 环境继承当前控制台的输入输出，不创建 Windows Terminal 新标签页或新控制台。启动全程无「启动 X？(Y/n)」确认。

所有环境均以「发起后即退出」方式启动：pcs 用 `spawn` 启动子进程后立即返回退出，不等待子进程结束。WSL / PowerShell 会话继续挂载在原控制台窗口上运行（Windows 下父进程退出不会终止子进程），控制台标题也沿用 pcs 设置的「项目名 - 分组名」；VS Code / Cursor 则在后台静默启动。因此打开项目后 pcs 进程会立即消失，不会残留驻留。

## 8. 验证

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo build --release`
- 在真实 Windows Terminal / conhost 中验证菜单导航、两级启动选择、旧 JSON 加载和 CLI CRUD。

## 9. 非目标

- 不提供全屏 TUI 或备用屏幕；交互菜单使用短时选择和输入提示。
- 不做 shell `cd` 集成。
- 不删除原 Flutter 源码。
- 不适配 Linux/macOS；本项目仅面向 Windows。
