# 产品讨论清单与「最近打开」设计

日期：2026-09-20  
状态：第 1、2 项已落地（main）；其余为待讨论 backlog  
定位：`pcs` 是轻量书签式路径注册表 + 快速启动入口，不是全能项目管理器。

## 产品边界（共识）

- 不做拼音、全局 fuzzy（体积与心智成本不值；过滤继续子串；中文名靠 `alias`）。
- 不做 shell `cd` 内建；路径打印与脚本包装即可。
- 优先缩短「常用项目 → 已在对的环境」路径。

## Backlog（按优先级）

| # | 主题 | 状态 | 备注 |
|---|------|------|------|
| 1 | 最近打开快入口 | **已落地** | 见下文 |
| 2 | SSH 自定义命令 | **已落地** | 仅 `env=ssh` 一锤子命令；见 `2026-09-20-ssh-custom-commands-design.md`。connection CLI / Remote IDE 仍降级待议 |
| 3 | 轻量导入 / 迁机 | 待讨论 | `.git` 目录扫入或导出；旧 P5 已取消，需重筛 |
| 4 | 收藏或置顶 | 待讨论 | 项目变多后分组 alone 不够 |

曾否决或降级：独立 `pcs go`（本轮不做）、同项目多方式多行 MRU、拼音/fuzzy。

---

## 第 1 项：最近打开（已锁定）

### 目标

常用项目有限。Browse 一键打开最近弹窗，Enter 用**上次实际启动方式**直接 spawn，覆盖约 90% 日常开法。偶发换方式用 Space 分叉到现有 LaunchPicker。

### 键位

| 场景 | 键 | 行为 |
|------|-----|------|
| Browse，右栏非回收站 | `r` | 打开最近弹窗 |
| Browse，回收站 | `r` | 保持「恢复」（不变） |
| 最近弹窗 | `Enter` | 用该行记下的上次方式直接 spawn |
| 最近弹窗 | `Space` | 对当前行打开现有 LaunchPicker |
| 最近弹窗 | 可打印字符 / Backspace | 子串过滤（name / alias / 方式标签） |
| 最近弹窗 | `Esc` | 关闭，回 Browse |
| 最近弹窗 | `j`/`k` 或方向键 | 移动高亮 |

### 列表与去重

- 上限 **20**。
- **按 `project_id` 去重**：同项目只留一条；方式字段始终是最近一次**成功** spawn。
- 行展示大致：`分组 / 项目名 · 方式标签`（例如 `Work / pcs · WSL · grok`）。
- 项目已删或不存在：渲染时跳过，不挡列表。

### 记账

- 任意路径启动**成功**后写入或刷新（TUI 与 CLI 直连均记）。
- 启动失败不记账。
- 分叉后用新方式成功启动 → 该项目行的方式更新。

### 非目标（本能力）

- CLI `pcs go`、跨组全局搜、拼音、fuzzy。
- 改变 Browse 里对项目按 Enter 进 LaunchPicker 的既有行为（本轮不强制「默认直启」）。
- 同项目按多种方式各占一行。

### 数据形状（实现约束）

独立文件，与旧 P2 意向一致，放在数据根旁：

`recent.json`（与 `projects.json` 同目录；跟随 `PCS_DATA_DIR` / debug·release 数据根）。

建议记录字段：

```json
[
  {
    "id": "<project uuid>",
    "ts": 1723456789,
    "env": "wsl",
    "toolName": "grok",
    "command": "grok"
  }
]
```

- `env`：与 `LaunchEnv` 可互转的稳定字符串（`wsl` / `powershell` / `ide` / `explorer` / `ssh`）。
- `toolName` + `command`：足够在无配置漂移时重建启动；终端类 `command` 可为空。
- 加载失败 / 缺失 → 空列表。保存用与 Store 同类的原子写。
- `push`：同 id 移除旧条 → 插队首 → 截断 20。

### 与现有代码的衔接点

- TUI：`Mode` 新增最近弹窗态；`Outcome::Launch` 成功路径记账。
- 复用：`build_launch_options` / `LaunchPicker` / `do_launch`。
- CLI：`pcs wsl` / `ps` / `code` / `ssh` / `open -*` / `run` 成功后同样 `push`。

### 验证

- 领域/持久化单测：`push` 去重、上限、坏 JSON 空列表、原子路径可测。
- TUI 单测：非回收站 `r` 进弹窗；回收站 `r` 仍恢复；Enter / Space 映射。
- `cargo test`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`。
