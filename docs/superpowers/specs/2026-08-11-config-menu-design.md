# config.json 菜单内管理 — 设计规格

日期：2026-08-11
状态：已批准
关联：v2.5 引入的 `config.json` 启动配置加载（config.rs）

## 1. 目标

在 `pcs` 交互菜单中提供 `config.json` 的查看与修改能力，避免手动编辑文件：

- 顶级菜单新增「配置管理」入口。
- 按环境（WSL / PowerShell / IDE）列出、新增、编辑、删除工具。
- 支持一键「恢复默认配置」。
- 改动即时生效于当前菜单会话，并写回 `config.json`。

非目标：不提供 CLI 子命令（仅菜单）；不做配置文件并发控制。

## 2. 架构与模块变更

```
main.rs    let mut config = Config::load();            # 改为可变，传入 &mut
menu.rs    新增 config_menu(&mut AppConfig, theme)      # 顶级菜单第三入口
config.rs  纯函数工具操作 + 保存能力
```

- `config.rs` 新增纯函数操作（仿照 `ops.rs`，便于单测）：`add_tool` / `edit_tool` / `remove_tool`（按环境 + 索引），统一复用 `validate_tool` 校验规则：name 非空、非保留名「终端」、command 非空且不含空白、同环境内 name 不重复。
- `config.rs` 新增 `Config::save()`：写 exe 同目录 `config.json`，与 `Store` 一致采用原子写（`.tmp` → 删旧 → rename）；失败仅 stderr 警告，不中断（延续现有「仅警告」哲学）。
- `main.rs`：`let mut config = Config::load();` 传入 `menu::run`。菜单就地修改内存中的 `AppConfig`，`choose_tool` 等下次读取立即看到新工具，无需重启。

### 2.1 模块职责

| 单元 | 职责 | 依赖 |
|---|---|---|
| `config.rs` 纯函数 | 工具增删改、校验、恢复默认、保存 | 无（纯数据） |
| `config.rs` `Config::save` | 原子写盘 | std::fs |
| `menu.rs` `config_menu` | 交互流程编排 | dialoguer、config.rs |
| `menu.rs` 现有选择逻辑 | 读取更新后的 config 选择工具 | config.rs |

## 3. 菜单流程

顶级菜单（分组选择）变为：

```
[分组…] | 新增分组 | 配置管理 | 退出
```

「配置管理」子菜单：

```
WSL 工具
PowerShell 工具
IDE 工具
恢复默认配置
返回
```

- 选任一环境 → 列出当前工具（`名称 (command)`）+「新增工具」+「返回」；选择某个工具 → 「编辑 / 删除 / 返回」。
- 编辑：`name`、`command` 默认填充当前值，直接回车保留。
- 新增：输入 `name` 与 `command`，校验失败（空、保留名、含空白、重名）报错并留在当前层，不保存。
- 删除：`Confirm` 确认后移除。
- 恢复默认配置：`Confirm` 确认后三环境全部重置为 `AppConfig::defaults()`。
- 每次成功改动立即写回 `config.json`；空环境显示「（暂无工具）」仍可新增。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| 输入校验失败 | 报错信息，留在当前菜单层，不保存 |
| 写盘失败（exe 目录只读） | stderr 警告，本次内存修改仍生效，菜单继续 |
| 保存期间文件被外部改动 | 按现有 `load` 语义处理，不做并发控制 |

## 5. 测试

- `config.rs` 单测：add / edit / remove 各环境、重名 / 保留名 / 空白命令拒绝、恢复默认、`save → load` 往返一致（临时目录）。
- 校验规则复用现有 `validate_tool` 测试思路。
- 菜单交互无法自动化，按 AGENTS.md 在真实终端手工验证。

## 6. 验证命令

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`
