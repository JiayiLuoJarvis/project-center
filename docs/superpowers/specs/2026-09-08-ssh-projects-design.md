# SSH 远程项目与秘密存储 — 设计文档

日期：2026-09-08
状态：已与用户逐项确认；2026-09-08 按审查结论修订（v2）；2026-09-08 实施审查后修订（v3，见 §11 spike 结论与 §12 偏差修正）
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
- `ProjectData` 为对象结构且未用 `deny_unknown_fields`，顶层扩展字段对旧数据/旧版程序均安全可解析（已核实）。

### 回收站生命周期（DeletedItem）

`pcs rm` 是软删：项目进 `trash` 快照，过期才由 `purge_expired_trash` 真删，且支持恢复。必须覆盖：

- `DeletedItem` 快照新增与 `Project` 相同的 SSH 秘密字段（`sshTarget`/`sshKeyFile`/`sshKeyPath`/`sshPasswordEnc`/`sshKeyPassEnc`，全部 `#[serde(default)]`），否则软删→恢复会丢数据。
- **软删保留 key 文件**（可恢复）；**purge 真删时**删除对应 `keys\<id>.key`（失败 → `pendingKeyDeletes`）。
- 恢复（restore）时 key 文件天然仍在；id 冲突触发 `regenerate_id()` 后 JSON 中的 `sshKeyFile` 路径不变，仍然有效。
- `pin reset --force` 同时清空 **groups 与 trash 快照**中所有秘密字段。

### 秘密一律不进 projects.json

- 密码、口令：DPAPI 密文（见 §4），换机器/换 Windows 用户解不开。
- 私钥：不嵌 JSON（避免 JSON 膨胀），存独立加密文件（见 §5）。
- PIN：只存校验哈希，且在 `config.json`（见 §6），不在 projects.json。

## 3. 启动器（src/launcher.rs）

### LaunchEnv 与参数

- `LaunchEnv` 新增 `Ssh` 变体：`label()` = `SSH`，`short_label()` = `ssh`。
- `ssh_args(project) -> Vec<String>`：
  - 目标解析：`user@host:port` → `ssh [-p port] user@host`；无端口 → `ssh user@host`。仅当 host 部分不含 `:` 时把末段视为端口；**IPv6 目标 v1 不支持**（已知限制，避免 `[::1]:22` 与裸 `::1` 歧义）。
  - 有 `ssh_key_file`：解密 sidecar 落临时文件后追加 `-i <临时文件>`。
  - 有远程 path：追加 `-t "cd '<path>' 2>/dev/null; exec $SHELL"`（路径内单引号转义为 `''`）。
  - **路径容错**：用 `;` 而非 `&&`，cd 失败（不存在/无权限）时错误被 `2>/dev/null` 吞掉，`exec $SHELL` 照常落在用户默认 shell（home），连接保持。
- `spawn_ssh()`：`Command::new("ssh")`，在当前控制台运行；启动前 `set_console_title` 照常。`spawn_direct` 返回类型改为 `SpawnedDirect { child: Option<Child>, temp_key_path: Option<PathBuf> }`（WSL/PowerShell/IDE/Explorer 分支 `temp_key_path` 为 `None`），新增 `wait_spawned()`：内部沿用 `wait_console_child`（Ctrl+C 抑制），**返回后覆写并删除临时密钥文件**——调用方把 `wait_direct` 换成 `wait_spawned` 即可，清理不依赖调用方自觉。
- `spawn_direct` 分支：SSH 项目仅允许 `LaunchEnv::Ssh`，其他环境报错「SSH 项目不支持该启动方式」；`ssh.exe` 不存在/启动失败返回 `Err`（不 panic）。
- ssh.exe 缺失或 OpenSSH 版本过老不支持 `SSH_ASKPASS_REQUIRE`：env 被忽略，自动退化为交互输密码，功能不损坏。

### askpass 自动填充

