# P3 数据安全：备份与回收站 — 设计规格

日期：2026-08-12
状态：草稿
关联：P3 of 「产品功能/优化」批次（P2 最近/快速启动已批准、P4 项目自定义命令、P5 录入效率）

## 1. 背景与目标

`projects.json` 是用户唯一数据源，目前仅靠「损坏回退为空」兜底——一旦写坏或误删，数据无法找回。`config.json` 已有 `.bak` 自愈先例（损坏时备份后重建），但 projects 数据量大、价值高，需要更主动的保护。

P3 目标：

1. 自动备份：每次成功保存 projects.json 前，将旧文件轮转为带时间戳的备份，保留最近 N 份。
2. 回收站：删除项目/分组进入回收站（软删除），可恢复；支持手动清空与按保留期自动清理。
3. 损坏自愈升级：projects.json 损坏时，依次尝试「最新备份 → 空数据」，并在恢复时保留损坏原件（改名 `.corrupt`）。

非目标：跨设备同步、云备份（不引入网络依赖）；加密。

## 2. 架构与模块变更

```
store.rs   备份轮转（save 前旧文件 -> backups/YYYYMMDD-HHMMSS.json）+ 损坏恢复链（备份 -> 空）
ops.rs     remove_project/remove_group 改为软删除（进 trash）；恢复/清空操作
models.rs  ProjectData 新增 trash 字段（序列化名 "trash"），DeletedItem 结构
menu.rs    顶层「回收站」入口 + 恢复/清空流程
main.rs    CLI：pcs trash list / trash restore / trash empty
```

### 2.1 自动备份（store.rs）

- 备份目录：`<数据目录>\backups\`（与 projects.json 同目录策略，APPDATA 或 USERPROFILE 回退）。
- 时机与规则：每次 `Store::save`（CLI/菜单任何写入）成功**前**，把当前 `projects.json` 复制为 `backups\projects-YYYYMMDD-HHMMSS.json`；**仅当**当前文件存在且非空（避免把空数据轮转成备份、避免首次创建时产生无谓备份）。
- 文件名时间戳用 **UTC**（单调、跨时区/夏令时安全）；同一秒内多次保存追加 `-N` 后缀（`projects-...-2.json`），快照互不覆盖。
- 轮转上限 `MAX_BACKUPS = 10`：写入新备份后按文件名时间序删除最旧的多余备份。
- 失败语义：备份失败不影响保存本身（stderr 警告），延续「仅警告」哲学。
- 用 `std::fs::copy` 而非 rename（备份是快照，原文件仍参与本次原子写序列）。
- 测试注入：`backup_to(dir)` / `prune_backups(dir, max)` 抽为纯函数（`#[cfg(test)]` 暴露路径参数版本），保存路径用临时目录。

### 2.2 回收站（models.rs / ops.rs）

`ProjectData` 新增：

```json
"trash": [
  { "id": "uuid", "type": "project", "group": "Work",
    "name": "old-app", "path": "E:\\...", "wslPath": "/mnt/e/...",
    "defaultTool": "opencode", "deletedAt": 1723456789 },
  { "id": "uuid2", "type": "group", "group": "",
    "name": "Archive", "projects": [ ...完整项目数组... ],
    "deletedAt": 1723456790 }
]
```

- 字段：`type`（project/group）、被删对象完整数据快照（删除时的 JSON，含 id/name/path/wslPath/defaultTool）、`deletedAt`（unix 秒）。
- `remove_project` / `remove_group`（含 by_id 变体）：默认软删除——克隆快照进 `trash` 并保留 `deletedAt`，再从原处移除；原返回值语义不变（返回被删项目/分组对象）。
- `rm --force` / 菜单「彻底删除」：真删（不经过回收站），现有调用点语义不变。
- 恢复规则：
  - 项目恢复：回到原分组 `group`；若该分组已不存在 → 优先恢复到同名分组（大小写不敏感），否则第一个分组，并提示去向。
  - 分组恢复：整组恢复（含内部项目原 id），若已有同名分组 → 恢复为 `原名(恢复)`。
  - 恢复时若 id 冲突（原 id 已被占用，如恢复同名分组时）→ 重新生成 id（`ensure_id` 语义）。
- 清空：`trash empty` 直接丢弃全部；回收站为软删，恢复后从 trash 移除。
- 保留期自动清理：`TRASH_RETENTION_DAYS = 30`，加载时清理 `deletedAt` 超过保留期的项（`deletedAt <= 0` 的旧数据不清理），有清理则写回。

### 2.3 损坏自愈升级（store.rs）

`load_from_inner` 失败路径改为：

1. 当前文件损坏（JSON 解析失败）→ 改名带时间戳的 `projects-<utc>.corrupt`（保留现场，多份损坏原件各自保留不覆盖）→ 从新到旧遍历备份，**取最新可解析的一份**（最新备份损坏/截断时仍可回退更早快照）；备份全不可用 → 空数据。
2. 恢复成功后**立即写回主文件**（与 id 回填一致，只读命令也能固化恢复结果）。
3. 文件缺失 → 空数据（现状不变，不走备份链）。

### 2.4 菜单与 CLI

菜单顶层 labels：`[分组…] | 最近打开 | 回收站 | 新增分组 | 配置 | 退出`。

回收站菜单：列出回收站内容（`[项目] 组名/名称 (删除于 2026-08-12)`、`[分组] 分组名 (N 个项目)`），选中 → 「恢复 / 彻底删除 / 返回」；顶部「清空回收站」（Confirm 后 `trash empty`）。空回收站提示「回收站为空」并返回。

CLI：

```text
pcs trash list            # 列出回收站内容
pcs trash restore <id>    # 恢复指定项（@<id> 前缀语义同项目）
pcs trash empty           # 清空回收站（--force 免确认）
```

删除 CLI `pcs rm` 增加 `--force` 彻底删除（菜单「删除分组」已有 force 参数，对齐命名）。

## 3. 数据契约

- `projects.json` 新增可选 `trash` 数组（`#[serde(default)]`，旧数据回退空）。
- 备份文件为完整 projects.json 快照，schema 与主文件一致。
- 主文件字段不变（`wslPath`、`defaultTool`、`#[serde(default)]` 全保留）。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| 备份失败 | stderr 警告，保存照常 |
| 主文件损坏 | 改名带时间戳 `.corrupt` 保留现场；回退最新可用备份；无可用备份回退空；恢复结果立即写回 |
| 恢复时原分组已删除 | 恢复至第一个分组并提示 |
| 恢复时重名/重 id | 分组/项目重名循环追加「(恢复)」「(恢复)2」…；id 冲突重新生成 |
| 回收站为空 | 提示「回收站为空」 |
| trash 文件损坏（随主文件） | 随主文件自愈链处理（trash 字段 serde 回退空）；回收站项/分组快照内项目缺 id 时加载回填 |

## 5. 测试

- `store.rs`：备份轮转（上限、跳过空文件）、损坏恢复链（主文件坏 → 取最新备份）、`.corrupt` 改名。
- `ops.rs`：软删进 trash（含 by_id）、force 真删、项目恢复（原分组存在/不存在/重名）、分组恢复（含项目 id 保留）。
- `models.rs`：trash 序列化往返 + 旧数据回退。
- `main.rs`：trash 三个子命令接线（list 输出格式、restore 不存在的 id 报错）。
- 全量：`cargo test` / `fmt --check` / `clippy -D warnings` / `build --release`；菜单流程真实终端手工验证。

## 6. 验证命令

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

部署到 `E:\dev_tool\pcs\pcs.exe`（需先结束运行中的 pcs.exe 进程）。
