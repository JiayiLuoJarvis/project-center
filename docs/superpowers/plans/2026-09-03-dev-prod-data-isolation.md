# debug / release 数据目录隔离 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** debug 构建读写 `%APPDATA%\project_center_dev\`，release 构建继续读写生产 `%APPDATA%\project_center\`，避免开发覆盖正式 projects.json。

**Architecture:** 在 `Store::file_path()` 用编译期 `cfg!(debug_assertions)` 选择目录名；备份/加载/保存逻辑不变，仅父目录切换。

**Tech Stack:** Rust 2024、现有 `store.rs` 单测。

## Global Constraints

- CLI 输出与源码注释：简体中文。
- 不改 `config.json`（仍 exe 同目录）。
- 不引入环境变量覆盖。
- 不自动恢复生产数据。
- 测试用 `load_from`/`save_to` 临时路径，不依赖真实 APPDATA。

---

### Task 1: store 路径隔离 + 测试 + AGENTS

**Files:**
- Modify: `src/store.rs`
- Modify: `AGENTS.md`
- Spec: `docs/superpowers/specs/2026-09-03-dev-prod-data-isolation-design.md`

- [x] **Step 1: 在 `store.rs` 增加目录名辅助函数并改 `file_path`**

```rust
fn data_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "project_center_dev"
    } else {
        "project_center"
    }
}

fn home_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        ".project_center_dev"
    } else {
        ".project_center"
    }
}
```

`file_path` 的 APPDATA 分支 `join(data_dir_name())`，HOME 回退 `join(home_dir_name())`；更新 doc comment。

- [x] **Step 2: 增加 debug 路径单测**

```rust
#[test]
fn file_path_uses_dev_dir_under_debug() {
    let path = Store::file_path();
    let text = path.to_string_lossy();
    assert!(
        text.contains("project_center_dev"),
        "debug build must use project_center_dev, got {text}"
    );
    assert!(
        path.ends_with("projects.json"),
        "expected projects.json suffix, got {text}"
    );
}
```

- [x] **Step 3: 更新 AGENTS.md Data Contracts**

写明 release → `project_center`，debug → `project_center_dev`；开发手测用 debug，勿随意 `cargo run --release`。

- [x] **Step 4: 验证**

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [x] **Step 5: 将 spec 状态改为已批准/已实现（可选）**
