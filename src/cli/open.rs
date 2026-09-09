use anyhow::{Result, bail};

use crate::launch as launcher;
use crate::launch::LaunchEnv;
use crate::persist::{AppConfig, Config, Store};
use crate::tui;

use super::args::{OpenArgs, ProjectSelector};
use super::common::find_selected;

pub(crate) fn cmd_open(args: OpenArgs) -> Result<()> {
    let config = Config::load();
    let (project, group_name) = find_selected(&args.selector)?;
    let direct = if args.wsl {
        Some((LaunchEnv::Wsl, String::new()))
    } else if args.powershell {
        Some((LaunchEnv::PowerShell, String::new()))
    } else if args.code {
        Some((LaunchEnv::Ide, first_ide_command(&config)?))
    } else if args.ssh {
        Some((LaunchEnv::Ssh, String::new()))
    } else {
        None
    };
    if let Some((env, command)) = direct {
        let spawned = launcher::spawn_direct(&project, &group_name, env, &command)
            .map_err(anyhow::Error::msg)?;
        launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
        return Ok(());
    }
    let mut data = Store::load();
    let mut config = Config::load();
    tui::choose_launch(&mut data, &mut config, &group_name, &project.id)
}

pub(crate) fn cmd_direct(selector: &ProjectSelector, env: LaunchEnv) -> Result<()> {
    let (project, group_name) = find_selected(selector)?;
    let spawned =
        launcher::spawn_direct(&project, &group_name, env, "").map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
    Ok(())
}

/// `pcs code` 使用 config.ide 的第一个工具（默认 VS Code），跟随用户配置。
pub(crate) fn cmd_code(selector: &ProjectSelector, config: &AppConfig) -> Result<()> {
    let command = first_ide_command(config)?;
    let (project, group_name) = find_selected(selector)?;
    let spawned = launcher::spawn_direct(&project, &group_name, LaunchEnv::Ide, &command)
        .map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
    Ok(())
}

pub(crate) fn first_ide_command(config: &AppConfig) -> Result<String> {
    config
        .ide
        .first()
        .map(|tool| tool.command.clone())
        .ok_or_else(|| anyhow::anyhow!("IDE 工具列表为空，请用 pcs config 添加工具或恢复默认配置"))
}

pub(crate) fn cmd_path(selector: &ProjectSelector, wsl: bool) -> Result<()> {
    let (project, _) = find_selected(selector)?;
    if wsl {
        println!("{}", project.linux_path());
    } else if project.has_windows_path() {
        println!("{}", project.path);
    } else {
        bail!("项目 `{}` 没有 Windows 路径", project.name);
    }
    Ok(())
}
