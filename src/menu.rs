use anyhow::{Result, bail};
use dialoguer::{Confirm, FuzzySelect, Input, Select, theme::ColorfulTheme};

use crate::config::{AppConfig, Config, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config};
use crate::launcher::{self, LaunchEnv};
use crate::models::{
    DeletedItem, Project, ProjectCommand, ProjectData, format_date, is_linux_path, normalize,
    win_path_to_linux,
};
use crate::ops;
use crate::store::Store;

/// 单个可启动选项：环境 + 工具名 + 启动命令（终端命令为空串）。
#[derive(Clone, Debug)]
pub struct LaunchOption {
    pub env: LaunchEnv,
    pub tool_name: String,
    pub command: String,
    /// 是否为项目自定义命令（显示 `⚙ 名称 (env)` 风格标签）。
    pub is_custom: bool,
}

impl LaunchOption {
    pub fn label(&self) -> String {
        match self.env {
            LaunchEnv::Explorer => "文件夹".to_string(),
            _ if self.is_custom => {
                format!("⚙ {} ({})", self.tool_name, self.env.short_label())
            }
            _ => format!("{} · {}", self.env.label(), self.tool_name),
        }
    }
}

fn push_terminal(options: &mut Vec<LaunchOption>, env: LaunchEnv) {
    options.push(LaunchOption {
        env,
        tool_name: "终端".into(),
        command: String::new(),
        is_custom: false,
    });
}

/// 构建项目的全部启动选项：WSL（终端 + config.wsl）；
/// 有 Windows 路径时追加 PowerShell（终端 + config.powershell）、IDE（config.ide）、文件夹；
/// 尾部追加项目自定义命令段。
pub fn build_launch_options(project: &Project, config: &AppConfig) -> Vec<LaunchOption> {
    let mut options = Vec::new();
    push_terminal(&mut options, LaunchEnv::Wsl);
    options.extend(config.wsl.iter().map(|tool| LaunchOption {
        env: LaunchEnv::Wsl,
        tool_name: tool.name.clone(),
        command: tool.command.clone(),
        is_custom: false,
    }));
    if project.has_windows_path() {
        push_terminal(&mut options, LaunchEnv::PowerShell);
        options.extend(config.powershell.iter().map(|tool| LaunchOption {
            env: LaunchEnv::PowerShell,
            tool_name: tool.name.clone(),
            command: tool.command.clone(),
            is_custom: false,
        }));
        options.extend(config.ide.iter().map(|tool| LaunchOption {
            env: LaunchEnv::Ide,
            tool_name: tool.name.clone(),
            command: tool.command.clone(),
            is_custom: false,
        }));
        options.push(LaunchOption {
            env: LaunchEnv::Explorer,
            tool_name: "文件夹".into(),
            command: String::new(),
            is_custom: false,
        });
    }
    // 自定义命令按环境门禁：WSL-only 项目只挂 wsl 命令（PS/IDE 在空 Windows 路径下无法工作）。
    options.extend(
        project
            .commands
            .iter()
            .filter(|command| {
                let env = LaunchEnv::from_command_env(&command.env);
                project.has_windows_path() || env == LaunchEnv::Wsl
            })
            .map(|command| LaunchOption {
                env: LaunchEnv::from_command_env(&command.env),
                tool_name: command.name.clone(),
                command: command.command.clone(),
                is_custom: true,
            }),
    );
    options
}

/// 项目默认工具命中的选项索引（大小写不敏感）；默认工具可指向自定义命令名。
pub fn default_option_index(options: &[LaunchOption], project: &Project) -> Option<usize> {
    if !project.has_default_tool() {
        return None;
    }
    options
        .iter()
        .position(|option| option.tool_name.eq_ignore_ascii_case(&project.default_tool))
}

/// 默认工具命中时置顶并标 `[默认] ` 前缀；返回排序后的选项与显示标签。
fn default_first(
    options: Vec<LaunchOption>,
    project: &Project,
) -> (Vec<LaunchOption>, Vec<String>) {
    let Some(index) = default_option_index(&options, project) else {
        let labels = options.iter().map(|option| option.label()).collect();
        return (options, labels);
    };
    let mut items = options;
    let item = items.remove(index);
    let label = format!("[默认] {}", item.label());
    items.insert(0, item);
    let mut labels = vec![label];
    labels.extend(items.iter().skip(1).map(|option| option.label()));
    (items, labels)
}

