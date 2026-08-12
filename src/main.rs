mod config;
mod launcher;
mod menu;
mod models;
mod ops;
mod store;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::config::{AppConfig, Config, ConfigEnv, add_tool, edit_tool, remove_tool, reset_config};
use crate::launcher::LaunchEnv;
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
    #[command(about = "运行项目的自定义命令")]
    Run(RunArgs),
    #[command(about = "添加项目")]
    Add(AddArgs),
    #[command(about = "编辑项目")]
    Edit(EditArgs),
    #[command(about = "删除项目")]
    Rm(RmArgs),
    #[command(about = "移动项目到另一个分组")]
    Mv(MoveArgs),
    #[command(subcommand, about = "管理分组")]
    Group(GroupCommand),
    #[command(subcommand, about = "管理启动配置")]
    Config(ConfigCommand),
    #[command(subcommand, about = "管理回收站")]
    Trash(TrashCommand),
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

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    selector: ProjectSelector,
    #[arg(long, help = "列出项目的自定义命令")]
    list: bool,
    /// 自定义命令名
    command: Option<String>,
}

#[derive(Args)]
struct RmArgs {
    #[command(flatten)]
    selector: ProjectSelector,
    #[arg(long, help = "彻底删除，不进入回收站")]
    force: bool,
}

#[derive(Subcommand)]
enum TrashCommand {
    #[command(about = "列出回收站内容")]
    List,
    #[command(about = "恢复指定项")]
    Restore { id: String },
    #[command(about = "清空回收站")]
    Empty {
        #[arg(long, help = "跳过确认")]
        force: bool,
    },
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

#[derive(Subcommand)]
enum ConfigCommand {
    #[command(about = "列出配置工具")]
    List {
        #[arg(long)]
        env: Option<String>,
    },
    #[command(about = "添加工具")]
    Add {
        #[arg(long)]
        env: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        command: String,
    },
    #[command(about = "编辑工具")]
    Edit {
        #[arg(long)]
        env: String,
        #[arg(long)]
        index: usize,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        command: Option<String>,
    },
    #[command(about = "删除工具")]
    Rm {
        #[arg(long)]
        env: String,
        #[arg(long)]
        index: usize,
    },
    #[command(about = "恢复默认配置")]
    Reset,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load();
    match cli.command {
        None | Some(Command::Menu) => {
            let mut data = Store::load();
            menu::run(&mut data, &mut config)
        }
        Some(Command::Ls { group, json }) => cmd_ls(group.as_deref(), json),
        Some(Command::Open(args)) => cmd_open(args),
        Some(Command::Wsl(selector)) => cmd_direct(&selector, LaunchEnv::Wsl),
        Some(Command::PowerShell(selector)) => cmd_direct(&selector, LaunchEnv::PowerShell),
        Some(Command::Code(selector)) => cmd_code(&selector, &config),
        Some(Command::Path(selector)) => cmd_path(&selector, false),
        Some(Command::WslPath(selector)) => cmd_path(&selector, true),
        Some(Command::Add(args)) => cmd_add(args),
        Some(Command::Edit(args)) => cmd_edit(args),
        Some(Command::Rm(args)) => cmd_rm(args),
        Some(Command::Mv(args)) => cmd_mv(args),
        Some(Command::Run(args)) => cmd_run(args),
        Some(Command::Group(command)) => cmd_group(command),
        Some(Command::Config(command)) => cmd_config(command),
        Some(Command::Trash(command)) => cmd_trash(command),
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
                ..Default::default()
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
            let path = if project.has_windows_path() {
                project.path.clone()
            } else {
                project.linux_path()
            };
            if path.is_empty() {
                println!("  {}", project.name);
            } else {
                println!("  {}  ({})", project.name, path);
            }
        }
    }
}

