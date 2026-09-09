use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pcs", about = "Project Center 项目管理器")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
pub(crate) struct ProjectSelector {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) group: Option<String>,
}

#[derive(Args)]
pub(crate) struct OpenArgs {
    #[command(flatten)]
    pub(crate) selector: ProjectSelector,
    #[arg(short = 'w', long, conflicts_with_all = ["powershell", "code", "ssh"])]
    pub(crate) wsl: bool,
    #[arg(short = 'p', long, conflicts_with_all = ["wsl", "code", "ssh"])]
    pub(crate) powershell: bool,
    #[arg(short = 'c', long, conflicts_with_all = ["wsl", "powershell", "ssh"])]
    pub(crate) code: bool,
    #[arg(short = 's', long, conflicts_with_all = ["wsl", "powershell", "code"])]
    pub(crate) ssh: bool,
}

#[derive(Args)]
pub(crate) struct AddArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) group: String,
    #[arg(long)]
    pub(crate) alias: Option<String>,
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    pub(crate) wsl_path: Option<String>,
    #[arg(
        long = "ssh",
        help = "SSH 目标（user@host 或 user@host:端口）；设置后为 SSH 远程项目"
    )]
    pub(crate) ssh: Option<String>,
    #[arg(
        long = "ssh-path",
        help = "远程 Linux 路径（登录后尝试 cd，失败则留在默认 shell）"
    )]
    pub(crate) ssh_path: Option<String>,
    #[arg(long = "ssh-key", help = "导入私钥文件（DPAPI 加密存入数据目录）")]
    pub(crate) ssh_key: Option<PathBuf>,
    #[arg(long = "password-stdin", help = "从 stdin 读入登录密码并加密保存")]
    pub(crate) password_stdin: bool,
    #[arg(long = "key-pass-stdin", help = "从 stdin 读入私钥口令并加密保存")]
    pub(crate) key_pass_stdin: bool,
}

#[derive(Args)]
pub(crate) struct EditArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) group: Option<String>,
    #[arg(long)]
    pub(crate) new_name: Option<String>,
    #[arg(long)]
    pub(crate) alias: Option<String>,
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
    #[arg(long = "wsl-path")]
    pub(crate) wsl_path: Option<String>,
    #[arg(long = "ssh", help = "更新 SSH 目标（user@host 或 user@host:端口）")]
    pub(crate) ssh: Option<String>,
    #[arg(long = "ssh-path", help = "更新远程 Linux 路径；空串清除")]
    pub(crate) ssh_path: Option<String>,
    #[arg(long = "ssh-key", help = "导入私钥文件（覆盖已存密钥）")]
    pub(crate) ssh_key: Option<PathBuf>,
    #[arg(long = "password-stdin", help = "从 stdin 读入登录密码并加密保存")]
    pub(crate) password_stdin: bool,
    #[arg(long = "key-pass-stdin", help = "从 stdin 读入私钥口令并加密保存")]
    pub(crate) key_pass_stdin: bool,
    #[arg(
        long = "clear-ssh",
        help = "清除 SSH 配置（目标、远程路径、密钥与全部秘密）"
    )]
    pub(crate) clear_ssh: bool,
}

#[derive(Args)]
pub(crate) struct MoveArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) to: String,
    #[arg(long)]
    pub(crate) group: Option<String>,
}

#[derive(Args)]
pub(crate) struct RunArgs {
    #[command(flatten)]
    pub(crate) selector: ProjectSelector,
    #[arg(long, help = "列出项目的自定义命令")]
    pub(crate) list: bool,
    /// 自定义命令名
    pub(crate) command: Option<String>,
}

#[derive(Args)]
pub(crate) struct RmArgs {
    #[command(flatten)]
    pub(crate) selector: ProjectSelector,
    #[arg(long, help = "彻底删除，不进入回收站")]
    pub(crate) force: bool,
}

#[derive(Subcommand)]
pub(crate) enum TrashCommand {
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
pub(crate) enum PinCommand {
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
pub(crate) enum SecretCommand {
    #[command(about = "查看项目的保存密码与私钥口令（需 PIN）")]
    Show(ProjectSelector),
}

#[derive(Subcommand)]
pub(crate) enum GroupCommand {
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
pub(crate) enum ConfigCommand {
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
