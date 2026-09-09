# Project Center (`pcs`)

Windows 原生的 Rust 2024 单二进制 CLI，用于按分组管理开发项目，并在 WSL、PowerShell、IDE（VS Code / Cursor）和 SSH 远程之间快速启动。

## 特性

- **分组管理**：按分组组织项目，支持增删改查与跨组移动。
- **快速启动**：WSL、PowerShell、VS Code、Cursor、文件夹（explorer）、SSH 远程终端，一键打开。
- **TUI 菜单**：`pcs` / `pcs menu` 进入全屏 ratatui 选择菜单；同时保留全部无对话框的子命令供脚本使用。
- **SSH 远程项目**：DPAPI 加密存储密码/私钥口令，私钥存放于数据根 `keys\`，SSH_ASKPASS 自动填充，PIN（PBKDF2）保护查看。
- **回收站**：软删可恢复，支持清空与过期清理。
- **数据安全**：原子写入、备份、旧数据自动补 id。

## 构建

```sh
cargo build --release
```

产物为 `target\release\pcs.exe`（LTO + strip）。

## 验证

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

## 使用

```text
pcs                                # 打开 TUI 菜单
pcs ls [--group <分组>] [--json]   # 列出项目
pcs open <项目> [-w|-p|-c|-s]      # 选择/直接启动
pcs wsl|ps|code|ssh <项目>         # 指定环境直接启动
pcs path|wslpath <项目>            # 打印路径
pcs add <项目> --group <分组> [--dir <路径>] [--wsl-path <路径>]
pcs edit <项目> [--new-name <名称>] [--dir <路径>] [--wsl-path <路径>]
pcs rm <项目>                      # 删除（入回收站）
pcs mv <项目> --to <目标分组>
pcs group add|rename|rm <分组>
pcs trash ls|restore|empty --force # 回收站
pcs pin set|change|reset --force   # 查看 PIN 管理
pcs secret show <项目>             # 查看保存的 SSH 秘密
```

任意 `<项目>` 位置也接受 `@<id>`（支持唯一前缀），id 不受重命名影响。项目名跨分组不唯一时必须提供 `--group`。

## 数据位置

- **release**：`%APPDATA%\project_center\projects.json`，回退 `%USERPROFILE%\.project_center\projects.json`。
- **debug**（`cargo run`/`cargo test`）：`%APPDATA%\project_center_dev\projects.json`，回退 `%USERPROFILE%\.project_center_dev\projects.json`。
- 备份位于数据根 `backups\`；SSH 私钥位于数据根 `keys\`；`config.json`（启动工具配置、PIN）位于 exe 旁。

## 详细规格

参见 [`docs/spec.md`](docs/spec.md)（功能参考；以代码为准）。
