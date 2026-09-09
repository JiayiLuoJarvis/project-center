//! CLI：clap 定义、子命令分发与人读输出。

mod args;
mod common;
mod config;
mod group;
mod open;
mod output;
mod pin;
mod project;
mod secret;
mod trash;

use anyhow::Result;
use clap::Parser;

use crate::launch::LaunchEnv;
use crate::persist::{Config, Store};
use crate::tui;

use args::*;

pub fn run() -> Result<()> {
    // OpenSSH 把 SSH_ASKPASS 当独立程序调用：`pcs.exe <prompt>`，不会带
    // `__askpass` 子命令。父进程注入 PCS_ASKPASS_TOKEN 时在 clap 之前拦截，
    // 否则 clap 会把 prompt（含空格/撇号）当成未识别子命令并写 stderr，
    // 导致认证失败且污染控制台。
    if std::env::var_os("PCS_ASKPASS_TOKEN").is_some() {
        return secret::cmd_askpass(&secret::askpass_prompt_from_args());
    }
    let cli = Cli::parse();
    // 手工/测试入口：`pcs __askpass <prompt>`（无 token 时 cmd_askpass 输出空）。
    if let Some(Command::Askpass { prompt }) = &cli.command {
        return secret::cmd_askpass(prompt);
    }
    let mut config = Config::load();
    match cli.command {
        None | Some(Command::Menu) => {
            let mut data = Store::load();
            tui::run(&mut data, &mut config)
        }
        Some(Command::Ls { group, json }) => output::cmd_ls(group.as_deref(), json),
        Some(Command::Open(args)) => open::cmd_open(args),
        Some(Command::Wsl(selector)) => open::cmd_direct(&selector, LaunchEnv::Wsl),
        Some(Command::PowerShell(selector)) => open::cmd_direct(&selector, LaunchEnv::PowerShell),
        Some(Command::Ssh(selector)) => open::cmd_direct(&selector, LaunchEnv::Ssh),
        Some(Command::Code(selector)) => open::cmd_code(&selector, &config),
        Some(Command::Path(selector)) => open::cmd_path(&selector, false),
        Some(Command::WslPath(selector)) => open::cmd_path(&selector, true),
        Some(Command::Add(args)) => project::cmd_add(args),
        Some(Command::Edit(args)) => project::cmd_edit(args),
        Some(Command::Rm(args)) => project::cmd_rm(args),
        Some(Command::Mv(args)) => project::cmd_mv(args),
        Some(Command::Run(args)) => project::cmd_run(args),
        Some(Command::Group(command)) => group::cmd_group(command),
        Some(Command::Config(command)) => config::cmd_config(command),
        Some(Command::Trash(command)) => trash::cmd_trash(command),
        Some(Command::Pin(command)) => pin::cmd_pin(command),
        Some(Command::Secret(command)) => secret::cmd_secret(command),
        // 上面已提前 return；此臂仅满足 match 穷尽。
        Some(Command::Askpass { prompt }) => secret::cmd_askpass(&prompt),
    }
}
