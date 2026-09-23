use crate::domain as ops;
use crate::domain::models::{
    Connection, Project, ProjectData, format_date, is_wsl_path, normalize, rfc3339_now,
    win_path_to_linux,
};
use crate::domain::{ConnectionDraft, ConnectionPatch, KeyChange};
use crate::launch::{LaunchOption, build_launch_options, default_first};
use crate::persist::Store;
use crate::persist::{
    AppConfig, Config, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config,
};

/// TUI 动作错误：保留领域 / 持久化类型，校验失败仍用中文说明。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Domain(#[from] crate::domain::Error),
    #[error(transparent)]
    Persist(#[from] crate::persist::Error),
    #[error("{0}")]
    Message(String),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}

impl From<Error> for String {
    fn from(value: Error) -> Self {
        value.to_string()
    }
}

pub fn save_data(data: &ProjectData) -> Result<(), Error> {
    Store::save(data)?;
    Ok(())
}

pub fn save_config(config: &AppConfig) -> Result<(), Error> {
    Config::save(config)?;
    Ok(())
}

pub fn trash_label(item: &crate::domain::models::DeletedItem) -> String {
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

pub fn command_label(command: &crate::domain::models::ProjectCommand) -> String {
    format!(
        "{} ({} {})",
        command.name,
        crate::launch::LaunchEnv::from_command_env(&command.env).short_label(),
        command.command
    )
}

pub fn launch_labels(project: &Project, config: &AppConfig) -> (Vec<LaunchOption>, Vec<String>) {
    default_first(build_launch_options(project, config), project)
}

pub fn add_group(data: &mut ProjectData, name: &str, alias: &str) -> Result<String, Error> {
    ops::add_group_with_alias(data, name, alias)?;
    save_data(data)?;
    Ok(format!("分组已添加: {}", name.trim()))
}

pub fn rename_group(
    data: &mut ProjectData,
    old: &str,
    new: &str,
    alias: &str,
) -> Result<String, Error> {
    ops::rename_group_with_alias(data, old, new, Some(alias))?;
    save_data(data)?;
    Ok(format!("分组已重命名: {}", new.trim()))
}

pub fn remove_group(data: &mut ProjectData, name: &str) -> Result<String, Error> {
    ops::remove_group(data, name, false)?;
    save_data(data)?;
    Ok(format!("分组已删除: {name}（已移入回收站）"))
}

#[allow(clippy::too_many_arguments)]
pub fn add_project_paths(
    data: &mut ProjectData,
    group: &str,
    name: &str,
    alias: &str,
    path: &str,
    wsl_path: &str,
    git_remote: &str,
    notes: &str,
) -> Result<String, Error> {
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
    let project = Project::new(name, path, wsl)
        .with_alias(alias.trim())
        .with_git_remote(git_remote.trim())
        .with_notes(notes.trim());
    ops::add_project(data, group, project)?;
    save_data(data)?;
    Ok(format!("项目已添加: {name}"))
}

#[allow(clippy::too_many_arguments)]
pub fn edit_project(
    data: &mut ProjectData,
    group: &str,
    old_name: &str,
    new_name: &str,
    alias: &str,
    path: &str,
    wsl_path: &str,
    git_remote: &str,
    notes: &str,
) -> Result<String, Error> {
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
        Some(git_remote.trim()),
        Some(notes.trim()),
    )?;
    save_data(data)?;
    Ok(format!("项目已更新: {new_name}"))
}

pub struct ProjectInput {
    pub name: String,
    pub alias: String,
    pub win_path: String,
    pub wsl_path: String,
    pub connection_id: String,
    pub remote_path: String,
    pub git_remote: String,
    pub notes: String,
}

pub struct SecretInput {
    pub key_source: String,
    pub password: String,
    pub key_pass: String,
}

