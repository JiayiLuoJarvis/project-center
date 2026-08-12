# P2 最近打开与快速启动 — 设计规格

日期：2026-08-12
状态：已取消（2026-08-12 功能与代码整体移除，不再实现；本文档留档）
关联：P2 of 「产品功能/优化」批次（P3 数据安全、P4 项目自定义命令、P5 录入效率）

## 1. 背景与目标

日常使用中，打开项目需要先进入菜单 → 选分组 → 选项目 → 选启动方式，路径较长。高频项目应能「最近打开」直达；任意项目应能一条命令全局搜索即启。

P2 目标：

1. 最近打开（MRU）：记录实际启动过的项目，菜单顶层提供「最近打开」入口。
2. 全局快速启动 `pcs go`：一个模糊搜索框搜全部分组所有项目，回车即按默认方式启动。

非目标：数据备份与回收站（P3）、项目自定义命令（P4）、批量录入（P5）。

## 2. 架构与模块变更

```
recent.rs  新增模块：MRU 纯数据操作与原子持久化（不感知 Project/Group）
menu.rs    顶层「最近打开」入口 + 最近列表渲染
main.rs    `pcs go` 子命令 + 全启动成功路径记录 MRU
launcher.rs 不改（记录时机在调用方，launcher 保持纯净）
```

### 2.1 MRU 数据契约（recent.rs）

独立文件：`%APPDATA%\project_center\recent.json`；APPDATA 缺失回退 `%USERPROFILE%\.project_center\recent.json`（与 `Store::file_path` 同目录策略）。

```json
[{ "id": "uuid", "ts": 1723456789 }, ...]
```

- `Record { id: String, ts: i64 }`；`ts` 为 unix 秒，`#[serde(default)]` 缺失回退 0。
- 只存项目 id + 时间戳，不存路径/名称；项目被删后记录自然失效（渲染时按 id 查不到即跳过）。
- 上限 `MAX_RECENTS = 20`；`push` 同 id 去重（移除旧位置）、新记录插队首、截断上限，返回新 ts。
- `load()`：文件缺失/损坏/解析失败一律视为空；返回按 ts 倒序。
- `save()`：原子写（`.json.tmp` → 删旧 → rename），失败返回 false，调用方仅 stderr 警告。
- 为测试提供 `save_to(path)` / `load_from(path)`（`#[cfg(test)]` 暴露）。

### 2.2 记录时机

所有实际启动成功路径后记录（spawn 成功后、阻塞等待前）：

- `pcs wsl` / `pcs ps` / `pcs code`（CLI 直连）
- `pcs open` 带 `-w|-p|-c` 标志
- `pcs open` 无标志经菜单选择（取消不记录）
- 菜单「打开」动作（`project_actions`）

启动失败（launcher 返回 Err）不记录。封装为 `record_recent(project_id)`（load → push → save，写失败仅警告）。

### 2.3 菜单：「最近打开」入口

顶层 labels：`[分组…] | 最近打开 | 新增分组 | 配置 | 退出`。

- 进入后按 ts 倒序列出最近项目（label 复用项目列表样式 `名称 (路径)`，最多 20 条，跳过已不存在项目 id）。
- 选中项目 → 进入该项目操作页（复用 `project_actions`）。
- 列表尾部「清空最近记录」：Confirm 后 `save(&[])` 并提示。
- 空记录时提示「暂无最近打开记录」并返回。

渲染抽为纯函数 `recent_entries(data, records) -> Vec<((usize, usize), String)>`（便于单测）。

### 2.4 `pcs go` 全局快速启动

```text
pcs go
```

- 单一 FuzzySelect：选项 = 全部项目，label `分组名/项目名`（跨分组同名可区分），按 id 定位（`ops::find_project_by_id`）不歧义。
- 回车选中：若项目 `default_tool` 命中当前配置 → 直接按默认方式启动（复用 `menu::build_launch_options` + `default_option_index`）；否则弹单屏启动选择（复用 `menu::choose_launch_option`）。
- 取消（Esc）→ 退出，无副作用。
- 无项目时提示「暂无项目」，退出 0。
- 启动成功后按 §2.2 记录 MRU。

## 3. 数据契约

- `recent.json` 为新增独立文件，不触碰 `projects.json` / `config.json`。
- 启动时若 MRU 记录数量为 0，不创建文件（惰性创建）。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| recent.json 损坏/缺失 | 视为空记录，不报错 |
| recent.json 写盘失败 | stderr 警告，不中断启动 |
| 记录指向已删除项目 | 菜单渲染跳过；`pcs go` 不涉及 |
| `pcs go` 无项目 | 提示「暂无项目」，退出 0 |
| `pcs go` 启动失败 | 正常错误返回（CLI 退出码非零） |

## 5. 测试

- `recent.rs`：push 去重 + 截断上限、load 损坏回退空、save/load 往返（临时目录）、ts 倒序。
- `menu.rs`：`recent_entries` 过滤失效 id、倒序、label 构造。
- `main.rs`：`pcs go` 无项目路径。（交互选择无法自动化，真实终端手工验证。）
- `cargo test` / `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo build --release`。

## 6. 验证命令

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

部署到 `E:\dev_tool\pcs\pcs.exe`（需先结束运行中的 pcs.exe 进程）。
