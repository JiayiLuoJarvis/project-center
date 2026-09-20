use anyhow::{Result, bail};

use crate::domain as ops;
use crate::domain::models::{self, Endpoint, Project, ProjectData, rfc3339_now};
use crate::domain::{ConnectionPatch, KeyChange};
use crate::launch as launcher;
use crate::launch::{LaunchEnv, LaunchOption};
use crate::persist as secret;
use crate::persist::Store;

use super::args::{AddArgs, EditArgs, MoveArgs, RmArgs, RunArgs};
use super::common::{read_stdin_line, resolve_project_name, save, select_project};

pub(crate) fn cmd_add(args: AddArgs) -> Result<()> {
    let mut data = Store::load();
    let mut project = if let Some(ssh_target) = args.ssh.as_deref() {
        let endpoint = Endpoint::parse(ssh_target)?;
        let now = rfc3339_now();
        let cid = data.find_or_create_connection(&endpoint, &now);
        let ssh_path = args.ssh_path.as_deref().unwrap_or("").trim().to_string();
        Project::new(&args.name, ssh_path, "").with_connection(cid)
    } else {
        let path = match args.dir {
            Some(path) => path.to_string_lossy().trim().to_string(),
            None if args.wsl_path.is_some() => String::new(),
            None => rfd::FileDialog::new()
                .pick_folder()
                .ok_or_else(|| anyhow::anyhow!("未选择项目目录"))?
                .to_string_lossy()
                .trim()
                .to_string(),
        };
        if path.is_empty() && args.wsl_path.is_none() {
            bail!("项目路径不能为空");
        }
        let wsl_path = args
            .wsl_path
            .map(|path| path.trim().to_string())
            .unwrap_or_else(|| models::win_path_to_linux(&path));
        if path.is_empty() && wsl_path.is_empty() {
            bail!("WSL 路径不能为空");
        }
        Project::new(&args.name, path, wsl_path)
    };
    if let Some(alias) = args.alias {
        project.alias = alias.trim().to_string();
    }
    ops::add_project(&mut data, &args.group, project)?;
    apply_connection_secrets(
        &mut data,
        &args.name,
        Some(&args.group),
        args.ssh_key.as_deref(),
        args.password_stdin,
        args.key_pass_stdin,
    )?;
    save(&data)?;
    println!("项目已添加。");
    Ok(())
}

pub(crate) fn cmd_edit(args: EditArgs) -> Result<()> {
    let path = args
        .dir
        .map(|path| path.to_string_lossy().trim().to_string());
    let mut data = Store::load();
    let (name, resolved_group) = resolve_project_name(&data, &args.name, args.group.as_deref())?;
    let group = args.group.as_deref().or(resolved_group.as_deref());
    let has_local = args.new_name.is_some()
        || args.alias.is_some()
        || path.is_some()
        || args.wsl_path.is_some();
    if has_local {
        ops::edit_project_full(
            &mut data,
            &name,
            group,
            args.new_name.as_deref(),
            args.alias.as_deref(),
            path.as_deref(),
            args.wsl_path.as_deref(),
        )?;
    }
    if args.clear_ssh {
        clear_project_ssh(&mut data, &name, group);
    }
    if let Some(ssh_target) = args.ssh.as_deref() {
        let endpoint = Endpoint::parse(ssh_target)?;
        let now = rfc3339_now();
        let cid = data.find_or_create_connection(&endpoint, &now);
        let (gi, pi) = select_project(&data, &name, group)?;
        let remote = args
            .ssh_path
            .as_deref()
            .map(|p| p.trim().to_string())
            .unwrap_or_else(|| data.groups[gi].projects[pi].path.clone());
        data.attach_connection(gi, pi, &cid, &remote)?;
    } else if let Some(ssh_path) = args.ssh_path.as_deref() {
        let (gi, pi) = select_project(&data, &name, group)?;
        if !data.groups[gi].projects[pi].is_ssh_project() {
            bail!("仅 SSH 项目可设置 --ssh-path");
        }
        data.groups[gi].projects[pi].path = ssh_path.trim().to_string();
    }
    apply_connection_secrets(
        &mut data,
        &name,
        group,
        args.ssh_key.as_deref(),
        args.password_stdin,
        args.key_pass_stdin,
    )?;
    save(&data)?;
    println!("项目已更新。");
    Ok(())
}

