//! 持久化错误。用户可见中文文案勿改，须与 CLI/TUI 既有输出保持一致。

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("保存数据失败")]
    SaveFailed,
    #[error("{0}")]
    Message(String),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}
