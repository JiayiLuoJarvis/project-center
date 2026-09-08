# SSH 远程项目与秘密存储 — 设计文档

日期：2026-09-08
状态：已与用户逐项确认，待实施
关联：`docs/spec.md`（§7 启动表实施时补充 SSH 一节）

## 1. 目标与范围

在 `pcs` 中新增「SSH 远程项目」：

- SSH 是一种**新的项目类型**（非现有项目的附加启动方式）：没有本地/WSL 路径，只有 SSH 目标 + 可选远程路径。
- 打开即用 Windows 自带 OpenSSH（`ssh.exe`）登录；若有远程路径则尝试 `cd`，**路径不存在/错误时静默降级到默认 shell，绝不中断连接、不 panic**。
- 支持保存密码、私钥口令（passphrase）、私钥文件，实现自动登录；**秘密一律不明文落盘**。
- 密码/口令支持「查看」功能，查看前必须通过用户设置的 PIN；PIN 不可找回，忘记则重置并清空全部秘密。
- 非目标：VS Code Remote-SSH、`env=ssh` 自定义命令、端口转发、密钥导出、多跳/ProxyJump。

## 2. 数据模型（src/models.rs）

`Project` 新增字段（全部 `#[serde(default)]`，旧数据零迁移）：

| Rust 字段 | JSON 键 | 含义 |
|---|---|---|
| `ssh_target: String` | `sshTarget` | 如 `user@172.16.14.10` 或 `user@host:2222`（可选 `:端口` 后缀）；非空即 SSH 项目 |
| `ssh_key_file: String` | `sshKeyFile` | 密钥 sidecar 文件的**相对路径**（相对数据根，如 `keys/<uuid>.key`）；非空且文件存在即有密钥 |
| `ssh_key_path: String` | `sshKeyPath` | 导入来源路径，仅显示用，明文无秘密 |
| `ssh_password_enc: String` | `sshPasswordEnc` | 登录密码 DPAPI 密文（base64） |
| `ssh_key_pass_enc: String` | `sshKeyPassEnc` | 私钥口令 DPAPI 密文（base64） |

- `path` 字段复用为**远程 Linux 路径**（`/opt/foo`）。以 `/` 开头天然满足 `!has_windows_path()`，PowerShell/IDE/Explorer 自动不可用，无需额外判断。
- 新增 `Project::is_ssh_project() -> bool`：`!ssh_target.is_empty()`。
- `projects.json` **顶层**新增 `pending_key_deletes: Vec<String>`（`pendingKeyDeletes`，`#[serde(default)]`）：删除失败的 key 文件相对路径，延迟重试。

### 秘密一律不进 projects.json

- 密码、口令：DPAPI 密文（见 §4），换机器/换 Windows 用户解不开。
- 私钥：不嵌 JSON（避免 JSON 膨胀），存独立加密文件（见 §5）。
- PIN：只存校验哈希，且在 `config.json`（见 §6），不在 projects.json。

## 3. 启动器（src/launcher.rs）

### LaunchEnv 与参数

- `LaunchEnv` 新增 `Ssh` 变体：`label()` = `SSH`，`short_label()` = `ssh`。
- `ssh_args(project) -> Vec<String>`：
  - 目标解析：`user@host:port` → `ssh [-p port] user@host`；无端口 → `ssh user@host`。
  - 有 `ssh_key_file`：解密 sidecar 落临时文件后追加 `-i <临时文件>`。
  - 有远程 path：追加 `-t "cd '<path>' 2>/dev/null; exec $SHELL"`（路径内单引号转义为 `''`）。
  - **路径容错**：用 `;` 而非 `&&`，cd 失败（不存在/无权限）时错误被 `2>/dev/null` 吞掉，`exec $SHELL` 照常落在用户默认 shell（home），连接保持。
- `spawn_ssh()`：`Command::new("ssh")`，在当前控制台运行，返回 `Some(child)`，走现有 `wait_direct`（Ctrl+C 抑制），与 WSL/PowerShell 行为一致；启动前 `set_console_title` 照常。
- `spawn_direct` 分支：SSH 项目仅允许 `LaunchEnv::Ssh`，其他环境报错「SSH 项目不支持该启动方式」；`ssh.exe` 不存在/启动失败返回 `Err`（不 panic）。
- ssh.exe 缺失或 OpenSSH 版本过老不支持 `SSH_ASKPASS_REQUIRE`：env 被忽略，自动退化为交互输密码，功能不损坏。

### askpass 自动填充

pcs 是单二进制，**自己充当 SSH_ASKPASS 程序**（Windows OpenSSH ≥ 8.6 支持 `SSH_ASKPASS_REQUIRE=force`）：

