use std::path::PathBuf;
use std::process::Command;

use zeroize::Zeroize;

use crate::domain::models::{Connection, Project, ProjectData};
use crate::persist as secret;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchEnv {
    Wsl,
    PowerShell,
    Ide,
    Explorer,
    Ssh,
}

impl LaunchEnv {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wsl => "WSL",
            Self::PowerShell => "PowerShell",
            Self::Ide => "IDE",
            Self::Explorer => "文件夹",
            Self::Ssh => "SSH",
        }
    }

    /// 自定义命令显示用短标签：wsl / ps / ide / ssh。
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "ps",
            Self::Ide => "ide",
            Self::Explorer => "",
            Self::Ssh => "ssh",
        }
    }

    /// 写入 `recent.json` 的稳定串。
    pub fn as_recent_str(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "powershell",
            Self::Ide => "ide",
            Self::Explorer => "explorer",
            Self::Ssh => "ssh",
        }
    }

    /// 解析 `recent.json` 的 `env`。大小写不敏感。对不上则 `None`。
    pub fn from_recent_str(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("wsl") {
            Some(Self::Wsl)
        } else if s.eq_ignore_ascii_case("powershell") {
            Some(Self::PowerShell)
        } else if s.eq_ignore_ascii_case("ide") {
            Some(Self::Ide)
        } else if s.eq_ignore_ascii_case("explorer") {
            Some(Self::Explorer)
        } else if s.eq_ignore_ascii_case("ssh") {
            Some(Self::Ssh)
        } else {
            None
        }
    }

    /// 解析项目自定义命令的 env 字符串：wsl / powershell / ide / ssh（大小写不敏感），空或非法视为 ide。
    /// 与 `domain::command::canonical_env` 共用同一套词表。
    pub fn from_command_env(env: &str) -> LaunchEnv {
        if env.eq_ignore_ascii_case("wsl") {
            Self::Wsl
        } else if env.eq_ignore_ascii_case("powershell") {
            Self::PowerShell
        } else if env.eq_ignore_ascii_case("ssh") {
            Self::Ssh
        } else {
            Self::Ide
        }
    }
}

/// 在当前控制台内等待子进程结束；期间忽略 Ctrl+C / Ctrl+Break，
/// 避免信号误杀本进程后由外层 shell 抢回控制台输入，导致被启动的工具收不到按键。
/// 返回子进程退出码；被信号终止（`code()` 为 None）映射为 -1。
fn wait_console_child(mut child: std::process::Child) -> std::io::Result<i32> {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1);
    }
    let result = child.wait();
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0);
    }
    result.map(|status| status.code().unwrap_or(-1))
}

fn console_title(project: &Project, group_name: &str) -> String {
    format!("{} - {}", project.name, group_name)
}

fn apply_console_title(title: &str) {
    #[cfg(windows)]
    {
        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        // 标题设置失败不应阻止项目启动。
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleTitleW(title.as_ptr());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = title;
    }
}

fn set_console_title(project: &Project, group_name: &str) {
    apply_console_title(&console_title(project, group_name));
}

/// 构造 `wsl.exe` 参数：`--cd <linux> [-- <command>]`；command 为空表示裸终端。
fn wsl_args(linux: &str, command: &str) -> Vec<String> {
    let mut args = vec!["--cd".to_string(), linux.to_string()];
    if !command.trim().is_empty() {
        // `-e bash -lic` 而非 `-- <cmd>`：后者会把整条命令当单个可执行名（
        // 含空格/管道/引号的命令会报 command not found），bash 会正确重新解析。
        // `-i` 加载完整 `.bashrc`（nvm/fnm/cargo 等 PATH），与手动打开 WSL 终端一致。
        args.push("-e".to_string());
        args.push("bash".to_string());
        args.push("-lic".to_string());
        args.push(command.to_string());
    }
    args
}

