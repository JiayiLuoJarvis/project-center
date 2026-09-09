use std::path::PathBuf;
use std::process::Command;

use zeroize::Zeroize;

use windows_sys::Win32::System::Console::{SetConsoleCtrlHandler, SetConsoleTitleW};

use crate::models::Project;
use crate::secret;

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

    /// 自定义命令显示用短标签：wsl / ps / ide。
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "ps",
            Self::Ide => "ide",
            Self::Explorer => "",
            Self::Ssh => "ssh",
        }
    }

    /// 解析项目自定义命令的 env 字符串：wsl / powershell / ide（大小写不敏感），空或非法视为 ide。
    pub fn from_command_env(env: &str) -> LaunchEnv {
        if env.eq_ignore_ascii_case("wsl") {
            Self::Wsl
        } else if env.eq_ignore_ascii_case("powershell") {
            Self::PowerShell
        } else {
            Self::Ide
        }
    }
}

/// 在当前控制台内等待子进程结束；期间忽略 Ctrl+C / Ctrl+Break，
/// 避免信号误杀本进程后由外层 shell 抢回控制台输入，导致被启动的工具收不到按键。
fn wait_console_child(mut child: std::process::Child) -> std::io::Result<()> {
    unsafe {
        SetConsoleCtrlHandler(None, 1);
    }
    let result = child.wait();
    unsafe {
        SetConsoleCtrlHandler(None, 0);
    }
    result.map(|_| ())
}

fn console_title(project: &Project, group_name: &str) -> String {
    format!("{} - {}", project.name, group_name)
}