1. `pcs` 启动 ssh 前：若 `sshPasswordEnc` 或 `sshKeyPassEnc` 非空，尝试解密；**解密失败一律降级为不注入 env（回退交互输密码），绝不 panic、绝不断连**。
2. 解密成功则注入 env：`SSH_ASKPASS=<pcs.exe 自身路径>`、`SSH_ASKPASS_REQUIRE=force`、`PCS_ASKPASS_ID=<项目uuid>`、`PCS_ASKPASS_TOKEN=<随机 32 字节 hex>`。**env 中只有 id 与 token，无明文秘密**。
3. ssh 请求秘密时回调 `pcs __askpass`（隐藏子命令，见 §8）：校验 token → 按 prompt 分派 → DPAPI 解密 → 输出到 stdout。
4. **prompt 分派规则**：prompt 含 `password`/`密码` → 回密码；含 `passphrase` → 回口令；**其余（host key 的 yes/no 等）一律输出空，绝不代答**，避免绕过 TOFU 确认。
5. token 缺失/不匹配 → 输出空。防护目标：杜绝「直接敲一行 `pcs __askpass <id>` 拿明文」。诚实边界：同 Windows 用户的进程理论上可读 ssh 子进程 env，但该用户本就在 DPAPI 信任边界内（直接跑 `pcs ssh` 也能登录）。
6. 密码错误时 ssh 正常报 `Permission denied` 退出/重试，pcs 只等待子进程，不干预。

### 临时密钥文件生命周期

