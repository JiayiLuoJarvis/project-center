# P5 录入效率：批量导入 — 设计规格

日期：2026-08-12
状态：已取消（2026-08-12 功能与代码整体移除，不再实现；本文档留档）
关联：P5 of 「产品功能/优化」批次（P2 最近/快速启动已批准、P3 数据安全、P4 项目自定义命令）

## 1. 背景与目标

当前 `pcs add` 一次只能通过文件夹选择器录入一个项目。新环境/新代码目录成批出现时（如克隆多个仓库、新机器迁移），逐个录入成本高。

P5 目标：`pcs import` 批量注册项目——扫描目录树，将子目录批量加入指定分组。

非目标：增量同步（重复扫描时已存在同名项目自动跳过，不做差异更新）；CSV/文件导入导出。

## 2. 架构与模块变更

```
main.rs    CLI：pcs import --dir <路径> [--group <组>] [--recursive]
ops.rs     import_dirs 纯函数：目录枚举 + 去重 + 冲突跳过
models.rs  不变（复用 add_project / Project::new）
```

### 2.1 CLI 设计

```text
pcs import --dir <根目录> [--group <组名>] [--recursive]
```

- `--dir`（必填）：Windows 绝对路径（也接受 `\` 结尾）。目录不存在 → 报错退出（非零）。
- `--group`（必填）：目标分组，与 `pcs add` 的 `--group` 一致；分组不存在 → 报错退出（非零）。菜单入口在 Select 中选分组，行为相同。
- `--recursive`（可选）：默认只扫 `--dir` 的直接子目录；加 `--recursive` 递归全树。递归时项目名 = 相对路径（`sub/dir-name`），避免不同层级同名冲突。

### 2.2 导入规则（ops.rs）

- 对每个候选子目录：目录名作为项目名（trim 后非空）；`path` = 目录绝对路径；`wsl_path` 由 `path` 按既有规则换算（`/mnt/<drive>/...`，保留手工 wsl_path 覆盖能力 → 先自动换算，用户可后续 `pcs edit` 改）。
- 跳过规则（逐个报告，不整体失败）：
  - 名称与目标分组内现有项目同名（大小写不敏感）→ 跳过并提示「已存在，跳过」。
  - 目录无读取权限/枚举失败 → 警告并跳过。
  - 名称以 `.` 开头（隐藏目录）→ 跳过（避免把 `.git`/`.config` 当项目）。
  - junction/symlink 循环链接目录 → 警告并跳过（以 canonicalize 后的路径去重，防止无限递归崩溃）。
- 根目录统一 `canonicalize` 归一化为绝对路径（同时剥离 Windows `\\?\` 前缀）：相对 `--dir`、`\.`、`\..\`、尾部反斜杠写法下，导入项目的 `path` 始终是绝对路径、`wsl_path` 始终按 `/mnt/<drive>/...` 换算。
- 全部处理完打印汇总：`导入 N 个，跳过 M 个（K 个已存在）。`
- 每次成功导入一个项目即保存？否——**最后统一保存一次**（原子写一次，避免 N 次写盘）。

### 2.3 菜单联动

- 菜单「新增分组」层级下增加「批量导入目录」入口：Select 选分组 → 输入根目录（Input，支持粘贴路径）→ 询问是否递归（Confirm）→ 走同一 `import_dirs` 逻辑并保存。
- CLI 与菜单共用 `ops::import_dirs`。

## 3. 数据契约

- 不新增字段，不触碰 trash/recent/config。
- 项目 id 由 `Project::new` 生成（uuid），与既有录入一致。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| `--dir` 不存在 | 报错退出（非零） |
| 分组不存在 | 报错退出（非零） |
| 目录无权限 | 跳过该目录并警告，不中断整体 |
| 同名冲突 | 跳过并提示「已存在」，不覆盖 |
| 0 个候选目录 | 提示「未发现可导入的目录」 |
| 保存失败 | stderr 警告（沿用哲学），CLI 仍 0 退出；菜单内警告并继续，不终止会话 |

## 5. 测试

- `ops.rs`：import_dirs——直接子目录枚举、recursive 相对名、隐藏目录跳过、同名跳过、汇总计数、空目录、目录不存在。
- `main.rs`：`pcs import` 参数校验（缺 --dir、坏路径、坏分组）、汇总输出格式。
- 全量：`cargo test` / `fmt --check` / `clippy -D warnings` / `build --release`；菜单批量导入真实终端手工验证。

## 6. 验证命令

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

部署到 `E:\dev_tool\pcs\pcs.exe`（需先结束运行中的 pcs.exe 进程）。
