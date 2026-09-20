//! Project Center 库根。二进制只调用 [`cli::run`]。

pub mod cli;

pub(crate) mod domain;
pub(crate) mod launch;
pub(crate) mod persist;
pub(crate) mod tui;
