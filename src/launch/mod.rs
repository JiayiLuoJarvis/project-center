//! 进程启动与启动选项。

mod error;
mod options;
mod spawn;

pub(crate) use error::{Error, Result};
pub(crate) use options::*;
pub(crate) use spawn::*;
