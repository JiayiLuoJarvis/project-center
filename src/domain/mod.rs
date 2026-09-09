//! 内存中的项目数据、查找与 CRUD。

mod command;
mod error;
mod group;
mod lookup;
pub(crate) mod models;
mod project;
mod trash;

pub(crate) use command::*;
pub(crate) use error::{Error, Result};
pub(crate) use group::*;
pub(crate) use lookup::*;
pub(crate) use project::*;
pub(crate) use trash::*;

#[cfg(test)]
mod tests;