1. SSH 启动前：清理 `keys_tmp\` 残留（幂等）。
2. 解密 sidecar → 写 `<数据根>\keys_tmp\<随机名>`（数据根而非系统 TEMP，减少被清理工具/同步盘扫到的面）。
3. `ssh -i <临时文件>`；`wait_direct` 返回后（成功/失败/中断都走）**覆写内容后删除**。
4. 即使进程崩溃，残留窗口止于「下次使用 pcs 为止」，且文件在 ssh 退出前本就以明文形态存在于磁盘（任何 ssh 客户端无法避免）。

## 4. 秘密模块（新 src/secret.rs）

密码学边界收口于此：

- `protect(plain) -> Result<String>` / `unprotect(b64) -> Result<String>`：Windows DPAPI（CryptProtectData / CryptUnprotectData，经 `windows` crate 现有依赖体系）。密文绑定「当前用户 + 本机」。选用 DPAPI 而非自实现「盐+机器码派生 AES」：等价于机器绑定且额外绑定用户，密钥由系统管理不落盘（业界主流：Tabby/Credential Manager、Tunnex、Mshell 均如此）。
- 明文缓冲用 `zeroize` crate 主动擦除（新增纯 Rust 小依赖）。
- key sidecar 文件：原子写（`.tmp` → rename，复用 `store.rs` 模式）、读、删；删除失败返回 `Err`，由调用方记录到 `pendingKeyDeletes`（见 §7）。
- 统一清理函数（幂等，见 §7）。
- 单元测试：加解密往返、密文≠明文、sidecar 原子写读删、删除失败→列表残留→再次清理收敛、PIN 错误拒绝。

## 5. 密钥 sidecar 文件

- 位置：`<数据根>\keys\<项目id>.key`，内容为 DPAPI 密文。以 `id` 命名：改名/`mv` 换组不受影响。
- 导入：`add`/`edit --ssh-key <path>` 语义为「读文件 → DPAPI 加密 → 原子写入 sidecar → 更新 JSON 字段」；**不存在「仅存路径」模式**，源文件移动/删除不影响已导入项目。`sshKeyPath` 仅记录来源供显示。
- 更换密钥：同 id 同名原子覆盖；写失败不破坏旧文件。
- `rm` 项目：删除对应 key 文件 + JSON 字段。
- JSON 只存相对路径（`sshKeyFile`），release/dev 两套数据根自动适配。

## 6. PIN 保护的密码查看

- 存储位置：exe 旁 `config.json` 新增 `pin` 字段：`{ salt, iterations, hash }`。PBKDF2-HMAC-SHA256（BCrypt 派生，Windows 自带，不加新依赖）、≥600k 迭代、16 字节随机盐、常数时间比较。存的是**校验哈希**，不是 PIN。
- PIN **仅用于查看秘密**（`secret show`、TUI 查看）；askpass 自动登录不需要 PIN——PIN 是防随手查看的门槛，不是硬安全边界（能自动登录的本地会话理论上也能解密，与 Tabby 等水位一致）。
- 命令：
  - `pcs pin set`：设置（输两遍确认；已存在则提示先 change/reset）。
  - `pcs pin change`：验证旧 PIN → 设新 PIN。
  - `pcs pin reset --force`：忘记 PIN 用（见 §7 清单）。
  - `pcs secret show <name> [--group]`：未设 PIN → 报错引导 `pin set`；设了 → 输入 PIN（不回显，Windows console API）→ 验证 → 解密 → 打印。错误重试 3 次后退出。展示密码与口令；**密钥内容不提供查看/导出**（YAGNI，多一个明文出口）。
- 密码/口令录入：`add`/`edit` 经控制台不回显读入（或 TUI 对话框），立即加密存储；**永不进命令行参数、不进历史、不进日志**。

## 7. 清理路径清单（硬性检查项）

| 触发 | 清理动作 |
|---|---|
| `pin reset --force` | 清空所有项目 `sshPasswordEnc`/`sshKeyPassEnc`；删除 `keys\` 全部文件并清 `sshKeyFile`/`sshKeyPath`；清除 `config.json` 的 `pin` |
| `pcs rm <项目>` | 删除 `keys\<id>.key` + JSON 字段随项目消失 |
| `edit` 更换密钥 | 旧 sidecar 原子覆盖 |
| SSH 启动前 | `keys\` 无引用文件清理 + `keys_tmp\` 残留清理 |
| ssh 退出后 | 覆写并删除本次 `keys_tmp\` 临时文件 |

- **删除失败 → 延迟重试**：删除失败（被占用/权限）→ 相对路径加入顶层 `pendingKeyDeletes` → 照常保存 JSON，不 panic 不打断主流程，仅 flash 提示。
- **重试**：每次进程加载数据后（`Store::load` 完成后执行一次，CLI 与 TUI 均覆盖）遍历尝试删除，成功移除。与孤儿清理合并为同一幂等函数，每次启动收敛。
- 目录清空后移除空目录；清理全程失败不 panic，下次启动兜底。

## 8. CLI（src/main.rs）

| 命令 | 行为 |
|---|---|
| `pcs ssh <name> [--group]` | 镜像 `pcs wsl`：直接 SSH 连接（`cmd_direct(LaunchEnv::Ssh)`） |
| `pcs open <name> -s/--ssh` | 加入 OpenArgs 互斥 flag 组 |
| `pcs pin set` / `change` / `reset --force` | 见 §6 |
| `pcs secret show <name>` | 见 §6 |
| `pcs __askpass` | 隐藏子命令：校验 token env → 读 JSON → 按 prompt 分派解密 → stdout |
| `pcs add/edit` | 新增 `--ssh <target>`、`--ssh-key <path>`（导入语义）；密码/口令不回显交互录入 |

项目查找（名字/@id/分组消歧）零改动，自动生效。

## 9. 菜单与 TUI

- `build_launch_options`（src/menu.rs）：`is_ssh_project()` 时**只**产出「SSH · 终端」一项；WSL/PowerShell/IDE/文件夹/自定义命令全部不生成。单选项时启动选择器自然直达。
- widgets.rs：SSH 项目显示黄色 `SSH` 文字标签（同现有 `WSL` 标签风格），路径行显示远程 path。
- 新增/编辑对话框：新增「SSH 目标」「导入密钥文件路径」「密码」「私钥口令」输入项；密码/口令显示 `******`，留空不修改。
- 查看密码：PIN 验证对话框（复用现有对话框样式）。

## 10. 泄漏审计结论

| 泄漏面 | 状态 |
|---|---|
| projects.json（密码/口令） | DPAPI 密文；备份 JSON 不泄密 |
| 私钥 | 独立文件，DPAPI 密文；JSON 仅存相对路径 |
| 命令行参数 | 只含 target/-p/-i/cd 命令，无秘密（弃用 plink `-pw` 的原因） |
| 环境变量 | 仅 id + 随机 token |
| askpass 直调 | token 校验拦截 |
| prompt 误答 | 仅 password/passphrase 类 prompt 回答，其余输出空 |
| 日志/flash | pcs 无日志系统；约定错误消息不含秘密 |
| `secret show` 终端明文 | 功能本身，PIN 保护 |
| 内存残留 | `zeroize` 主动擦除 |
| 临时密钥文件 | 最小窗口 + 覆写删除 + 启动清理 |
| 本地同用户会话 | 公认信任边界（DPAPI 边界），不设防 |

## 11. 实施顺序

1. models.rs：字段 + `is_ssh_project()` + 兼容测试。
2. secret.rs：DPAPI、PIN、sidecar、清理 + 测试（新增 `zeroize` 依赖）。
3. launcher.rs：`Ssh` 变体、`ssh_args`、`spawn_ssh`、临时文件生命周期、askpass env 注入 + 测试。
4. main.rs：`__askpass`、`pin`、`secret show`、`ssh`、`open -s`、`add/edit` 新 flag。
5. config.rs：`pin` 字段读写。
6. menu.rs：SSH 项目单选项 + 测试。
7. ops.rs：`rm` 联动。
8. TUI：标签、对话框、PIN 查看。
9. docs/spec.md §7 补 SSH（顺手修正该节已过时的「发起后即退出」表述仅限涉及段落）。
10. 验证：`cargo test` → `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo build --release`；不做真实 ssh 连接测试。
