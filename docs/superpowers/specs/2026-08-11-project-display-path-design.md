# 项目列表路径显示优化 — 设计规格

日期：2026-08-11
状态：已批准
关联：交互菜单项目列表与 `pcs ls` 的路径展示逻辑

## 1. 目标

当前项目列表在展示路径时存在空括号问题：

- `menu.rs` 项目选择列表使用 `format!("{}  ({})", name, path)` 生成标签，当项目没有 Windows 路径（仅 WSL 路径）时显示为 `name  ()`。
- `main.rs` 的 `pcs ls` 输出为 `name  path`，空路径时留下多余尾部空格。

优化目标：有 Windows 路径时照常显示 Windows 路径；没有 Windows 路径时回退显示 WSL/Linux 路径；两者皆空时只显示项目名，不渲染括号。

## 2. 行为规则

对每个项目按以下顺序决定展示内容：

1. `has_windows_path()` 为真 → 显示 `project.path`。
2. 否则显示 `project.linux_path()`（该函数已覆盖：优先手动 `wsl_path`，其次归一化 Linux `path`，再次 Windows→`/mnt/<盘符>/...` 转换）。
3. 二者结果皆为空串 → 只显示项目名，不带括号。

不引入新的辅助函数；复用 `models.rs` 已有的 `has_windows_path()` / `linux_path()`。

## 3. 改动位置

### 3.1 `src/menu.rs`（项目选择列表标签）

`src/menu.rs` 第 95-99 行，`group.projects.iter().map(...)` 生成标签处：

```rust
.map(|project| {
    let path = if project.has_windows_path() {
        project.path.clone()
    } else {
        project.linux_path()
    };
    if path.is_empty() {
        project.name.clone()
    } else {
        format!("{}  ({})", project.name, path)
    }
})
```

### 3.2 `src/main.rs`（`pcs ls` 输出）

`src/main.rs` 的 `print_group` 函数，项目行打印处：

```rust
let path = if project.has_windows_path() {
    project.path.clone()
} else {
    project.linux_path()
};
if path.is_empty() {
    println!("  {}", project.name);
} else {
    println!("  {}  ({})", project.name, path);
}
```

两处采用相同方案，格式统一为 `name  (path)`。

## 4. 非目标

- 不修改数据模型、序列化 schema、`Project` 字段。
- 不修改 `linux_path()` / `has_windows_path()` 的语义。
- 不改动启动器（launcher）的行为。

## 5. 错误处理与边界情况

- `path` 与 `wsl_path` 均为空 → `linux_path()` 返回空串，仅显示项目名。
- `path` 为 Linux 路径（`is_linux_path` 为真）→ `has_windows_path()` 为假，走 `linux_path()` 归一化分支，正常显示。

## 6. 测试与验证

`models.rs` 现有单元测试已覆盖 `linux_path()` / `has_windows_path()` 的路径转换行为，本次改动不触碰这些函数，无需新增数据层测试。验证命令：

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

人工验证：菜单中展示一个仅 WSL 路径的项目，确认显示为 `name  (/home/... )` 而非 `name  ()`；`pcs ls` 输出同步一致。

## 7. 风险

低风险。仅改动两处显示逻辑，不涉及持久化与启动链路。
