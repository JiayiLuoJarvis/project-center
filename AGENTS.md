# AGENTS.md

`pcs` is a Windows-only Rust 2024 single-binary CLI. CLI output and source comments are Simplified Chinese; preserve that convention.

## Verify

- `cargo test` 跑模块内单测（`src/domain/`、`src/persist/`、`src/launch/`、`src/cli/`、`src/tui/app/`）以及 `tests/cli_smoke.rs`；`cargo test <filter>` 可聚焦。
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release` produces `target\release\pcs.exe` with LTO and stripping (see `[profile.release]`).
- CI：`.github/workflows/ci.yml`（`windows-latest` 跑 fmt/clippy/test/release；`ubuntu-latest` 跑 `cargo deny check`）。
- `docs/spec.md` is the feature reference, but code is authoritative if they differ (spec §7's "发起后即退出" is stale: the launcher now blocks on console children).

## Runtime

- `pcs` / `pcs menu` and `pcs open <name>` without `-w|-p|-c` are interactive **ratatui** fullscreen TUI (alternate screen) requiring a real Windows Terminal/conhost. Only `KeyEventKind::Press` is handled (Windows double-fire). Launch: WSL/PS restore→wait→re-enter TUI; IDE/explorer stay in TUI. `q`/`Ctrl+C` 先确认再 restore and exit.
- Dialog-free, scriptable commands: `ls`, `path`, `wslpath`, `group ...`, `add`/`edit`/`rm`/`mv` given explicit paths, and `open`/`wsl`/`ps`/`code` with an explicit flag. `add` opens the native `rfd` folder picker only when neither `--dir` nor `--wsl-path` is given. `pcs trash empty` requires `--force`. Avoid TUI/rfd commands in non-interactive verification.
- Launching uses installed `wsl.exe`, `powershell.exe`, and `cmd /c code` / `cmd /c cursor`. WSL/PowerShell run in the current console and the launcher waits for the child to exit while suppressing Ctrl+C (`SetConsoleCtrlHandler`); do not revert this to spawn-and-return. VS Code/Cursor spawn quietly.
- Copy the release binary to the user's chosen install directory only when deployment is requested.

## Data Contracts

- Runtime projects data depends on build profile (`cfg!(debug_assertions)`):
  - **release** (`cargo build --release` / deployed exe): `%APPDATA%\project_center\projects.json`, fallback `%USERPROFILE%\.project_center\projects.json` (or `HOME`).
  - **debug** (`cargo run` / `cargo build` / default `cargo test` binary): `%APPDATA%\project_center_dev\projects.json`, fallback `%USERPROFILE%\.project_center_dev\projects.json`.
  - Backups live under each data root's `backups\`. Dev must not use `cargo run --release` unless intentionally mutating production data. Tests use `load_from`/`save_to` with temporary paths; `config.json` stays next to the exe.
- 测试缝（不作为用户命令）：`PCS_DATA_DIR` 非空时作为数据根（`projects.json` / `backups\` / `keys\` / `keys_tmp\`）；`PCS_CONFIG_PATH` 非空时作为配置文件完整路径（读写该路径本身，不强制 basename 为 `config.json`）。未设置时路径规则不变。
- Every project has a UUID `id`. Any `<name>` argument also accepts `@<id>` (case-insensitive, unique prefix allowed); ids survive renames, and `Store::load` backfills missing ids on legacy data and writes back immediately. A project name starting with `@` is unreachable by name.
- Keep the legacy JSON schema: the WSL field is `wslPath` (`#[serde(rename = "wslPath")]`) and optional fields use `#[serde(default)]`.
- `Store::load` treats missing or corrupt JSON as empty data. `Store::save_to` must retain its atomic sequence: write `.json.tmp`, remove the old file, then rename.
- Project and group lookup is case-insensitive. A project name found in multiple groups requires `--group`; do not silently choose a match.

## Path Semantics

- `Project::linux_path()` prefers a non-empty manual `wsl_path`; otherwise Linux paths are normalized and Windows paths convert to `/mnt/<drive>/...`.
- Linux-only projects have an empty `path` and no usable Windows path, so PowerShell and VS Code must remain unavailable for them (`has_windows_path()`). Keep path behavior covered by `domain/models.rs` tests.

## Module Boundaries

- `src/main.rs`: 薄入口，调用 `pcs::cli::run()`，失败打印并 `exit(1)`。
- `src/lib.rs`: 库根；对外仅 `cli::run`。
- `src/cli/`: `mod.rs` 分发；`args.rs` clap 定义；`common` 共享查找/保存；`output` / `open` / `project` / `group` / `config` / `trash` / `pin` / `secret` 各子命令。
- `src/domain/`: 数据模型、查找、项目/分组/回收站/命令的内存 CRUD；领域错误 `thiserror`。
- `src/persist/`: `projects.json` 原子写、exe 旁 `config.json`、DPAPI/PIN/密钥 sidecar。
- `src/launch/`: 启动选项（原 `menu.rs`）与 WSL/PowerShell/IDE/SSH 进程启动。
- `src/tui/`: ratatui UI；`app/` 为状态机（browse/form/launch/filter 等），`actions` → domain/persist。