pcs 是单二进制，**自己充当 SSH_ASKPASS 程序**。⚠️ **版本依赖未验证**：`SSH_ASKPASS_REQUIRE=force` 需较新 OpenSSH（老版本仅有 `require`，有 tty 时不会走 askpass；Win32-OpenSSH 对该机制的支持需实测）。**实施第 0 步为 spike**：本机 `ssh -V` + 一次性脚本实测 askpass 注入是否生效，再决定注入 `force` 还是降级。不支持时自动填密码不可用、手动输密码不受影响（不注入 env 即自然回退）：

1. `pcs` 启动 ssh 前：若 `sshPasswordEnc` 或 `sshKeyPassEnc` 非空，尝试解密；**解密失败一律降级为不注入 env（回退交互输密码），绝不 panic、绝不断连**。
2. 解密成功则注入 env：`SSH_ASKPASS=<pcs.exe 自身路径>`、`SSH_ASKPASS_REQUIRE=force`、`DISPLAY=:0`、`PCS_ASKPASS_ID=<项目uuid>`、`PCS_ASKPASS_TOKEN=<随机 32 字节 hex>`、`PCS_ASKPASS_TOKEN_FILE=<数据根>\keys_tmp\askpass-<uuid>.token`。**env 中只有 id 与 token 路径，无明文秘密**；token 本体同时写入该校验文件，供 `__askpass` 比对（实现 §3.5 的「不匹配 → 输出空」）。ssh 退出后 `wait_spawned()` 覆写删除校验文件；进程崩溃残留由启动维护的 `keys_tmp` 清理兜底。**仅当项目存有可解密的密码/口令时才注入**——无保存秘密时不注入（force 会把交互密码提示也路由到 askpass 并回空，用户反而无法手动输密码）。
3. ssh 请求秘密时回调 `pcs.exe <prompt>`（OpenSSH 的 `SSH_ASKPASS` 协议：直接把 prompt 当 argv[1]，**不会**带 `__askpass` 子命令）。pcs 在 `Cli::parse` 之前检测 `PCS_ASKPASS_TOKEN` 环境变量即进入 askpass 路径，避免 clap 把 prompt 当未识别子命令写 stderr。隐藏子命令 `pcs __askpass <prompt>` 仍保留供手工/测试。校验 token（env 值与校验文件内容一致，64 位 hex，大小写不敏感）→ 按 prompt 分派 → DPAPI 解密 → 输出到 stdout。
4. **prompt 分派规则**：prompt 含 `password`/`密码` → 回密码；含 `passphrase` → 回口令；**其余（host key 的 yes/no 等）一律输出空，绝不代答**，避免绕过 TOFU 确认。
5. token 缺失/不匹配/校验文件缺失或已删 → 输出空。防护目标：杜绝「直接敲一行 `pcs __askpass <id>` 拿明文」。诚实边界：同 Windows 用户的进程可读取校验文件内容，但该用户本就在 DPAPI 信任边界内（直接跑 `pcs ssh` 也能登录）。
6. **`__askpass` 全程只读**：仅纯解析 JSON（跳过 id 回填写回与启动维护清理），避免与正在运行的父进程产生 JSON 写竞态。
7. host key 首连：若 `force` 把 yes/no 确认也路由到 askpass，我们回空 = 拒绝，**首连会失败**（用户可见，非静默错误）。文档注明：首次连接建议先手动 `ssh` 一次接受 host key；**绝不代答 `yes`**（等于放弃 TOFU）。
8. 密码错误时 ssh 正常报 `Permission denied` 退出/重试，pcs 只等待子进程，不干预。

### 临时密钥文件生命周期

