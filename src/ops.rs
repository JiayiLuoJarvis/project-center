use anyhow::{Result, bail};

use crate::models::{
    DeletedItem, Group, Project, ProjectCommand, ProjectData, current_unix_ts, win_path_to_linux,
};

pub fn find_group(data: &ProjectData, name: &str) -> Result<usize> {
    let matches: Vec<usize> = data
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.name.eq_ignore_ascii_case(name))
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => bail!("未找到分组: {name}"),
        _ => bail!("分组名不唯一: {name}"),
    }
}

fn project_matches<F>(
    data: &ProjectData,
    group: Option<&str>,
    predicate: F,
) -> Result<Vec<(usize, usize)>>
where
    F: Fn(&Project) -> bool,
{
    let group_indices: Vec<usize> = match group {
        Some(group_name) => vec![find_group(data, group_name)?],
        None => (0..data.groups.len()).collect(),
    };
    Ok(group_indices
        .into_iter()
        .flat_map(|group_index| {
            data.groups[group_index]
                .projects
                .iter()
                .enumerate()
                .filter(|(_, project)| predicate(project))
                .map(move |(project_index, _)| (group_index, project_index))
        })
        .collect())
}

pub fn find_project(data: &ProjectData, name: &str, group: Option<&str>) -> Result<(usize, usize)> {
    let matches = project_matches(data, group, |project| {
        project.name.eq_ignore_ascii_case(name)
    })?;
    match matches.as_slice() {
        [match_index] => Ok(*match_index),
        [] => bail!("未找到项目: {name}"),
        _ => bail!("项目名不唯一: {name}，请使用 --group 指定分组"),
    }
}

/// 项目 id 精确匹配（忽略大小写）或以 id 为唯一前缀。
pub fn find_project_by_id(
    data: &ProjectData,
    id: &str,
    group: Option<&str>,
) -> Result<(usize, usize)> {
    let id = id.trim();
    if id.is_empty() {
        bail!("项目 id 不能为空");
    }
    let matches = project_matches(data, group, |project| {
        project.id.eq_ignore_ascii_case(id)
            || project
                .id
                .get(..id.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(id))
    })?;
    match matches.as_slice() {
        [match_index] => Ok(*match_index),
        [] => bail!("未找到项目 id: {id}"),
        _ => bail!("项目 id 前缀不唯一: {id}"),
    }
}

pub fn add_group(data: &mut ProjectData, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("分组名不能为空");
    }
    if data
        .groups
        .iter()
        .any(|group| group.name.eq_ignore_ascii_case(name))
    {
        bail!("分组已存在: {name}");
    }
    data.groups.push(Group::new(name));
    Ok(())
}

pub fn rename_group(data: &mut ProjectData, old_name: &str, new_name: &str) -> Result<()> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        bail!("分组名不能为空");
    }
    let group_index = find_group(data, old_name)?;
    if data
        .groups
        .iter()
        .enumerate()
        .any(|(index, group)| index != group_index && group.name.eq_ignore_ascii_case(new_name))
    {
        bail!("分组已存在: {new_name}");
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

pub fn add_project(data: &mut ProjectData, group_name: &str, project: Project) -> Result<()> {
    let group_index = find_group(data, group_name)?;
    if data.groups[group_index]
        .projects
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&project.name))
    {
        bail!("该分组中项目已存在: {}", project.name);
    }
    data.groups[group_index].projects.push(project);
    Ok(())
}

pub fn edit_project(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    new_name: Option<&str>,
    path: Option<&str>,
    wsl_path: Option<&str>,
) -> Result<()> {
    if new_name.is_none() && path.is_none() && wsl_path.is_none() {
        bail!("至少提供一个修改项: --new-name、--dir 或 --wsl-path");
    }
    let (group_index, project_index) = find_project(data, name, group)?;
    if let Some(new_name) = new_name {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            bail!("项目名不能为空");
        }
        if data.groups[group_index]
            .projects
            .iter()
            .enumerate()
            .any(|(index, item)| index != project_index && item.name.eq_ignore_ascii_case(new_name))
        {
            bail!("该分组中项目已存在: {new_name}");
        }
        data.groups[group_index].projects[project_index].name = new_name.to_string();
    }
    if let Some(path) = path {
        let path = path.trim();
        data.groups[group_index].projects[project_index].path = path.to_string();
        // 路径留空视为清除 Windows 路径（转为 WSL-only）；仅非空路径且未显式给 wsl_path 时自动推导。
        if !path.is_empty() && wsl_path.is_none() {
            data.groups[group_index].projects[project_index].wsl_path = win_path_to_linux(path);
        }
    }
    if let Some(wsl_path) = wsl_path {
        data.groups[group_index].projects[project_index].wsl_path = wsl_path.trim().to_string();
    }
    Ok(())
}