/// 导入私钥与读入密码/口令，DPAPI 加密后写入项目引用的连接。
pub(crate) fn apply_connection_secrets(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    ssh_key: Option<&std::path::Path>,
    password_stdin: bool,
    key_pass_stdin: bool,
) -> Result<()> {
    if ssh_key.is_none() && !password_stdin && !key_pass_stdin {
        return Ok(());
    }
    let (gi, pi) = select_project(data, name, group)?;
    let cid = data.groups[gi].projects[pi]
        .connection_id_opt()
        .ok_or_else(|| anyhow::anyhow!("项目不是 SSH 项目，无法写入连接秘密"))?
        .to_string();
    let now = rfc3339_now();
    let mut patch = ConnectionPatch::default();
    if let Some(key_path) = ssh_key {
        let plain = std::fs::read_to_string(key_path)
            .map_err(|e| anyhow::anyhow!("读取私钥文件失败: {e}"))?;
        if plain.trim().is_empty() {
            bail!("私钥文件为空");
        }
        let file = secret::write_key_file(&cid, &plain).map_err(anyhow::Error::msg)?;
        patch.key = KeyChange::Import {
            file,
            source: key_path.to_string_lossy().into_owned(),
        };
    }
    if password_stdin {
        let plain = read_stdin_line("登录密码")?;
        if !plain.is_empty() {
            patch.password_enc = Some(secret::protect(&plain).map_err(anyhow::Error::msg)?);
        }
    }
    if key_pass_stdin {
        let plain = read_stdin_line("私钥口令")?;
        if !plain.is_empty() {
            patch.key_pass_enc = Some(secret::protect(&plain).map_err(anyhow::Error::msg)?);
        }
    }
    data.edit_connection(&cid, patch, &now)?;
    Ok(())
}

/// 项目脱离连接（连接本身保留）。
pub(crate) fn clear_project_ssh(data: &mut ProjectData, name: &str, group: Option<&str>) {
    let Ok((gi, pi)) = select_project(data, name, group) else {
        return;
    };
    data.detach_connection(gi, pi);
}

pub(crate) fn cmd_rm(args: RmArgs) -> Result<()> {
    let mut data = Store::load();
    let (name, resolved_group) =
        resolve_project_name(&data, &args.selector.name, args.selector.group.as_deref())?;
    let group = args.selector.group.as_deref().or(resolved_group.as_deref());
    let removed = ops::remove_project(&mut data, &name, group, args.force)?;
    save(&data)?;
    if args.force {
        println!("已彻底删除项目: {}", removed.name);
    } else {
        println!("已删除项目: {}（已移入回收站）", removed.name);
    }
    Ok(())
}

pub(crate) fn cmd_mv(args: MoveArgs) -> Result<()> {
    let mut data = Store::load();
    let (name, resolved_group) = resolve_project_name(&data, &args.name, args.group.as_deref())?;
    let group = args.group.as_deref().or(resolved_group.as_deref());
    ops::move_project(&mut data, &name, group, &args.to)?;
    save(&data)?;
    println!("项目已移动到: {}", args.to);
    Ok(())
}

/// 项目自定义命令列表行：`名称 (env 命令)`。
pub(crate) fn command_lines(project: &Project) -> Vec<String> {
    project
        .commands
        .iter()
        .map(|command| {
            format!(
                "{} ({} {})",
                command.name,
                LaunchEnv::from_command_env(&command.env).short_label(),
                command.command
            )
        })
        .collect()
}

