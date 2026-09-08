use crate::config::{AppConfig, Config, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config};
use crate::menu::{LaunchOption, build_launch_options, default_first};
use crate::models::{Project, ProjectData, format_date, is_wsl_path, normalize, win_path_to_linux};
use crate::ops;
use crate::store::Store;

pub fn save_data(data: &ProjectData) -> Result<(), String> {
    if Store::save(data) {
        Ok(())
    } else {
        Err("保存数据失败".into())
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    if Config::save(config) {
        Ok(())
    } else {
        Err("无法写入 config.json".into())
    }
}

pub fn trash_label(item: &crate::models::DeletedItem) -> String {
    if item.is_group() {
        format!("[分组] {} ({} 个项目)", item.name, item.projects.len())
    } else {
        format!(
            "[项目] {}/{} (删除于 {})",
            item.group,
            item.name,
            format_date(item.deleted_at)
        )
    }
}

pub fn command_label(command: &crate::models::ProjectCommand) -> String {
    format!(
        "{} ({} {})",
        command.name,
        crate::launcher::LaunchEnv::from_command_env(&command.env).short_label(),
        command.command
    )
}

pub fn launch_labels(project: &Project, config: &AppConfig) -> (Vec<LaunchOption>, Vec<String>) {
    default_first(build_launch_options(project, config), project)
}

pub fn add_group(data: &mut ProjectData, name: &str, alias: &str) -> Result<String, String> {
    ops::add_group_with_alias(data, name, alias).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("分组已添加: {}", name.trim()))
}

pub fn rename_group(
    data: &mut ProjectData,
    old: &str,
    new: &str,
    alias: &str,
) -> Result<String, String> {
    ops::rename_group_with_alias(data, old, new, Some(alias)).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("分组已重命名: {}", new.trim()))
}

pub fn remove_group(data: &mut ProjectData, name: &str) -> Result<String, String> {
    ops::remove_group(data, name, false).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("分组已删除: {name}（已移入回收站）"))
}

pub fn add_project_paths(
    data: &mut ProjectData,
    group: &str,
    name: &str,
    alias: &str,
    path: &str,
    wsl_path: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("项目名不能为空".into());
    }
    let path = path.trim().to_string();
    let wsl_raw = wsl_path.trim();
    if path.is_empty() && wsl_raw.is_empty() {
        return Err("请填写 Windows 路径或 WSL 路径".into());
    }
    let wsl = if path.is_empty() {
        let wsl = normalize(wsl_raw);
        if !is_wsl_path(&wsl) {
            return Err("WSL 路径必须以 / 或 ~ 开头".into());
        }
        wsl
    } else if wsl_raw.is_empty() {
        win_path_to_linux(&path)
    } else {
        let wsl = normalize(wsl_raw);
        if !is_wsl_path(&wsl) {
            return Err("WSL 路径必须以 / 或 ~ 开头".into());
        }
        wsl
    };
    let project = Project::new(name, path, wsl).with_alias(alias.trim());
    ops::add_project(data, group, project).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("项目已添加: {name}"))
}

pub fn edit_project(
    data: &mut ProjectData,
    group: &str,
    old_name: &str,
    new_name: &str,
    alias: &str,
    path: &str,
    wsl_path: &str,
) -> Result<String, String> {
    let new_name = new_name.trim();
    let path = path.trim();
    let wsl_path = wsl_path.trim();
    ops::edit_project_full(
        data,
        old_name,
        Some(group),
        Some(new_name),
        Some(alias.trim()),
        Some(path),
        Some(wsl_path),
    )
    .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("项目已更新: {new_name}"))
}

/// 新增 SSH 远程项目（path 复用为远程 Linux 路径）。
pub fn add_project_ssh(
    data: &mut ProjectData,
    group: &str,
    name: &str,
    alias: &str,
    ssh_target: &str,
    ssh_path: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("项目名不能为空".into());
    }
    let target = ssh_target.trim();
    if target.is_empty() {
        return Err("SSH 目标不能为空".into());
    }
    let mut project = Project::new(name, ssh_path.trim(), "");
    project.ssh_target = target.to_string();
    project.alias = alias.trim().to_string();
    ops::add_project(data, group, project).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("项目已添加: {name}"))
}

/// 把普通项目转为 SSH 项目 / 更新 SSH 目标与远程路径。
pub fn set_ssh_target(
    data: &mut ProjectData,
    group: &str,
    project_id: &str,
    ssh_target: &str,
    ssh_path: &str,
) -> Result<String, String> {
    let target = ssh_target.trim().to_string();
    if target.is_empty() {
        return Err("SSH 目标不能为空".into());
    }
    ops::edit_ssh_fields(data, &format!("@{project_id}"), Some(group), |p| {
        p.ssh_target = target;
        p.path = ssh_path.trim().to_string();
    })
    .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok("项目已更新".into())
}

