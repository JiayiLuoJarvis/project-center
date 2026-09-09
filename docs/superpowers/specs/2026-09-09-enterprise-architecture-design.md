# pcs 企业级单 crate 架构改造

日期：2026-09-09  
状态：已批准  
范围：不改功能；重排模块、错误类型、测试分层、工程闸门与文档

## 目标

把现有约 1.2 万行、单 crate、无 `lib.rs` 的 Windows CLI，改成与 ripgrep / fd / bat 同级的可维护形态：

- 薄 `main.rs` + `src/lib.rs`，按 cli / domain / persist / launch / tui 分层
- 库内 `thiserror`，二进制边界 `anyhow`
- 模块单测保留并升级关键错误断言；新增 `tests/` 二进制冒烟
- toolchain / fmt / clippy / cargo-deny / GitHub Actions
- `AGENTS.md`、`docs/spec.md` 结构段、`README.md` 与代码对齐

对齐对象：Cargo Book 包布局、cargo 的 ops/core 分界（不过度 workspace）、2026 Cargo 生产实践（`lib`+`bin`、`rust-version`、deny、CI）。不上 helix/cargo 级多 crate。

## 非目标

- 任何用户可见功能、CLI 参数、中文文案、JSON `wslPath` schema 的变化
- 改变 debug/release 数据目录默认值、启动等待子进程、`SetConsoleCtrlHandler`、软删/PIN/SSH 语义
- Cargo workspace、Repository trait、application 层、tracing、tokio、miette、mockall
- `#![forbid(unsafe_code)]`、clippy pedantic/nursery、codecov、pre-commit、Linux 上编译 `pcs`
- 代填 `license` 字段或新增 LICENSE
- 把模块单测搬出 `src/`；TUI 端到端（真控制台）

## 目标目录

仍是一个 crate、一个 `pcs.exe`。

```text
src/
  lib.rs
  main.rs
  cli/
    mod.rs              # cli::run()
    args.rs             # clap 定义
    output.rs           # ls 等人读输出
    project.rs
    open.rs
    group.rs
    config.rs
    trash.rs
    pin.rs
    secret.rs           # secret show、__askpass
  domain/
    mod.rs
    error.rs
    models.rs
    lookup.rs
    project.rs
    group.rs
    trash.rs
    command.rs
  persist/
    mod.rs
    error.rs
    store.rs
    config.rs
    secret.rs
  launch/
    mod.rs
    error.rs
    spawn.rs
    options.rs          # 现 menu.rs
  tui/
    mod.rs
    actions.rs
    theme.rs
    ui.rs
    widgets.rs
    app/
      mod.rs
      state.rs
      browse.rs
      form.rs
      launch.rs
      filter.rs
      confirm.rs
      action_menu.rs
      secret_viewer.rs
tests/
  cli_smoke.rs
.github/workflows/ci.yml
rust-toolchain.toml
rustfmt.toml
deny.toml
```

`secret.rs`（约 740 行）本期不拆；内聚则保持单文件。

## 文件映射

| 现在 | 之后 |
|---|---|
| `src/main.rs` clap + 分发 | `src/cli/args.rs` + `cli/*.rs`；`main.rs` 只调 `pcs::cli::run()` |
| `src/main.rs` askpass | `src/cli/secret.rs`（测试随迁） |
| `src/models.rs` | `src/domain/models.rs` |
| `src/ops.rs` 查找 | `src/domain/lookup.rs` |
| `src/ops.rs` 项目 CRUD / SSH 字段 / 默认工具 | `src/domain/project.rs` |
| `src/ops.rs` 分组 | `src/domain/group.rs` |
| `src/ops.rs` 回收站 | `src/domain/trash.rs` |
| `src/ops.rs` 项目命令 | `src/domain/command.rs` |
| `src/store.rs` | `src/persist/store.rs` |
| `src/config.rs` | `src/persist/config.rs` |
| `src/secret.rs` | `src/persist/secret.rs` |
| `src/launcher.rs` | `src/launch/spawn.rs` |
| `src/menu.rs` | `src/launch/options.rs` |
| `src/tui/app.rs` | `src/tui/app/*`（单测留在 `app/mod.rs` 或 `app/tests.rs` 子模块） |
| 其余 `src/tui/*` | 路径不变，import 跟上 |