/// 删除项目；`force=true` 真删（不经过回收站），否则软删除进回收站。
pub fn remove_project(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    force: bool,
) -> Result<Project> {
    let (group_index, project_index) = find_project(data, name, group)?;
    Ok(remove_project_at(data, group_index, project_index, force))
}

pub fn remove_project_by_id(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
    force: bool,
) -> Result<Project> {
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    Ok(remove_project_at(data, group_index, project_index, force))
}

fn remove_project_at(
    data: &mut ProjectData,
    group_index: usize,
    project_index: usize,
    force: bool,
) -> Project {
    let group_name = data.groups[group_index].name.clone();
    let project = data.groups[group_index].projects.remove(project_index);
    if !force {
        data.trash.push(DeletedItem::from_project(
            &project,
            &group_name,
            current_unix_ts(),
        ));
    }
    project
}

/// 在回收站中按 id 定位（`@` 前缀、唯一前缀、大小写不敏感）。
pub fn find_deleted_by_id(data: &ProjectData, id: &str) -> Result<usize> {
    let id = id.strip_prefix('@').unwrap_or(id).trim();
    if id.is_empty() {
        bail!("回收站项 id 不能为空");
    }
    let matches: Vec<usize> = data
        .trash
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.id.eq_ignore_ascii_case(id)
                || item
                    .id
                    .get(..id.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(id))
        })
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => bail!("未找到回收站项 id: {id}"),
        _ => bail!("回收站项 id 前缀不唯一: {id}"),
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
        Err(_) => bail!(
            "无法恢复项目 `{}`：不存在任何分组，请先添加分组。",
            item.name
        ),
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
        path: item.path.clone(),
        wsl_path: item.wsl_path.clone(),
        default_tool: item.default_tool.clone(),
        commands: item.commands.clone(),
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
    Ok(data.trash.remove(index))
}

/// 回收站保留期（天）：超过保留期的项在加载时自动清理。
pub const TRASH_RETENTION_DAYS: i64 = 30;

/// 清理超过保留期的回收站项（deletedAt <= 0 视为旧数据，不清理）；
/// 返回是否清理了任何项。
pub fn purge_expired_trash(data: &mut ProjectData) -> bool {
    let cutoff = current_unix_ts() - TRASH_RETENTION_DAYS * 86_400;
    let len_before = data.trash.len();
    data.trash
        .retain(|item| item.deleted_at <= 0 || item.deleted_at >= cutoff);
    data.trash.len() != len_before
}

/// 清空回收站。
pub fn empty_trash(data: &mut ProjectData) {
    data.trash.clear();
}

pub fn move_project(
    data: &mut ProjectData,
    name: &str,
    source_group: Option<&str>,
    destination_group: &str,
) -> Result<()> {
    let (source_index, project_index) = find_project(data, name, source_group)?;
    move_project_at(data, source_index, project_index, destination_group)
}

pub fn move_project_by_id(
    data: &mut ProjectData,
    id: &str,
    source_group: Option<&str>,
    destination_group: &str,
) -> Result<()> {
    let (source_index, project_index) = find_project_by_id(data, id, source_group)?;
    move_project_at(data, source_index, project_index, destination_group)
}

/// 设置或清除（tool 为 None）项目的默认启动工具；工具名按配置中名称存储。
pub fn set_default_tool(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
    tool: Option<&str>,
) -> Result<()> {
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    data.groups[group_index].projects[project_index].default_tool =
        tool.unwrap_or("").trim().to_string();
    Ok(())
}

/// env 字符串规范化：wsl / powershell / ide（大小写不敏感），空或非法视为 ide。
fn canonical_env(env: &str) -> &'static str {
    if env.eq_ignore_ascii_case("wsl") {
        "wsl"
    } else if env.eq_ignore_ascii_case("powershell") {
        "powershell"
    } else {
        "ide"
    }
}