/// 单屏启动选择：所有「环境 · 工具」组合平铺，默认工具置顶标注。
pub fn choose_launch_option(project: &Project, config: &AppConfig) -> Result<Option<LaunchOption>> {
    let options = build_launch_options(project, config);
    let (items, labels) = default_first(options, project);
    let Some(index) = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("选择启动方式")
        .items(&labels)
        .default(0)
        .interact_opt()?
    else {
        return Ok(None);
    };
    Ok(items.get(index).cloned())
}

pub fn run(data: &mut ProjectData, config: &mut AppConfig) -> Result<()> {
    let theme = ColorfulTheme::default();
    loop {
        let mut labels: Vec<String> = data.groups.iter().map(|group| group.name.clone()).collect();
        let group_count = labels.len();
        labels.push("回收站".into());
        labels.push("新增分组".into());
        labels.push("配置".into());
        labels.push("退出".into());
        let Some(index) = FuzzySelect::with_theme(&theme)
            .with_prompt("选择项目分组")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == group_count {
            trash_menu(data, &theme)?;
            continue;
        }
        if index == group_count + 1 {
            add_group(data, &theme)?;
            continue;
        }
        if index == group_count + 2 {
            config_menu(config, &theme)?;
            continue;
        }
        if index == group_count + 3 {
            return Ok(());
        }
        let group_name = data.groups[index].name.clone();
        if project_menu(data, config, group_name, &theme)? {
            return Ok(());
        }
    }
}

fn project_menu(
    data: &mut ProjectData,
    config: &mut AppConfig,
    mut group_name: String,
    theme: &ColorfulTheme,
) -> Result<bool> {
    loop {
        let group_index = match ops::find_group(data, &group_name) {
            Ok(index) => index,
            Err(_) => return Ok(false),
        };
        let group = &data.groups[group_index];
        let mut labels: Vec<String> = group
            .projects
            .iter()
            .map(|project| {
                let path = if project.has_windows_path() {
                    project.path.clone()
                } else {
                    project.linux_path()
                };
                if path.is_empty() {
                    project.name.clone()
                } else {
                    format!("{}  ({})", project.name, path)
                }
            })
            .collect();
        let project_count = labels.len();
        labels.push("新增项目".into());
        labels.push("管理本分组".into());
        labels.push("返回分组列表".into());
        let Some(index) = FuzzySelect::with_theme(theme)
            .with_prompt(format!("{}：选择项目", group_name))
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(false);
        };
        if index == project_count {
            add_project(data, &group_name, theme)?;
            continue;
        }
        if index == project_count + 1 {
            match manage_group(data, &group_name, theme)? {
                Some(new_name) => group_name = new_name,
                None => return Ok(false),
            }
            continue;
        }
        if index == project_count + 2 {
            return Ok(false);
        }

        let project_id = data.groups[group_index].projects[index].id.clone();
        if project_actions(data, config, &group_name, &project_id, theme)? {
            return Ok(true);
        }
    }
}

