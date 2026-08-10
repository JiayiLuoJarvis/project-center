# AGENTS.md

`pcs` is a Windows-only Rust 2024 single-binary CLI. CLI output and source comments are Simplified Chinese; preserve that convention.

## Verify

- `cargo test` runs the inline unit tests in `models.rs`, `ops.rs`, and `store.rs`; use `cargo test <filter>` for a focused test.
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release` produces `target\release\pcs.exe` with LTO and stripping enabled.
- The repository has no integration-test or CI configuration. `docs/spec.md` is the feature reference, but code is authoritative if they differ.

## Runtime

- `pcs` and `pcs menu` require a real Windows Terminal/conhost because `dialoguer` is interactive; commands such as `cargo run -- ls`, `cargo run -- path <name>`, and `cargo run -- wsl <name>` are scriptable.
- Launching uses installed `wsl.exe`, `powershell.exe`, and `cmd /c code`; WSL and PowerShell run in the current console, while VS Code is spawned quietly.
- Folder selection uses a native Windows `rfd` dialog. Avoid invoking dialog-based commands in non-interactive verification.
- The usual deployment target is `E:\dev_tool\pcs\pcs.exe`; copy the release binary there only when deployment is requested.

## Data Contracts

- Runtime data is `%APPDATA%\project_center\projects.json`, falling back to `%USERPROFILE%\.project_center\projects.json` (or `HOME` if neither is available). CLI/menu mutations affect this real file; tests use `load_from`/`save_to` with temporary paths.
- Keep the legacy JSON schema: the WSL field is `wslPath` (`#[serde(rename = "wslPath")]`) and optional fields use `#[serde(default)]`.
- `Store::load` treats missing or corrupt JSON as empty data. `Store::save_to` must retain its atomic sequence: write `.json.tmp`, remove the old file, then rename.
- Project and group lookup is case-insensitive. A project name found in multiple groups requires `--group`; do not silently choose a match.

## Path Semantics

- `Project::linux_path()` prefers a non-empty manual `wsl_path`; otherwise Linux paths are normalized and Windows paths convert to `/mnt/<drive>/...`.
- Linux-only projects have no usable Windows path, so PowerShell and VS Code must remain unavailable for them. Keep path behavior covered by `models.rs` tests.

## Module Boundaries

- `src/main.rs`: clap definitions and CLI dispatch.
- `src/menu.rs`: short-lived interactive CRUD, launcher choice, confirmation, and menu saves.
- `src/ops.rs`: pure CRUD and unambiguous name lookup.
- `src/models.rs`: data model, legacy serialization, and path conversion.
- `src/store.rs`: JSON loading and atomic persistence.
- `src/launcher.rs`: WSL, PowerShell, and VS Code process launching.