## 依赖方向

靠 import 约束，不引入 trait：

- `cli` / `tui` → `domain` / `persist` / `launch`
- `launch` → `domain::models`、`persist::secret`
- `persist` 不依赖 `cli` / `tui` / `launch`
- **承认现状：** `domain` 删除/恢复仍调用 `persist::secret` 清理密钥 sidecar，不抽 application 层

可见性：crate 内以 `pub(crate)` 为主。`lib.rs` 对外只保证 `pcs::cli::run` 可被二进制调用。`tests/` 走二进制，不把 CRUD 做成公开库 API。不发布 crates.io。

## 错误与数据流

### 错误

三个 `thiserror` 枚举 + crate 级汇总：

- `domain::Error`：查找、重名、校验。`#[error("...")]` 与现有 `bail!` **逐字相同**（例如 `未找到项目: {name}`、`项目名不唯一: {name}，请使用 --group 指定分组`）。
- `persist::Error`：写盘失败、DPAPI/PIN/密钥 IO。`Store::load` 缺文件或损坏仍视为空数据，不是错误。
- `launch::Error`：替换 `Result<T, String>`（无 Windows 路径、非 SSH 项目用 SSH、等待子进程失败等），Display 与现字符串一致。

`cli::run()` 返回 `anyhow::Result<()>`，仅在边界加 context。`main`：

```rust
fn main() {
    if let Err(err) = pcs::cli::run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}
```

clap 用法错误仍由 clap 以退出码 2 处理。不引入多退出码表。TUI 闪字走 `Display`，用户中文不变。

新增依赖：`thiserror`（2.x）。保留 `anyhow`。

### 数据流（行为冻结）

```text
main → cli::run
         ├─ Store::load / Config::load
         ├─ domain 内存操作
         ├─ Store::save / Config::save
         ├─ launch::spawn → wait_spawned（控制台子进程阻塞）
         └─ menu / 无标志 open → tui（同一套 domain/persist/launch）
```

TUI 写 `config.json` 失败仍只警告；CLI 写 `projects.json` 失败仍报错退出。`__askpass` 仍是隐藏命令。

### 测试缝（不对用户暴露命令）

| 环境变量 | 含义 | 未设置 |
|---|---|---|
| `PCS_DATA_DIR` | 数据根：`projects.json`、`backups\`、`keys\`、`keys_tmp\` | 仍按 `debug_assertions` 走 `%APPDATA%\project_center[_dev]` 及 HOME 回退 |
| `PCS_CONFIG_PATH` | `config.json` 完整路径 | 仍在 exe 旁 |

只写进 `AGENTS.md`，不写进面向用户的 README 用法。现有 `load_from` / `save_to` 保留给单元测试。

## 测试

| 层 | 位置 | 内容 |
|---|---|---|
| 模块单测 | 随文件 `#[cfg(test)]` | 现有用例全部搬家，必须继续绿 |
| 错误断言 | lookup / 重名等关键路径 | `matches!(err, domain::Error::ProjectAmbiguous { .. })`；其余可仍 `is_err()` |
| 二进制冒烟 | `tests/cli_smoke.rs` | `assert_cmd`：`pcs --help`、`pcs ls`（临时 `PCS_DATA_DIR`） |
| 启动器 | 现有纯函数测 | `ssh_args`、askpass 分类等；CI 不真启 wsl.exe |
| TUI | `App::handle` 单测随迁 | 不做全屏集成测试 |

新 dev-dependency：仅 `assert_cmd`。不上 `predicates`、mockall、insta、trycmd。

## 工程闸门

