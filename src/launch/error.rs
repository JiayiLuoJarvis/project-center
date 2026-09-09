//! 启动错误。用户可见中文文案勿改，须与 CLI/TUI 既有输出保持一致。

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("项目 `{name}` 不是 SSH 项目，不能使用 SSH 启动")]
    NotSshProject { name: String },
    #[error("项目 `{name}` 没有 Windows 路径，不能使用 {env}")]
    NoWindowsPath { name: String, env: String },
    #[error("项目 `{name}` 是 SSH 远程项目，只支持 SSH 终端启动")]
    SshOnly { name: String },
    #[error("{0}")]
    Message(String),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