fn cmd_open(args: OpenArgs) -> Result<()> {
    let config = Config::load();
    let (project, group_name) = find_selected(&args.selector)?;
    let direct = if args.wsl {
        Some((LaunchEnv::Wsl, String::new()))
    } else if args.powershell {
        Some((LaunchEnv::PowerShell, String::new()))
    } else if args.code {
        Some((LaunchEnv::Ide, first_ide_command(&config)?))
    } else {
        None
    };
    if let Some((env, command)) = direct {
        let child = launcher::spawn_direct(&project, &group_name, env, &command)
            .map_err(anyhow::Error::msg)?;
        if let Some(child) = child {
            launcher::wait_direct(child).map_err(anyhow::Error::msg)?;
        }
        return Ok(());
    }
    let Some(option) = menu::choose_launch_option(&project, &config)? else {
        return Ok(());
    };
    let child = launcher::spawn_direct(&project, &group_name, option.env, &option.command)
        .map_err(anyhow::Error::msg)?;
    if let Some(child) = child {
        launcher::wait_direct(child).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

fn cmd_direct(selector: &ProjectSelector, env: LaunchEnv) -> Result<()> {
    let (project, group_name) = find_selected(selector)?;
    let child =
        launcher::spawn_direct(&project, &group_name, env, "").map_err(anyhow::Error::msg)?;
    if let Some(child) = child {
        launcher::wait_direct(child).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

/// `pcs code` 使用 config.ide 的第一个工具（默认 VS Code），跟随用户配置。
fn cmd_code(selector: &ProjectSelector, config: &AppConfig) -> Result<()> {
    let command = first_ide_command(config)?;
    let (project, group_name) = find_selected(selector)?;
    let child = launcher::spawn_direct(&project, &group_name, LaunchEnv::Ide, &command)
        .map_err(anyhow::Error::msg)?;
    if let Some(child) = child {
        launcher::wait_direct(child).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

fn first_ide_command(config: &AppConfig) -> Result<String> {
    config
        .ide
        .first()
        .map(|tool| tool.command.clone())
        .ok_or_else(|| anyhow::anyhow!("IDE 工具列表为空，请用 pcs config 添加工具或恢复默认配置"))
}

fn parse_config_env(name: &str) -> Result<ConfigEnv> {
    if name.eq_ignore_ascii_case("wsl") {
        Ok(ConfigEnv::Wsl)
    } else if name.eq_ignore_ascii_case("powershell") {
        Ok(ConfigEnv::PowerShell)
    } else if name.eq_ignore_ascii_case("ide") {
        Ok(ConfigEnv::Ide)
    } else {
        bail!("未知环境: {name}，可选 wsl/powershell/ide")
    }
}

/// 写盘失败仅警告（延续「仅警告」哲学），不中断操作。
fn save_config(config: &AppConfig) {
    if !Config::save(config) {
        eprintln!("警告：无法写入 config.json，本次修改未持久化。");
    }
}

fn cmd_config(command: ConfigCommand) -> Result<()> {
    let mut config = Config::load();
    match command {
        ConfigCommand::List { env } => {
            let envs: Vec<ConfigEnv> = match env {
                Some(name) => vec![parse_config_env(&name)?],
                None => vec![ConfigEnv::Wsl, ConfigEnv::PowerShell, ConfigEnv::Ide],
            };
            for env in envs {
                println!("[{}]", env.label());
                let tools = env.tools(&config);
                if tools.is_empty() {
                    println!("  （暂无工具）");
                } else {
                    for tool in tools {
                        println!("  {} ({})", tool.name, tool.command);
                    }
                }
            }
        }
        ConfigCommand::Add { env, name, command } => {
            add_tool(&mut config, parse_config_env(&env)?, &name, &command)
                .map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已添加: {}", name.trim());
        }
        ConfigCommand::Edit {
            env,
            index,
            name,
            command,
        } => {
            let env = parse_config_env(&env)?;
            let tools = env.tools(&config);
            let current = tools
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("工具索引无效"))?;
            let name = name.unwrap_or_else(|| current.name.clone());
            let command = command.unwrap_or_else(|| current.command.clone());
            edit_tool(&mut config, env, index, &name, &command).map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已更新: {}", name.trim());
        }
        ConfigCommand::Rm { env, index } => {
            remove_tool(&mut config, parse_config_env(&env)?, index).map_err(anyhow::Error::msg)?;
            save_config(&config);
            println!("工具已删除。");
        }
        ConfigCommand::Reset => {
            reset_config(&mut config);
            save_config(&config);
            println!("配置已恢复默认。");
        }
    }
    Ok(())
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

fn cmd_rm(args: RmArgs) -> Result<()> {
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

/// 回收站列表行：`[项目] 组名/名称 (删除于 2026-08-12)  @id`；
/// `[分组] 分组名 (N 个项目)  @id`。尾部附 id 便于 `trash restore`。
fn trash_lines(data: &ProjectData) -> Vec<String> {
    data.trash
        .iter()
        .map(|item| {
            let id_suffix = format!("  @{}", item.id);
            if item.is_group() {
                format!(
                    "[分组] {} ({} 个项目){}",
                    item.name,
                    item.projects.len(),
                    id_suffix
                )
            } else {
                format!(
                    "[项目] {}/{} (删除于 {}){}",
                    item.group,
                    item.name,
                    models::format_date(item.deleted_at),
                    id_suffix
                )
            }
        })
        .collect()
}

/// 恢复回收站项；返回提示信息。
fn trash_restore(data: &mut ProjectData, id: &str) -> Result<String> {
    ops::restore_item(data, id).map_err(anyhow::Error::msg)
}

fn cmd_trash(command: TrashCommand) -> Result<()> {
    match command {
        TrashCommand::List => {
            let data = Store::load();
            let lines = trash_lines(&data);
            if lines.is_empty() {
                println!("回收站为空");
            } else {
                for line in lines {
                    println!("{line}");
                }
            }
        }
        TrashCommand::Restore { id } => {
            let mut data = Store::load();
            let message = trash_restore(&mut data, &id)?;
            save(&data)?;
            println!("{message}");
        }
        TrashCommand::Empty { force } => {
            if !force
                && !dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("确认清空回收站？")
                    .default(false)
                    .interact()?
            {
                return Ok(());
            }
            let mut data = Store::load();
            ops::empty_trash(&mut data);
            save(&data)?;
            println!("回收站已清空。");
        }
    }
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

/// 项目自定义命令列表行：`名称 (env 命令)`。
fn command_lines(project: &Project) -> Vec<String> {
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
fn resolve_run_command(project: &Project, name: &str) -> Result<(LaunchEnv, String)> {
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

fn cmd_run(args: RunArgs) -> Result<()> {
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
    let child =
        launcher::spawn_direct(project, &group_name, env, &command).map_err(anyhow::Error::msg)?;
    if let Some(child) = child {
        launcher::wait_direct(child).map_err(anyhow::Error::msg)?;
    }
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
            ..Default::default()
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

    #[test]
    fn parse_env_case_insensitive() {
        assert_eq!(parse_config_env("WSL").unwrap(), ConfigEnv::Wsl);
        assert_eq!(
            parse_config_env("powershell").unwrap(),
            ConfigEnv::PowerShell
        );
        assert_eq!(parse_config_env("IDE").unwrap(), ConfigEnv::Ide);
        assert!(parse_config_env("bash").is_err());
    }

    fn trash_test_data() -> ProjectData {
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                projects: vec![Project {
                    id: "11111111-0000-0000-0000-000000000000".into(),
                    ..Project::new("app", r"E:\w\app", "")
                }],
            }],
            trash: vec![
                models::DeletedItem {
                    id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
                    kind: "project".into(),
                    group: "Work".into(),
                    name: "old-app".into(),
                    path: r"E:\old".into(),
                    wsl_path: "/mnt/e/old".into(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    projects: Vec::new(),
                    deleted_at: 1700000000,
                },
                models::DeletedItem {
                    id: "bbbbbbbb-0000-0000-0000-000000000000".into(),
                    kind: "group".into(),
                    group: String::new(),
                    name: "Archive".into(),
                    path: String::new(),
                    wsl_path: String::new(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    projects: vec![Project::new("inner", r"E:\inner", "")],
                    deleted_at: 1700000000,
                },
            ],
        }
    }

    #[test]
    fn trash_lines_formats_project_and_group() {
        let data = trash_test_data();
        let lines = trash_lines(&data);
        assert_eq!(lines.len(), 2);
        let date = models::format_date(1700000000);
        assert_eq!(
            lines[0],
            format!("[项目] Work/old-app (删除于 {date})  @aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        );
        assert_eq!(
            lines[1],
            "[分组] Archive (1 个项目)  @bbbbbbbb-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn trash_lines_empty() {
        let data = ProjectData::default();
        assert!(trash_lines(&data).is_empty());
    }

    #[test]
    fn trash_restore_unknown_id_errors() {
        let mut data = ProjectData::default();
        assert!(trash_restore(&mut data, "zzzz").is_err());
    }

    #[test]
    fn trash_restore_restores_and_removes_from_trash() {
        let mut data = trash_test_data();
        let message = trash_restore(&mut data, "@aaaaaaaa").unwrap();
        assert_eq!(message, "已恢复项目: old-app");
        assert_eq!(data.groups[0].projects.len(), 2);
        assert_eq!(data.trash.len(), 1);
    }

    fn run_data() -> ProjectData {
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
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
}
