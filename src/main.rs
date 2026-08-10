mod launcher;
mod menu;
mod models;
mod ops;
mod store;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::launcher::LauncherKind;
use crate::models::{Project, ProjectData};
use crate::store::Store;

#[derive(Parser)]
#[command(name = "pcs", about = "Project Center 项目管理器")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "打开项目选择菜单")]
    Menu,
    #[command(about = "列出项目")]
    Ls {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "选择启动方式并打开项目")]
    Open(OpenArgs),
    #[command(name = "wsl", about = "使用 WSL 打开项目")]
    Wsl(ProjectSelector),
    #[command(name = "ps", about = "使用 PowerShell 打开项目")]
    PowerShell(ProjectSelector),
    #[command(name = "code", about = "使用 VS Code 打开项目")]
    Code(ProjectSelector),
    #[command(name = "path", about = "打印 Windows 路径")]
    Path(ProjectSelector),
    #[command(name = "wslpath", about = "打印 WSL 路径")]
    WslPath(ProjectSelector),
    #[command(about = "添加项目")]
    Add(AddArgs),
    #[command(about = "编辑项目")]
    Edit(EditArgs),
    #[command(about = "删除项目")]
    Rm(ProjectSelector),
    #[command(about = "移动项目到另一个分组")]
    Mv(MoveArgs),
    #[command(subcommand, about = "管理分组")]
    Group(GroupCommand),
}

#[derive(Args)]
struct ProjectSelector {
    name: String,
    #[arg(long)]
    group: Option<String>,
}

#[derive(Args)]
struct OpenArgs {
    #[command(flatten)]
    selector: ProjectSelector,
    #[arg(short = 'w', long, conflicts_with_all = ["powershell", "code"])]
    wsl: bool,
    #[arg(short = 'p', long, conflicts_with_all = ["wsl", "code"])]
    powershell: bool,
    #[arg(short = 'c', long, conflicts_with_all = ["wsl", "powershell"])]
    code: bool,
}

#[derive(Args)]
struct AddArgs {
    name: String,
    #[arg(long)]
    group: String,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    wsl_path: Option<String>,
}

#[derive(Args)]
struct EditArgs {
    name: String,
    #[arg(long)]
    group: Option<String>,
    #[arg(long)]
    new_name: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    wsl_path: Option<String>,
}

#[derive(Args)]
struct MoveArgs {
    name: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    group: Option<String>,
}

#[derive(Subcommand)]
enum GroupCommand {
    #[command(about = "添加分组")]
    Add { name: String },
    #[command(about = "重命名分组")]
    Rename { old_name: String, new_name: String },
    #[command(about = "删除分组")]
    Rm {
        name: String,
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None | Some(Command::Menu) => {
            let mut data = Store::load();
            menu::run(&mut data)
        }
        Some(Command::Ls { group, json }) => cmd_ls(group.as_deref(), json),
        Some(Command::Open(args)) => cmd_open(args),
        Some(Command::Wsl(selector)) => cmd_direct(&selector, LauncherKind::Wsl),
        Some(Command::PowerShell(selector)) => cmd_direct(&selector, LauncherKind::PowerShell),
        Some(Command::Code(selector)) => cmd_direct(&selector, LauncherKind::VsCode),
        Some(Command::Path(selector)) => cmd_path(&selector, false),
        Some(Command::WslPath(selector)) => cmd_path(&selector, true),
        Some(Command::Add(args)) => cmd_add(args),
        Some(Command::Edit(args)) => cmd_edit(args),
        Some(Command::Rm(selector)) => cmd_rm(&selector),
        Some(Command::Mv(args)) => cmd_mv(args),
        Some(Command::Group(command)) => cmd_group(command),
    }
}

fn find_selected(selector: &ProjectSelector) -> Result<(Project, String)> {
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    Ok((
        data.groups[group_index].projects[project_index].clone(),
        data.groups[group_index].name.clone(),
    ))
}

/// 名字以 `@` 开头时按项目 id 查找，否则按名字查找。
fn select_project(data: &ProjectData, name: &str, group: Option<&str>) -> Result<(usize, usize)> {
    if let Some(id) = name.strip_prefix('@') {
        ops::find_project_by_id(data, id, group)
    } else {
        ops::find_project(data, name, group)
    }
}

/// 把 `@<id>` 解析为真实项目名及所在分组，供 edit / mv / rm 复用现有按名操作；
/// 返回所在分组可避免跨组同名导致的二次查找歧义。
fn resolve_project_name(
    data: &ProjectData,
    name: &str,
    group: Option<&str>,
) -> Result<(String, Option<String>)> {
    if name.starts_with('@') {
        let (group_index, project_index) = select_project(data, name, group)?;
        Ok((
            data.groups[group_index].projects[project_index]
                .name
                .clone(),
            Some(data.groups[group_index].name.clone()),
        ))
    } else {
        Ok((name.to_string(), None))
    }
}

