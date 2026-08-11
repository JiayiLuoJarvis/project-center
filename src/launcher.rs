use std::process::{Command, Stdio};

use crate::models::Project;
use windows_sys::Win32::System::Console::{SetConsoleCtrlHandler, SetConsoleTitleW};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherKind {
    Wsl,
    PowerShell,
    VsCode,
}

impl LauncherKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wsl => "WSL",
            Self::PowerShell => "PowerShell",
            Self::VsCode => "IDE 启动",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Opencode,
    CursorAgent,
    Terminal,
    Vscode,
    Cursor,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::CursorAgent => "cursor-agent",
            Self::Terminal => "终端",
            Self::Vscode => "VS Code",
            Self::Cursor => "Cursor",
        }
    }
}

/// 非交互式子进程：丢弃 stdio，避免占用父进程管道句柄。
fn spawn_quiet(cmd: &str, args: &[&str]) -> std::io::Result<()> {
    let mut c = Command::new(cmd);
    c.args(args);
    c.stdout(Stdio::null()).stderr(Stdio::null());
    c.spawn().map(|_| ())
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

pub fn open_vs_code(p: &Project) -> Result<(), String> {
    spawn_quiet("cmd", &["/c", "code", p.path.as_str()])
        .map_err(|e| format!("VS Code 启动失败: {e}"))
}

pub fn open_cursor(p: &Project) -> Result<(), String> {
    spawn_quiet("cmd", &["/c", "cursor", p.path.as_str()])
        .map_err(|e| format!("Cursor 启动失败: {e}"))
}

fn open_wsl_tool(p: &Project, group_name: &str, tool: &str) -> Result<(), String> {
    set_console_title(p, group_name);
    let linux = p.linux_path();
    let child = Command::new("wsl.exe")
        .args(["--cd", linux.as_str(), "--", tool])
        .spawn()
        .map_err(|e| format!("{} 启动失败: {e}", tool))?;
    wait_console_child(child).map_err(|e| format!("等待 {} 结束失败: {e}", tool))
}

fn open_ps_tool(p: &Project, group_name: &str, tool: &str) -> Result<(), String> {
    set_console_title(p, group_name);
    let escaped = p.path.replace('\'', "''");
    let script = format!("Set-Location -LiteralPath '{escaped}'; {tool}");
    let child = Command::new("powershell.exe")
        .args(["-Command", script.as_str()])
        .spawn()
        .map_err(|e| format!("{} 启动失败: {e}", tool))?;
    wait_console_child(child).map_err(|e| format!("等待 {} 结束失败: {e}", tool))
}

pub fn open_powershell(p: &Project, group_name: &str) -> Result<(), String> {
    set_console_title(p, group_name);
    let escaped = p.path.replace('\'', "''");
    let script = format!("Set-Location -LiteralPath '{escaped}'");
    let child = Command::new("powershell.exe")
        .args(["-NoExit", "-Command", script.as_str()])
        .spawn()
        .map_err(|e| format!("PowerShell 启动失败: {e}"))?;
    wait_console_child(child).map_err(|e| format!("等待 PowerShell 结束失败: {e}"))
}

pub fn open_wsl(p: &Project, group_name: &str) -> Result<(), String> {
    set_console_title(p, group_name);
    let linux = p.linux_path();
    let child = Command::new("wsl.exe")
        .args(["--cd", linux.as_str()])
        .spawn()
        .map_err(|e| format!("WSL 启动失败: {e}"))?;
    wait_console_child(child).map_err(|e| format!("等待 WSL 结束失败: {e}"))
}

pub fn launch(
    p: &Project,
    group_name: &str,
    kind: LauncherKind,
    tool: ToolKind,
) -> Result<(), String> {
    match (kind, tool) {
        (LauncherKind::Wsl, ToolKind::Terminal) => open_wsl(p, group_name),
        (LauncherKind::Wsl, ToolKind::Opencode) => open_wsl_tool(p, group_name, "opencode"),
        (LauncherKind::Wsl, ToolKind::CursorAgent) => open_wsl_tool(p, group_name, "cursor-agent"),
        (LauncherKind::PowerShell, ToolKind::Terminal) => open_powershell(p, group_name),
        (LauncherKind::PowerShell, ToolKind::Opencode) => open_ps_tool(p, group_name, "opencode"),
        (LauncherKind::PowerShell, ToolKind::CursorAgent) => {
            open_ps_tool(p, group_name, "cursor-agent")
        }
        (LauncherKind::VsCode, ToolKind::Vscode) => open_vs_code(p),
        (LauncherKind::VsCode, ToolKind::Cursor) => open_cursor(p),
        _ => Err("不支持的启动组合".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_title_uses_project_and_group() {
        let project = Project::new("pcs", "C:\\dev\\pcs", "/mnt/c/dev/pcs");

        assert_eq!(console_title(&project, "工具"), "pcs - 工具");
    }
}
