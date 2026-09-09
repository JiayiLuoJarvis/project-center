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
└── launcher.rs   # WSL、PowerShell、opencode、cursor-agent、VS Code、Cursor、资源管理器启动
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
5. 打开项目时先选择启动环境（WSL / PowerShell / IDE 启动 / 文件夹，仅展示当前项目可用的环境），再选择具体工具（文件夹无二级工具，直接打开），全程无启动确认。
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
| 文件夹 | — | 后台运行 `explorer.exe <winPath>`，不创建新控制台 |

WSL 和 PowerShell 环境继承当前控制台的输入输出，不创建 Windows Terminal 新标签页或新控制台。启动全程无「启动 X？(Y/n)」确认。

WSL、PowerShell 启动为「当前控制台 + 等待子进程退出（抑制 Ctrl+C）」；IDE/文件夹后台静默启动、不等待。

### SSH 远程项目

`sshTarget` 非空的项目为 SSH 远程项目：启动选项仅有「SSH · 终端」（当前控制台运行 `ssh.exe`，等待子进程退出）。

- 启动参数：`ssh [-p 端口] [-i 临时密钥] user@host [-t "cd '<远程路径>' 2>/dev/null; exec $SHELL"]`；远程路径缺失/错误时静默落在默认 shell，不断连。
- 秘密存储：密码/私钥口令以 DPAPI 密文存 projects.json；私钥以 DPAPI 密文存数据根 `keys\<项目id>.key`（JSON 仅存相对路径）。
- 自动填充：仅当项目存有可解密的密码/口令时注入 `SSH_ASKPASS=<pcs 自身>`、`SSH_ASKPASS_REQUIRE=force` 与一次性 token（token 校验文件落于数据根 `keys_tmp\`，ssh 退出即清理；1 小时内的活跃会话文件不受启动维护清理影响）。OpenSSH 回调为 `pcs.exe <prompt>`（不带子命令）；pcs 在 clap 解析前检测 `PCS_ASKPASS_TOKEN` 后按 prompt 回答密码/口令（token 缺失/不匹配输出空；不代答 host key 确认）。隐藏子命令 `pcs __askpass <prompt>` 仍保留供手工/测试。无保存秘密或解密失败时回退交互输入。
- 首连保护：主机不在 known_hosts（用 `ssh-keygen -F` 判定，原生支持 hashed 条目；退出码 0 或输出含 found 为命中，ssh-keygen 缺失/调用失败按未知处理）时跳过注入并提示——force 会把首连 host key 确认也路由给 askpass（回空 = 拒绝）导致秒败；首连转交互（确认 host key + 手动输一次密码），连接成功后自动恢复填充。查找名与 ssh 写入格式一致：无端口/端口 22 → 裸主机名，否则 `[host]:port`。
- 失败可读：TUI 启动 ssh 后等待子进程；非零退出（含被信号终止）时在控制台暂停显示「SSH 连接失败（退出码 N / 异常终止）。按 Enter 返回…」，避免报错一闪而过；WSL/PowerShell/IDE 不受影响，spawn 失败仍走 TUI flash。
- 查看保存的秘密：`pcs secret show <项目>` 或 TUI 中 `v`，需先 `pcs pin set` 设置 PIN（PBKDF2 校验哈希存 config.json）；`pcs pin change` 修改、`pcs pin reset --force` 重置（清空 PIN 与全部已存密码、口令、密钥文件）。
- CLI：`pcs ssh <项目>` 直连；`pcs add/edit` 支持 `--ssh`、`--ssh-path`、`--ssh-key`（导入加密）与 `--password-stdin`、`--key-pass-stdin`（控制台输入不回显，提示语走 stderr，重定向喂入不受影响）；`pcs open -s` 等价于 SSH 启动。
- 删除与回收站：软删保留密钥文件（可恢复）；真删（force / 回收站单项删除 / 过期清理 / 清空回收站）删除密钥文件，删除失败记入 `pendingKeyDeletes` 每次启动重试。孤儿密钥文件按 JSON 引用清理，且仅在数据非退化（非损坏兜底/备份恢复）时执行，避免误删。
- TUI：新增/编辑项目对话框将 SSH 目标拆分为「登录用户」「主机」「端口」三字段分开填写（端口默认 22，留空用当前用户登录），保存时拼为 sshTarget；另可填写远程路径、导入密钥、密码与私钥口令（密码/口令掩码显示），一次提交完成秘密保存。

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
