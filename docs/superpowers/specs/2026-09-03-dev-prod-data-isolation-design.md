# debug / release 项目数据目录隔离 — 设计规格

日期：2026-09-03  
状态：已实现  
关联：开发时 `cargo run` 与正式 `pcs.exe` 共用 `%APPDATA%\project_center\projects.json`，导致生产项目数据被开发操作覆盖。

## 1. 背景与目标

当前 `Store::file_path()` 固定指向：

- `%APPDATA%\project_center\projects.json`
- APPDATA 缺失时：`%USERPROFILE%\.project_center\projects.json`（或 `HOME`）

debug 与 release 二进制共用该路径。本地开发（增删改、清空、测试手测）会直接改写正式环境数据。

**目标：**

1. **仅 release 构建**（`cargo build --release` 及由此得到的 exe，不论安装目录）读写生产数据目录 `project_center`。
2. **debug 构建**（`cargo run` / `cargo build` / 默认 `cargo test` 链接的二进制）**禁止**使用生产目录，改用并列的 `project_center_dev`。
3. 单测继续使用 `load_from` / `save_to` 临时路径，不依赖真实 APPDATA。

**非目标：**

- 不改 `config.json` 位置（仍为 exe 同目录；debug/正式 exe 目录本就分离）。
- 不引入 `PCS_DATA_DIR` 等环境变量覆盖。
- 不自动恢复已被覆盖的生产 `projects.json`（备份恢复为运维步骤，另做）。
- 不改变 release 部署流程（仍按需复制 `pcs.exe` 到 `E:\dev_tool\pcs\`）。

## 2. 规则

| 场景 | 判定 | 数据根目录 | projects 路径 |
|---|---|---|---|
| `cargo build --release` / 部署的 release exe | `not(debug_assertions)` | `%APPDATA%\project_center\` | `...\projects.json` |
| `cargo run` / `cargo build` / debug exe | `debug_assertions` | `%APPDATA%\project_center_dev\` | `...\projects.json` |
| APPDATA 不可用 + release | 同上 | `%USERPROFILE%\.project_center\` | `...\projects.json` |
| APPDATA 不可用 + debug | 同上 | `%USERPROFILE%\.project_center_dev\` | `...\projects.json` |
| 单元测试 CRUD | 调用方传入路径 | 临时目录 | `load_from` / `save_to` |

- 判定方式：编译期 `cfg!(debug_assertions)`（或等价 `#[cfg]`）。编入二进制后与启动目录无关。
- 备份目录：始终为「当前数据根」下的 `backups\`（现有 `Store::save_to` 行为不变，只随 `file_path` 父目录切换）。
- `cargo run --release` / 直接运行 `target\release\pcs.exe` **仍写生产目录**（符合「release = 生产」）。

## 3. 架构与模块变更

仅改 `src/store.rs` 路径解析，外加文档。

```
store.rs   data_dir_name() + file_path() 使用 debug/release 不同目录名
AGENTS.md  Data Contracts 写明两套路径与开发约束
```

### 3.1 `store.rs`

- 新增（或内联）目录名选择，语义等价于：

```rust
fn data_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "project_center_dev"
    } else {
        "project_center"
    }
}
```

- HOME 回退目录名：

```rust
fn home_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        ".project_center_dev"
    } else {
        ".project_center"
    }
}
```

- `file_path()`：APPDATA 分支 `join(data_dir_name()).join("projects.json")`；回退分支 `join(home_dir_name()).join("projects.json")`。
- `load` / `save` / 备份 / 损坏恢复逻辑不变，全部基于 `file_path()` 的父目录。

### 3.2 测试

- 在 `store.rs` 增加针对路径的断言：默认 `cargo test`（debug）下 `file_path()` 的路径字符串包含 `project_center_dev`，且**不**把生产目录名 `project_center` 作为数据段误匹配时可用更稳的检查（例如 path 组件等于 `project_center_dev`，或 ends_with `project_center_dev\projects.json`）。
- **不要求** `cargo test --release` 专门断言生产路径（避免无谓 release 测试链；且现有测试不调用 `Store::load()`）。
- 现有 `load_from` / `save_to` 临时目录测试保持不变。

### 3.3 文档

- **必须**更新 `AGENTS.md` Data Contracts：
  - release → `%APPDATA%\project_center\projects.json`
  - debug → `%APPDATA%\project_center_dev\projects.json`
  - 明确：开发手测用 debug，勿用 `cargo run --release` 除非有意改生产数据。
- `docs/spec.md` 可选一行同步；以代码与 AGENTS 为准（既有约定）。

## 4. 数据契约

- 生产 JSON schema、字段名（`wslPath`、`trash` 等）、原子写与备份轮转规则**不变**。
- debug 与 release 是两套独立文件；不自动同步、不迁移。
- `config.json` 仍在 exe 旁：`E:\dev_tool\pcs\config.json` vs `target\debug\config.json`。

## 5. 错误处理

无新失败模式。目录不存在时仍由 `save_to` 的 `create_dir_all` 创建（现有行为）。

## 6. 风险与接受行为

| 行为 | 说明 |
|---|---|
| release 任意路径启动 | 读写生产 `project_center`（有意） |
| debug exe 拷到 `E:\dev_tool\pcs\` | 不改生产 projects；但会读写该目录 `config.json`（config 跟 exe 目录，既有语义） |
| `cargo test --release` 若误调 `Store::load()` | 会碰生产；现状测试只走临时路径，保持即可 |

## 7. 验证

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

手工（可选）：debug `pcs ls` 不出现生产项目（或仅 dev 目录内容）；release / 部署 exe 仍读 `project_center`。

## 8. 范围外：生产数据恢复

实现本规格**不**自动恢复。当前正式 `projects.json` 可能已被掏空；完整快照可能存在于：

- 仓库 `backups\2026-09-03-pre-ratatui\projects.json`
- `%APPDATA%\project_center\backups\projects-*.json`

恢复为单独运维步骤，需用户明确要求后再做。
