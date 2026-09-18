# 可复用 SSH 远程连接 + 设置枢纽 — 设计文档

日期：2026-09-18  
状态：已与用户逐项确认  
关联：`docs/superpowers/specs/2026-09-08-ssh-projects-design.md`（SSH 项目与秘密存储基线）、`docs/spec.md`

## 1. 目标与范围

### 目标

1. **DataGrip 式远程连接**：主机/账号/认证抽成可复用的 `Connection`；多个项目共享同一连接，只各自保留远程路径。
2. **设置枢纽**：左栏只保留分组；回收站、启动工具配置、远程连接统一由快捷键打开的设置弹窗进入。
3. **项目表单滚动修复**：新增/编辑弹窗可把焦点字段（含底部密码类字段）完整滚入可见区。

### 非目标（本轮）

- 独立 CLI 子命令组 `pcs connection` / `pcs remote`（表面命令不变；`--ssh` 内部自动建/复用连接）。
- ProxyJump、端口转发、VS Code Remote-SSH、IPv6。
- 连接进回收站、连接级「最近使用」以外的复杂组织。

## 2. 数据模型

### 2.1 `Connection`（`projects.json` 顶层 `connections`）

| Rust 字段 | JSON 键 | 含义 |
|---|---|---|
| `id: String` | `id` | UUID；规则与项目 id 一致（大小写不敏感、唯一前缀可解析） |
| `name: String` | `name` | 显示名（如 `14.10`）；**大小写不敏感唯一** |
| `user: String` | `user` | 登录用户；可空（等价于仅 host） |
| `host: String` | `host` | 主机；必填 |
| `port: u16` | `port` | 默认 `22` |
| `ssh_key_file: String` | `sshKeyFile` | 相对数据根的 sidecar（`keys/<connectionId>.key`） |
| `ssh_key_path: String` | `sshKeyPath` | 导入来源路径，仅显示 |
| `ssh_password_enc: String` | `sshPasswordEnc` | 登录密码 DPAPI 密文 |
| `ssh_key_pass_enc: String` | `sshKeyPassEnc` | 私钥口令 DPAPI 密文 |
| `updated_at: String` | `updatedAt` | RFC3339；迁移冲突与「最近编辑」判定用 |

`ProjectData` 增加：

```text
connections: Vec<Connection>   // JSON: "connections", #[serde(default)]
```

`pendingKeyDeletes` 语义不变，引用对象改为连接上的 key 路径。

### 2.2 `Project` 变更

| 变更 | 说明 |
|---|---|
| 新增 `connection_id: String`（`connectionId`） | 非空且能解析到连接 ⇒ SSH 项目 |
| `path` | SSH 时仍为远程 Linux 路径 |
| **删除写出** `sshTarget` / `sshKeyFile` / `sshKeyPath` / `sshPasswordEnc` / `sshKeyPassEnc` | 迁移完成后不再序列化到项目 |

- `Project::is_ssh_project()` → `!connection_id.trim().is_empty()`（启动前再校验连接存在）。
- 本地项目：`connection_id` 为空；Windows/WSL 路径语义不变。
- `DeletedItem` 快照：保留 `connectionId` + 远程 `path`（及本地字段）；**不再**快照项目级 SSH 秘密字段。恢复后仍指向原连接；若连接已不存在（仅在异常/手改 JSON 时），恢复后打开报错，需用户重选连接。

### 2.3 查找与校验

- 连接按 `name` 或 `@<id>`（唯一前缀）查找；重名禁止。
- **删除连接**：任意 `groups` 或 `trash` 中的项目 `connectionId` 引用该连接 → **拒绝**，提示先改项目或删项目。
- 编辑连接名称：仅改显示名，id 不变；项目引用不受影响。
- 密钥 sidecar **以连接 id 命名**；孤儿清理引用集 = 所有连接的 `sshKeyFile`（连接本身不进 trash，软删项目只保留 id 引用，key 随连接存活）。

## 3. 迁移

在 `Store::load`（可信数据、非损坏兜底空数据）时执行，成功后立即写回：

1. 扫描 `groups` + `trash` 中仍带旧 `sshTarget`（或等价旧字段）的项目。
2. `split_ssh_target` → `(user, host, port)`。
3. **合并键**：规范化后的 `user@host:port`（port 缺省按 22）。
4. 同键多项目：
   - 合并为一条连接；
   - **认证冲突**（密码密文 / key 文件内容或路径 / 口令密文不一致）→ 取 **`updatedAt` 最新**的项目所带认证（无时间戳则按扫描顺序后者覆盖，并在写回前为连接写入当前时间）。
5. 连接 `name`：优先 `host`；重名则 `host (user)`，仍冲突则追加数字后缀。
6. 项目：写入 `connectionId`，清空旧 SSH 字段；`path` 保留为远程路径。
7. 密钥文件：若仍按**旧项目 id** 命名，迁移时复制/重命名为 `keys/<connectionId>.key` 并更新连接上的 `sshKeyFile`；合并后多余旧文件进入孤儿清理（受既有门控）。
8. 迁移幂等：已无旧字段则跳过。

旧版程序读新 JSON：未知顶层 `connections` 可忽略；但项目缺 `sshTarget` 会导致旧版不认 SSH——接受为单向升级（与既有 schema 演进一致）。

## 4. TUI：设置枢纽

### 4.1 左栏

- 仅分组列表（含过滤）。
- **移除**固定末尾「回收站」「配置」。

### 4.2 入口与导航

- Browse 下按 `,` 打开居中 **设置** 列表弹窗：
  1. 远程连接
  2. 回收站
  3. 启动工具