fn project_actions(
    data: &mut ProjectData,
    config: &mut AppConfig,
    group_name: &str,
    project_id: &str,
    theme: &ColorfulTheme,
) -> Result<bool> {
    loop {
        let (group_index, project_index) =
            ops::find_project_by_id(data, project_id, Some(group_name))?;
        let project = data.groups[group_index].projects[project_index].clone();
        let actions = [
            "打开",
            "默认启动方式",
            "自定义命令",
            "编辑",
            "删除",
            "移动到其他分组",
            "返回项目列表",
        ];
        let Some(action) = Select::with_theme(theme)
            .with_prompt(format!("{}：选择操作", project.name))
            .items(&actions)
            .default(0)
            .interact_opt()?
        else {
            return Ok(false);
        };
        match action {
            0 => {
                let Some(option) = choose_launch_option(&project, config)? else {
                    continue;
                };
                let child =
                    launcher::spawn_direct(&project, group_name, option.env, &option.command)
                        .map_err(|error| anyhow::anyhow!(error))?;
                if let Some(child) = child {
                    launcher::wait_direct(child).map_err(|error| anyhow::anyhow!(error))?;
                }
                continue;
            }
            1 => set_default_tool(data, group_name, &project, theme)?,
            2 => manage_commands(data, group_name, project_id, theme)?,
            3 => edit_project(data, group_name, &project, theme)?,
            4 => {
                if Confirm::with_theme(theme)
                    .with_prompt(format!("确认删除项目 `{}`？", project.name))
                    .default(false)
                    .interact()?
                {
                    let removed =
                        ops::remove_project_by_id(data, project_id, Some(group_name), false)?;
                    save(data)?;
                    println!("已删除项目: {}（已移入回收站）", removed.name);
                    return Ok(false);
                }
            }
            5 => {
                if move_project(data, group_name, project_id, theme)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
}

/// 回收站条目显示标签。
fn trash_label(item: &DeletedItem) -> String {
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

/// 「回收站」子菜单：列出回收站内容，支持恢复、彻底删除与清空。
fn trash_menu(data: &mut ProjectData, theme: &ColorfulTheme) -> Result<()> {
    loop {
        if data.trash.is_empty() {
            println!("回收站为空");
            return Ok(());
        }
        let mut labels: Vec<String> = data.trash.iter().map(trash_label).collect();
        let item_count = labels.len();
        labels.push("清空回收站".into());
        labels.push("返回".into());
        let Some(index) = FuzzySelect::with_theme(theme)
            .with_prompt("回收站：选择项目")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == item_count {
            if Confirm::with_theme(theme)
                .with_prompt("确认清空回收站？")
                .default(false)
                .interact()?
            {
                ops::empty_trash(data);
                save(data)?;
                println!("回收站已清空。");
            }
            return Ok(());
        }
        if index == item_count + 1 {
            return Ok(());
        }
        trash_item_actions(data, index, theme)?;
    }
}

/// 回收站单项操作：恢复 / 彻底删除 / 返回；操作后回到回收站列表。
fn trash_item_actions(data: &mut ProjectData, index: usize, theme: &ColorfulTheme) -> Result<()> {
    let item_id = data.trash[index].id.clone();
    let actions = ["恢复", "彻底删除", "返回"];
    let Some(action) = Select::with_theme(theme)
        .with_prompt("回收站项：选择操作")
        .items(&actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    match action {
        0 => match ops::restore_item(data, &item_id) {
            Ok(message) => {
                save(data)?;
                println!("{message}");
            }
            Err(error) => eprintln!("恢复失败：{error}"),
        },
        1 if Confirm::with_theme(theme)
            .with_prompt("确认彻底删除该项？此操作不可恢复。")
            .default(false)
            .interact()? =>
        {
            let item = ops::delete_trash_item(data, &item_id).map_err(anyhow::Error::msg)?;
            save(data)?;
            println!("已彻底删除: {}", item.name);
        }
        _ => {}
    }
    Ok(())
}

/// 设置项目的默认启动方式；从启动选项列表中选择，可选「不设默认」。
fn set_default_tool(
    data: &mut ProjectData,
    group_name: &str,
    project: &Project,
    theme: &ColorfulTheme,
) -> Result<()> {
    let config = Config::load();
    let options = build_launch_options(project, &config);
    let mut labels: Vec<String> = options.iter().map(|option| option.label()).collect();
    labels.push("不设默认".into());
    let Some(index) = Select::with_theme(theme)
        .with_prompt("选择默认启动方式")
        .items(&labels)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    let tool = if index < options.len() {
        Some(options[index].tool_name.clone())
    } else {
        None
    };
    ops::set_default_tool(data, &project.id, Some(group_name), tool.as_deref())?;
    save(data)?;
    println!("默认启动方式已更新。");
    Ok(())
}

/// 自定义命令显示标签：`名称 (env 命令)`。
fn command_label(command: &ProjectCommand) -> String {
    format!(
        "{} ({} {})",
        command.name,
        LaunchEnv::from_command_env(&command.env).short_label(),
        command.command
    )
}

/// 环境选择表单：WSL / PowerShell / IDE；Esc 取消返回 None。
fn choose_command_env(theme: &ColorfulTheme, current: &str) -> Result<Option<String>> {
    let envs = [ConfigEnv::Wsl, ConfigEnv::PowerShell, ConfigEnv::Ide];
    let labels: Vec<String> = envs.iter().map(|env| env.label().to_string()).collect();
    let default = envs
        .iter()
        .position(|env| env.key().eq_ignore_ascii_case(current));
    let Some(index) = Select::with_theme(theme)
        .with_prompt("运行环境")
        .items(&labels)
        .default(default.unwrap_or(0))
        .interact_opt()?
    else {
        return Ok(None);
    };
    Ok(Some(envs[index].key().to_string()))
}

/// 「自定义命令」子菜单：命令列表（名称 (env 命令)）+ 新增命令 / 返回；
/// 选中命令 → 编辑 / 删除 / 返回。校验失败仅提示，不保存。
fn manage_commands(
    data: &mut ProjectData,
    group_name: &str,
    project_id: &str,
    theme: &ColorfulTheme,
) -> Result<()> {
    loop {
        let (group_index, project_index) =
            ops::find_project_by_id(data, project_id, Some(group_name))?;
        let project = &data.groups[group_index].projects[project_index];
        let mut labels: Vec<String> = project.commands.iter().map(command_label).collect();
        let command_count = labels.len();
        labels.push("新增命令".into());
        labels.push("返回".into());
        let Some(index) = Select::with_theme(theme)
            .with_prompt("自定义命令")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == command_count {
            add_command_flow(data, group_name, project_id, theme)?;
            continue;
        }
        if index == command_count + 1 {
            return Ok(());
        }
        command_actions(data, group_name, project_id, index, theme)?;
    }
}

fn command_actions(
    data: &mut ProjectData,
    group_name: &str,
    project_id: &str,
    index: usize,
    theme: &ColorfulTheme,
) -> Result<()> {
    let (group_index, project_index) = ops::find_project_by_id(data, project_id, Some(group_name))?;
    let command = data.groups[group_index].projects[project_index].commands[index].clone();
    let actions = ["编辑", "删除", "返回"];
    let Some(action) = Select::with_theme(theme)
        .with_prompt(format!("{}：选择操作", command.name))
        .items(&actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    match action {
        0 => edit_command_flow(data, group_name, project_id, index, &command, theme)?,
        1 if Confirm::with_theme(theme)
            .with_prompt(format!("确认删除命令 `{}`？", command.name))
            .default(false)
            .interact()? =>
        {
            let removed = ops::remove_project_command(data, project_id, Some(group_name), index)
                .map_err(anyhow::Error::msg)?;
            save(data)?;
            println!("命令已删除: {}", removed.name);
        }
        _ => {}
    }
    Ok(())
}

fn add_command_flow(
    data: &mut ProjectData,
    group_name: &str,
    project_id: &str,
    theme: &ColorfulTheme,
) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("命令名称")
        .interact_text()?;
    let Some(env) = choose_command_env(theme, "")? else {
        return Ok(());
    };
    let command: String = Input::with_theme(theme)
        .with_prompt("启动命令")
        .interact_text()?;
    match ops::add_project_command(data, project_id, Some(group_name), &name, &env, &command) {
        Ok(()) => {
            save(data)?;
            println!("命令已添加: {}", name.trim());
        }
        Err(reason) => eprintln!("添加失败：{reason}"),
    }
    Ok(())
}

fn edit_command_flow(
    data: &mut ProjectData,
    group_name: &str,
    project_id: &str,
    index: usize,
    command: &ProjectCommand,
    theme: &ColorfulTheme,
) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("命令名称")
        .default(command.name.clone())
        .interact_text()?;
    let Some(env) = choose_command_env(theme, &command.env)? else {
        return Ok(());
    };
    let command_text: String = Input::with_theme(theme)
        .with_prompt("启动命令")
        .default(command.command.clone())
        .interact_text()?;
    match ops::edit_project_command(
        data,
        project_id,
        Some(group_name),
        index,
        &name,
        &env,
        &command_text,
    ) {
        Ok(()) => {
            save(data)?;
            println!("命令已更新: {}", name.trim());
        }
        Err(reason) => eprintln!("编辑失败：{reason}"),
    }
    Ok(())
}

fn add_group(data: &mut ProjectData, theme: &ColorfulTheme) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("分组名")
        .interact_text()?;
    ops::add_group(data, &name)?;
    save(data)?;
    println!("分组已添加: {}", name.trim());
    Ok(())
}

fn add_project(data: &mut ProjectData, group_name: &str, theme: &ColorfulTheme) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("项目名")
        .interact_text()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        bail!("项目名不能为空");
    }
    let path_types = ["Windows 路径（选择文件夹）", "仅 WSL 路径（手动输入）"];
    let Some(path_type) = Select::with_theme(theme)
        .with_prompt("选择项目位置")
        .items(&path_types)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    let (path, wsl_path) = if path_type == 0 {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            println!("已取消新增项目。");
            return Ok(());
        };
        let path = path.to_string_lossy().trim().to_string();
        if path.is_empty() {
            bail!("项目路径不能为空");
        }
        let wsl_path = win_path_to_linux(&path);
        (path, wsl_path)
    } else {
        let input: String = Input::with_theme(theme)
            .with_prompt("WSL 路径")
            .interact_text()?;
        let wsl_path = normalize(input.trim());
        if !is_linux_path(&wsl_path) {
            bail!("WSL 路径必须以 / 开头");
        }
        (String::new(), wsl_path)
    };
    ops::add_project(data, group_name, Project::new(name.clone(), path, wsl_path))?;
    save(data)?;
    println!("项目已添加: {}", name);
    Ok(())
}