/// 构造 PowerShell 脚本：`Set-Location -LiteralPath '<win>'; <command>`；
/// command 为空时仅切换目录（裸终端）。
fn powershell_script(win_path: &str, command: &str) -> String {
    let escaped = win_path.replace('\'', "''");
    let mut script = format!("Set-Location -LiteralPath '{escaped}'");
    if !command.trim().is_empty() {
        script.push_str(&format!("; {command}"));
    }
    script
}

fn spawn_wsl(p: &Project, group_name: &str, command: &str) -> super::Result<std::process::Child> {
    set_console_title(p, group_name);
    Command::new("wsl.exe")
        .args(wsl_args(&p.linux_path(), command))
        .spawn()
        .map_err(|e| super::Error::from(format!("WSL 启动失败: {e}")))
}

fn spawn_powershell(
    p: &Project,
    group_name: &str,
    command: &str,
) -> super::Result<std::process::Child> {
    set_console_title(p, group_name);
    let script = powershell_script(&p.path, command);
    Command::new("powershell.exe")
        .args(["-NoExit", "-Command", script.as_str()])
        .spawn()
        .map_err(|e| super::Error::from(format!("PowerShell 启动失败: {e}")))
}

fn spawn_ide(p: &Project, command: &str) -> super::Result<std::process::Child> {
    let mut args = vec!["/c".to_string(), command.to_string(), p.path.clone()];
    args.retain(|arg| !arg.trim().is_empty());
    Command::new("cmd")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| super::Error::from(format!("IDE 启动失败: {e}")))
}

fn spawn_explorer(p: &Project) -> super::Result<std::process::Child> {
    Command::new("explorer.exe")
        .arg(&p.path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| super::Error::from(format!("资源管理器启动失败: {e}")))
}

/// 构造 ssh 参数：从 Connection 取 user/host/port；项目 path 作为远程 cwd。
/// 默认端口 22 不传 `-p`（与 OpenSSH 缺省一致）。
/// `remote_command` 空：交互终端（path 非空时静默 cd，失败仍进默认 shell）。
/// `remote_command` 非空：一锤子（path 非空时 `cd &&`，cd 失败即非零退出）。
fn ssh_args(
    connection: &Connection,
    key_path: Option<&str>,
    remote_path: &str,
    remote_command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if connection.port != 22 {
        args.push("-p".into());
        args.push(connection.port.to_string());
    }
    if let Some(key) = key_path {
        args.push("-i".into());
        args.push(key.to_string());
    }
    args.push(connection.userhost());
    let remote = remote_path.trim();
    let cmd = remote_command.trim();
    if cmd.is_empty() {
        if !remote.is_empty() {
            let escaped = remote.replace('\'', "''");
            args.push("-t".into());
            args.push(format!("cd '{escaped}' 2>/dev/null; exec $SHELL"));
        }
    } else {
        args.push("-t".into());
        if remote.is_empty() {
            args.push(cmd.to_string());
        } else {
            let escaped = remote.replace('\'', "''");
            args.push(format!("cd '{escaped}' && {cmd}"));
        }
    }
    args
}