/// 选了连接 ⇒ SSH（只写 connection_id + 远程路径）；否则本地。
pub fn save_project(
    data: &mut ProjectData,
    group: &str,
    existing_id: Option<&str>,
    input: ProjectInput,
) -> Result<String, Error> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("项目名不能为空".into());
    }
    let cid = input.connection_id.trim();
    if !cid.is_empty() {
        if data.connection(cid).is_none() {
            return Err("所选远程连接不存在，请重新选择".into());
        }
        match existing_id {
            Some(id) => {
                let old_name = find_project_ref(data, group, id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| name.to_string());
                ops::edit_project_full(
                    data,
                    &old_name,
                    Some(group),
                    Some(name),
                    Some(input.alias.trim()),
                    None,
                    None,
                    Some(input.git_remote.trim()),
                    Some(input.notes.trim()),
                )?;
                let (gi, pi) = ops::find_project_by_id(data, id, Some(group))?;
                data.attach_connection(gi, pi, cid, &input.remote_path)?;
                save_data(data)?;
                Ok(format!("项目已更新: {name}"))
            }
            None => {
                let project = Project::new(name, input.remote_path.trim(), "")
                    .with_connection(cid)
                    .with_alias(input.alias.trim())
                    .with_git_remote(input.git_remote.trim())
                    .with_notes(input.notes.trim());
                ops::add_project(data, group, project)?;
                save_data(data)?;
                Ok(format!("项目已添加: {name}"))
            }
        }
    } else {
        match existing_id {
            Some(id) => {
                if let Ok((gi, pi)) = ops::find_project_by_id(data, id, Some(group)) {
                    data.detach_connection(gi, pi);
                }
                let old_name = find_project_ref(data, group, id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| name.to_string());
                edit_project(
                    data,
                    group,
                    &old_name,
                    name,
                    input.alias.trim(),
                    input.win_path.trim(),
                    input.wsl_path.trim(),
                    input.git_remote.trim(),
                    input.notes.trim(),
                )
            }
            None => add_project_paths(
                data,
                group,
                name,
                input.alias.trim(),
                input.win_path.trim(),
                input.wsl_path.trim(),
                input.git_remote.trim(),
                input.notes.trim(),
            ),
        }
    }
}

fn connection_secrets_patch(
    data: &ProjectData,
    connection_id: &str,
    secrets: &SecretInput,
    clear_key: bool,
) -> Result<ConnectionPatch, Error> {
    let old_source = data
        .connection(connection_id)
        .map(|c| c.ssh_key_path.trim().to_string())
        .unwrap_or_default();
    let source = secrets.key_source.trim().to_string();
    let key = if !source.is_empty() && source != old_source {
        let plain =
            std::fs::read_to_string(&source).map_err(|e| format!("读取私钥文件失败: {e}"))?;
        if plain.trim().is_empty() {
            return Err("私钥文件为空".into());
        }
        let file = crate::persist::write_key_file(connection_id, &plain)?;
        KeyChange::Import { file, source }
    } else if clear_key && source.is_empty() {
        KeyChange::Clear
    } else {
        KeyChange::Keep
    };
    let password_enc = if secrets.password.is_empty() {
        None
    } else {
        Some(crate::persist::protect(&secrets.password)?)
    };
    let key_pass_enc = if secrets.key_pass.is_empty() {
        None
    } else {
        Some(crate::persist::protect(&secrets.key_pass)?)
    };
    Ok(ConnectionPatch {
        key,
        password_enc,
        key_pass_enc,
        ..ConnectionPatch::default()
    })
}

pub fn add_connection(
    data: &mut ProjectData,
    draft: ConnectionDraft,
    secrets: SecretInput,
) -> Result<String, Error> {
    let now = rfc3339_now();
    let id = data.add_connection(draft, &now)?;
    let patch = connection_secrets_patch(data, &id, &secrets, false)?;
    if !matches!(patch.key, KeyChange::Keep)
        || patch.password_enc.is_some()
        || patch.key_pass_enc.is_some()
    {
        data.edit_connection(&id, patch, &now)?;
    }
    save_data(data)?;
    Ok(id)
}

