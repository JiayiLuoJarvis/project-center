mod config;
mod launcher;
mod menu;
mod models;
mod ops;
mod secret;
mod store;
mod tui;

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
    #[command(name = "ssh", about = "SSH 连接远程项目")]
    Ssh(ProjectSelector),
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
    #[command(subcommand, about = "管理查看 PIN")]
    Pin(PinCommand),
    #[command(subcommand, about = "查看项目保存的 SSH 秘密")]
    Secret(SecretCommand),
    #[command(name = "__askpass", hide = true)]
    Askpass { prompt: String },
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
    #[arg(short = 'w', long, conflicts_with_all = ["powershell", "code", "ssh"])]
    wsl: bool,
    #[arg(short = 'p', long, conflicts_with_all = ["wsl", "code", "ssh"])]
    powershell: bool,
    #[arg(short = 'c', long, conflicts_with_all = ["wsl", "powershell", "ssh"])]
    code: bool,
    #[arg(short = 's', long, conflicts_with_all = ["wsl", "powershell", "code"])]
    ssh: bool,
}

#[derive(Args)]
struct AddArgs {
    name: String,
    #[arg(long)]
    group: String,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    wsl_path: Option<String>,
    #[arg(
        long = "ssh",
        help = "SSH 目标（user@host 或 user@host:端口）；设置后为 SSH 远程项目"
    )]
    ssh: Option<String>,
    #[arg(
        long = "ssh-path",
        help = "远程 Linux 路径（登录后尝试 cd，失败则留在默认 shell）"
    )]
    ssh_path: Option<String>,
    #[arg(long = "ssh-key", help = "导入私钥文件（DPAPI 加密存入数据目录）")]
    ssh_key: Option<PathBuf>,
    #[arg(long = "password-stdin", help = "从 stdin 读入登录密码并加密保存")]
    password_stdin: bool,
    #[arg(long = "key-pass-stdin", help = "从 stdin 读入私钥口令并加密保存")]
    key_pass_stdin: bool,
}