fn set_console_title(project: &Project, group_name: &str) {
    let title: Vec<u16> = console_title(project, group_name)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // 标题设置失败不应阻止项目启动。
    unsafe {
        SetConsoleTitleW(title.as_ptr());
    }
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

fn spawn_wsl(p: &Project, group_name: &str, command: &str) -> Result<std::process::Child, String> {
    set_console_title(p, group_name);
    Command::new("wsl.exe")
        .args(wsl_args(&p.linux_path(), command))
        .spawn()
        .map_err(|e| format!("WSL 启动失败: {e}"))
}

fn spawn_powershell(
    p: &Project,
    group_name: &str,
    command: &str,
) -> Result<std::process::Child, String> {
    set_console_title(p, group_name);
    let script = powershell_script(&p.path, command);
    Command::new("powershell.exe")
        .args(["-NoExit", "-Command", script.as_str()])
        .spawn()
        .map_err(|e| format!("PowerShell 启动失败: {e}"))
}

fn spawn_ide(p: &Project, command: &str) -> Result<std::process::Child, String> {
    let mut args = vec!["/c".to_string(), command.to_string(), p.path.clone()];
    args.retain(|arg| !arg.trim().is_empty());
    Command::new("cmd")
        .args(&args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("IDE 启动失败: {e}"))
}

fn spawn_explorer(p: &Project) -> Result<std::process::Child, String> {
    Command::new("explorer.exe")
        .arg(&p.path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("资源管理器启动失败: {e}"))
}

/// 解析 SSH 目标：`(user@host:port)` -> `(user@host, Option<port>)`。
/// 仅当 host 部分不含 `:` 且末段为纯数字时视为端口；IPv6 目标 v1 不支持。
fn parse_ssh_target(target: &str) -> (String, Option<String>) {
    let target = target.trim();
    match target.rfind(':') {
        Some(pos) => {
            let (host, port) = target.split_at(pos);
            let port = &port[1..];
            if !host.is_empty()
                && !host.contains(':')
                && !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
            {
                (host.to_string(), Some(port.to_string()))
            } else {
                (target.to_string(), None)
            }
        }
        None => (target.to_string(), None),
    }
}

/// 构造 ssh 参数：`[-p port] [-i key] user@host [-t "cd '<path>' 2>/dev/null; exec $SHELL"]`。
/// 远程路径用 `;` 串联（cd 失败静默落到默认 shell，不断连）。
fn ssh_args(target: &str, key_path: Option<&str>, remote_path: &str) -> Vec<String> {
    let (userhost, port) = parse_ssh_target(target);
    let mut args: Vec<String> = Vec::new();
    if let Some(port) = port {
        args.push("-p".into());
        args.push(port);
    }
    if let Some(key) = key_path {
        args.push("-i".into());
        args.push(key.to_string());
    }
    args.push(userhost);
    let remote = remote_path.trim();
    if !remote.is_empty() {
        let escaped = remote.replace('\'', "''");
        args.push("-t".into());
        args.push(format!("cd '{escaped}' 2>/dev/null; exec $SHELL"));
    }
    args
}

/// 随机 token 的 hex 编码（askpass 注入校验用）。
fn random_token_hex() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    secret::fill_random(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 把 askpass token 写入数据根 `keys_tmp\`，供 `__askpass` 与 env 中的
/// token 比对（设计 §3.5：token 缺失/不匹配 → 输出空，防直调拿明文）。
/// ssh 退出后由 `wait_spawned` 覆写删除；进程崩溃残留由启动维护兜底。
fn write_askpass_token_file_in(dir: &std::path::Path, token: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("创建 keys_tmp 失败: {e}"))?;
    let path = dir.join(format!("askpass-{}.token", uuid::Uuid::new_v4()));
    std::fs::write(&path, token).map_err(|e| format!("写入 token 校验文件失败: {e}"))?;
    Ok(path)
}

fn write_askpass_token_file(token: &str) -> Result<PathBuf, String> {
    write_askpass_token_file_in(&secret::data_root().join("keys_tmp"), token)
}

/// 父进程预解密验证：所有已保存的秘密都能解开才注入 askpass env
///（否则 force 模式下 askpass 输出空会导致认证必败且无法回退 tty 提示）。
fn askpass_env_ok(p: &Project) -> bool {
    let password_ok =
        p.ssh_password_enc.trim().is_empty() || secret::unprotect(&p.ssh_password_enc).is_ok();
    let key_pass_ok =
        p.ssh_key_pass_enc.trim().is_empty() || secret::unprotect(&p.ssh_key_pass_enc).is_ok();
    password_ok && key_pass_ok
}

/// 项目是否存有任一密码/口令密文：askpass 注入的前提。
/// 无秘密时不注入——force 会把交互密码提示也路由到 askpass（输出空），
/// 用户反而无法手动输密码登录。
fn has_saved_secrets(p: &Project) -> bool {
    !p.ssh_password_enc.trim().is_empty() || !p.ssh_key_pass_enc.trim().is_empty()
}

fn spawn_ssh(p: &Project, group_name: &str) -> Result<SpawnedDirect, String> {
    set_console_title(p, group_name);

    // 私钥：解密成功才落临时文件；失败降级为不带密钥启动（回退密码/交互）。
    let mut temp_key: Option<PathBuf> = None;
    let mut key_arg: Option<String> = None;
    let mut temp_token: Option<PathBuf> = None;
    if !p.ssh_key_file.trim().is_empty() {
        match secret::read_key_file(&p.ssh_key_file) {
            Ok(mut plain) => {
                let dir = secret::data_root().join("keys_tmp");
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    eprintln!("警告：无法创建密钥临时目录（{e}），本次不带密钥启动。");
                } else {
                    let path = dir.join(format!("{}.key", uuid::Uuid::new_v4()));
                    match std::fs::write(&path, plain.as_bytes()) {
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
    cmd.args(ssh_args(&p.ssh_target, key_arg.as_deref(), &p.path));

    // askpass 自动填充：仅当存有可解密的密码/口令时注入；
    // env 只携带项目 id 与一次性 token，秘密由 __askpass 自行解密。
    if !has_saved_secrets(p) {
        // 无保存秘密：不注入，全部走正常交互（密码/口令/host key 确认）。
    } else if !askpass_env_ok(p) {
        eprintln!("提示：保存的密码/口令解密失败，本次回退交互输入。");
    } else if let Ok(exe) = std::env::current_exe() {
        match random_token_hex() {
            Ok(token) => match write_askpass_token_file(&token) {
                Ok(token_path) => {
                    cmd.env("SSH_ASKPASS", &exe);
                    cmd.env("SSH_ASKPASS_REQUIRE", "force");
                    cmd.env("DISPLAY", ":0");
                    cmd.env("PCS_ASKPASS_ID", &p.id);
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
            Err(format!("SSH 启动失败: {e}"))
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
pub fn spawn_direct(
    p: &Project,
    group_name: &str,
    env: LaunchEnv,
    command: &str,
) -> Result<SpawnedDirect, String> {
    if env == LaunchEnv::Ssh {
        if !p.is_ssh_project() {
            return Err(format!(
                "项目 `{}` 不是 SSH 项目，不能使用 SSH 启动",
                p.name
            ));
        }
        return spawn_ssh(p, group_name);
    }
    if matches!(
        env,
        LaunchEnv::PowerShell | LaunchEnv::Ide | LaunchEnv::Explorer
    ) && !p.has_windows_path()
    {
        return Err(format!(
            "项目 `{}` 没有 Windows 路径，不能使用 {}",
            p.name,
            env.label()
        ));
    }
    if p.is_ssh_project() {
        return Err(format!(
            "项目 `{}` 是 SSH 远程项目，只支持 SSH 终端启动",
            p.name
        ));
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
/// askpass token 校验文件（成功/失败/中断都走）。
pub fn wait_spawned(spawned: SpawnedDirect) -> Result<(), String> {
    let result = match spawned.child {
        Some(child) => wait_console_child(child).map_err(|e| format!("等待子进程结束失败: {e}")),
        None => Ok(()),
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

    #[test]
    fn parse_ssh_target_basic_port_and_ipv6_guard() {
        assert_eq!(
            parse_ssh_target("abc@172.16.14.10"),
            ("abc@172.16.14.10".into(), None)
        );
        assert_eq!(
            parse_ssh_target("abc@host:2222"),
            ("abc@host".into(), Some("2222".into()))
        );
        assert_eq!(
            parse_ssh_target("abc@host:22x"),
            ("abc@host:22x".into(), None)
        );
        assert_eq!(parse_ssh_target(":2222"), (":2222".into(), None));
        // IPv6 一律不拆端口（v1 不支持，但保证不拆坏目标串）
        assert_eq!(parse_ssh_target("abc@::1"), ("abc@::1".into(), None));
        assert_eq!(
            parse_ssh_target("abc@[::1]:22"),
            ("abc@[::1]:22".into(), None)
        );
    }

    #[test]
    fn ssh_args_bare_and_with_path() {
        assert_eq!(
            ssh_args("abc@172.16.14.10", None, ""),
            vec!["abc@172.16.14.10"]
        );
        assert_eq!(
            ssh_args("abc@h:2222", None, "/opt/foo"),
            vec![
                "-p",
                "2222",
                "abc@h",
                "-t",
                "cd '/opt/foo' 2>/dev/null; exec $SHELL"
            ]
        );
        assert_eq!(
            ssh_args("abc@h", Some("C:\\tmp\\k.key"), ""),
            vec!["-i", "C:\\tmp\\k.key", "abc@h"]
        );
        // 路径含单引号转义
        assert_eq!(
            ssh_args("abc@h", None, "/opt/i't's"),
            vec!["abc@h", "-t", "cd '/opt/i''t''s' 2>/dev/null; exec $SHELL"]
        );
    }

    #[test]
    fn askpass_env_ok_requires_all_secrets_decryptable() {
        let mut p = Project::new("srv", "", "");
        // 无任何秘密：env 无用但也无害（不会自动填），视为 ok
        assert!(askpass_env_ok(&p));
        // 密文损坏 -> 不允许注入（force 下空输出会导致认证必败）
        p.ssh_password_enc = "broken-b64!!".into();
        assert!(!askpass_env_ok(&p));
        // 合法 DPAPI 密文 -> 允许
        p.ssh_password_enc = secret::protect("pw").unwrap();
        assert!(askpass_env_ok(&p));
        // 口令密文损坏 -> 阻断
        p.ssh_key_pass_enc = "!!".into();
        assert!(!askpass_env_ok(&p));
    }

    #[test]
    fn has_saved_secrets_gates_askpass_injection() {
        let mut p = Project::new("srv", "", "");
        // 无任何秘密：不注入（force 会劫持交互密码提示）
        assert!(!has_saved_secrets(&p));
        // 仅密码 / 仅口令 / 都有：注入
        p.ssh_password_enc = secret::protect("pw").unwrap();
        assert!(has_saved_secrets(&p));
        let mut p2 = Project::new("srv", "", "");
        p2.ssh_key_pass_enc = secret::protect("kp").unwrap();
        assert!(has_saved_secrets(&p2));
        // 空白密文视同未保存
        let mut p3 = Project::new("srv", "", "");
        p3.ssh_password_enc = "  ".into();
        assert!(!has_saved_secrets(&p3));
    }

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
        let token = random_token_hex().unwrap();
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