pub fn edit_connection(
    data: &mut ProjectData,
    id: &str,
    draft: ConnectionDraft,
    secrets: SecretInput,
    clear_key: bool,
) -> Result<String, Error> {
    let now = rfc3339_now();
    let mut patch = connection_secrets_patch(data, id, &secrets, clear_key)?;
    patch.name = Some(draft.name);
    patch.user = Some(draft.user);
    patch.host = Some(draft.host);
    patch.port = Some(draft.port);
    data.edit_connection(id, patch, &now)?;
    save_data(data)?;
    Ok("连接已更新".into())
}

pub fn remove_connection(data: &mut ProjectData, id: &str) -> Result<String, Error> {
    let removed = data.remove_connection(id)?;
    ops::drop_key_file(data, &removed.ssh_key_file);
    save_data(data)?;
    Ok(format!("连接已删除: {}", removed.name))
}

pub fn connection_label(conn: &Connection, refs: usize) -> String {
    format!("{}  {}  ({refs} 项目)", conn.name, conn.label())
}

pub fn remove_project(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
) -> Result<String, Error> {
    let removed = ops::remove_project_by_id(data, project_id, Some(group), false)?;
    save_data(data)?;
    Ok(format!("已删除项目: {}（已移入回收站）", removed.name))
}

pub fn move_project(
    data: &mut ProjectData,
    project_id: &str,
    source_group: &str,
    dest: &str,
) -> Result<String, Error> {
    ops::move_project_by_id(data, project_id, Some(source_group), dest)?;
    save_data(data)?;
    Ok(format!("项目已移动到: {dest}"))
}

pub fn set_default_tool(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    tool: Option<&str>,
) -> Result<String, Error> {
    ops::set_default_tool(data, project_id, Some(group), tool)?;
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
) -> Result<String, Error> {
    ops::add_project_command(data, project_id, Some(group), name, env, command)?;
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
) -> Result<String, Error> {
    ops::edit_project_command(data, project_id, Some(group), index, name, env, command)?;
    save_data(data)?;
    Ok(format!("命令已更新: {}", name.trim()))
}

pub fn remove_command(
    data: &mut ProjectData,
    project_id: &str,
    group: &str,
    index: usize,
) -> Result<String, Error> {
    let removed = ops::remove_project_command(data, project_id, Some(group), index)?;
    save_data(data)?;
    Ok(format!("命令已删除: {}", removed.name))
}

pub fn restore_trash(data: &mut ProjectData, id: &str) -> Result<String, Error> {
    let msg = ops::restore_item(data, id)?;
    save_data(data)?;
    Ok(msg)
}

pub fn purge_trash_item(data: &mut ProjectData, id: &str) -> Result<String, Error> {
    let item = ops::delete_trash_item(data, id)?;
    save_data(data)?;
    Ok(format!("已彻底删除: {}", item.name))
}

pub fn empty_trash(data: &mut ProjectData) -> Result<String, Error> {
    ops::empty_trash(data);
    save_data(data)?;
    Ok("回收站已清空。".into())
}

pub fn add_config_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    name: &str,
    command: &str,
) -> Result<String, Error> {
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
) -> Result<String, Error> {
    edit_tool(config, env, index, name, command)?;
    save_config(config)?;
    Ok(format!("工具已更新: {}", name.trim()))
}

pub fn remove_config_tool(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
) -> Result<String, Error> {
    let name = env
        .tools(config)
        .get(index)
        .map(|t| t.name.clone())
        .unwrap_or_default();
    remove_tool(config, env, index)?;
    save_config(config)?;
    Ok(format!("工具已删除: {name}"))
}

pub fn reset_app_config(config: &mut AppConfig) -> Result<String, Error> {
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
