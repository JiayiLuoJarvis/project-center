use anyhow::{Result, bail};

use crate::launch as launcher;
use crate::launch::{LaunchEnv, LaunchOption};
use crate::persist::{AppConfig, Config, Store};
use crate::tui;

use super::args::{OpenArgs, ProjectSelector};
use super::common::{find_selected, select_project};

pub(crate) fn cmd_open(args: OpenArgs) -> Result<()> {
    let config = Config::load();
    let mut data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &args.selector.name, args.selector.group.as_deref())?;
    let group_name = data.groups[group_index].name.clone();
    let project_id = data.groups[group_index].projects[project_index].id.clone();
    let direct = if args.wsl {
        Some(terminal_option(LaunchEnv::Wsl))
    } else if args.powershell {
        Some(terminal_option(LaunchEnv::PowerShell))
    } else if args.code {
        Some(first_ide_option(&config)?)
    } else if args.ssh {
        Some(terminal_option(LaunchEnv::Ssh))
    } else {
        None
    };
    if let Some(option) = direct {
        let project = &data.groups[group_index].projects[project_index];
        launcher::launch(&data, project, &group_name, &option).map_err(anyhow::Error::msg)?;
        return Ok(());
    }
    let mut config = Config::load();
    tui::choose_launch(&mut data, &mut config, &group_name, &project_id)
}

pub(crate) fn cmd_direct(selector: &ProjectSelector, env: LaunchEnv) -> Result<()> {
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    let group_name = data.groups[group_index].name.clone();
    let project = &data.groups[group_index].projects[project_index];
    let option = terminal_option(env);
    launcher::launch(&data, project, &group_name, &option).map_err(anyhow::Error::msg)?;
    Ok(())
}

/// `pcs code` 使用 config.ide 的第一个工具（默认 VS Code），跟随用户配置。
pub(crate) fn cmd_code(selector: &ProjectSelector, config: &AppConfig) -> Result<()> {
    let option = first_ide_option(config)?;
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    let group_name = data.groups[group_index].name.clone();
    let project = &data.groups[group_index].projects[project_index];
    launcher::launch(&data, project, &group_name, &option).map_err(anyhow::Error::msg)?;
    Ok(())
}

fn terminal_option(env: LaunchEnv) -> LaunchOption {
    LaunchOption {
        env,
        tool_name: "终端".into(),
        command: String::new(),
        is_custom: false,
    }
}

fn first_ide_option(config: &AppConfig) -> Result<LaunchOption> {
    let tool = config.ide.first().ok_or_else(|| {
        anyhow::anyhow!("IDE 工具列表为空，请用 pcs config 添加工具或恢复默认配置")
    })?;
    Ok(LaunchOption {
        env: LaunchEnv::Ide,
        tool_name: tool.name.clone(),
        command: tool.command.clone(),
        is_custom: false,
    })
}

pub(crate) fn cmd_path(selector: &ProjectSelector, wsl: bool) -> Result<()> {
    let (project, _) = find_selected(selector)?;
    if wsl {
        println!("{}", project.linux_path());
    } else if let Some(path) = project.windows_path() {
        println!("{path}");
    } else {
        bail!("项目 `{}` 没有 Windows 路径", project.name);
    }
    Ok(())
}
