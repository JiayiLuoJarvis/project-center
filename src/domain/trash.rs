use super::{Error, Result, find_deleted_by_id, find_group};
use crate::domain::models::{DeletedItem, Group, Project, ProjectData, current_unix_ts};

/// 删除一个 key 文件；失败记入 `pendingKeyDeletes` 延迟重试（调用方负责保存）。
pub(crate) fn drop_key_file(data: &mut ProjectData, relative: &str) {
    if relative.trim().is_empty() {
        return;
    }
    if crate::persist::delete_key_file(relative).is_err()
        && !data.pending_key_deletes.iter().any(|r| r == relative)
    {
        data.pending_key_deletes.push(relative.to_string());
    }
}

/// 彻底移除一个回收站项引用的全部密钥文件（项自身 + 分组快照内项目）。
fn drop_item_key_files(data: &mut ProjectData, item: &DeletedItem) {
    drop_key_file(data, &item.ssh_key_file);
    for project in &item.projects {
        drop_key_file(data, &project.ssh_key_file);
    }
}
/// 恢复回收站中的指定项；返回面向用户的提示信息（含恢复去向）。
pub fn restore_item(data: &mut ProjectData, id: &str) -> Result<String> {
    let index = find_deleted_by_id(data, id)?;
    let item = data.trash[index].clone();
    // 先执行恢复逻辑，成功后再从回收站移除，失败时回收站项保持原样。
    let message = if item.is_group() {
        restore_group(data, item)?
    } else {
        restore_project(data, item)?
    };
    data.trash.remove(index);
    Ok(message)
}

/// 项目恢复：回到原分组（大小写不敏感）；原分组不存在时恢复到第一个分组并提示去向。
/// 目标分组内已有同名项目时追加 `(恢复)` 后缀（循环去重）。
fn restore_project(data: &mut ProjectData, item: DeletedItem) -> Result<String> {
    let group_index = match find_group(data, &item.group) {
        Ok(group_index) => group_index,
        Err(_) if !data.groups.is_empty() => 0,
        Err(_) => {
            return Err(Error::RestoreNoGroups {
                name: item.name.clone(),
            });
        }
    };
    let mut name = item.name.clone();
    let renamed = if data.groups[group_index]
        .projects
        .iter()
        .any(|project| project.name.eq_ignore_ascii_case(&name))
    {
        let base = name.clone();
        let mut n = 2;
        loop {
            let candidate = if n == 2 {
                format!("{base}(恢复)")
            } else {
                format!("{base}(恢复){}", n - 1)
            };
            let exists = data.groups[group_index]
                .projects
                .iter()
                .any(|project| project.name.eq_ignore_ascii_case(&candidate));
            if !exists {
                name = candidate;
                break;
            }
            n += 1;
        }
        true
    } else {
        false
    };
    let final_name = name.clone();
    let mut project = Project {
        id: item.id.clone(),
        name,
        alias: item.alias.clone(),
        path: item.path.clone(),
        wsl_path: item.wsl_path.clone(),
        default_tool: item.default_tool.clone(),
        commands: item.commands.clone(),
        ssh_target: item.ssh_target.clone(),
        ssh_key_file: item.ssh_key_file.clone(),
        ssh_key_path: item.ssh_key_path.clone(),
        ssh_password_enc: item.ssh_password_enc.clone(),
        ssh_key_pass_enc: item.ssh_key_pass_enc.clone(),
    };
    project.ensure_id();
    // id 已被现有项目占用时重新生成
    let id_conflict = data.groups.iter().any(|group| {
        group
            .projects
            .iter()
            .any(|existing| existing.id.eq_ignore_ascii_case(&project.id))
    });
    if id_conflict {
        project.regenerate_id();
    }
    let group_name = data.groups[group_index].name.clone();
    let fell_back = !group_name.eq_ignore_ascii_case(&item.group);
    data.groups[group_index].projects.push(project);
    match (fell_back, renamed) {
        (false, false) => Ok(format!("已恢复项目: {}", item.name)),
        (false, true) => Ok(format!(
            "已恢复项目: {}（同名项目已存在，恢复为 {}）",
            item.name, final_name
        )),
        (true, false) => Ok(format!(
            "已恢复项目: {}（原分组 `{}` 不存在，已放入分组 `{}`）",
            item.name, item.group, group_name
        )),
        (true, true) => Ok(format!(
            "已恢复项目: {}（原分组 `{}` 不存在，已放入分组 `{}`；同名项目已存在，恢复为 {}）",
            item.name, item.group, group_name, final_name
        )),
    }
}

/// 分组恢复：整组恢复（内部项目原 id 保留，冲突才重生成）；
/// 已有同名分组时恢复为 `原名(恢复)`。
fn restore_group(data: &mut ProjectData, item: DeletedItem) -> Result<String> {
    let mut name = item.name.clone();
    let renamed = if data
        .groups
        .iter()
        .any(|group| group.name.eq_ignore_ascii_case(&name))
    {
        let base = name.clone();
        let mut n = 2;
        loop {
            let candidate = if n == 2 {
                format!("{base}(恢复)")
            } else {
                format!("{base}(恢复){}", n - 1)
            };
            let exists = data
                .groups
                .iter()
                .any(|group| group.name.eq_ignore_ascii_case(&candidate));
            if !exists {
                name = candidate;
                break;
            }
            n += 1;
        }
        true
    } else {
        false
    };
    let final_name = name.clone();
    let mut projects = item.projects.clone();
    let mut used_ids: Vec<String> = data
        .groups
        .iter()
        .flat_map(|group| group.projects.iter().map(|project| project.id.clone()))
        .collect();
    for project in &mut projects {
        project.ensure_id();
        while used_ids
            .iter()
            .any(|id| id.eq_ignore_ascii_case(&project.id))
        {
            project.regenerate_id();
        }
        used_ids.push(project.id.clone());
    }
    data.groups.push(Group {
        name: name.clone(),
        alias: item.alias.clone(),
        projects,
    });
    if renamed {
        Ok(format!(
            "已恢复分组: {}（原名已被占用，恢复为 {}）",
            item.name, final_name
        ))
    } else {
        Ok(format!("已恢复分组: {}", final_name))
    }
}

/// 从回收站中彻底删除指定项（不可恢复）。
pub fn delete_trash_item(data: &mut ProjectData, id: &str) -> Result<DeletedItem> {
    let index = find_deleted_by_id(data, id)?;
    let item = data.trash.remove(index);
    drop_item_key_files(data, &item);
    Ok(item)
}

/// 回收站保留期（天）：超过保留期的项在加载时自动清理。
pub const TRASH_RETENTION_DAYS: i64 = 30;

/// 清理超过保留期的回收站项（deletedAt <= 0 视为旧数据，不清理）；
/// 返回是否清理了任何项。
pub fn purge_expired_trash(data: &mut ProjectData) -> bool {
    let cutoff = current_unix_ts() - TRASH_RETENTION_DAYS * 86_400;
    let (expired, kept): (Vec<DeletedItem>, Vec<DeletedItem>) = data
        .trash
        .drain(..)
        .partition(|item| item.deleted_at > 0 && item.deleted_at < cutoff);
    for item in &expired {
        drop_item_key_files(data, item);
    }
    data.trash = kept;
    !expired.is_empty()
}

/// 清空回收站（连同各项目引用的密钥文件）。
pub fn empty_trash(data: &mut ProjectData) {
    let items: Vec<DeletedItem> = std::mem::take(&mut data.trash);
    for item in &items {
        drop_item_key_files(data, item);
    }
}