1. 每次数据加载：清理 `keys_tmp\` 中早于保留阈值（1 小时）的残留（幂等）；新于阈值的文件视为存活会话文件跳过，避免并发 pcs 进程误删进行中会话的临时密钥与 token 校验文件。
2. 解密 sidecar → 写 `<数据根>\keys_tmp\<随机名>`（数据根而非系统 TEMP，减少被清理工具/同步盘扫到的面）。
3. `ssh -i <临时文件>`；`wait_spawned()` 返回后（成功/失败/中断都走）**覆写内容后删除**。
4. 即使进程崩溃，残留窗口止于「下次使用 pcs 为止」，且文件在 ssh 退出前本就以明文形态存在于磁盘（任何 ssh 客户端无法避免）。

## 4. 秘密模块（新 src/secret.rs）

密码学边界收口于此：

- `protect(plain) -> Result<String>` / `unprotect(b64) -> Result<String>`：Windows DPAPI（CryptProtectData / CryptUnprotectData）。经现有依赖 `windows-sys 0.61` 新增 feature `Win32_Security_Cryptography`（DPAPI + BCrypt PBKDF2）与 `Win32_Security`（`DATA_BLOB`），**无新依赖**（`zeroize` 是唯一新增 crate）。密文绑定「当前用户 + 本机」。选用 DPAPI 而非自实现「盐+机器码派生 AES」：等价于机器绑定且额外绑定用户，密钥由系统管理不落盘（业界主流：Tabby/Credential Manager、Tunnex、Mshell 均如此）。
- 明文缓冲用 `zeroize` crate 主动擦除（新增纯 Rust 小依赖）。
- key sidecar 文件：原子写（`.tmp` → rename，复用 `store.rs` 模式）、读、删；删除失败返回 `Err`，由调用方记录到 `pendingKeyDeletes`（见 §7）。
- 统一清理函数（幂等，见 §7）。
- 单元测试：加解密往返、密文≠明文、sidecar 原子写读删、删除失败→列表残留→再次清理收敛、PIN 错误拒绝。

## 5. 密钥 sidecar 文件

- 位置：`<数据根>\keys\<项目id>.key`，内容为 DPAPI 密文。以 `id` 命名：改名/`mv` 换组不受影响。⚠️ 孤儿清理**必须以 JSON 引用为准**（所有 `groups` + `trash` 快照中的 `sshKeyFile` 集合），**不得按 id 反推文件名**——恢复时 `regenerate_id()` 会使 id 与文件名失配，按 id 反推会误删有效密钥。
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
  - `pcs pin reset --force`：忘记 PIN 用（见 §7 清单；清空范围含 groups 与 trash 快照）。
  - `pcs secret show <name> [--group]`：未设 PIN → 报错引导 `pin set`；设了 → 输入 PIN（不回显，Windows console API）→ 验证 → 解密 → 打印。错误重试 3 次后退出。展示密码与口令；**密钥内容不提供查看/导出**（YAGNI，多一个明文出口）。
- 密码/口令录入：`add`/`edit` 经控制台**不回显**读入（`--password-stdin` / `--key-pass-stdin`：stdin 为控制台时走 Win32 不回显读取，重定向/管道喂入走普通读行；或 TUI 对话框密码掩码字段），立即加密存储；**永不进命令行参数、不进历史、不进日志**。

## 7. 清理路径清单（硬性检查项）

| 触发 | 清理动作 |
|---|---|
| `pin reset --force` | 清空 **groups 与 trash 快照**中所有 `sshPasswordEnc`/`sshKeyPassEnc`/`sshKeyFile`/`sshKeyPath`；删除 `keys\` 全部文件；清除 `config.json` 的 `pin` |
| `pcs rm <项目>`（软删） | **不删 key 文件**（快照保留引用，可恢复）；秘密字段随快照保留 |
| `purge_expired_trash`（真删） | 删除 `keys\<id>.key`（失败 → `pendingKeyDeletes`）；秘密字段随快照消失 |
| `edit` 更换密钥 | 旧 sidecar 原子覆盖 |
| 每次数据加载（`Store::load`） | 以 JSON 引用为准清理 `keys\` 无引用文件（**仅数据非退化时**，见 §12-1）+ `keys_tmp\` 残留清理（**跳过修改时间在 1 小时内的活跃会话文件**，避免误删进行中 ssh 会话的临时密钥/token 校验文件）+ `keys\*.key.tmp` 写失败残留清理 |
| ssh 退出后 | `wait_spawned()` 覆写并删除本次 `keys_tmp\` 临时文件与 askpass token 校验文件 |

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
| `pcs __askpass` | 隐藏子命令：校验 token env → 纯只读解析 JSON（跳过回填写回与启动维护）→ 按 prompt 分派解密 → stdout |
| `pcs add/edit` | 新增 `--ssh <target>`、`--ssh-key <path>`（导入语义）；密码/口令不回显交互录入。`--ssh` 存在即判定 SSH 项目：**跳过 rfd 目录选择器**（非交互验证禁 rfd），path 参数直接作远程 Linux 路径 |

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
| 回收站快照（trash） | 秘密字段同为 DPAPI 密文；软删保留 key 文件，purge 真删时删除 |
| 命令行参数 | 只含 target/-p/-i/cd 命令，无秘密（弃用 plink `-pw` 的原因） |
| 环境变量 | 仅 id + 随机 token + token 校验文件路径 |
| askpass 直调 | token 与校验文件双重比对拦截（缺失/不匹配输出空） |
| askpass token 校验文件 | 明文 token 落 `keys_tmp\`（同用户可读，DPAPI 信任边界内）；ssh 退出即覆写删除，崩溃残留由启动维护兜底 |
| prompt 误答 | 仅 password/passphrase 类 prompt 回答，其余输出空 |
| 日志/flash | pcs 无日志系统；约定错误消息不含秘密 |
| `secret show` 终端明文 | 功能本身，PIN 保护 |
| 内存残留 | `zeroize` 主动擦除 |
| 临时密钥文件 | 最小窗口 + 覆写删除 + 启动清理 |
| 本地同用户会话 | 公认信任边界（DPAPI 边界），不设防 |

## 11. 实施顺序

0. **Spike（先行验证）**：本机 `ssh -V` + 一次性脚本实测 `SSH_ASKPASS_REQUIRE=force` 注入是否生效；结论决定后续注入策略，spike 结果记录进本节。
   **Spike 结果（2026-09-08 实测，v3 补录）**：
   - `ssh -V`：`OpenSSH_for_Windows_8.6p1, LibreSSL 3.4.3`（System32 自带，2022 构建）。上游 8.4 引入 `SSH_ASKPASS_REQUIRE=force`，8.6p1 支持该值。
   - 实测方法：`SSH_ASKPASS` 指向记录 prompt 的一次性 exe，`SSH_ASKPASS_REQUIRE=force` + `DISPLAY=:0`，连接 `git@ssh.github.com:443` 触发 host key 确认。
   - 结论：**force 生效**——host key 的 yes/no 确认 prompt 确实路由到 askpass（prompt 全文为 `The authenticity of host ... Are you sure you want to continue connecting (yes/no/[fingerprint])?`），印证 §3.7 的设计假设；askpass 输出空 → `Host key verification failed.`（安全拒绝，未代答）。
   - 附加发现：askpass 程序必须是可直接 CreateProcess 的 exe（`.cmd`/`.bat` 报 `CreateProcessW failed error:2`）；spawn 失败时 ssh 优雅失败，不影响 pcs 稳定性。pcs.exe 本体即为 exe，满足要求。
   - 残留风险：本次实测环境 stdin 非 tty；「有 tty 时 force 仍走 askpass」依据上游文档（force = 始终使用 askpass）与版本判定，未在有 tty 的控制台内复测。若个别版本忽略 force，行为退化为交互输密码，功能不损坏（降级安全）。
1. models.rs：`Project` 与 `DeletedItem` 字段 + `is_ssh_project()` + 旧数据兼容测试。
2. secret.rs：DPAPI、PIN、sidecar、引用驱动清理 + 测试（新增 `zeroize` 依赖；`windows-sys` 加 feature）。
3. launcher.rs：`Ssh` 变体、`ssh_args`、`spawn_ssh`、`SpawnedDirect`/`wait_spawned`、临时文件生命周期、askpass env 注入 + 测试。
4. main.rs：`__askpass`、`pin`、`secret show`、`ssh`、`open -s`、`add/edit` 新 flag。
5. config.rs：`pin` 字段读写。
6. menu.rs：SSH 项目单选项 + 测试。
7. ops.rs：`rm` 软删与 `purge_expired_trash` 真删的 key 文件联动。
8. TUI：标签、对话框、PIN 查看。
9. docs/spec.md §7 补 SSH（顺手修正该节已过时的「发起后即退出」表述仅限涉及段落）。
10. 验证：`cargo test` → `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo build --release`。不做真实 ssh 连接测试；单元测试必含：trash purge/restore × key 文件保留与删除、`regenerate_id` 后引用驱动清理不误删、askpass prompt 分派与 token 拒绝、`ssh_args` 端口/转义、PIN 往返与错误拒绝、删除失败 → `pendingKeyDeletes` → 再次清理收敛。

## 12. 实施审查后的偏差修正（v3）

对实现提交的审查发现以下问题，均已修复或显式声明：

1. **孤儿 key 清理加数据退化门控（Critical）**：原实现把孤儿清理挂在每次 `Store::load`，与「损坏 JSON 兜底为空数据/备份恢复」组合会静默销毁全部密文 sidecar。修正：`Store::load` 仅在「非备份恢复且 groups/trash 至少一个非空」（引用集可信）时允许孤儿清理；`pendingKeyDeletes` 重试与 tmp 残留清理不受门控影响。设计 §7 中清理触发时机据此由「SSH 启动前」调整为「每次数据加载（受门控）」。
2. **`__askpass` token 真实比对**：原实现只检查 token 非空，未落实 §3.5 的「不匹配 → 输出空」。修正：父进程把 token 写入 `keys_tmp\askpass-<uuid>.token` 并注入 `PCS_ASKPASS_TOKEN_FILE`，`__askpass` 比对 env token 与文件内容（64 位 hex，大小写不敏感），不一致即拒绝；ssh 退出后覆写删除（§3.5/§10 已同步更新）。
3. **`--password-stdin` / `--key-pass-stdin` 控制台不回显**：原实现普通 `read_line` 会回显明文。修正：stdin 为控制台时走既有 `read_hidden_line`（Win32 不回显），重定向/管道喂入不受影响（§6 已同步更新）。
4. **TUI 新增对话框补 SSH 秘密字段**：新增表单追加「私钥来源路径」「登录密码」「私钥口令」（仅 SSH 目标非空时生效），新增即可直接保存秘密，无需二次编辑（§9 原要求）。
5. **设计 §11 必测项补齐**：新增 askpass token 拒绝测试、`regenerate_id` 后引用驱动清理不误删测试、回收站单项删除/清空对磁盘 key 文件真删测试。
6. **顺手加固**：`keys\*.key.tmp` 写失败残留纳入启动维护清理；`verify_pin` 迭代数钳制上限（config.json 被篡改成超大值时的本机 DoS 防护，超限记录校验必败）；`Project::with_ssh_target` 移入 `models.rs`；`__askpass` 提前于 `Config::load` 执行，保证其全程只读；需要临时改写 `APPDATA` 的测试统一串行锁 + Drop 恢复。
7. **已知低水位声明**：`unprotect` 返回的明文 `String` 及输入源字符串在内存中的残留不做 `Zeroizing` 包装（设计本就认可同用户内存低水位）；`zeroize` 覆盖加密/解密过程中的临时字节缓冲。

实施审查复审后的跟进修复（v3 续）：

8. **`keys_tmp\` 清理加会话保护**：启动维护无差别覆写删除 `keys_tmp\` 全部文件，ssh 会话进行中另一 pcs 进程加载数据会误删存活会话的临时密钥与 token 校验文件。修正：按修改时间跳过 1 小时内的文件（§3 生命周期与 §7 清理表已同步更新）。
9. **无保存秘密时不注入 askpass**：原实现对无秘密项目也注入 force（`askpass_env_ok` 对空秘密返回 true），交互密码提示会被劫持为空。修正：注入前提为项目存有密码/口令密文（§3.5 已同步）。
10. **录入提示语走 stderr**：`--password-stdin` / `--key-pass-stdin` 与 PIN 录入的提示语统一走 stderr，保持 stdout 干净供脚本管道使用。
11. **新增表单秘密字段生效条件（声明）**：新增对话框的「私钥来源路径」「登录密码」「私钥口令」仅在 SSH 目标非空时生效；SSH 目标留空（普通项目）时这些字段被忽略，字段标签已注明「SSH 项目，可选」，不再额外报错。
