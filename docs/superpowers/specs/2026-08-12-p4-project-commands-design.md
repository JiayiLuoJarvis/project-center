# P4 项目自定义命令 — 设计规格

日期：2026-08-12
状态：草稿
关联：P4 of 「产品功能/优化」批次（P2 最近/快速启动已批准、P3 数据安全、P5 录入效率）

## 1. 背景与目标

目前每个项目只能按「环境+配置工具」启动，工具命令对所有项目一视同仁（如 `opencode` 全局一致）。实际需求：不同项目需要不同启动命令（如 `code --reuse-window`、带参数的构建/运行脚本、`cursor-agent --model xxx`）。

P4 目标：项目级自定义命令——每个项目可配若干「自定义命令」，在启动选择列表与 CLI 中直接启动。

非目标：全局宏/变量展开、命令参数模板引擎（命令原样交给 shell，不做解析）；环境变量注入。

## 2. 架构与模块变更

```
models.rs  Project 新增 commands 数组（序列化名 "commands"），ProjectCommand 结构
menu.rs    启动选择列表尾部追加「自定义命令」段；项目操作页「自定义命令」管理
main.rs    CLI：pcs run <项目> <命令名>（直连启动）；pcs edit 支持 --cmd-* 维护
ops.rs     add/edit/remove_project_command 纯函数
```

### 2.1 数据契约（models.rs）

```json
{
  "name": "app",
  "commands": [
    { "name": "opencode 快速", "env": "ide", "command": "code -n --reuse-window" }
  ]
}
```

- `ProjectCommand { name: String, env: String, command: String }`，`#[serde(rename = "commands", default)]`，旧数据回退空数组。
- `env` 取值沿用启动模型：`wsl` / `powershell` / `ide`（大小写不敏感）；空或非法 → 视为 `ide`（与 CLI 默认一致）。
- 命令原样执行，不做插值；启动方式与现有 `LaunchEnv` 完全一致（WSL/PS 阻塞等待、IDE 静默），仅 command 来自项目而非 config。
- 同一项目内命令名唯一（大小写不敏感校验，违者报错）。

### 2.2 启动选择列表（menu.rs）

`build_launch_options` 在现有选项（终端/配置工具/文件夹）之后追加「自定义命令」段：

```
WSL · 终端
WSL · opencode
── 自定义命令 ──
⚙ 构建 (wsl)
⚙ 运行脚本 (ps)
```

- label 格式 `⚙ {name} ({env})`，与普通选项同列表（FuzzySelect 可搜索）。
- `default_tool` 可指向自定义命令名：命中时同样置顶标 `[默认]`（现有 `default_option_index` 按工具名匹配逻辑扩展为也匹配命令名）。
- 文件夹 / `pcs code` / `open -w|-p|-c` 语义不变（不走自定义命令）。

### 2.3 项目操作页（menu.rs）

「自定义命令」操作项 → 子菜单：命令列表（`名称 (env 命令)`）+ 「新增命令 / 返回」；选中命令 → 「编辑 / 删除 / 返回」。

- 新增/编辑表单：名称（必填、唯一、可含空格）、环境（WSL/PowerShell/IDE 三选，Select）、命令（必填，原样存储）。
- 校验失败报错返回，不保存；成功立即 `save`。

### 2.4 CLI（main.rs）

```text
pcs run <项目> <命令名>          # 按项目自定义命令直连启动（可带 --group）
pcs run <项目> --list           # 列出该项目自定义命令（名称 + env + command）
```

- 项目/命令名定位：项目按现有 name/id 语义；命令名大小写不敏感，项目内唯一。
- 找不到项目/命令 → 报错退出（非零）。
- 启动成功记录 MRU（与 P2 `record_recent` 一致）。
- 维护走 `pcs edit` 扩展或专用子命令：为保持 `pcs edit` 参数面不膨胀，命令维护仅菜单提供；CLI 提供 `pcs run --list` 只读查看。（若后续需要，再单独扩展。）

### 2.5 与 config 工具的关系

- 自定义命令是项目级覆盖，不与 config 工具冲突：同名时启动选择列表两者都出现（自定义在后段）。
- `pcs code` / `open -c` 仍用 `config.ide[0]`，不自动落到自定义命令。

## 3. 数据契约

- `projects.json` 项目对象新增可选 `commands` 数组；其余字段不变。
- 命令字符串原样存 JSON（含引号/空格/管道），执行时原样交给目标 shell。
- 各环境执行方式：`wsl` → `wsl.exe --cd <linux> -e bash -lc "<命令>"`（bash 重新解析引号/管道/`&&`）；`powershell` → 先 `Set-Location -LiteralPath '<win>'` 再拼接命令；`ide` → `cmd /c <命令> <项目路径>`——路径作为末尾参数追加（与 config.ide 工具一致，仅适用于编辑器类命令），命令在 pcs 当前目录运行。
- WSL-only 项目（无 Windows 路径）不提供 `powershell`/`ide` 自定义命令（菜单与 CLI 一致）。

## 4. 错误处理

| 场景 | 行为 |
|---|---|
| 命令名重复（同项目） | 新增/编辑时校验报错，不保存 |
| env 非法 | 视为 ide（宽容处理） |
| 项目不存在 | `pcs run` 报错「未找到项目」 |
| 命令不存在 | `pcs run` 报错「未找到命令: x」 |
| 启动失败 | launcher 错误正常返回，不记录 MRU |
| 旧数据无 commands | 回退空数组，菜单不显示自定义段 |

## 5. 测试

- `models.rs`：commands 序列化往返、旧数据回退、env 默认值。
- `ops.rs`：add/edit/remove_project_command（重名校验、大小写不敏感、by id 定位）。
- `menu.rs`：build_launch_options 追加自定义段（顺序、label、default_tool 命中自定义命令置顶）。
- `main.rs`：`pcs run` 项目/命令定位与报错路径、`--list` 输出格式。
- 全量：`cargo test` / `fmt --check` / `clippy -D warnings` / `build --release`；菜单流程真实终端手工验证。

## 6. 验证命令

```text
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

部署到 `E:\dev_tool\pcs\pcs.exe`（需先结束运行中的 pcs.exe 进程）。