/// 校验自定义命令字段并返回构造好的命令；name 同一项目内唯一（大小写不敏感）。
fn validate_command(name: &str, env: &str, command: &str) -> Result<ProjectCommand> {
    let name = name.trim();
    if name.is_empty() {
        bail!("命令名不能为空");
    }
    let command = command.trim();
    if command.is_empty() {
        bail!("启动命令不能为空");
    }
    Ok(ProjectCommand::new(name, canonical_env(env), command))
}

/// 为项目添加自定义命令；命令名同一项目内唯一（大小写不敏感），env 非法视为 ide。
pub fn add_project_command(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
    name: &str,
    env: &str,
    command: &str,
) -> Result<()> {
    let command = validate_command(name, env, command)?;
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    let project = &mut data.groups[group_index].projects[project_index];
    if project
        .commands
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&command.name))
    {
        bail!("命令已存在: {}", command.name);
    }
    project.commands.push(command);
    Ok(())
}

/// 编辑项目自定义命令（按下标定位）；名称冲突校验同新增。
pub fn edit_project_command(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
    index: usize,
    name: &str,
    env: &str,
    command: &str,
) -> Result<()> {
    let command = validate_command(name, env, command)?;
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    let project = &mut data.groups[group_index].projects[project_index];
    if index >= project.commands.len() {
        bail!("命令索引无效");
    }
    if project
        .commands
        .iter()
        .enumerate()
        .any(|(i, item)| i != index && item.name.eq_ignore_ascii_case(&command.name))
    {
        bail!("命令已存在: {}", command.name);
    }
    project.commands[index] = command;
    Ok(())
}

/// 删除项目自定义命令（按下标定位）；返回被删命令。
pub fn remove_project_command(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
    index: usize,
) -> Result<ProjectCommand> {
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    let commands = &mut data.groups[group_index].projects[project_index].commands;
    if index >= commands.len() {
        bail!("命令索引无效");
    }
    Ok(commands.remove(index))
}