fn manage_group(
    data: &mut ProjectData,
    group_name: &str,
    theme: &ColorfulTheme,
) -> Result<Option<String>> {
    let actions = ["重命名本分组", "删除本分组", "返回"];
    let Some(action) = Select::with_theme(theme)
        .with_prompt(format!("{}：分组管理", group_name))
        .items(&actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(Some(group_name.to_string()));
    };
    match action {
        0 => {
            let new_name: String = Input::with_theme(theme)
                .with_prompt("新的分组名")
                .default(group_name.to_string())
                .interact_text()?;
            ops::rename_group(data, group_name, &new_name)?;
            save(data)?;
            println!("分组已重命名: {}", new_name.trim());
            Ok(Some(new_name.trim().to_string()))
        }
        1 => {
            let project_count = data.groups[ops::find_group(data, group_name)?]
                .projects
                .len();
            let prompt = if project_count == 0 {
                format!("确认删除分组 `{group_name}`？")
            } else {
                format!("确认删除分组 `{group_name}` 及其中 {project_count} 个项目？")
            };
            if !Confirm::with_theme(theme)
                .with_prompt(prompt)
                .default(false)
                .interact()?
            {
                return Ok(Some(group_name.to_string()));
            }
            ops::remove_group(data, group_name, false)?;
            save(data)?;
            println!("分组已删除: {group_name}（已移入回收站）");
            Ok(None)
        }
        _ => Ok(Some(group_name.to_string())),
    }
}