/// SSH 项目编辑：目标/远程路径/密钥导入/密码口令（留空不改）。
#[allow(clippy::too_many_arguments)]
pub fn edit_project_ssh(
    data: &mut ProjectData,
    group: &str,
    project_id: &str,
    ssh_target: &str,
    ssh_path: &str,
    key_source: &str,
    password: &str,
    key_pass: &str,
) -> Result<String, String> {
    let target = ssh_target.trim();
    if target.is_empty() {
        return Err("SSH 目标不能为空".into());
    }
    let (old_key_source, id) = {
        let p = find_project_ref(data, group, project_id).ok_or("未找到项目")?;
        (p.ssh_key_path.trim().to_string(), p.id.clone())
    };
    // 密钥导入：来源路径变化时读文件重新加密落盘
    let source = key_source.trim().to_string();
    let new_key_file = if !source.is_empty() && source != old_key_source {
        let plain =
            std::fs::read_to_string(&source).map_err(|e| format!("读取私钥文件失败: {e}"))?;
        if plain.trim().is_empty() {
            return Err("私钥文件为空".into());
        }
        Some(crate::secret::write_key_file(&id, &plain)?)
    } else {
        None
    };
    let password_enc = if password.is_empty() {
        None
    } else {
        Some(crate::secret::protect(password)?)
    };
    let key_pass_enc = if key_pass.is_empty() {
        None
    } else {
        Some(crate::secret::protect(key_pass)?)
    };
    ops::edit_ssh_fields(data, &format!("@{project_id}"), Some(group), |p| {
        p.ssh_target = target.to_string();
        p.path = ssh_path.trim().to_string();
        if let Some(relative) = &new_key_file {
            p.ssh_key_file = relative.clone();
            p.ssh_key_path = source.clone();
        }
        if let Some(enc) = password_enc {
            p.ssh_password_enc = enc;
        }
        if let Some(enc) = key_pass_enc {
            p.ssh_key_pass_enc = enc;
        }
    })
    .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok("项目已更新".into())
}

pub fn remove_project(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
) -> Result<String, String> {
    let removed = ops::remove_project_by_id(data, project_id, Some(group), false)
        .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("已删除项目: {}（已移入回收站）", removed.name))
}

pub fn move_project(
    data: &mut ProjectData,
    project_id: &str,
    source_group: &str,
    dest: &str,
) -> Result<String, String> {
    ops::move_project_by_id(data, project_id, Some(source_group), dest)
        .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("项目已移动到: {dest}"))
}

pub fn set_default_tool(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    tool: Option<&str>,
) -> Result<String, String> {
    ops::set_default_tool(data, project_id, Some(group), tool).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok("默认启动方式已更新。".into())
}

pub fn add_command(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    name: &str,
    env: &str,
    command: &str,
) -> Result<String, String> {
    ops::add_project_command(data, project_id, Some(group), name, env, command)
        .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("命令已添加: {}", name.trim()))
}

pub fn edit_command(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    index: usize,
    name: &str,
    env: &str,
    command: &str,
) -> Result<String, String> {
    ops::edit_project_command(data, project_id, Some(group), index, name, env, command)
        .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("命令已更新: {}", name.trim()))
}

pub fn remove_command(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    index: usize,
) -> Result<String, String> {
    let removed = ops::remove_project_command(data, project_id, Some(group), index)
        .map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("命令已删除: {}", removed.name))
}

pub fn restore_trash(data: &mut ProjectData, id: &str) -> Result<String, String> {
    let msg = ops::restore_item(data, id).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(msg)
}

pub fn purge_trash_item(data: &mut ProjectData, id: &str) -> Result<String, String> {
    let item = ops::delete_trash_item(data, id).map_err(|e| e.to_string())?;
    save_data(data)?;
    Ok(format!("已彻底删除: {}", item.name))
}

pub fn empty_trash(data: &mut ProjectData) -> Result<String, String> {
    ops::empty_trash(data);
    save_data(data)?;
    Ok("回收站已清空。".into())
}

pub fn add_config_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    name: &str,
    command: &str,
) -> Result<String, String> {
    add_tool(config, env, name, command)?;
    save_config(config)?;
    Ok(format!("工具已添加: {}", name.trim()))
}

pub fn edit_config_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
    name: &str,
    command: &str,
) -> Result<String, String> {
    edit_tool(config, env, index, name, command)?;
    save_config(config)?;
    Ok(format!("工具已更新: {}", name.trim()))
}

pub fn remove_config_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
) -> Result<String, String> {
    let name = env
        .tools(config)
        .get(index)
        .map(|t| t.name.clone())
        .unwrap_or_default();
    remove_tool(config, env, index)?;
    save_config(config)?;
    Ok(format!("工具已删除: {name}"))
}

pub fn reset_app_config(config: &mut AppConfig) -> Result<String, String> {
    reset_config(config);
    save_config(config)?;
    Ok("配置已恢复默认。".into())
}

pub fn find_project_ref<'a>(
    data: &'a ProjectData,
    group: &str,
    project_id: &str,
) -> Option<&'a Project> {
    let (gi, pi) = ops::find_project_by_id(data, project_id, Some(group)).ok()?;
    Some(&data.groups[gi].projects[pi])
}