/// 按命令名定位（大小写不敏感）；返回启动环境与命令。
pub(crate) fn resolve_run_command(project: &Project, name: &str) -> Result<(LaunchEnv, String)> {
    let Some(command) = project
        .commands
        .iter()
        .find(|command| command.name.eq_ignore_ascii_case(name))
    else {
        bail!("未找到命令: {name}");
    };
    Ok((
        LaunchEnv::from_command_env(&command.env),
        command.command.clone(),
    ))
}

pub(crate) fn cmd_run(args: RunArgs) -> Result<()> {
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &args.selector.name, args.selector.group.as_deref())?;
    let project = &data.groups[group_index].projects[project_index];
    let group_name = data.groups[group_index].name.clone();
    if args.list {
        let lines = command_lines(project);
        if lines.is_empty() {
            println!("项目 `{}` 暂无自定义命令", project.name);
        } else {
            for line in lines {
                println!("{line}");
            }
        }
        return Ok(());
    }
    let Some(name) = args.command.as_deref() else {
        bail!("缺少命令名，或使用 --list 列出项目自定义命令");
    };
    let (env, command) = resolve_run_command(project, name)?;
    let tool_name = project
        .commands
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(name))
        .map(|item| item.name.clone())
        .unwrap_or_else(|| name.to_string());
    let option = LaunchOption {
        env,
        tool_name,
        command,
        is_custom: true,
    };
    launcher::launch(&data, project, &group_name, &option).map_err(anyhow::Error::msg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Endpoint, Group, Project, ProjectData};

    fn run_data() -> ProjectData {
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project {
                    id: "11111111-0000-0000-0000-000000000000".into(),
                    commands: vec![
                        models::ProjectCommand::new("build", "wsl", "make build"),
                        models::ProjectCommand::new("deploy", "powershell", "deploy.ps1"),
                    ],
                    ..Project::new("app", r"E:\w\app", "")
                }],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn command_lines_formats_env_short_label() {
        let data = run_data();
        let project = &data.groups[0].projects[0];
        let lines = command_lines(project);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "build (wsl make build)");
        assert_eq!(lines[1], "deploy (ps deploy.ps1)");
    }

    #[test]
    fn command_lines_empty() {
        let project = Project::new("a", r"E:\a", "");
        assert!(command_lines(&project).is_empty());
    }

    #[test]
    fn resolve_run_command_finds_case_insensitive() {
        let data = run_data();
        let project = &data.groups[0].projects[0];
        let (env, command) = resolve_run_command(project, "build").unwrap();
        assert_eq!(env, LaunchEnv::Wsl);
        assert_eq!(command, "make build");
        let (env, command) = resolve_run_command(project, "DEPLOY").unwrap();
        assert_eq!(env, LaunchEnv::PowerShell);
        assert_eq!(command, "deploy.ps1");
    }

    #[test]
    fn resolve_run_command_missing_errors() {
        let data = run_data();
        let project = &data.groups[0].projects[0];
        assert!(resolve_run_command(project, "不存在").is_err());
        assert!(resolve_run_command(&Project::new("a", "", ""), "构建").is_err());
    }

    #[test]
    fn ssh_add_uses_connection_id_not_ssh_target() {
        let mut data = ProjectData::default();
        data.groups.push(Group {
            name: "Work".into(),
            alias: String::new(),
            projects: Vec::new(),
        });
        let endpoint = Endpoint::parse("abc@h:2222").unwrap();
        let cid = data.find_or_create_connection(&endpoint, "2026-09-18T00:00:00Z");
        let project = Project::new("srv", "/opt/x", "").with_connection(&cid);
        ops::add_project(&mut data, "Work", project).unwrap();
        let p = &data.groups[0].projects[0];
        assert_eq!(p.connection_id, cid);
        assert!(p.is_ssh_project());
        assert_eq!(data.connections.len(), 1);
        let again = data.find_or_create_connection(&endpoint, "2026-09-18T00:00:00Z");
        assert_eq!(again, cid);
    }
}
