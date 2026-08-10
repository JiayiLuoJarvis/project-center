# Project Tab Title Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set the current Windows Terminal/conhost title to `项目名 - 分组名` whenever WSL or PowerShell is launched.

**Architecture:** Preserve the existing launcher behavior and pass the canonical group name alongside the selected project through menu and CLI dispatch. The Windows-only launcher will set the current console title immediately before starting WSL or PowerShell; VS Code remains unchanged.

**Tech Stack:** Rust 2024, Windows console API, existing `clap` and process-launching code.

## Global Constraints

- CLI output and source comments remain Simplified Chinese.
- WSL and PowerShell continue to run in the current console and wait for exit.
- Project and group lookup remains case-insensitive and ambiguous project names still require `--group`.
- Linux-only projects remain unavailable for PowerShell and VS Code.
- No persisted-data schema changes.

---

### Task 1: Propagate Group Name and Set Launch Title

**Files:**
- Modify: `src/launcher.rs`
- Modify: `src/menu.rs`
- Modify: `src/main.rs`

**Interfaces:**
- `launcher::launch(project: &Project, group_name: &str, kind: LauncherKind)` sets the title for WSL and PowerShell.
- `menu::launch_direct(project: &Project, group_name: &str, kind: LauncherKind)` forwards the group name.
- CLI project selection returns the selected project and its canonical group name.

- [ ] **Step 1: Add a title helper and pass group names through all launch paths**

Build the title from the stored project and group names, set the current Windows console title using Unicode-safe Windows API calls, and call it only for WSL and PowerShell. Update menu and direct CLI dispatch so both paths use the same launcher interface.

- [ ] **Step 2: Run formatting and focused compilation checks**

Run: `cargo fmt --check`

Expected: formatting check passes.

Run: `cargo check`

Expected: the crate compiles without errors.

### Task 2: Verify Existing Behavior

**Files:**
- Test: `src/launcher.rs` (inline unit tests if a pure title helper is introduced)

- [ ] **Step 1: Run the complete test and lint suite**

Run: `cargo test`

Expected: all existing tests pass.

Run: `cargo clippy --all-targets -- -D warnings`

Expected: no warnings or errors.

- [ ] **Step 2: Build the release binary**

Run: `cargo build --release`

Expected: `target\\release\\pcs.exe` is produced successfully.

- [ ] **Step 3: Manually verify the title in a real Windows Terminal/conhost**

Run: `cargo run -- wsl <项目名> --group <分组名>` and `cargo run -- ps <项目名> --group <分组名>` with a test project.

Expected: the active tab/window title becomes `项目名 - 分组名` while the selected shell is running. A shell profile that emits its own title may overwrite it.