#[derive(Args)]
struct EditArgs {
    name: String,
    #[arg(long)]
    group: Option<String>,
    #[arg(long)]
    new_name: Option<String>,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    wsl_path: Option<String>,
    #[arg(long = "ssh", help = "更新 SSH 目标（user@host 或 user@host:端口）")]
    ssh: Option<String>,
    #[arg(long = "ssh-path", help = "更新远程 Linux 路径；空串清除")]
    ssh_path: Option<String>,
    #[arg(long = "ssh-key", help = "导入私钥文件（覆盖已存密钥）")]
    ssh_key: Option<PathBuf>,
    #[arg(long = "password-stdin", help = "从 stdin 读入登录密码并加密保存")]
    password_stdin: bool,
    #[arg(long = "key-pass-stdin", help = "从 stdin 读入私钥口令并加密保存")]
    key_pass_stdin: bool,
    #[arg(
        long = "clear-ssh",
        help = "清除 SSH 配置（目标、远程路径、密钥与全部秘密）"
    )]
    clear_ssh: bool,
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
enum PinCommand {
    #[command(about = "设置查看 PIN（查看保存的密码/口令时必须）")]
    Set,
    #[command(about = "修改 PIN（需验证旧 PIN）")]
    Change,
    #[command(about = "重置 PIN：清除 PIN 并清空所有已存密码、口令与密钥文件")]
    Reset {
        #[arg(long, help = "跳过确认")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum SecretCommand {
    #[command(about = "查看项目的保存密码与私钥口令（需 PIN）")]
    Show(ProjectSelector),
}

#[derive(Subcommand)]
enum GroupCommand {
    #[command(about = "添加分组")]
    Add {
        name: String,
        #[arg(long)]
        alias: Option<String>,
    },
    #[command(about = "重命名分组")]
    Rename {
        old_name: String,
        new_name: String,
        #[arg(long)]
        alias: Option<String>,
    },
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
    // OpenSSH 把 SSH_ASKPASS 当独立程序调用：`pcs.exe <prompt>`，不会带
    // `__askpass` 子命令。父进程注入 PCS_ASKPASS_TOKEN 时在 clap 之前拦截，
    // 否则 clap 会把 prompt（含空格/撇号）当成未识别子命令并写 stderr，
    // 导致认证失败且污染控制台。
    if std::env::var_os("PCS_ASKPASS_TOKEN").is_some() {
        return cmd_askpass(&askpass_prompt_from_args());
    }
    let cli = Cli::parse();
    // 手工/测试入口：`pcs __askpass <prompt>`（无 token 时 cmd_askpass 输出空）。
    if let Some(Command::Askpass { prompt }) = &cli.command {
        return cmd_askpass(prompt);
    }
    let mut config = Config::load();
    match cli.command {
        None | Some(Command::Menu) => {
            let mut data = Store::load();
            tui::run(&mut data, &mut config)
        }
        Some(Command::Ls { group, json }) => cmd_ls(group.as_deref(), json),
        Some(Command::Open(args)) => cmd_open(args),
        Some(Command::Wsl(selector)) => cmd_direct(&selector, LaunchEnv::Wsl),
        Some(Command::PowerShell(selector)) => cmd_direct(&selector, LaunchEnv::PowerShell),
        Some(Command::Ssh(selector)) => cmd_direct(&selector, LaunchEnv::Ssh),
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
        Some(Command::Pin(command)) => cmd_pin(command),
        Some(Command::Secret(command)) => cmd_secret(command),
        // 运行时不可达（上方已提前返回），仅为 match 穷尽性保留。
        Some(Command::Askpass { prompt }) => cmd_askpass(&prompt),
    }
}

/// 从 argv 提取 askpass prompt。兼容两种调用：
/// - OpenSSH：`pcs.exe <prompt>`（prompt 可能含空格，已由系统按单参传入）
/// - 手工：`pcs.exe __askpass <prompt>`
fn askpass_prompt_from_args() -> String {
    askpass_prompt_from(std::env::args().skip(1))
}

/// 纯解析：供单元测试直接喂入参数列表，避免与生产逻辑分叉。
fn askpass_prompt_from(mut args: impl Iterator<Item = String>) -> String {
    match args.next() {
        Some(first) if first == "__askpass" => args.next().unwrap_or_default(),
        Some(first) => first,
        None => String::new(),
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

fn cmd_direct(selector: &ProjectSelector, env: LaunchEnv) -> Result<()> {
    let (project, group_name) = find_selected(selector)?;
    let spawned =
        launcher::spawn_direct(&project, &group_name, env, "").map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
    Ok(())
}

/// `pcs code` 使用 config.ide 的第一个工具（默认 VS Code），跟随用户配置。
fn cmd_code(selector: &ProjectSelector, config: &AppConfig) -> Result<()> {
    let command = first_ide_command(config)?;
    let (project, group_name) = find_selected(selector)?;
    let spawned = launcher::spawn_direct(&project, &group_name, LaunchEnv::Ide, &command)
        .map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
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
    let mut data = Store::load();
    let mut project = if let Some(ssh_target) = args.ssh.as_deref() {
        // SSH 远程项目：不弹目录选择器，path 复用为远程 Linux 路径。
        let ssh_target = ssh_target.trim().to_string();
        if ssh_target.is_empty() {
            bail!("SSH 目标不能为空");
        }
        let ssh_path = args.ssh_path.as_deref().unwrap_or("").trim().to_string();
        Project::new(&args.name, ssh_path, "").with_ssh_target(ssh_target)
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
    apply_ssh_secrets(
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

fn cmd_edit(args: EditArgs) -> Result<()> {
    let path = args
        .dir
        .map(|path| path.to_string_lossy().trim().to_string());
    let mut data = Store::load();
    let (name, resolved_group) = resolve_project_name(&data, &args.name, args.group.as_deref())?;
    let group = args.group.as_deref().or(resolved_group.as_deref());
    ops::edit_project_full(
        &mut data,
        &name,
        group,
        args.new_name.as_deref(),
        args.alias.as_deref(),
        path.as_deref(),
        args.wsl_path.as_deref(),
    )?;
    if args.clear_ssh {
        clear_project_ssh(&mut data, &name, group);
    }
    if let Some(ssh_target) = args.ssh.as_deref() {
        let ssh_target = ssh_target.trim().to_string();
        if ssh_target.is_empty() {
            bail!("SSH 目标不能为空（清除 SSH 配置请用 --clear-ssh）");
        }
        ops::edit_ssh_fields(&mut data, &name, group, |p| {
            p.ssh_target = ssh_target;
        })?;
    }
    if let Some(ssh_path) = args.ssh_path.as_deref() {
        let ssh_path = ssh_path.trim().to_string();
        ops::edit_ssh_fields(&mut data, &name, group, |p| {
            p.path = ssh_path;
        })?;
    }
    apply_ssh_secrets(
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

/// 导入私钥与读入密码/口令（stdin），DPAPI 加密后写入项目。
fn apply_ssh_secrets(
    data: &mut ProjectData,
    name: &str,
    group: Option<&str>,
    ssh_key: Option<&std::path::Path>,
    password_stdin: bool,
    key_pass_stdin: bool,
) -> Result<()> {
    if let Some(key_path) = ssh_key {
        let plain = std::fs::read_to_string(key_path)
            .map_err(|e| anyhow::anyhow!("读取私钥文件失败: {e}"))?;
        if plain.trim().is_empty() {
            bail!("私钥文件为空");
        }
        let relative = {
            let (group_index, project_index) = select_project(data, name, group)?;
            let id = data.groups[group_index].projects[project_index].id.clone();
            secret::write_key_file(&id, &plain).map_err(anyhow::Error::msg)?
        };
        let source = key_path.to_string_lossy().into_owned();
        ops::edit_ssh_fields(data, name, group, |p| {
            p.ssh_key_file = relative;
            p.ssh_key_path = source;
        })?;
    }
    if password_stdin {
        let plain = read_stdin_line("登录密码")?;
        if !plain.is_empty() {
            let enc = secret::protect(&plain).map_err(anyhow::Error::msg)?;
            ops::edit_ssh_fields(data, name, group, |p| {
                p.ssh_password_enc = enc;
            })?;
        }
    }
    if key_pass_stdin {
        let plain = read_stdin_line("私钥口令")?;
        if !plain.is_empty() {
            let enc = secret::protect(&plain).map_err(anyhow::Error::msg)?;
            ops::edit_ssh_fields(data, name, group, |p| {
                p.ssh_key_pass_enc = enc;
            })?;
        }
    }
    Ok(())
}

/// 清除项目的 SSH 配置（目标、远程路径、密钥引用与全部秘密）。
fn clear_project_ssh(data: &mut ProjectData, name: &str, group: Option<&str>) {
    let (group_index, project_index) = match select_project(data, name, group) {
        Ok(indexes) => indexes,
        Err(_) => return,
    };
    let old_key = data.groups[group_index].projects[project_index]
        .ssh_key_file
        .clone();
    if !old_key.trim().is_empty()
        && secret::delete_key_file(&old_key).is_err()
        && !data.pending_key_deletes.iter().any(|r| r == &old_key)
    {
        data.pending_key_deletes.push(old_key);
    }
    if let Ok(project) = ops::edit_ssh_fields(data, name, group, |p| {
        p.ssh_target.clear();
        p.ssh_key_file.clear();
        p.ssh_key_path.clear();
        p.ssh_password_enc.clear();
        p.ssh_key_pass_enc.clear();
    }) {
        let _ = project;
    }
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
            if !force {
                bail!("清空回收站需指定 --force，例如: pcs trash empty --force");
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
    let spawned =
        launcher::spawn_direct(project, &group_name, env, &command).map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)?;
    Ok(())
}

/// 从 stdin 读取一行（`--password-stdin` / `--key-pass-stdin`）。
/// 控制台输入不回显（设计 §6：密码/口令录入不回显）；
/// 重定向/管道走普通读行，脚本喂入不受影响。
fn read_stdin_line(label: &str) -> Result<String> {
    read_hidden_line(&format!("请输入{label}: "))
}

/// 不回显读入一行：控制台走 Win32 逐字符读；重定向/管道回退普通 stdin。
/// 提示语走 stderr，保持 stdout 干净供脚本使用。
#[cfg(windows)]
fn read_hidden_line(prompt: &str) -> Result<String> {
    use windows_sys::Win32::System::Console::{
        ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, GetConsoleMode, GetStdHandle,
        ReadConsoleW, STD_INPUT_HANDLE, SetConsoleMode,
    };
    eprint!("{prompt}");
    use std::io::Write as _;
    std::io::stderr().flush()?;

    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut original = 0;
        if GetConsoleMode(handle, &mut original) != 0 {
            let raw = original & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT) | ENABLE_PROCESSED_INPUT;
            if SetConsoleMode(handle, raw) != 0 {
                let mut chars: Vec<u16> = Vec::new();
                let mut buf = [0u16; 1];
                let mut read;
                loop {
                    read = 0;
                    if ReadConsoleW(
                        handle,
                        buf.as_mut_ptr().cast(),
                        1,
                        &mut read,
                        std::ptr::null_mut(),
                    ) == 0
                        || read == 0
                    {
                        break;
                    }
                    let c = buf[0];
                    match c {
                        0x0D => break,                        // Enter
                        0x0A if chars.is_empty() => continue, // 吞掉上一次读取残留的换行
                        0x08 => {
                            chars.pop(); // Backspace
                        }
                        0x20..=0xFFFE => chars.push(c),
                        _ => {}
                    }
                }
                SetConsoleMode(handle, original);
                eprintln!();
                return Ok(String::from_utf16_lossy(&chars));
            }
        }
    }
    // 非 console stdin（重定向）：退化为普通读行。
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(not(windows))]
fn read_hidden_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    use std::io::Write as _;
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// 保存 PIN 记录；失败仅警告。
fn save_pin(config: &mut AppConfig, record: Option<secret::PinRecord>) {
    config.pin = record;
    if !Config::save(config) {
        eprintln!("警告：无法保存 config.json，PIN 状态可能未持久化。");
    }
}

fn cmd_pin(command: PinCommand) -> Result<()> {
    let mut config = Config::load();
    match command {
        PinCommand::Set => {
            if config.pin.is_some() {
                bail!("PIN 已设置，使用 `pcs pin change` 修改或 `pcs pin reset --force` 重置");
            }
            let pin = read_hidden_line("设置 PIN (4-12 位数字): ")?;
            let confirm = read_hidden_line("确认 PIN: ")?;
            if pin != confirm {
                bail!("两次输入不一致");
            }
            let record = secret::pin_record_from(&pin).map_err(anyhow::Error::msg)?;
            save_pin(&mut config, Some(record));
            println!("PIN 已设置。查看密码请用 `pcs secret show <项目>`。");
        }
        PinCommand::Change => {
            let Some(record) = config.pin.clone() else {
                bail!("尚未设置 PIN，先 `pcs pin set`");
            };
            let old = read_hidden_line("当前 PIN: ")?;
            if !secret::verify_pin(&old, &record).map_err(anyhow::Error::msg)? {
                bail!("PIN 不正确");
            }
            let new_pin = read_hidden_line("新 PIN (4-12 位数字): ")?;
            let confirm = read_hidden_line("确认新 PIN: ")?;
            if new_pin != confirm {
                bail!("两次输入不一致");
            }
            let record = secret::pin_record_from(&new_pin).map_err(anyhow::Error::msg)?;
            save_pin(&mut config, Some(record));
            println!("PIN 已修改。");
        }
        PinCommand::Reset { force } => {
            if config.pin.is_none() {
                bail!("尚未设置 PIN");
            }
            if !force {
                println!("这将清除 PIN 并清空所有已存密码、口令与密钥文件，且不可恢复！");
                let confirm = read_hidden_line("确认请输入 yes: ")?;
                if !confirm.eq_ignore_ascii_case("yes") {
                    bail!("已取消");
                }
            }
            let mut data = Store::load();
            clear_all_saved_secrets(&mut data);
            save(&data)?;
            save_pin(&mut config, None);
            println!("PIN 与全部已存秘密已清除。SSH 项目仍可手动输密码/口令登录。");
        }
    }
    Ok(())
}

/// 清空 groups 与 trash 快照中的全部秘密字段，并删除整个密钥目录。
/// 目录删除失败仅警告：字段已清，残留文件由下次启动的孤儿清理兜底。
fn clear_all_saved_secrets(data: &mut ProjectData) {
    let clear = |p: &mut Project| {
        p.ssh_key_file.clear();
        p.ssh_key_path.clear();
        p.ssh_password_enc.clear();
        p.ssh_key_pass_enc.clear();
    };
    for project in data.groups.iter_mut().flat_map(|g| g.projects.iter_mut()) {
        clear(project);
    }
    for item in &mut data.trash {
        item.ssh_key_file.clear();
        item.ssh_key_path.clear();
        item.ssh_password_enc.clear();
        item.ssh_key_pass_enc.clear();
        for project in &mut item.projects {
            clear(project);
        }
    }
    data.pending_key_deletes.clear();
    if let Err(e) = std::fs::remove_dir_all(secret::data_root().join("keys"))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("警告：密钥目录删除失败（{e}），残留文件将在下次启动时清理。");
    }
}

fn cmd_secret(command: SecretCommand) -> Result<()> {
    let SecretCommand::Show(selector) = command;
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    let project = &data.groups[group_index].projects[project_index];
    if !project.is_ssh_project() {
        bail!("项目 `{}` 不是 SSH 项目", project.name);
    }
    if project.ssh_password_enc.is_empty() && project.ssh_key_pass_enc.is_empty() {
        bail!("项目 `{}` 未保存密码或私钥口令", project.name);
    }
    let config = Config::load();
    let Some(record) = &config.pin else {
        bail!("尚未设置 PIN，先 `pcs pin set`");
    };
    let mut verified = false;
    for attempt in 1..=3 {
        let pin = read_hidden_line("PIN: ")?;
        if secret::verify_pin(&pin, record).map_err(anyhow::Error::msg)? {
            verified = true;
            break;
        }
        if attempt < 3 {
            eprintln!("PIN 不正确，还剩 {} 次机会。", 3 - attempt);
        }
    }
    if !verified {
        bail!("PIN 验证失败");
    }
    println!("项目: {}", project.name);
    if !project.ssh_password_enc.is_empty() {
        match secret::unprotect(&project.ssh_password_enc) {
            Ok(pw) => println!("登录密码: {pw}"),
            Err(e) => println!("登录密码: <解密失败: {e}>"),
        }
    }
    if !project.ssh_key_pass_enc.is_empty() {
        match secret::unprotect(&project.ssh_key_pass_enc) {
            Ok(kp) => println!("私钥口令: {kp}"),
            Err(e) => println!("私钥口令: <解密失败: {e}>"),
        }
    }
    Ok(())
}

/// askpass prompt 分派：只回答密码与私钥口令，其余（host key yes/no 等）忽略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AskpassKind {
    Password,
    KeyPass,
    Ignore,
}

fn askpass_kind(prompt: &str) -> AskpassKind {
    let lower = prompt.to_lowercase();
    if lower.contains("password") || prompt.contains("密码") {
        AskpassKind::Password
    } else if lower.contains("passphrase") {
        AskpassKind::KeyPass
    } else {
        AskpassKind::Ignore
    }
}

/// askpass token 校验：env 中 token 必须与父进程落在
/// `PCS_ASKPASS_TOKEN_FILE` 的内容一致（64 位 hex）。
/// 缺失/不匹配一律拒绝（设计 §3.5：防「直接敲一行 `pcs __askpass` 拿明文」；
/// 诚实边界：同 Windows 用户进程可读取文件内容，本就在 DPAPI 信任边界内）。
fn askpass_token_ok(token: &str, token_file: &str) -> bool {
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let path = token_file.trim();
    if path.is_empty() {
        return false;
    }
    match std::fs::read_to_string(path) {
        Ok(expected) => expected.trim().eq_ignore_ascii_case(token),
        Err(_) => false,
    }
}

/// SSH_ASKPASS 回调：校验 token -> 按 prompt 分派 -> 解密 -> stdout。
/// 只读加载（不写回、不维护），避免与父进程的 projects.json 写竞态。
/// 非密码/口令类 prompt（host key 的 yes/no 等）一律输出空，绝不代答。
fn cmd_askpass(prompt: &str) -> Result<()> {
    let token = std::env::var("PCS_ASKPASS_TOKEN").unwrap_or_default();
    let token_file = std::env::var("PCS_ASKPASS_TOKEN_FILE").unwrap_or_default();
    if !askpass_token_ok(&token, &token_file) {
        // token 缺失/不匹配：拒绝输出。
        return Ok(());
    }
    let id = std::env::var("PCS_ASKPASS_ID").unwrap_or_default();
    let data = Store::load_readonly();
    let Some(project) = data
        .groups
        .iter()
        .flat_map(|g| g.projects.iter())
        .find(|p| p.id.eq_ignore_ascii_case(&id))
    else {
        return Ok(());
    };
    let enc = match askpass_kind(prompt) {
        AskpassKind::Password => &project.ssh_password_enc,
        AskpassKind::KeyPass => &project.ssh_key_pass_enc,
        AskpassKind::Ignore => return Ok(()),
    };
    if enc.trim().is_empty() {
        return Ok(());
    }
    if let Ok(plain) = secret::unprotect(enc) {
        println!("{plain}");
    }
    Ok(())
}

fn cmd_group(command: GroupCommand) -> Result<()> {
    let mut data = Store::load();
    match command {
        GroupCommand::Add { name, alias } => {
            ops::add_group_with_alias(&mut data, &name, alias.as_deref().unwrap_or(""))?;
            println!("分组已添加: {name}");
        }
        GroupCommand::Rename {
            old_name,
            new_name,
            alias,
        } => {
            ops::rename_group_with_alias(&mut data, &old_name, &new_name, alias.as_deref())?;
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

    #[test]
    fn askpass_prompt_from_args_shapes() {
        // 直接测生产解析：OpenSSH 单参 / 手工 __askpass / 空参。
        let from = |args: &[&str]| -> String {
            askpass_prompt_from(args.iter().map(|s| (*s).to_string()))
        };
        assert_eq!(
            from(&["abc@172.16.14.10's password: "]),
            "abc@172.16.14.10's password: "
        );
        assert_eq!(
            from(&["__askpass", "Enter passphrase for key:"]),
            "Enter passphrase for key:"
        );
        assert_eq!(from(&["__askpass"]), "");
        assert_eq!(from(&[]), "");
    }

    #[test]
    fn askpass_kind_dispatch() {
        assert_eq!(
            askpass_kind("abc@172.16.14.10's password: "),
            AskpassKind::Password
        );
        assert_eq!(askpass_kind("请输入密码："), AskpassKind::Password);
        assert_eq!(
            askpass_kind("Enter passphrase for key 'C:\\keys\\id_ed25519': "),
            AskpassKind::KeyPass
        );
        // host key 的 yes/no 等绝不代答
        assert_eq!(
            askpass_kind("Are you sure you want to continue connecting (yes/no/[fingerprint])? "),
            AskpassKind::Ignore
        );
        assert_eq!(askpass_kind("Verification code: "), AskpassKind::Ignore);
    }

    fn temp_token_file(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pcs_askpass_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("askpass.token");
        std::fs::write(&path, content).unwrap();
        path
    }

    fn cleanup_token_file(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn askpass_token_ok_requires_matching_file() {
        let token = "a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2c3D4e5F6a1B2";
        let path = temp_token_file(token);
        // 匹配（大小写不敏感）
        assert!(askpass_token_ok(token, &path.to_string_lossy()));
        assert!(askpass_token_ok(
            &token.to_uppercase(),
            &path.to_string_lossy()
        ));
        // 不匹配 / 缺失 / 空
        assert!(!askpass_token_ok("ff", &path.to_string_lossy()));
        assert!(!askpass_token_ok("", &path.to_string_lossy()));
        assert!(!askpass_token_ok(token, ""));
        assert!(!askpass_token_ok(token, "Z:\\no\\such\\file.token"));
        // env 伪造 token 但文件缺失 -> 拒绝
        assert!(!askpass_token_ok(token, "  "));
        cleanup_token_file(&path);
        // 文件被删除后（ssh 已退出的正常清理）即拒绝
        assert!(!askpass_token_ok(token, &path.to_string_lossy()));
    }

    #[test]
    fn askpass_token_ok_rejects_malformed_token() {
        let path = temp_token_file("x");
        // 非 hex / 长度不对的 token 一律拒绝，无论文件内容
        assert!(!askpass_token_ok("xyz", &path.to_string_lossy()));
        assert!(!askpass_token_ok("ff", &path.to_string_lossy()));
        cleanup_token_file(&path);
    }
    use crate::models::Group;

    fn dup_data() -> ProjectData {
        ProjectData {
            groups: vec![
                Group {
                    name: "Work".into(),
                    alias: String::new(),
                    projects: vec![Project {
                        id: "11111111-0000-0000-0000-000000000000".into(),
                        ..Project::new("app", r"E:\w\app", "")
                    }],
                },
                Group {
                    name: "Personal".into(),
                    alias: String::new(),
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
                alias: String::new(),
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
                    alias: String::new(),
                    path: r"E:\old".into(),
                    wsl_path: "/mnt/e/old".into(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    ssh_target: String::new(),
                    ssh_key_file: String::new(),
                    ssh_key_path: String::new(),
                    ssh_password_enc: String::new(),
                    ssh_key_pass_enc: String::new(),
                    projects: Vec::new(),
                    deleted_at: 1700000000,
                },
                models::DeletedItem {
                    id: "bbbbbbbb-0000-0000-0000-000000000000".into(),
                    kind: "group".into(),
                    group: String::new(),
                    name: "Archive".into(),
                    alias: String::new(),
                    path: String::new(),
                    wsl_path: String::new(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    ssh_target: String::new(),
                    ssh_key_file: String::new(),
                    ssh_key_path: String::new(),
                    ssh_password_enc: String::new(),
                    ssh_key_pass_enc: String::new(),
                    projects: vec![Project::new("inner", r"E:\inner", "")],
                    deleted_at: 1700000000,
                },
            ],
            pending_key_deletes: Vec::new(),
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
}