- Enter → 对应子视图；Esc：子视图 → 设置 → Browse。
- `?` 帮助仍为独立 `Mode::Help`；帮助文案更新快捷键说明。

### 4.3 子视图

| 入口 | 行为 |
|---|---|
| 远程连接 | 连接列表；`a` 新增、`e` 编辑、`d` 删除（有引用则 flash 拒绝）、`v` 查看秘密（PIN） |
| 回收站 | 现有恢复 / 单项删除 / `D` 清空 |
| 启动工具 | 现有环境 → 工具列表增删改 / 恢复默认 |

右栏/焦点模型可复用现有 `RightPane`，但进入路径改为设置栈，而不是左栏选中项。

## 5. TUI：项目表单与连接表单

### 5.1 项目新增/编辑

- **本地**：名称、别名、Win 路径、浏览文件夹、WSL 路径（不变）。
- **SSH**：字段收敛为：
  - `远程连接`：只读展示当前连接名；Enter 打开连接选择器；
  - `远程路径`。
- 不再在项目表单收集 user/host/port/密码/密钥。
- 判定 SSH：选了连接 ⇒ SSH；未选连接且无远程专用必填 ⇒ 本地（与「填了 host 才 SSH」旧逻辑对齐为「选了连接才 SSH」）。
- 提交时选了连接但连接 id 无效 → 错误。

### 5.2 连接选择器

- 列出全部连接（名称 + `user@host:port`）。
- 末项：**＋ 新建连接…**
- 选中已有 → 写回项目表单连接字段并关闭选择器。
- 选「新建」→ **压栈**打开连接表单；保存成功后 pop，项目表单自动选中新建连接；取消则回到选择器/项目表单且不改选中。

### 5.3 连接表单字段

名称、用户、主机、端口、密钥来源、浏览密钥、清除密钥（编辑）、密码、密钥口令。校验：host 必填；名称唯一；user/host 不含 `@` 等沿用现有 compose 规则。

### 5.4 表单滚动修复

- 模态高度仍约 60%；内容超出时用 `form_scroll_offset`。
- **保证焦点字段整块可见**（含底部 Password 行）：打开表单、Tab/方向键改焦点时重算；必要时允许滚到内容末尾，避免「焦点在密码但视口停在上部」。
- 回归：新增项目表单从首字段 Tab 到末字段，密码/口令行必须出现在可视区内。

## 6. 启动与秘密

### 6.1 启动

- `ssh_args` / `spawn_ssh`：从 **连接** 取 user/host/port/认证；从 **项目** 取远程 `path`。
- askpass 的 `PCS_ASKPASS_ID` 改为 **连接 id**（或同时可解析连接）；token 机制不变。
- 连接缺失：返回明确错误，不 panic。

### 6.2 秘密查看与 PIN

- 项目上 `v`：若为 SSH，展示**引用连接**的秘密（需 PIN）。
- 连接列表上 `v`：直接针对该连接。
- `pin reset --force`：清空**所有连接**的密码/口令/密钥文件，并清 PIN；不再遍历项目级 SSH 字段。

## 7. CLI（本轮兼容）

- 不新增 `pcs connection`。
- 现有 `pcs add`/`edit` 的 `--ssh`、`--ssh-key`、`--password-stdin` 等：**内部**解析目标 → 按 `user@host:port` 查找或创建连接 → 项目只存 `connectionId` + 路径。
- `pcs ssh` / `open -s`：经项目解析连接后启动。
- 列表展示：SSH 项目可显示连接名或 `user@host`（实现时与 `pcs ls` 现有列兼容即可）。

## 8. 模块边界（预期改动面）

| 区域 | 改动 |
|---|---|
| `domain/models.rs` | `Connection`、`ProjectData.connections`、项目字段、DeletedItem |
| `domain/` 新或扩 CRUD | 连接增删改查、引用计数、删除保护 |
| `persist/store.rs` | 加载迁移、写回；孤儿 key 引用集 |
| `persist/secret.rs` | key 路径按连接 id；PIN reset 扫连接 |
| `launch/` | 从连接拼 ssh 参数与 askpass id |
| `tui/` | 设置枢纽 Mode/栈；左栏瘦身；表单/选择器；滚动；帮助文案 |
| `cli/project.rs` 等 | `--ssh` 自动建/复用连接 |
| `docs/spec.md` | 产品说明同步 |

## 9. 测试清单

- 迁移：多项目同 `user@host:port` 合并为一条；认证冲突取最近编辑；写回后项目无旧 `sshTarget`。
- 删连接：有 groups/trash 引用 → 拒绝；无引用 → 成功且 key 删除/入 pending。
- 设置枢纽：`,` 打开；三项进入与 Esc 返回栈；左栏无回收站/配置。
- 项目表单：选连接保存；「＋ 新建连接」回填；无连接提交 SSH 失败提示。
- 滚动：焦点到底部密码字段时该行可见。
- CLI `--ssh` 自动建/复用。
- 回归：askpass prompt 分派、PIN、trash 软删/恢复/purge 与 key 生命周期、本地项目路径。

## 10. 验收

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

## 11. 已确认决策摘要

| 项 | 选择 |
|---|---|
| 字段切分 | 连接持有认证与 host；项目持有路径 + `connectionId` |
| 迁移合并 | 按 `user@host:port` 自动合并 |
| 认证冲突 | 取最近编辑 |
| 删连接 | 有引用禁止删 |
| 设置 UI | `,` → 居中列表 → 远程连接 / 回收站 / 启动工具 |
| 项目选连接 | 选择器 + 「＋ 新建连接…」压栈 |
| CLI | 本轮不暴露 connection 子命令；`--ssh` 内部复用 |
| 存储 | `projects.json` 顶层 `connections[]`（非独立文件） |