| 项 | 决定 |
|---|---|
| `rust-toolchain.toml` | `channel = "stable"`，`components = ["rustfmt", "clippy"]` |
| `rust-version` | `"1.87"`（edition 2024 需 1.85；代码已用 `is_multiple_of`） |
| `rustfmt.toml` | 仅 `edition = "2024"`，其余官方默认 |
| Clippy | 闸门就是 `cargo clippy --all-targets -- -D warnings`（CI 与 AGENTS 同一条）。不另写一套 Cargo.toml clippy 表，避免和 `-D warnings` 分叉；不启用 pedantic/nursery |
| `deny.toml` | advisories：漏洞与 yanked 为 fail。licenses 允许：MIT、Apache-2.0、BSD-2-Clause、BSD-3-Clause、ISC、Unicode-3.0、Zlib、NCSA（实现时按 `cargo deny init` 结果只加实际需要的，不放宽到任意许可）。sources：只 crates.io |
| 不安全代码 | 不 forbid；Win32 DPAPI / 控制台 / 本地时间保持现有 `unsafe` |
| release | 现有 `lto = true`、`strip = true` 不动 |

`.github/workflows/ci.yml`：

- `windows-latest`：checkout → rust-toolchain → `fmt --check` → clippy `-D warnings` → `test --all-targets` → `build --release`
- `ubuntu-latest`：只 `cargo deny check`（不编译 Windows 代码）

触发：`push` / `pull_request` 到 `master`。

## 文档

- `AGENTS.md`：模块边界、验证命令、两个测试环境变量、CI 与本地一致
- `docs/spec.md`：只改过时的结构段（仍写 dialoguer / 旧 `src/*.rs` 列表）；功能条文不改
- `README.md`：验证步骤；不把测试环境变量写成用户功能
- `src/lib.rs` 与各层 `mod.rs`：短 `//!` 中文职责说明
- 不新增 CONTRIBUTING.md

## 实现阶段（同一分支、可 bisect；每步 `cargo test` 全绿）

**阶段 0 — 落规格。** 将本文件写入 `docs/superpowers/specs/2026-09-09-enterprise-architecture-design.md`。

**阶段 1 — 工程先绿。** toolchain、rustfmt、`rust-version`、lints、`deny.toml`、CI、`PCS_DATA_DIR` / `PCS_CONFIG_PATH`。若 `cargo fmt` 全仓有 diff，单独一次提交。

**阶段 2 — 库根。** 增加 `src/lib.rs`，现有 `mod` 原样迁入；`main.rs` 只解析并调用库。不改路径。

**阶段 3 — 错误类型。** 引入分层 `thiserror`，替换 `bail!` / `Result<_, String>`，Display 逐字一致；关键单测改 `matches!`。

**阶段 4 — 按层挪文件。** `domain/`、`persist/`、`launch/`、`cli/`。

**阶段 5 — 拆 TUI。** `tui/app.rs` → `tui/app/`；`App::handle` 测试跟着走。风险最高，单独提交。

**阶段 6 — 冒烟与文档。** `tests/cli_smoke.rs`；更新 AGENTS / spec 结构段 / README。

验证（每阶段结束与最终）：

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

最终另加：CI 工作流存在且命令与上表一致；`cargo deny check` 在 Ubuntu job 描述中。不在本机 Linux 上要求 `pcs` 链接通过。

## 成功标准

- 用户可观察行为与改造前一致（CLI 输出、TUI 按键语义、数据路径默认值、启动等待）
- `src/main.rs` 显著变薄；不再存在 3000+ 行的 `tui/app.rs` 或 1300+ 行的 `ops.rs`/`main.rs`
- 现有单测全部通过；新增至少 `--help` 与 `ls` 的二进制冒烟
- Windows CI 跑 fmt/clippy/test/release；deny 在 Linux job
- AGENTS.md 模块边界与仓库真实布局一致

## 风险与处理

- **搬家漏测：** 每阶段全量 `cargo test`；TUI 拆分单独提交，失败可回滚该提交
- **文案微差：** 错误 Display 从现 `bail!`/`Err(format!(...))` 复制，禁止顺手改句
- **环境变量误用：** 未设置时分支与现在完全相同；文档标明仅测试/CI
- **fmt 噪音：** 与逻辑提交分开