fn edit_project(
    data: &mut ProjectData,
    group_name: &str,
    project: &Project,
    theme: &ColorfulTheme,
) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("项目名")
        .default(project.name.clone())
        .interact_text()?;
    let path: String = Input::with_theme(theme)
        .with_prompt("Windows/Linux 路径")
        .default(project.path.clone())
        .interact_text()?;
    let wsl_path: String = Input::with_theme(theme)
        .with_prompt("WSL 路径")
        .default(project.wsl_path.clone())
        .interact_text()?;
    let name = name.trim().to_string();
    let path = path.trim().to_string();
    let wsl_path = wsl_path.trim().to_string();
    let name_changed = name != project.name;
    let path_changed = path != project.path;
    let wsl_changed = wsl_path != project.wsl_path;
    if !name_changed && !path_changed && !wsl_changed {
        println!("没有修改项目。");
        return Ok(());
    }
    ops::edit_project(
        data,
        &project.name,
        Some(group_name),
        name_changed.then_some(name.as_str()),
        path_changed.then_some(path.as_str()),
        wsl_changed.then_some(wsl_path.as_str()),
    )?;
    save(data)?;
    println!("项目已更新: {}", name);
    Ok(())
}

fn move_project(
    data: &mut ProjectData,
    source_group: &str,
    project_id: &str,
    theme: &ColorfulTheme,
) -> Result<bool> {
    let source_index = ops::find_group(data, source_group)?;
    let destinations: Vec<String> = data
        .groups
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != source_index)
        .map(|(_, group)| group.name.clone())
        .collect();
    if destinations.is_empty() {
        println!("暂无其他分组可移动。");
        return Ok(false);
    }
    let mut labels = destinations;
    labels.push("取消".into());
    let Some(index) = Select::with_theme(theme)
        .with_prompt("移动到哪个分组")
        .items(&labels)
        .default(0)
        .interact_opt()?
    else {
        return Ok(false);
    };
    if index == labels.len() - 1 {
        return Ok(false);
    }
    let destination = labels[index].clone();
    ops::move_project_by_id(data, project_id, Some(source_group), &destination)?;
    save(data)?;
    println!("项目已移动到: {destination}");
    Ok(true)
}

