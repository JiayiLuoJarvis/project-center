use super::{
    Error, Result, find_group, find_project, find_project_by_id, project_label_taken, trash,
};
use crate::domain::models::{
    DeletedItem, Project, ProjectData, current_unix_ts, win_path_to_linux,
};

pub fn add_project(data: &mut ProjectData, group_name: &str, project: Project) -> Result<()> {
    let group_index = find_group(data, group_name)?;
    let group = &data.groups[group_index];
    if project_label_taken(group, &project.name, None) {
        return Err(Error::ProjectExists {
            name: project.name.clone(),
        });
    }
    if project_label_taken(group, &project.alias, None) {
        return Err(Error::ProjectAliasExists {
            alias: project.alias.trim().to_string(),
        });
    }
    let alias = project.alias.trim();
    if !alias.is_empty() && alias.eq_ignore_ascii_case(project.name.trim()) {
        return Err(Error::ProjectAliasSameAsName);
    }
    data.groups[group_index].projects.push(project);
    Ok(())
}

pub fn edit_project_full(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    new_name: Option<&str>,
    alias: Option<&str>,
    path: Option<&str>,
    wsl_path: Option<&str>,
) -> Result<()> {
    if new_name.is_none() && alias.is_none() && path.is_none() && wsl_path.is_none() {
        return Err(Error::EditProjectNoFields);
    }
    let (group_index, project_index) = find_project(data, name, group)?;
    if let Some(new_name) = new_name {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(Error::ProjectNameEmpty);
        }
        if project_label_taken(&data.groups[group_index], new_name, Some(project_index)) {
            return Err(Error::ProjectExists {
                name: new_name.to_string(),
            });
        }
        data.groups[group_index].projects[project_index].name = new_name.to_string();
    }
    if let Some(alias) = alias {
        let alias = alias.trim();
        if project_label_taken(&data.groups[group_index], alias, Some(project_index)) {
            return Err(Error::ProjectAliasExists {
                alias: alias.to_string(),
            });
        }
        let current_name = data.groups[group_index].projects[project_index]
            .name
            .clone();
        if !alias.is_empty() && alias.eq_ignore_ascii_case(&current_name) {
            return Err(Error::ProjectAliasSameAsName);
        }
        data.groups[group_index].projects[project_index].alias = alias.to_string();
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

/// 定位项目并原地更新 SSH 字段；返回更新后的项目克隆。
/// 定位支持名字 / `@<id>`（大小写不敏感、唯一前缀、分组消歧）。
pub fn edit_ssh_fields(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    update: impl FnOnce(&mut Project),
) -> Result<Project> {
    let (group_index, project_index) = if let Some(id) = name.strip_prefix('@') {
        find_project_by_id(data, id, group)?
    } else {
        find_project(data, name, group)?
    };
    let project = &mut data.groups[group_index].projects[project_index];
    update(project);
    Ok(project.clone())
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
    } else {
        trash::drop_key_file(data, &project.ssh_key_file);
    }
    project
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
        return Err(Error::ProjectAlreadyInGroup {
            name: data.groups[destination_index].name.clone(),
        });
    }
    if data.groups[destination_index]
        .projects
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&name))
    {
        return Err(Error::ProjectExistsInTarget { name });
    }
    let project = data.groups[source_index].projects.remove(project_index);
    data.groups[destination_index].projects.push(project);
    Ok(())
}
