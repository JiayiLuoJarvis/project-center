//! Project Center 库根。二进制只调用 [`cli::run`]。

pub mod cli;

pub(crate) mod domain;
pub(crate) mod launch;
pub(crate) mod persist;
pub(crate) mod tui;

/// 库级错误：领域 / 持久化 / 启动。
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub(crate) enum Error {
    #[error(transparent)]
    Domain(#[from] domain::Error),
    #[error(transparent)]
    Persist(#[from] persist::Error),
    #[error(transparent)]
    Launch(#[from] launch::Error),
}