/// 配置菜单：按环境管理工具、恢复默认配置。
fn config_menu(config: &mut AppConfig, theme: &ColorfulTheme) -> Result<()> {
    loop {
        let envs = [ConfigEnv::Wsl, ConfigEnv::PowerShell, ConfigEnv::Ide];
        let mut labels: Vec<String> = envs
            .iter()
            .map(|env| format!("{} 工具", env.label()))
            .collect();
        labels.push("恢复默认配置".into());
        labels.push("返回".into());
        let Some(index) = Select::with_theme(theme)
            .with_prompt("配置")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index < envs.len() {
            config_tools(config, envs[index], theme)?;
            continue;
        }
        if index == envs.len() {
            if Confirm::with_theme(theme)
                .with_prompt("确认恢复默认配置？")
                .default(false)
                .interact()?
            {
                reset_config(config);
                save_config(config);
                println!("配置已恢复默认。");
            }
            continue;
        }
        return Ok(());
    }
}

fn config_tools(config: &mut AppConfig, env: ConfigEnv, theme: &ColorfulTheme) -> Result<()> {
    loop {
        let mut labels: Vec<String> = env
            .tools(config)
            .iter()
            .map(|tool| format!("{} ({})", tool.name, tool.command))
            .collect();
        let tool_count = labels.len();
        labels.push("新增工具".into());
        labels.push("返回".into());
        let Some(index) = Select::with_theme(theme)
            .with_prompt(format!("{} 工具", env.label()))
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == tool_count {
            add_tool_flow(config, env, theme)?;
            continue;
        }
        if index == tool_count + 1 {
            return Ok(());
        }
        tool_actions(config, env, index, theme)?;
    }
}

