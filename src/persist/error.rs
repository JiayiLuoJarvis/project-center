//! 持久化错误。用户可见中文文案前缀勿改，须与 CLI/TUI 既有输出保持一致。

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 保存 `projects.json` 失败。前缀保持「保存数据失败」，并附带系统原因。
    #[error("保存数据失败: {source}")]
    SaveFailed {
        #[source]
        source: std::io::Error,
    },
    /// 保存 `config.json` 失败。前缀保持「无法写入 config.json」。
    #[error("无法写入 config.json: {source}")]
    ConfigWrite {
        #[source]
        source: std::io::Error,
    },
    #[error("仅支持 Windows")]
    WindowsOnly,
    #[error("密钥路径非法")]
    InvalidKeyPath,
    #[error("PIN 记录损坏")]
    PinRecordCorrupt,
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

pub(crate) fn io_invalid(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into())
}