/// 随机 token 的 hex 编码（askpass 注入校验用）。
fn random_token_hex() -> super::Result<String> {
    let mut bytes = [0u8; 32];
    secret::fill_random(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 把 askpass token 写入数据根 `keys_tmp\`，供 `__askpass` 与 env 中的
/// token 比对（设计 §3.5：token 缺失/不匹配 → 输出空，防直调拿明文）。
/// ssh 退出后由 `wait_spawned` 覆写删除；进程崩溃残留由启动维护兜底。
fn write_askpass_token_file_in(dir: &std::path::Path, token: &str) -> super::Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .map_err(|e| super::Error::from(format!("创建 keys_tmp 失败: {e}")))?;
    let path = dir.join(format!("askpass-{}.token", uuid::Uuid::new_v4()));
    secret::write_private_file(&path, token.as_bytes())
        .map_err(|e| super::Error::from(format!("写入 token 校验文件失败: {e}")))?;
    Ok(path)
}

fn write_askpass_token_file(token: &str) -> super::Result<PathBuf> {
    write_askpass_token_file_in(&secret::data_root().join("keys_tmp"), token)
}

/// 父进程预解密验证：所有已保存的秘密都能解开才注入 askpass env
///（否则 force 模式下 askpass 输出空会导致认证必败且无法回退 tty 提示）。
fn askpass_env_ok(connection: &Connection) -> bool {
    let password_ok = connection.ssh_password_enc.trim().is_empty()
        || secret::unprotect(&connection.ssh_password_enc).is_ok();
    let key_pass_ok = connection.ssh_key_pass_enc.trim().is_empty()
        || secret::unprotect(&connection.ssh_key_pass_enc).is_ok();
    password_ok && key_pass_ok
}

/// askpass 注入决策。存有可解密秘密但主机不在 known_hosts 时跳过注入：
/// force 会把首连的 host key 确认也路由给 askpass（回空 = 拒绝）导致秒败；
/// 首连转交互（确认 host key + 手动输一次密码）后，known_hosts 命中即恢复注入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AskpassPlan {
    /// 无保存秘密：不注入，全交互。
    NoSecrets,
    /// 有秘密但解密失败：不注入并提示。
    DecryptFailed,
    /// 主机已在 known_hosts：注入。
    Inject,
    /// 首次连接：不注入并提示手动确认。
    FirstConnect,
}

fn askpass_plan(has_secrets: bool, decryptable: bool, known: bool) -> AskpassPlan {
    if !has_secrets {
        AskpassPlan::NoSecrets
    } else if !decryptable {
        AskpassPlan::DecryptFailed
    } else if known {
        AskpassPlan::Inject
    } else {
        AskpassPlan::FirstConnect
    }
}

/// known_hosts 查找名形态，与 ssh 写入格式一致：
/// 无端口或端口 22 → 裸主机名；否则 → `[host]:port`。
fn known_hosts_lookup_name(host: &str, port: Option<&str>) -> String {
    match port {
        Some(p) if p != "22" => format!("[{host}]:{p}"),
        _ => host.to_string(),
    }
}

/// 用 `ssh-keygen -F`（原生支持 hashed 条目）判定主机是否已在 known_hosts。
/// 退出码 0 或 stdout 含 "found" 均视为命中（双保险防版本差异）；
/// ssh-keygen 缺失/调用失败按未知处理——最坏情形是手动输一次密码（安全降级）。
fn host_known(host: &str, port: Option<&str>) -> bool {
    host_known_with(None, &known_hosts_lookup_name(host, port))
}

/// `file` 供测试注入自定义 known_hosts；`None` 用默认 `~/.ssh/known_hosts`。
fn host_known_with(file: Option<&std::path::Path>, lookup_name: &str) -> bool {
    let mut cmd = Command::new("ssh-keygen");
    cmd.arg("-F").arg(lookup_name);
    if let Some(path) = file {
        cmd.arg("-f").arg(path);
    }
    match cmd.output() {
        Ok(out) => out.status.success() || String::from_utf8_lossy(&out.stdout).contains("found"),
        Err(_) => false,
    }
}

/// 从 ProjectData 解析项目引用的连接。
/// 加载路径已迁移，只认 `connection_id`（不回退遗留 `ssh_target`）。
/// 缺 id → NotSshProject；id 指向不存在的连接 → ConnectionMissing（不 panic）。
fn resolve_ssh<'a>(data: &'a ProjectData, p: &Project) -> super::Result<&'a Connection> {
    match data.connection_of(p) {
        Ok(connection) => Ok(connection),
        Err(crate::domain::Error::ProjectNotSsh { name }) => {
            Err(super::Error::NotSshProject { name })
        }
        Err(crate::domain::Error::ConnectionMissing { project }) => {
            Err(super::Error::ConnectionMissing { project })
        }
        Err(e) => Err(super::Error::from(e.to_string())),
    }
}

/// 用连接认证启动 ssh：args 来自 Connection user/host/port，远程 cwd 为项目 path。
/// `command` 空走交互终端，非空走一锤子；askpass 的 `PCS_ASKPASS_ID` 是连接 id。
pub fn spawn_ssh(
    connection: &Connection,
    remote_path: &str,
    title: &str,
    command: &str,
) -> super::Result<SpawnedDirect> {
    apply_console_title(title);

    // 私钥：解密成功才落临时文件；失败降级为不带密钥启动（回退密码/交互）。
    let mut temp_key: Option<PathBuf> = None;
    let mut key_arg: Option<String> = None;
    let mut temp_token: Option<PathBuf> = None;
    if connection.has_key() {
        match secret::read_key_file(&connection.ssh_key_file) {
            Ok(mut plain) => {
                let dir = secret::data_root().join("keys_tmp");
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    eprintln!("警告：无法创建密钥临时目录（{e}），本次不带密钥启动。");
                } else {
                    let path = dir.join(format!("{}.key", uuid::Uuid::new_v4()));
                    match secret::write_private_file(&path, plain.as_bytes()) {
                        Ok(()) => {
                            key_arg = Some(path.to_string_lossy().into_owned());
                            temp_key = Some(path);
                        }
                        Err(e) => {
                            eprintln!("警告：密钥临时文件写入失败（{e}），本次不带密钥启动。")
                        }
                    }
                }
                plain.zeroize();
            }
            Err(e) => eprintln!("警告：私钥解密失败（{e}），本次不带密钥启动。"),
        }
    }

    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(
        connection,
        key_arg.as_deref(),
        remote_path,
        command,
    ));

    // askpass 自动填充：按决策注入；env 只携带连接 id 与一次性 token，
    // 秘密由 __askpass 自行解密。主机未知（首连）时跳过注入走交互。
    let port = connection.port.to_string();
    let plan = askpass_plan(
        connection.has_saved_secrets(),
        askpass_env_ok(connection),
        host_known(&connection.host, Some(&port)),
    );
    match plan {
        AskpassPlan::NoSecrets => {}
        AskpassPlan::DecryptFailed => {
            eprintln!("提示：保存的密码/口令解密失败，本次回退交互输入。");
        }
        AskpassPlan::FirstConnect => {
            eprintln!(
                "提示：首次连接 {}：请确认 host key 并手动输入密码，连接成功后自动填充。",
                connection.host
            );
        }
        AskpassPlan::Inject => {
            if let Ok(exe) = std::env::current_exe() {
                match random_token_hex() {
                    Ok(token) => match write_askpass_token_file(&token) {
                        Ok(token_path) => {
                            cmd.env("SSH_ASKPASS", &exe);
                            cmd.env("SSH_ASKPASS_REQUIRE", "force");
                            cmd.env("DISPLAY", ":0");
                            cmd.env("PCS_ASKPASS_ID", &connection.id);
                            cmd.env("PCS_ASKPASS_TOKEN", &token);
                            cmd.env("PCS_ASKPASS_TOKEN_FILE", &token_path);
                            temp_token = Some(token_path);
                        }
                        Err(e) => {
                            eprintln!("警告：askpass 校验文件写入失败（{e}），本次回退交互输入。")
                        }
                    },
                    Err(e) => eprintln!("警告：随机 token 生成失败（{e}），本次回退交互输入。"),
                }
            }
        }
    }

    match cmd.spawn() {
        Ok(child) => Ok(SpawnedDirect {
            child: Some(child),
            temp_key_path: temp_key,
            temp_token_path: temp_token,
        }),
        Err(e) => {
            // 启动失败也要清掉刚落的临时密钥与 token 校验文件。
            if let Some(path) = &temp_key {
                secret::shred_and_remove(path);
            }
            if let Some(path) = &temp_token {
                secret::shred_and_remove(path);
            }
            Err(super::Error::from(format!("SSH 启动失败: {e}")))
        }
    }
}

/// 一次成功 spawn 的产物：子进程（IDE/资源管理器为 `None`）与
/// SSH 临时密钥/askpass token 文件路径（非 SSH 为 `None`）。
pub struct SpawnedDirect {
    pub child: Option<std::process::Child>,
    /// SSH 场景的明文密钥临时文件；`wait_spawned` 退出后覆写删除。
    pub temp_key_path: Option<PathBuf>,
    /// SSH 场景的 askpass token 校验文件；`wait_spawned` 退出后覆写删除。
    pub temp_token_path: Option<PathBuf>,
}

/// 生成子进程（不等待）。校验 Windows 路径可用性（与旧 `launch_direct` 语义一致）。
/// SSH 项目仅支持 `LaunchEnv::Ssh`；其他环境对其报错。
/// SSH 走 `resolve_ssh` 再 `spawn_ssh`：只认 `connection_id`（加载已迁移）。
pub fn spawn_direct(
    data: &ProjectData,
    p: &Project,
    group_name: &str,
    env: LaunchEnv,
    command: &str,
) -> super::Result<SpawnedDirect> {
    if env == LaunchEnv::Ssh {
        let connection = resolve_ssh(data, p)?;
        return spawn_ssh(connection, &p.path, &console_title(p, group_name), command);
    }
    if matches!(
        env,
        LaunchEnv::PowerShell | LaunchEnv::Ide | LaunchEnv::Explorer
    ) && !p.has_windows_path()
    {
        return Err(super::Error::NoWindowsPath {
            name: p.name.clone(),
            env: env.label().to_string(),
        });
    }
    if p.is_ssh_project() {
        return Err(super::Error::SshOnly {
            name: p.name.clone(),
        });
    }
    match env {
        LaunchEnv::Wsl => spawn_wsl(p, group_name, command).map(|child| SpawnedDirect {
            child: Some(child),
            temp_key_path: None,
            temp_token_path: None,
        }),
        LaunchEnv::PowerShell => {
            spawn_powershell(p, group_name, command).map(|child| SpawnedDirect {
                child: Some(child),
                temp_key_path: None,
                temp_token_path: None,
            })
        }
        LaunchEnv::Ide => spawn_ide(p, command).map(|_| SpawnedDirect {
            child: None,
            temp_key_path: None,
            temp_token_path: None,
        }),
        LaunchEnv::Explorer => spawn_explorer(p).map(|_| SpawnedDirect {
            child: None,
            temp_key_path: None,
            temp_token_path: None,
        }),
        LaunchEnv::Ssh => unreachable!("已在上方处理"),
    }
}

/// 等待子进程退出（阻塞、抑制 Ctrl+C），随后清理 SSH 临时密钥与
/// askpass token 校验文件（成功/失败/中断都走）。返回子进程退出码
/// （无子进程为 0；被信号终止为 -1）。
pub fn wait_spawned(spawned: SpawnedDirect) -> super::Result<i32> {
    let result = match spawned.child {
        Some(child) => wait_console_child(child)
            .map_err(|e| super::Error::from(format!("等待子进程结束失败: {e}"))),
        None => Ok(0),
    };
    if let Some(path) = spawned.temp_key_path {
        secret::shred_and_remove(&path);
    }
    if let Some(path) = spawned.temp_token_path {
        secret::shred_and_remove(&path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_title_uses_project_and_group() {
        let project = Project::new("pcs", "C:\\dev\\pcs", "/mnt/c/dev/pcs");

        assert_eq!(console_title(&project, "工具"), "pcs - 工具");
    }

    #[test]
    fn env_labels() {
        assert_eq!(LaunchEnv::Wsl.label(), "WSL");
        assert_eq!(LaunchEnv::PowerShell.label(), "PowerShell");
        assert_eq!(LaunchEnv::Ide.label(), "IDE");
        assert_eq!(LaunchEnv::Explorer.label(), "文件夹");
    }

    #[test]
    fn env_short_labels() {
        assert_eq!(LaunchEnv::Wsl.short_label(), "wsl");
        assert_eq!(LaunchEnv::PowerShell.short_label(), "ps");
        assert_eq!(LaunchEnv::Ide.short_label(), "ide");
    }

    #[test]
    fn env_recent_str_round_trip() {
        assert_eq!(LaunchEnv::Wsl.as_recent_str(), "wsl");
        assert_eq!(LaunchEnv::PowerShell.as_recent_str(), "powershell");
        assert_eq!(LaunchEnv::Ide.as_recent_str(), "ide");
        assert_eq!(LaunchEnv::Explorer.as_recent_str(), "explorer");
        assert_eq!(LaunchEnv::Ssh.as_recent_str(), "ssh");
        assert_eq!(LaunchEnv::from_recent_str("WSL"), Some(LaunchEnv::Wsl));
        assert_eq!(
            LaunchEnv::from_recent_str("PowerShell"),
            Some(LaunchEnv::PowerShell)
        );
        assert_eq!(LaunchEnv::from_recent_str("IDE"), Some(LaunchEnv::Ide));
        assert_eq!(
            LaunchEnv::from_recent_str("explorer"),
            Some(LaunchEnv::Explorer)
        );
        assert_eq!(LaunchEnv::from_recent_str("ssh"), Some(LaunchEnv::Ssh));
        assert_eq!(LaunchEnv::from_recent_str("ps"), None);
        assert_eq!(LaunchEnv::from_recent_str("mystery"), None);
    }

    #[test]
    fn from_command_env_parses_case_insensitive() {
        assert_eq!(LaunchEnv::from_command_env("wsl"), LaunchEnv::Wsl);
        assert_eq!(LaunchEnv::from_command_env("WSL"), LaunchEnv::Wsl);
        assert_eq!(
            LaunchEnv::from_command_env("powershell"),
            LaunchEnv::PowerShell
        );
        assert_eq!(
            LaunchEnv::from_command_env("PowerShell"),
            LaunchEnv::PowerShell
        );
        assert_eq!(LaunchEnv::from_command_env("ide"), LaunchEnv::Ide);
        assert_eq!(LaunchEnv::from_command_env("IDE"), LaunchEnv::Ide);
        assert_eq!(LaunchEnv::from_command_env("ssh"), LaunchEnv::Ssh);
        assert_eq!(LaunchEnv::from_command_env("SSH"), LaunchEnv::Ssh);
    }

    #[test]
    fn from_command_env_invalid_falls_back_to_ide() {
        assert_eq!(LaunchEnv::from_command_env(""), LaunchEnv::Ide);
        assert_eq!(LaunchEnv::from_command_env("bash"), LaunchEnv::Ide);
        assert_eq!(LaunchEnv::from_command_env("  "), LaunchEnv::Ide);
    }

    #[test]
    fn ssh_env_labels() {
        assert_eq!(LaunchEnv::Ssh.label(), "SSH");
        assert_eq!(LaunchEnv::Ssh.short_label(), "ssh");
    }

    fn conn(user: &str, host: &str, port: u16) -> Connection {
        Connection {
            id: "cid".into(),
            name: host.into(),
            user: user.into(),
            host: host.into(),
            port,
            ..Default::default()
        }
    }

    #[test]
    fn known_hosts_lookup_name_forms() {
        assert_eq!(known_hosts_lookup_name("h", None), "h");
        assert_eq!(known_hosts_lookup_name("h", Some("22")), "h");
        assert_eq!(known_hosts_lookup_name("h", Some("2222")), "[h]:2222");
    }

    #[test]
    fn askpass_plan_covers_all_branches() {
        assert_eq!(askpass_plan(false, false, false), AskpassPlan::NoSecrets);
        assert_eq!(askpass_plan(true, false, true), AskpassPlan::DecryptFailed);
        assert_eq!(askpass_plan(true, true, true), AskpassPlan::Inject);
        assert_eq!(askpass_plan(true, true, false), AskpassPlan::FirstConnect);
    }

    #[test]
    fn host_known_with_matches_real_keygen() {
        // 依赖系统 ssh-keygen（与 ssh 同装同失）：缺失时跳过断言直接通过。
        if std::process::Command::new("ssh-keygen").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("pcs_kh_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("known_hosts");
        std::fs::write(
            &file,
            "h.local ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeFakeFakeFakeFakeFakeFakeFakeFak\n",
        )
        .unwrap();
        assert!(host_known_with(Some(&file), "h.local"));
        assert!(!host_known_with(Some(&file), "other.local"));
        assert!(!host_known_with(Some(&dir.join("missing")), "h.local"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wait_spawned_returns_exit_code() {
        let child = exit_child(7);
        let code = wait_spawned(SpawnedDirect {
            child: Some(child),
            temp_key_path: None,
            temp_token_path: None,
        })
        .unwrap();
        assert_eq!(code, 7);

        let child = exit_child(0);
        let code = wait_spawned(SpawnedDirect {
            child: Some(child),
            temp_key_path: None,
            temp_token_path: None,
        })
        .unwrap();
        assert_eq!(code, 0);
    }

    fn exit_child(code: i32) -> std::process::Child {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/c", &format!("exit {code}")])
                .spawn()
                .unwrap()
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("sh")
                .args(["-c", &format!("exit {code}")])
                .spawn()
                .unwrap()
        }
    }

    #[test]
    fn ssh_args_from_connection() {
        assert_eq!(
            ssh_args(&conn("abc", "192.0.2.10", 22), None, "", ""),
            vec!["abc@192.0.2.10"]
        );
        assert_eq!(
            ssh_args(&conn("abc", "h", 2222), None, "/opt/foo", ""),
            vec![
                "-p",
                "2222",
                "abc@h",
                "-t",
                "cd '/opt/foo' 2>/dev/null; exec $SHELL"
            ]
        );
        assert_eq!(
            ssh_args(&conn("abc", "h", 22), Some("C:\\tmp\\k.key"), "", ""),
            vec!["-i", "C:\\tmp\\k.key", "abc@h"]
        );
        assert_eq!(ssh_args(&conn("", "h", 22), None, "", ""), vec!["h"]);
        // 路径含单引号转义
        assert_eq!(
            ssh_args(&conn("abc", "h", 22), None, "/opt/i't's", ""),
            vec!["abc@h", "-t", "cd '/opt/i''t''s' 2>/dev/null; exec $SHELL"]
        );
    }

    #[test]
    fn ssh_args_one_shot_command() {
        assert_eq!(
            ssh_args(&conn("abc", "h", 22), None, "", "htop"),
            vec!["abc@h", "-t", "htop"]
        );
        assert_eq!(
            ssh_args(&conn("abc", "h", 22), None, "/opt/foo", "make deploy"),
            vec!["abc@h", "-t", "cd '/opt/foo' && make deploy"]
        );
        assert_eq!(
            ssh_args(&conn("abc", "h", 2222), None, "/opt/foo", "make"),
            vec!["-p", "2222", "abc@h", "-t", "cd '/opt/foo' && make"]
        );
        assert_eq!(
            ssh_args(&conn("abc", "h", 22), None, "/opt/i't's", "make"),
            vec!["abc@h", "-t", "cd '/opt/i''t''s' && make"]
        );
    }

    #[test]
    fn askpass_env_ok_requires_all_secrets_decryptable() {
        let mut c = conn("abc", "h", 22);
        // 无任何秘密：env 无用但也无害（不会自动填），视为 ok
        assert!(askpass_env_ok(&c));
        // 密文损坏 -> 不允许注入（force 下空输出会导致认证必败）
        c.ssh_password_enc = "broken-b64!!".into();
        assert!(!askpass_env_ok(&c));
        c.ssh_key_pass_enc = "!!".into();
        assert!(!askpass_env_ok(&c));
    }

    #[cfg(windows)]
    #[test]
    fn askpass_env_ok_accepts_dpapi_ciphertext() {
        let mut c = conn("abc", "h", 22);
        c.ssh_password_enc = secret::protect("pw").unwrap();
        assert!(askpass_env_ok(&c));
    }

    #[test]
    fn has_saved_secrets_gates_askpass_injection() {
        let mut c = conn("abc", "h", 22);
        // 无任何秘密：不注入（force 会劫持交互密码提示）
        assert!(!c.has_saved_secrets());
        c.ssh_password_enc = "enc".into();
        assert!(c.has_saved_secrets());
        let mut c2 = conn("abc", "h", 22);
        c2.ssh_key_pass_enc = "kp".into();
        assert!(c2.has_saved_secrets());
        let mut c3 = conn("abc", "h", 22);
        c3.ssh_password_enc = "  ".into();
        assert!(!c3.has_saved_secrets());
    }

    #[test]
    fn spawn_direct_ssh_missing_connection_is_error() {
        let p = Project::new("srv", "/opt/x", "").with_connection("missing-id");
        let Err(err) = spawn_direct(&ProjectData::default(), &p, "G", LaunchEnv::Ssh, "") else {
            panic!("expected ConnectionMissing");
        };
        assert!(matches!(
            err,
            crate::launch::Error::ConnectionMissing { ref project } if project == "srv"
        ));
        assert_eq!(
            err.to_string(),
            "项目 `srv` 引用的远程连接不存在，请重新选择连接"
        );
    }

    #[test]
    fn spawn_direct_ssh_requires_connection_id() {
        let p = Project::new("srv", "/opt/x", "");
        let Err(err) = spawn_direct(&ProjectData::default(), &p, "G", LaunchEnv::Ssh, "") else {
            panic!("expected NotSshProject");
        };
        assert!(matches!(
            err,
            crate::launch::Error::NotSshProject { name } if name == "srv"
        ));
    }

    #[test]
    fn spawn_direct_ssh_local_project_is_not_ssh() {
        let p = Project::new("app", r"C:\a", "");
        let Err(err) = spawn_direct(&ProjectData::default(), &p, "G", LaunchEnv::Ssh, "") else {
            panic!("expected NotSshProject");
        };
        assert!(matches!(
            err,
            crate::launch::Error::NotSshProject { name } if name == "app"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn random_token_hex_is_64_chars() {
        let t = random_token_hex().unwrap();
        assert_eq!(t.len(), 64);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(t, random_token_hex().unwrap(), "token 必须随机");
    }

    #[test]
    fn askpass_token_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("pcs_tok_{}", uuid::Uuid::new_v4()));
        let token = "a".repeat(64);
        let path = write_askpass_token_file_in(&dir, &token).unwrap();
        assert!(path.parent().unwrap() == dir);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), token);
        // 每次写入使用随机文件名，并发会话互不覆盖
        let other = write_askpass_token_file_in(&dir, &token).unwrap();
        assert_ne!(path, other);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wsl_args_bare_and_with_command() {
        assert_eq!(
            wsl_args("/mnt/e/dev/app", ""),
            vec!["--cd", "/mnt/e/dev/app"]
        );
        assert_eq!(
            wsl_args("/mnt/e/dev/app", "opencode"),
            vec!["--cd", "/mnt/e/dev/app", "-e", "bash", "-lic", "opencode"]
        );
        assert_eq!(
            wsl_args("/mnt/e/dev/app", "make build && npm run dev"),
            vec![
                "--cd",
                "/mnt/e/dev/app",
                "-e",
                "bash",
                "-lic",
                "make build && npm run dev"
            ]
        );
    }

    #[test]
    fn powershell_script_bare_and_with_command() {
        assert_eq!(
            powershell_script(r"E:\dev\app", ""),
            r"Set-Location -LiteralPath 'E:\dev\app'"
        );
        assert_eq!(
            powershell_script(r"E:\dev\app", "opencode"),
            r"Set-Location -LiteralPath 'E:\dev\app'; opencode"
        );
        assert_eq!(
            powershell_script(r"E:\it's", "opencode"),
            r"Set-Location -LiteralPath 'E:\it''s'; opencode"
        );
    }
}