fn move_project_at(
    data: &mut ProjectData,
    source_index: usize,
    project_index: usize,
    destination_group: &str,
) -> Result<()> {
    let name = data.groups[source_index].projects[project_index]
        .name
        .clone();
    let destination_index = find_group(data, destination_group)?;
    if source_index == destination_index {
        bail!("项目已经在分组 {} 中", data.groups[destination_index].name);
    }
    if data.groups[destination_index]
        .projects
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&name))
    {
        bail!("目标分组中项目已存在: {name}");
    }
    let project = data.groups[source_index].projects.remove(project_index);
    data.groups[destination_index].projects.push(project);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> ProjectData {
        ProjectData {
            groups: vec![
                Group {
                    name: "Work".into(),
                    projects: vec![Project::new("app", r"E:\dev\app", "")],
                },
                Group {
                    name: "Personal".into(),
                    projects: vec![Project::new("app", r"E:\personal\app", "")],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn duplicate_project_requires_group() {
        let data = data();
        assert!(find_project(&data, "app", None).is_err());
        assert_eq!(find_project(&data, "app", Some("work")).unwrap(), (0, 0));
    }

    #[test]
    fn group_crud_rejects_duplicates() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Work").unwrap();
        assert!(add_group(&mut data, "work").is_err());
        rename_group(&mut data, "Work", "Personal").unwrap();
        assert_eq!(data.groups[0].name, "Personal");
        remove_group(&mut data, "Personal", false).unwrap();
        assert!(data.groups.is_empty());
    }

    #[test]
    fn project_can_move_and_remove() {
        let mut data = data();
        move_project(&mut data, "app", Some("Work"), "Personal").unwrap_err();
        add_group(&mut data, "Archive").unwrap();
        move_project(&mut data, "app", Some("Work"), "Archive").unwrap();
        assert_eq!(data.groups[0].projects.len(), 0);
        let removed = remove_project(&mut data, "app", Some("Archive"), false).unwrap();
        assert_eq!(removed.name, "app");
        // 软删除进回收站
        assert_eq!(data.trash.len(), 1);
        assert_eq!(data.trash[0].name, "app");
        assert_eq!(data.trash[0].group, "Archive");
    }

    fn id_data() -> ProjectData {
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                projects: vec![
                    Project {
                        id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
                        ..Project::new("app", r"E:\dev\app", "")
                    },
                    Project {
                        id: "aaaabbbb-0000-1111-2222-333333333333".into(),
                        ..Project::new("other", r"E:\dev\other", "")
                    },
                ],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn find_by_id_exact_and_prefix() {
        let data = id_data();
        assert_eq!(
            find_project_by_id(&data, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", None).unwrap(),
            (0, 0)
        );
        assert_eq!(find_project_by_id(&data, "aaaaaaaa", None).unwrap(), (0, 0));
        assert_eq!(find_project_by_id(&data, "AAAABBBB", None).unwrap(), (0, 1));
    }

    #[test]
    fn find_by_id_rejects_unknown_and_ambiguous() {
        let data = id_data();
        assert!(find_project_by_id(&data, "zzzz", None).is_err());
        assert!(find_project_by_id(&data, "aaaa", None).is_err());
        assert!(find_project_by_id(&data, "", None).is_err());
    }

    #[test]
    fn remove_and_move_by_id() {
        let mut data = id_data();
        remove_project_by_id(&mut data, "aaaaaaaa", None, false).unwrap();
        assert_eq!(data.groups[0].projects.len(), 1);
        add_group(&mut data, "Archive").unwrap();
        move_project_by_id(&mut data, "aaaabbbb", Some("Work"), "Archive").unwrap();
        assert_eq!(data.groups[0].projects.len(), 0);
        assert_eq!(data.groups[1].projects[0].name, "other");
    }

    #[test]
    fn edit_project_updates_path_and_derived_wsl_path() {
        let mut data = data();
        edit_project(
            &mut data,
            "app",
            Some("Work"),
            Some("new-app"),
            Some(r"F:\dev\new-app"),
            None,
        )
        .unwrap();
        let project = &data.groups[0].projects[0];
        assert_eq!(project.name, "new-app");
        assert_eq!(project.wsl_path, "/mnt/f/dev/new-app");
    }

    #[test]
    fn edit_project_can_clear_windows_path_keeping_wsl() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        data.groups[0].projects[0].wsl_path = "/mnt/e/dev/app".into();
        edit_project(&mut data, "app", Some("Work"), None, Some(""), None).unwrap();
        let project = &data.groups[0].projects[0];
        assert_eq!(project.id, id);
        assert_eq!(project.path, "");
        assert_eq!(project.wsl_path, "/mnt/e/dev/app");
        assert!(!project.has_windows_path());
    }

    #[test]
    fn edit_project_can_clear_both_paths() {
        let mut data = data();
        edit_project(&mut data, "app", Some("Work"), None, Some(""), Some("")).unwrap();
        let project = &data.groups[0].projects[0];
        assert_eq!(project.path, "");
        assert_eq!(project.wsl_path, "");
    }

    #[test]
    fn edit_project_derived_wsl_keeps_manual_override() {
        let mut data = data();
        edit_project(
            &mut data,
            "app",
            Some("Work"),
            None,
            Some(r"F:\dev\app"),
            Some("/custom/path"),
        )
        .unwrap();
        let project = &data.groups[0].projects[0];
        assert_eq!(project.path, r"F:\dev\app");
        assert_eq!(project.wsl_path, "/custom/path");
    }

    #[test]
    fn set_default_tool_sets_and_clears() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        set_default_tool(&mut data, &id, Some("Work"), Some("opencode")).unwrap();
        assert_eq!(data.groups[0].projects[0].default_tool, "opencode");
        set_default_tool(&mut data, &id, Some("Work"), None).unwrap();
        assert_eq!(data.groups[0].projects[0].default_tool, "");
    }

    #[test]
    fn add_project_command_stores_canonical_env() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        add_project_command(
            &mut data,
            &id,
            Some("Work"),
            "构建",
            "PowerShell",
            "make build",
        )
        .unwrap();
        let commands = &data.groups[0].projects[0].commands;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "构建");
        assert_eq!(commands[0].env, "powershell");
        assert_eq!(commands[0].command, "make build");
        // 非法 env 视为 ide
        add_project_command(&mut data, &id, Some("Work"), "检查", "bash", "echo hi").unwrap();
        assert_eq!(data.groups[0].projects[0].commands[1].env, "ide");
    }

    #[test]
    fn add_project_command_rejects_duplicate_name() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
        assert!(
            add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make2").is_err()
        );
        assert!(
            add_project_command(&mut data, &id, Some("Work"), "BUILD", "wsl", "make2").is_err()
        );
        assert_eq!(data.groups[0].projects[0].commands.len(), 1);
    }

    #[test]
    fn add_project_command_rejects_empty_fields() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        assert!(add_project_command(&mut data, &id, Some("Work"), "", "wsl", "make").is_err());
        assert!(add_project_command(&mut data, &id, Some("Work"), "构建", "wsl", "").is_err());
        assert!(add_project_command(&mut data, &id, Some("Work"), "  ", "wsl", "make").is_err());
        assert!(data.groups[0].projects[0].commands.is_empty());
    }

    #[test]
    fn edit_project_command_updates_and_rejects_duplicate() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
        add_project_command(&mut data, &id, Some("Work"), "run", "wsl", "make run").unwrap();
        edit_project_command(
            &mut data,
            &id,
            Some("Work"),
            0,
            "build-new",
            "ide",
            "code .",
        )
        .unwrap();
        let commands = &data.groups[0].projects[0].commands;
        assert_eq!(commands[0].name, "build-new");
        assert_eq!(commands[0].env, "ide");
        assert_eq!(commands[0].command, "code .");
        // 改为已存在命令名 → 报错
        assert!(edit_project_command(&mut data, &id, Some("Work"), 0, "run", "wsl", "x").is_err());
        // 索引越界 → 报错
        assert!(edit_project_command(&mut data, &id, Some("Work"), 9, "x", "wsl", "x").is_err());
    }

    #[test]
    fn remove_project_command_removes_at_index() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        add_project_command(&mut data, &id, Some("Work"), "build", "wsl", "make").unwrap();
        add_project_command(&mut data, &id, Some("Work"), "run", "wsl", "make run").unwrap();
        let removed = remove_project_command(&mut data, &id, Some("Work"), 0).unwrap();
        assert_eq!(removed.name, "build");
        assert_eq!(data.groups[0].projects[0].commands.len(), 1);
        assert_eq!(data.groups[0].projects[0].commands[0].name, "run");
        assert!(remove_project_command(&mut data, &id, Some("Work"), 5).is_err());
    }

    #[test]
    fn commands_resolve_by_project_id_prefix() {
        let mut data = id_data();
        data.groups[0].projects[1]
            .commands
            .push(ProjectCommand::new("部署", "wsl", "deploy"));
        add_project_command(
            &mut data,
            "aaaabbbb",
            Some("Work"),
            "测试",
            "ide",
            "cargo test",
        )
        .unwrap();
        let commands = &data.groups[0].projects[1].commands;
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[1].name, "测试");
    }

    #[test]
    fn soft_delete_and_restore_keeps_commands() {
        let mut data = data();
        let id = data.groups[0].projects[0].id.clone();
        add_project_command(&mut data, &id, Some("Work"), "构建", "wsl", "make").unwrap();
        remove_project(&mut data, "app", Some("Work"), false).unwrap();
        assert_eq!(data.trash[0].commands.len(), 1);
        assert_eq!(data.trash[0].commands[0].name, "构建");
        restore_item(&mut data, &id).unwrap();
        let project = &data.groups[0].projects[0];
        assert_eq!(project.commands.len(), 1);
        assert_eq!(project.commands[0].env, "wsl");
    }

    /// Work 组保留、Personal 组已删除；trash 含两个项目项和一个分组项。
    fn trash_data() -> ProjectData {
        let mut data = data();
        remove_project(&mut data, "app", Some("Work"), false).unwrap();
        remove_project(&mut data, "app", Some("Personal"), false).unwrap();
        remove_group(&mut data, "Personal", false).unwrap();
        data
    }

    #[test]
    fn soft_delete_snapshots_project_and_group() {
        let data = trash_data();
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.trash.len(), 3);
        let projects: Vec<&DeletedItem> =
            data.trash.iter().filter(|item| !item.is_group()).collect();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].group, "Work");
        assert!(projects[0].deleted_at > 0);
        let group = data.trash.iter().find(|item| item.is_group()).unwrap();
        assert_eq!(group.name, "Personal");
        assert!(group.projects.is_empty());
    }

    #[test]
    fn force_delete_bypasses_trash() {
        let mut data = data();
        let removed = remove_project(&mut data, "app", Some("Work"), true).unwrap();
        assert_eq!(removed.name, "app");
        assert!(data.trash.is_empty());
        remove_group(&mut data, "Personal", true).unwrap();
        assert!(data.trash.is_empty());
        assert_eq!(data.groups.len(), 1);
    }

    #[test]
    fn group_soft_delete_includes_projects() {
        let mut data = data();
        remove_group(&mut data, "Work", false).unwrap();
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.trash.len(), 1);
        let item = &data.trash[0];
        assert!(item.is_group());
        assert_eq!(item.name, "Work");
        assert_eq!(item.projects.len(), 1);
        assert_eq!(item.projects[0].name, "app");
    }

    #[test]
    fn restore_project_to_original_group_keeps_id() {
        let mut data = trash_data();
        let project_id = data.trash[0].id.clone();
        let message = restore_item(&mut data, &project_id).unwrap();
        assert_eq!(message, "已恢复项目: app");
        let group = &data.groups[0];
        assert_eq!(group.name, "Work");
        assert_eq!(group.projects[0].id, project_id);
        assert_eq!(group.projects[0].path, r"E:\dev\app");
        assert!(!data.trash.iter().any(|item| item.id == project_id));
    }

    #[test]
    fn restore_project_to_first_group_when_original_missing() {
        let mut data = trash_data();
        let project_id = data.trash[1].id.clone();
        let message = restore_item(&mut data, &project_id).unwrap();
        // 原分组 Personal 已删除，恢复到第一个分组 Work（Work 自身项目已软删，故为 1 个）
        assert!(message.starts_with("已恢复项目: app（原分组 `Personal` 不存在"));
        assert_eq!(data.groups[0].projects.len(), 1);
        assert!(
            data.groups[0]
                .projects
                .iter()
                .any(|project| project.id == project_id)
        );
    }

    #[test]
    fn restore_project_without_groups_fails() {
        let mut data = ProjectData::default();
        let mut project = Project::new("orphan", r"E:\o", "");
        let id = project.id.clone();
        data.trash
            .push(DeletedItem::from_project(&project, "Gone", 1));
        project.id = id.clone();
        assert!(restore_item(&mut data, &id).is_err());
    }

    #[test]
    fn restore_project_regenerates_conflicting_id() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Work").unwrap();
        let mut project = Project::new("app", r"E:\dev\app", "");
        let id = project.id.clone();
        data.trash
            .push(DeletedItem::from_project(&project, "Work", 1));
        project.id = id.clone();
        // 同名同 id 项目已存在（如删除后又手工重建）
        data.groups[0].projects.push(project);
        restore_item(&mut data, &id).unwrap();
        let projects = &data.groups[0].projects;
        assert_eq!(projects.len(), 2);
        assert_ne!(projects[0].id, projects[1].id);
    }

    #[test]
    fn restore_group_keeps_project_ids_and_renames_on_conflict() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Existing").unwrap();
        data.groups[0]
            .projects
            .push(Project::new("live", r"E:\live", ""));
        let inner = Project::new("inner", r"E:\inner", "");
        let inner_id = inner.id.clone();
        let mut group = Group::new("Archive");
        group.projects.push(inner);
        let item = DeletedItem::from_group(&group, 1);
        let group_id = item.id.clone();
        data.trash.push(item);
        // 恢复前手工创建同名分组
        add_group(&mut data, "Archive").unwrap();
        let message = restore_item(&mut data, &group_id).unwrap();
        assert_eq!(
            message,
            "已恢复分组: Archive（原名已被占用，恢复为 Archive(恢复)）"
        );
        assert_eq!(data.groups.len(), 3);
        let restored = &data.groups[2];
        assert_eq!(restored.name, "Archive(恢复)");
        assert_eq!(restored.projects[0].id, inner_id);
    }

    #[test]
    fn find_deleted_by_id_exact_prefix_and_at() {
        let mut data = data();
        let mut project = Project::new("app", r"E:\dev\app", "");
        project.id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into();
        data.trash
            .push(DeletedItem::from_project(&project, "Work", 1));
        project.id = "aaaabbbb-0000-1111-2222-333333333333".into();
        data.trash
            .push(DeletedItem::from_project(&project, "Work", 2));
        assert_eq!(find_deleted_by_id(&data, "@aaaaaaaa").unwrap(), 0);
        assert_eq!(find_deleted_by_id(&data, "AAAABBBB").unwrap(), 1);
        assert!(find_deleted_by_id(&data, "aaaa").is_err());
        assert!(find_deleted_by_id(&data, "zzzz").is_err());
        assert!(find_deleted_by_id(&data, "").is_err());
    }

    #[test]
    fn delete_trash_item_and_empty() {
        let mut data = trash_data();
        let id = data.trash[0].id.clone();
        let removed = delete_trash_item(&mut data, &id).unwrap();
        assert_eq!(removed.id, id);
        assert_eq!(data.trash.len(), 2);
        empty_trash(&mut data);
        assert!(data.trash.is_empty());
    }

    #[test]
    fn purge_expired_trash_removes_only_expired() {
        let mut data = trash_data();
        let now = current_unix_ts();
        let retention_secs = TRASH_RETENTION_DAYS * 86_400;
        // [0] 刚好超过保留期 -> 清理；[1] 未过期 -> 保留；[2] 旧数据(0) -> 保留
        data.trash[0].deleted_at = now - retention_secs - 1;
        data.trash[1].deleted_at = now - 86_400;
        data.trash[2].deleted_at = 0;
        assert!(purge_expired_trash(&mut data));
        assert_eq!(data.trash.len(), 2);
        assert!(
            !data
                .trash
                .iter()
                .any(|item| item.deleted_at == now - retention_secs - 1)
        );
        // 恰好在边界上 -> 保留
        data.trash[0].deleted_at = now - retention_secs;
        assert!(!purge_expired_trash(&mut data));
        assert_eq!(data.trash.len(), 2);
    }

    #[test]
    fn restore_project_renames_on_name_conflict() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Work").unwrap();
        add_project(&mut data, "Work", Project::new("app", r"E:\app", "")).unwrap();
        let snapshot = DeletedItem::from_project(&data.groups[0].projects[0], "Work", 1);
        data.trash.push(snapshot);
        let id = data.trash[0].id.clone();
        let message = restore_item(&mut data, &id).unwrap();
        assert!(message.contains("app(恢复)"));
        assert_eq!(data.groups[0].projects.len(), 2);
        assert_eq!(data.groups[0].projects[1].name, "app(恢复)");
        assert!(data.trash.is_empty());
    }

    #[test]
    fn restore_group_suffix_increments_on_repeated_conflict() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Archive").unwrap();
        add_group(&mut data, "Archive(恢复)").unwrap();
        let group = Group::new("Archive");
        data.trash.push(DeletedItem::from_group(&group, 1));
        let id = data.trash[0].id.clone();
        restore_item(&mut data, &id).unwrap();
        assert_eq!(data.groups[2].name, "Archive(恢复)2");
    }

    #[test]
    fn restore_item_failure_keeps_trash_item() {
        let mut data = ProjectData::default();
        let project = Project::new("x", r"E:\x", "");
        data.trash
            .push(DeletedItem::from_project(&project, "Work", 1));
        let id = data.trash[0].id.clone();
        assert!(restore_item(&mut data, &id).is_err());
        assert_eq!(data.trash.len(), 1, "恢复失败时回收站项应保持原样");
    }

    #[test]
    fn restore_group_ids_conflict_regenerated() {
        let mut data = ProjectData::default();
        add_group(&mut data, "Work").unwrap();
        let existing = Project::new("live", r"E:\live", "");
        data.groups[0].projects.push(existing);
        // 回收站分组里包含与现有项目同 id 的项目
        let mut group = Group::new("Archive");
        group.projects.push(data.groups[0].projects[0].clone());
        let item = DeletedItem::from_group(&group, 1);
        let group_id = item.id.clone();
        data.trash.push(item);
        restore_item(&mut data, &group_id).unwrap();
        let restored = &data.groups[1];
        assert_eq!(restored.projects.len(), 1);
        assert_ne!(restored.projects[0].id, data.groups[0].projects[0].id);
    }
}