fn cmd_ls(group: Option<&str>, json: bool) -> Result<()> {
    let data = Store::load();
    if let Some(group_name) = group {
        let index = ops::find_group(&data, group_name)?;
        if json {
            let selected = ProjectData {
                groups: vec![data.groups[index].clone()],
            };
            println!("{}", serde_json::to_string_pretty(&selected)?);
        } else {
            print_group(&data.groups[index]);
        }
        return Ok(());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else if data.groups.is_empty() {
        println!("暂无项目。");
    } else {
        for group in &data.groups {
            print_group(group);
        }
    }
    Ok(())
}

fn print_group(group: &models::Group) {
    println!("[{}]", group.name);
    if group.projects.is_empty() {
        println!("  （暂无项目）");
    } else {
        for project in &group.projects {
            println!("  {}  {}", project.name, project.path);
        }
    }
}

fn cmd_open(args: OpenArgs) -> Result<()> {
    let (project, group_name) = find_selected(&args.selector)?;
    let direct_kind = if args.wsl {
        Some(LauncherKind::Wsl)
    } else if args.powershell {
        Some(LauncherKind::PowerShell)
    } else if args.code {
        Some(LauncherKind::VsCode)
    } else {
        None
    };
    if let Some(kind) = direct_kind {
        return menu::launch_direct(&project, &group_name, kind);
    }
    let Some(kind) = menu::choose_launcher(&project)? else {
        return Ok(());
    };
    menu::launch_direct(&project, &group_name, kind)
}

fn cmd_direct(selector: &ProjectSelector, kind: LauncherKind) -> Result<()> {
    let (project, group_name) = find_selected(selector)?;
    menu::launch_direct(&project, &group_name, kind)
}

fn cmd_path(selector: &ProjectSelector, wsl: bool) -> Result<()> {
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

fn cmd_add(args: AddArgs) -> Result<()> {
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
    let mut data = Store::load();
    ops::add_project(
        &mut data,
        &args.group,
        Project::new(args.name, path, wsl_path),
    )?;
    save(&data)?;
    println!("项目已添加。");
    Ok(())
}

fn cmd_edit(args: EditArgs) -> Result<()> {
    let path = args
        .dir
        .map(|path| path.to_string_lossy().trim().to_string());
    let mut data = Store::load();
    let (name, resolved_group) = resolve_project_name(&data, &args.name, args.group.as_deref())?;
    let group = args.group.as_deref().or(resolved_group.as_deref());
    ops::edit_project(
        &mut data,
        &name,
        group,
        args.new_name.as_deref(),
        path.as_deref(),
        args.wsl_path.as_deref(),
    )?;
    save(&data)?;
    println!("项目已更新。");
    Ok(())
}

fn cmd_rm(selector: &ProjectSelector) -> Result<()> {
    let mut data = Store::load();
    let (name, resolved_group) =
        resolve_project_name(&data, &selector.name, selector.group.as_deref())?;
    let group = selector.group.as_deref().or(resolved_group.as_deref());
    let removed = ops::remove_project(&mut data, &name, group)?;
    save(&data)?;
    println!("已删除项目: {}", removed.name);
    Ok(())
}

fn cmd_mv(args: MoveArgs) -> Result<()> {
    let mut data = Store::load();
    let (name, resolved_group) = resolve_project_name(&data, &args.name, args.group.as_deref())?;
    let group = args.group.as_deref().or(resolved_group.as_deref());
    ops::move_project(&mut data, &name, group, &args.to)?;
    save(&data)?;
    println!("项目已移动到: {}", args.to);
    Ok(())
}

fn cmd_group(command: GroupCommand) -> Result<()> {
    let mut data = Store::load();
    match command {
        GroupCommand::Add { name } => {
            ops::add_group(&mut data, &name)?;
            println!("分组已添加: {name}");
        }
        GroupCommand::Rename { old_name, new_name } => {
            ops::rename_group(&mut data, &old_name, &new_name)?;
            println!("分组已重命名: {new_name}");
        }
        GroupCommand::Rm { name, force } => {
            ops::remove_group(&mut data, &name, force)?;
            println!("分组已删除: {name}");
        }
    }
    save(&data)
}

fn save(data: &ProjectData) -> Result<()> {
    if Store::save(data) {
        Ok(())
    } else {
        bail!("保存数据失败")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Group;

    fn dup_data() -> ProjectData {
        ProjectData {
            groups: vec![
                Group {
                    name: "Work".into(),
                    projects: vec![Project {
                        id: "11111111-0000-0000-0000-000000000000".into(),
                        ..Project::new("app", r"E:\w\app", "")
                    }],
                },
                Group {
                    name: "Personal".into(),
                    projects: vec![Project {
                        id: "22222222-0000-0000-0000-000000000000".into(),
                        ..Project::new("app", r"E:\p\app", "")
                    }],
                },
            ],
        }
    }

    #[test]
    fn resolve_id_returns_name_and_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "@11111111", None).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group.as_deref(), Some("Work"));
    }

    #[test]
    fn resolve_name_keeps_no_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "app", Some("Work")).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group, None);
    }

    #[test]
    fn resolve_id_with_explicit_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "@22222222", Some("Personal")).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group.as_deref(), Some("Personal"));
    }

    #[test]
    fn resolve_id_uses_selector_group() {
        let data = dup_data();
        let result = resolve_project_name(&data, "@11111111", Some("Personal"));
        assert!(result.is_err());
    }
}
