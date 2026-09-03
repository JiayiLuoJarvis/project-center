use std::process::Command;

use windows_sys::Win32::System::Console::{SetConsoleCtrlHandler, SetConsoleTitleW};

use crate::models::Project;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchEnv {
    Wsl,
    PowerShell,
    Ide,
    Explorer,
}

impl LaunchEnv {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wsl => "WSL",
            Self::PowerShell => "PowerShell",
            Self::Ide => "IDE",
            Self::Explorer => "文件夹",
        }
    }

    /// 自定义命令显示用短标签：wsl / ps / ide。
    pub fn short_label(self) -> &'static str {
        match self {
            Self::Wsl => "wsl",
            Self::PowerShell => "ps",
            Self::Ide => "ide",
            Self::Explorer => "",
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

/// 生成子进程（不等待）：WSL/PowerShell 返回 `Some(child)` 供调用方在
/// 「生成后、等待前」插入记录等操作后自行 `wait_direct`；
/// IDE/资源管理器静默启动、无等待，返回 `None`。
/// 校验 Windows 路径可用性（与旧 `launch_direct` 语义一致）。
pub fn spawn_direct(
    p: &Project,
    group_name: &str,
    env: LaunchEnv,
    command: &str,
) -> Result<Option<std::process::Child>, String> {
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
    match env {
        LaunchEnv::Wsl => spawn_wsl(p, group_name, command).map(Some),
        LaunchEnv::PowerShell => spawn_powershell(p, group_name, command).map(Some),
        LaunchEnv::Ide => spawn_ide(p, command).map(|_| None),
        LaunchEnv::Explorer => spawn_explorer(p).map(|_| None),
    }
}

/// 等待 WSL/PowerShell 子进程退出（阻塞、抑制 Ctrl+C）。
pub fn wait_direct(child: std::process::Child) -> Result<(), String> {
    wait_console_child(child).map_err(|e| format!("等待子进程结束失败: {e}"))
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
