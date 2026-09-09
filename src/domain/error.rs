//! 领域错误。用户可见中文文案勿改，须与 CLI/TUI 既有输出保持一致。

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("未找到分组: {name}")]
    GroupNotFound { name: String },
    #[error("分组名不唯一: {name}")]
    GroupAmbiguous { name: String },
    #[error("未找到项目: {name}")]
    ProjectNotFound { name: String },
    #[error("项目名不唯一: {name}，请使用 --group 指定分组")]
    ProjectAmbiguous { name: String },
    #[error("项目 id 不能为空")]
    ProjectIdEmpty,
    #[error("未找到项目 id: {id}")]
    ProjectIdNotFound { id: String },
    #[error("项目 id 前缀不唯一: {id}")]
    ProjectIdAmbiguous { id: String },
    #[error("分组名不能为空")]
    GroupNameEmpty,
    #[error("分组已存在: {name}")]
    GroupExists { name: String },
    #[error("分组别名已存在: {alias}")]
    GroupAliasExists { alias: String },
    #[error("分组别名不能与分组名相同")]
    GroupAliasSameAsName,
    #[error("该分组中项目已存在: {name}")]
    ProjectExists { name: String },
    #[error("该分组中项目别名已存在: {alias}")]
    ProjectAliasExists { alias: String },
    #[error("项目别名不能与项目名相同")]
    ProjectAliasSameAsName,
    #[error("至少提供一个修改项: --new-name、--alias、--dir 或 --wsl-path")]
    EditProjectNoFields,
    #[error("项目名不能为空")]
    ProjectNameEmpty,
    #[error("回收站项 id 不能为空")]
    TrashIdEmpty,
    #[error("未找到回收站项 id: {id}")]
    TrashIdNotFound { id: String },
    #[error("回收站项 id 前缀不唯一: {id}")]
    TrashIdAmbiguous { id: String },
    #[error("无法恢复项目 `{name}`：不存在任何分组，请先添加分组。")]
    RestoreNoGroups { name: String },
    #[error("命令名不能为空")]
    CommandNameEmpty,
    #[error("启动命令不能为空")]
    CommandEmpty,
    #[error("命令已存在: {name}")]
    CommandExists { name: String },
    #[error("命令索引无效")]
    CommandIndexInvalid,
    #[error("项目已经在分组 {name} 中")]
    ProjectAlreadyInGroup { name: String },
    #[error("目标分组中项目已存在: {name}")]
    ProjectExistsInTarget { name: String },
}

pub type Result<T> = std::result::Result<T, Error>;
