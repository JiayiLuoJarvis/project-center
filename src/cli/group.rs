use anyhow::Result;

use crate::domain as ops;
use crate::persist::Store;

use super::args::GroupCommand;
use super::common::save;

pub(crate) fn cmd_group(command: GroupCommand) -> Result<()> {
    let mut data = Store::load();
    match command {
        GroupCommand::Add { name, alias } => {
            ops::add_group_with_alias(&mut data, &name, alias.as_deref().unwrap_or(""))?;
            println!("分组已添加: {name}");
        }
        GroupCommand::Rename {
            old_name,
            new_name,
            alias,
        } => {
            ops::rename_group_with_alias(&mut data, &old_name, &new_name, alias.as_deref())?;
            println!("分组已重命名: {new_name}");
        }
        GroupCommand::Rm { name, force } => {
            ops::remove_group(&mut data, &name, force)?;
            println!("分组已删除: {name}");
        }
    }
    save(&data)
}
