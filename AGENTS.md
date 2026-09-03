# AGENTS.md

`pcs` is a Windows-only Rust 2024 single-binary CLI. CLI output and source comments are Simplified Chinese; preserve that convention.

## Verify

- `cargo test` runs inline unit tests in `src/main.rs`, `src/launcher.rs`, `src/models.rs`, `src/ops.rs`, and `src/store.rs`; use `cargo test <filter>` for a focused test.
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release` produces `target\release\pcs.exe` with LTO and stripping (see `[profile.release]`).
- No integration tests or CI. `docs/spec.md` is the feature reference, but code is authoritative if they differ (spec §7's "发起后即退出" is stale: the launcher now blocks on console children).

## Runtime

- `pcs` / `pcs menu` and `pcs open <name>` without `-w|-p|-c` are interactive **ratatui** fullscreen TUI (alternate screen) requiring a real Windows Terminal/conhost. Only `KeyEventKind::Press` is handled (Windows double-fire). Launch: WSL/PS restore→wait→re-enter TUI; IDE/explorer stay in TUI. `q`/`Ctrl+C` 先确认再 restore and exit.
- Dialog-free, scriptable commands: `ls`, `path`, `wslpath`, `group ...`, `add`/`edit`/`rm`/`mv` given explicit paths, and `open`/`wsl`/`ps`/`code` with an explicit flag. `add` opens the native `rfd` folder picker only when neither `--dir` nor `--wsl-path` is given. `pcs trash empty` requires `--force`. Avoid TUI/rfd commands in non-interactive verification.
- Launching uses installed `wsl.exe`, `powershell.exe`, and `cmd /c code` / `cmd /c cursor`. WSL/PowerShell run in the current console and the launcher waits for the child to exit while suppressing Ctrl+C (`SetConsoleCtrlHandler`); do not revert this to spawn-and-return. VS Code/Cursor spawn quietly.
- The usual deployment target is `E:\dev_tool\pcs\pcs.exe`; copy the release binary there only when deployment is requested.

## Data Contracts

- Runtime projects data depends on build profile (`cfg!(debug_assertions)`):
  - **release** (`cargo build --release` / deployed exe): `%APPDATA%\project_center\projects.json`, fallback `%USERPROFILE%\.project_center\projects.json` (or `HOME`).
  - **debug** (`cargo run` / `cargo build` / default `cargo test` binary): `%APPDATA%\project_center_dev\projects.json`, fallback `%USERPROFILE%\.project_center_dev\projects.json`.
  - Backups live under each data root's `backups\`. Dev must not use `cargo run --release` unless intentionally mutating production data. Tests use `load_from`/`save_to` with temporary paths; `config.json` stays next to the exe.
- Every project has a UUID `id`. Any `<name>` argument also accepts `@<id>` (case-insensitive, unique prefix allowed); ids survive renames, and `Store::load` backfills missing ids on legacy data and writes back immediately. A project name starting with `@` is unreachable by name.
- Keep the legacy JSON schema: the WSL field is `wslPath` (`#[serde(rename = "wslPath")]`) and optional fields use `#[serde(default)]`.
- `Store::load` treats missing or corrupt JSON as empty data. `Store::save_to` must retain its atomic sequence: write `.json.tmp`, remove the old file, then rename.
- Project and group lookup is case-insensitive. A project name found in multiple groups requires `--group`; do not silently choose a match.

## Path Semantics

- `Project::linux_path()` prefers a non-empty manual `wsl_path`; otherwise Linux paths are normalized and Windows paths convert to `/mnt/<drive>/...`.
- Linux-only projects have an empty `path` and no usable Windows path, so PowerShell and VS Code must remain unavailable for them (`has_windows_path()`). Keep path behavior covered by `models.rs` tests.

## Module Boundaries

- `src/main.rs`: clap definitions and CLI dispatch.
- `src/menu.rs`: pure launch-option helpers (`LaunchOption` / `build_launch_options` / `default_first`) shared by TUI and tests.
- `src/tui/`: ratatui UI (`mod` lifecycle, `app` state machine, `ui`/`theme`/`widgets`, `actions` → ops/store/config).
- `src/ops.rs`: pure CRUD and unambiguous name/id lookup.
- `src/models.rs`: data model, legacy serialization, and path conversion.
- `src/store.rs`: JSON loading, id backfill, and atomic persistence.
- `src/launcher.rs`: WSL, PowerShell, and VS Code/Cursor process launching.
- `src/config.rs`: exe-dir `config.json` tools lists.
