use super::{Error, Result};
use crate::domain::models::{Group, Project, ProjectData};

pub(crate) fn eq_name_or_alias(name: &str, alias: &str, query: &str) -> bool {
    name.eq_ignore_ascii_case(query)
        || (!alias.trim().is_empty() && alias.eq_ignore_ascii_case(query))
}

pub fn find_group(data: &ProjectData, name: &str) -> Result<usize> {
    let matches: Vec<usize> = data
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| eq_name_or_alias(&group.name, &group.alias, name))
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(Error::GroupNotFound {
            name: name.to_string(),
        }),
        _ => Err(Error::GroupAmbiguous {
            name: name.to_string(),
        }),
    }
}

/// 分组 name/alias 是否与 `label` 冲突（忽略 `skip` 下标；空 label 不参与）。
pub(crate) fn group_label_taken(data: &ProjectData, label: &str, skip: Option<usize>) -> bool {
    let label = label.trim();
    if label.is_empty() {
        return false;
    }
    data.groups.iter().enumerate().any(|(index, group)| {
        if skip == Some(index) {
            return false;
        }
        group.name.eq_ignore_ascii_case(label)
            || (!group.alias.trim().is_empty() && group.alias.eq_ignore_ascii_case(label))
    })
}

pub(crate) fn project_label_taken(group: &Group, label: &str, skip: Option<usize>) -> bool {
    let label = label.trim();
    if label.is_empty() {
        return false;
    }
    group.projects.iter().enumerate().any(|(index, project)| {
        if skip == Some(index) {
            return false;
        }
        project.name.eq_ignore_ascii_case(label)
            || (!project.alias.trim().is_empty() && project.alias.eq_ignore_ascii_case(label))
    })
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
        eq_name_or_alias(&project.name, &project.alias, name)
    })?;
    match matches.as_slice() {
        [match_index] => Ok(*match_index),
        [] => Err(Error::ProjectNotFound {
            name: name.to_string(),
        }),
        _ => Err(Error::ProjectAmbiguous {
            name: name.to_string(),
        }),
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
        return Err(Error::ProjectIdEmpty);
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
        [] => Err(Error::ProjectIdNotFound { id: id.to_string() }),
        _ => Err(Error::ProjectIdAmbiguous { id: id.to_string() }),
    }
}

/// 在回收站中按 id 定位（`@` 前缀、唯一前缀、大小写不敏感）。
pub fn find_deleted_by_id(data: &ProjectData, id: &str) -> Result<usize> {
    let id = id.strip_prefix('@').unwrap_or(id).trim();
    if id.is_empty() {
        return Err(Error::TrashIdEmpty);
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
        [] => Err(Error::TrashIdNotFound { id: id.to_string() }),
        _ => Err(Error::TrashIdAmbiguous { id: id.to_string() }),
    }
}