fn tool_actions(
    config: &mut AppConfig,
    env: ConfigEnv,
    index: usize,
    theme: &ColorfulTheme,
) -> Result<()> {
    let tool = env.tools(config)[index].clone();
    let actions = ["编辑", "删除", "返回"];
    let Some(action) = Select::with_theme(theme)
        .with_prompt(format!("{}：选择操作", tool.name))
        .items(&actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    match action {
        0 => {
            let name: String = Input::with_theme(theme)
                .with_prompt("工具名称")
                .default(tool.name.clone())
                .interact_text()?;
            let command: String = Input::with_theme(theme)
                .with_prompt("启动命令")
                .default(tool.command.clone())
                .interact_text()?;
            match edit_tool(config, env, index, name.trim(), command.trim()) {
                Ok(()) => {
                    save_config(config);
                    println!("工具已更新: {}", name.trim());
                }
                Err(reason) => eprintln!("编辑失败：{reason}"),
            }
        }
        1 if Confirm::with_theme(theme)
            .with_prompt(format!("确认删除工具 `{}`？", tool.name))
            .default(false)
            .interact()? =>
        {
            remove_tool(config, env, index).map_err(|e| anyhow::anyhow!(e))?;
            save_config(config);
            println!("工具已删除: {}", tool.name);
        }
        _ => {}
    }
    Ok(())
}

fn add_tool_flow(config: &mut AppConfig, env: ConfigEnv, theme: &ColorfulTheme) -> Result<()> {
    let name: String = Input::with_theme(theme)
        .with_prompt("工具名称")
        .interact_text()?;
    let command: String = Input::with_theme(theme)
        .with_prompt("启动命令（不含参数）")
        .interact_text()?;
    match add_tool(config, env, name.trim(), command.trim()) {
        Ok(()) => {
            save_config(config);
            println!("工具已添加: {}", name.trim());
        }
        Err(reason) => eprintln!("添加失败：{reason}"),
    }
    Ok(())
}

fn save(data: &ProjectData) -> Result<()> {
    if Store::save(data) {
        Ok(())
    } else {
        bail!("保存数据失败")
    }
}

fn save_config(config: &AppConfig) {
    if !Config::save(config) {
        eprintln!("警告：无法写入 config.json，本次修改未持久化。");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn cfg() -> AppConfig {
        AppConfig::defaults()
    }

    #[test]
    fn build_options_windows_project() {
        let p = Project::new("a", r"E:\a", "");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 9);
        assert_eq!(opts[0].tool_name, "终端");
        assert_eq!(opts[1].tool_name, "opencode");
        assert_eq!(opts[3].tool_name, "终端");
        assert_eq!(opts[6].tool_name, "VS Code");
        assert_eq!(opts[8].env, LaunchEnv::Explorer);
        assert_eq!(opts[8].label(), "文件夹");
    }

    #[test]
    fn build_options_wsl_only() {
        let p = Project::new("a", "", "/mnt/e/a");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 3);
    }

    #[test]
    fn build_options_appends_custom_commands() {
        let p = Project {
            commands: vec![
                ProjectCommand::new("构建", "wsl", "make build"),
                ProjectCommand::new("运行脚本", "powershell", "npm run dev"),
                ProjectCommand::new("检查", "ide", "code --reuse-window"),
            ],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 12);
        assert_eq!(opts[9].label(), "⚙ 构建 (wsl)");
        assert_eq!(opts[10].label(), "⚙ 运行脚本 (ps)");
        assert_eq!(opts[11].label(), "⚙ 检查 (ide)");
    }

    #[test]
    fn build_options_custom_commands_wsl_only_project() {
        let p = Project {
            commands: vec![
                ProjectCommand::new("构建", "wsl", "make"),
                ProjectCommand::new("运行脚本", "powershell", "npm run dev"),
                ProjectCommand::new("检查", "ide", "code --reuse-window"),
            ],
            ..Project::new("a", "", "/mnt/e/a")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts.len(), 4);
        assert_eq!(opts[3].label(), "⚙ 构建 (wsl)");
        assert!(
            !opts
                .iter()
                .any(|opt| opt.is_custom && opt.env != LaunchEnv::Wsl),
            "WSL-only 项目不应出现 ps/ide 自定义命令"
        );
    }

    #[test]
    fn build_options_invalid_custom_env_falls_back_to_ide() {
        let p = Project {
            commands: vec![ProjectCommand::new("自检", "bash", "check")],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(opts[9].label(), "⚙ 自检 (ide)");
    }

    #[test]
    fn default_option_matches_custom_command() {
        let p = Project {
            default_tool: "构建".into(),
            commands: vec![ProjectCommand::new("构建", "wsl", "make")],
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), Some(9));
    }

    #[test]
    fn default_first_marks_custom_command_at_top() {
        let p = Project {
            default_tool: "构建".into(),
            commands: vec![ProjectCommand::new("构建", "wsl", "make")],
            ..Project::new("a", r"E:\a", "")
        };
        let (items, labels) = default_first(build_launch_options(&p, &cfg()), &p);
        assert_eq!(items[0].tool_name, "构建");
        assert_eq!(labels[0], "[默认] ⚙ 构建 (wsl)");
        assert_eq!(items.len(), labels.len());
        assert_eq!(items[9].tool_name, "文件夹");
    }

    #[test]
    fn default_option_match_case_insensitive() {
        let p = Project {
            default_tool: "OPENCODE".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), Some(1));
    }

    #[test]
    fn default_option_missing_returns_none() {
        let p = Project {
            default_tool: "ghost".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), None);
    }

    #[test]
    fn default_option_empty_returns_none() {
        let p = Project::new("a", r"E:\a", "");
        let opts = build_launch_options(&p, &cfg());
        assert_eq!(default_option_index(&opts, &p), None);
    }

    #[test]
    fn default_first_puts_marked_item_at_top() {
        let p = Project {
            default_tool: "Cursor".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let options = build_launch_options(&p, &cfg());
        let (items, labels) = default_first(options, &p);
        assert_eq!(items[0].tool_name, "Cursor");
        assert_eq!(labels[0], "[默认] IDE · Cursor");
        assert_eq!(items.len(), labels.len());
        assert_eq!(items[8].tool_name, "文件夹");
    }

    #[test]
    fn default_first_without_match_keeps_order() {
        let p = Project {
            default_tool: "ghost".into(),
            ..Project::new("a", r"E:\a", "")
        };
        let options = build_launch_options(&p, &cfg());
        let (items, labels) = default_first(options, &p);
        assert_eq!(items[0].tool_name, "终端");
        assert_eq!(labels[0], "WSL · 终端");
    }
}
