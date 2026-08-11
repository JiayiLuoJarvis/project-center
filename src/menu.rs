use anyhow::{Result, bail};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

use crate::launcher::{self, LauncherKind, ToolKind};
use crate::models::{Project, ProjectData, is_linux_path, normalize, win_path_to_linux};
use crate::ops;
use crate::store::Store;

pub fn choose_launcher(project: &Project) -> Result<Option<LauncherKind>> {
    let mut choices = vec![LauncherKind::Wsl];
    if project.has_windows_path() {
        choices.extend([LauncherKind::PowerShell, LauncherKind::VsCode]);
    }
    let labels: Vec<&str> = choices.iter().map(|kind| kind.label()).collect();
    let Some(index) = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("选择启动方式")
        .items(&labels)
        .default(0)
        .interact_opt()?
    else {
        return Ok(None);
    };
    Ok(choices.get(index).copied())
}

/// 根据启动环境选择具体工具；取消返回 None。
fn choose_tool(kind: LauncherKind, theme: &ColorfulTheme) -> Result<Option<ToolKind>> {
    let tools: Vec<ToolKind> = match kind {
        LauncherKind::Wsl | LauncherKind::PowerShell => {
            vec![
                ToolKind::Opencode,
                ToolKind::CursorAgent,
                ToolKind::Terminal,
            ]
        }
        LauncherKind::VsCode => vec![ToolKind::Vscode, ToolKind::Cursor],
    };
    let labels: Vec<&str> = tools.iter().map(|tool| tool.label()).collect();
    let Some(index) = Select::with_theme(theme)
        .with_prompt(format!("{}：选择启动方式", kind.label()))
        .items(&labels)
        .default(0)
        .interact_opt()?
    else {
        return Ok(None);
    };
    Ok(tools.get(index).copied())
}

pub fn run(data: &mut ProjectData) -> Result<()> {
    let theme = ColorfulTheme::default();
    loop {
        let mut labels: Vec<String> = data.groups.iter().map(|group| group.name.clone()).collect();
        let group_count = labels.len();
        labels.push("新增分组".into());
        labels.push("退出".into());
        let Some(index) = Select::with_theme(&theme)
            .with_prompt("选择项目分组")
            .items(&labels)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        if index == group_count {
            add_group(data, &theme)?;
            continue;
        }
        if index == group_count + 1 {
            return Ok(());
        }
        let group_name = data.groups[index].name.clone();
        if project_menu(data, group_name, &theme)? {
            return Ok(());
        }
    }
}

fn project_menu(
    data: &mut ProjectData,
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
            .map(|project| format!("{}  ({})", project.name, project.path))
            .collect();
        let project_count = labels.len();
        labels.push("新增项目".into());
        labels.push("管理本分组".into());
        labels.push("返回分组列表".into());
        let Some(index) = Select::with_theme(theme)
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
        if project_actions(data, &group_name, &project_id, theme)? {
            return Ok(true);
        }
    }
}

fn project_actions(
    data: &mut ProjectData,
    group_name: &str,
    project_id: &str,
    theme: &ColorfulTheme,
) -> Result<bool> {
    loop {
        let (group_index, project_index) =
            ops::find_project_by_id(data, project_id, Some(group_name))?;
        let project = data.groups[group_index].projects[project_index].clone();
        let actions = ["打开", "编辑", "删除", "移动到其他分组", "返回项目列表"];
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
                let Some(kind) = choose_launcher(&project)? else {
                    continue;
                };
                let Some(tool) = choose_tool(kind, theme)? else {
                    continue;
                };
                launcher::launch(&project, group_name, kind, tool)
                    .map_err(|error| anyhow::anyhow!(error))?;
                continue;
            }
            1 => edit_project(data, group_name, &project, theme)?,
            2 => {
                if Confirm::with_theme(theme)
                    .with_prompt(format!("确认删除项目 `{}`？", project.name))
                    .default(false)
                    .interact()?
                {
                    let removed = ops::remove_project_by_id(data, project_id, Some(group_name))?;
                    save(data)?;
                    println!("已删除项目: {}", removed.name);
                    return Ok(false);
                }
            }
            3 => {
                if move_project(data, group_name, project_id, theme)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
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
            ops::remove_group(data, group_name, true)?;
            save(data)?;
            println!("分组已删除: {group_name}");
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

fn save(data: &ProjectData) -> Result<()> {
    if Store::save(data) {
        Ok(())
    } else {
        bail!("保存数据失败")
    }
}

pub fn launch_direct(project: &Project, group_name: &str, kind: LauncherKind) -> Result<()> {
    if matches!(kind, LauncherKind::PowerShell | LauncherKind::VsCode)
        && !project.has_windows_path()
    {
        bail!(
            "项目 `{}` 没有 Windows 路径，不能使用 {}",
            project.name,
            kind.label()
        );
    }
    let tool = match kind {
        LauncherKind::Wsl | LauncherKind::PowerShell => ToolKind::Terminal,
        LauncherKind::VsCode => ToolKind::Vscode,
    };
    launcher::launch(project, group_name, kind, tool).map_err(|error| anyhow::anyhow!(error))
}
