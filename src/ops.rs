use anyhow::{Result, bail};

use crate::models::{Group, Project, ProjectData, win_path_to_linux};

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

pub fn remove_group(data: &mut ProjectData, name: &str, force: bool) -> Result<()> {
    let group_index = find_group(data, name)?;
    if !force && !data.groups[group_index].projects.is_empty() {
        bail!(
            "分组包含项目，请使用 --force 才能删除: {}",
            data.groups[group_index].name
        );
    }
    data.groups.remove(group_index);
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
        if path.is_empty() {
            bail!("项目路径不能为空");
        }
        data.groups[group_index].projects[project_index].path = path.to_string();
        if wsl_path.is_none() {
            data.groups[group_index].projects[project_index].wsl_path = win_path_to_linux(path);
        }
    }
    if let Some(wsl_path) = wsl_path {
        data.groups[group_index].projects[project_index].wsl_path = wsl_path.trim().to_string();
    }
    Ok(())
}

pub fn remove_project(data: &mut ProjectData, name: &str, group: Option<&str>) -> Result<Project> {
    let (group_index, project_index) = find_project(data, name, group)?;
    Ok(remove_project_at(data, group_index, project_index))
}

pub fn remove_project_by_id(
    data: &mut ProjectData,
    id: &str,
    group: Option<&str>,
) -> Result<Project> {
    let (group_index, project_index) = find_project_by_id(data, id, group)?;
    Ok(remove_project_at(data, group_index, project_index))
}

fn remove_project_at(data: &mut ProjectData, group_index: usize, project_index: usize) -> Project {
    data.groups[group_index].projects.remove(project_index)
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
        let removed = remove_project(&mut data, "app", Some("Archive")).unwrap();
        assert_eq!(removed.name, "app");
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
        remove_project_by_id(&mut data, "aaaaaaaa", None).unwrap();
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
}
