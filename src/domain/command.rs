use super::{Error, Result, find_project_by_id};
use crate::domain::models::{ProjectCommand, ProjectData};

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
        return Err(Error::CommandNameEmpty);
    }
    let command = command.trim();
    if command.is_empty() {
        return Err(Error::CommandEmpty);
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
        return Err(Error::CommandExists {
            name: command.name.clone(),
        });
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
        return Err(Error::CommandIndexInvalid);
    }
    if project
        .commands
        .iter()
        .enumerate()
        .any(|(i, item)| i != index && item.name.eq_ignore_ascii_case(&command.name))
    {
        return Err(Error::CommandExists {
            name: command.name.clone(),
        });
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
        return Err(Error::CommandIndexInvalid);
    }
    Ok(commands.remove(index))
}
