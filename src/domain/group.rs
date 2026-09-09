use super::{Error, Result, find_group, group_label_taken};
use crate::domain::models::{DeletedItem, Group, ProjectData, current_unix_ts};

pub fn add_group_with_alias(data: &mut ProjectData, name: &str, alias: &str) -> Result<()> {
    let name = name.trim();
    let alias = alias.trim();
    if name.is_empty() {
        return Err(Error::GroupNameEmpty);
    }
    if group_label_taken(data, name, None) {
        return Err(Error::GroupExists {
            name: name.to_string(),
        });
    }
    if group_label_taken(data, alias, None) {
        return Err(Error::GroupAliasExists {
            alias: alias.to_string(),
        });
    }
    data.groups.push(Group::new(name).with_alias(alias));
    Ok(())
}

/// `alias` 为 `None` 时不改别名；`Some("")` 清空。
pub fn rename_group_with_alias(
    data: &mut ProjectData,
    old_name: &str,
    new_name: &str,
    alias: Option<&str>,
) -> Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(Error::GroupNameEmpty);
    }
    let group_index = find_group(data, old_name)?;
    if group_label_taken(data, new_name, Some(group_index)) {
        return Err(Error::GroupExists {
            name: new_name.to_string(),
        });
    }
    let next_alias = match alias {
        Some(a) => a.trim().to_string(),
        None => data.groups[group_index].alias.clone(),
    };
    if !next_alias.is_empty() && next_alias.eq_ignore_ascii_case(new_name) {
        return Err(Error::GroupAliasSameAsName);
    }
    if let Some(alias) = alias {
        let alias = alias.trim();
        if group_label_taken(data, alias, Some(group_index)) {
            return Err(Error::GroupAliasExists {
                alias: alias.to_string(),
            });
        }
        data.groups[group_index].alias = alias.to_string();
    }
    data.groups[group_index].name = new_name.to_string();
    Ok(())
}

/// 删除分组；`force=true` 真删（不经过回收站），否则整组（含项目）软删除进回收站。
/// 软删除可恢复，因此不再限制非空分组。
pub fn remove_group(data: &mut ProjectData, name: &str, force: bool) -> Result<()> {
    let group_index = find_group(data, name)?;
    if force {
        data.groups.remove(group_index);
    } else {
        let group = data.groups.remove(group_index);
        data.trash
            .push(DeletedItem::from_group(&group, current_unix_ts()));
    }
    Ok(())
}
