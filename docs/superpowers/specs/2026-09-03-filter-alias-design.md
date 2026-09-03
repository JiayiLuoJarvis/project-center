# 过滤增强与别名设计

## 目标

1. **启动方式**：打开后直接打字过滤（类似 opencode models），无需 `/`。
2. **分组/项目**：保留 `/` 进入过滤；修好分组列表真正按过滤隐藏；`j/k` 不清除过滤。
3. **别名**：`Group.alias` / `Project.alias` 可选；过滤与 CLI 查找认 name|alias|@id。

## 一期：过滤

### LaunchPicker

- 增加 `filter: String`。
- 可打印字符追加；Backspace 删除；Esc：有内容先清空，再 Esc 退出。
- `j/k` 在过滤后的选项上移动；Enter 启动。
- 匹配：label、tool_name、command、env.label / short_label（大小写不敏感子串）。
- UI：模态顶部显示 `过滤: query`。

### 分组/项目 `/` 过滤

- `left_items()`：过滤后的分组 + 固定「回收站」「配置」；`left_sel` 为展示索引。
- `move_sel` 在 Groups 上**不再** `filter.clear()`（否则 j/k 会毁掉分组过滤）。
- 项目匹配：name、alias、path、linux_path、id 前缀。
- 分组匹配：name、alias。

## 二期：别名（与一期一并实现）

- JSON：`alias` 可选，`#[serde(default)]`。
- 唯一性（大小写不敏感）：
  - 分组：alias 不与任一分组 name/alias 冲突（空 alias 忽略）。
  - 项目：组内 alias 不与 name/alias 冲突。
- `find_group` / `find_project`：name 或非空 alias。
- 回收站快照带 alias；恢复写回。
- TUI 表单：分组名+别名；项目名+别名+路径…
- CLI：`add`/`edit` 支持 `--alias`；`group add/rename` 支持 `--alias`。

## 非目标

- 拼音/模糊/跨组快搜
- Browse 免 `/` 直接打字
- ActionMenu / ListPicker 过滤
