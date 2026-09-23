//! 全屏 ratatui TUI：生命周期、事件循环、启动后恢复。

mod actions;
mod app;
mod theme;
mod ui;
mod widgets;

use std::io::stdout;

use anyhow::Result;
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;

use crate::domain::models::{Project, ProjectData};
use crate::launch as launcher;
use crate::persist::AppConfig;
use crate::tui::app::{App, Mode, Outcome};

pub fn run(data: &mut ProjectData, config: &mut AppConfig) -> Result<()> {
    run_with(data, config, App::new(data))
}

pub fn choose_launch(
    data: &mut ProjectData,
    config: &mut AppConfig,
    group: &str,
    project_id: &str,
) -> Result<()> {
    run_with(
        data,
        config,
        App::short_launch(data, config, group, project_id),
    )
}

/// 离开备用屏后显式显示光标。
/// `ratatui::restore` 只关 raw / 退备用屏；TUI draw 会 hide 光标，
/// PowerShell 等控制台子进程会继承隐藏状态（WSL/bash 常自行复位）。
fn restore_terminal() {
    ratatui::restore();
    let _ = execute!(stdout(), Show);
}

fn run_with(data: &mut ProjectData, config: &mut AppConfig, mut app: App) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = loop_ui(&mut terminal, data, config, &mut app);
    restore_terminal();
    result
}

fn loop_ui(
    terminal: &mut ratatui::DefaultTerminal,
    data: &mut ProjectData,
    config: &mut AppConfig,
    app: &mut App,
) -> Result<()> {
    loop {
        app.clamp_selection(data, config);
        terminal.draw(|f| ui::render(f, app, data, config))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.handle(key, data, config) {
            Outcome::Continue => {}
            Outcome::Quit => return Ok(()),
            Outcome::Launch {
                project,
                group,
                option,
                exit_after,
            } => {
                restore_terminal();
                let launch_result = do_launch(data, &project, &group, &option);
                if exit_after || app.short_session {
                    return launch_result.map(|_| ());
                }
                match launch_result {
                    Ok(code) => {
                        // ssh 失败退出时控制台报错一闪而过，返回 TUI 前暂停让用户看清。
                        if code != 0 && option.env == crate::launch::LaunchEnv::Ssh {
                            pause_after_ssh_failure(code);
                        }
                    }
                    Err(e) => app.flash = Some(e.to_string()),
                }
                *terminal = ratatui::try_init()?;
                app.mode = Mode::Browse;
            }
            Outcome::PickFolder { target } => {
                restore_terminal();
                let path = rfd::FileDialog::new()
                    .pick_folder()
                    .map(|p| p.to_string_lossy().trim().to_string());
                app.resume_after_folder_pick(path, target);
                *terminal = ratatui::try_init()?;
            }
            Outcome::PickFile { target } => {
                restore_terminal();
                let path = rfd::FileDialog::new()
                    .add_filter("所有文件", &["*"])
                    .add_filter("私钥文件 (*.pem, *.key, *.ppk)", &["pem", "key", "ppk"])
                    .pick_file()
                    .map(|p| p.to_string_lossy().trim().to_string());
                app.resume_after_file_pick(path, target);
                *terminal = ratatui::try_init()?;
            }
            Outcome::SaveMigrateFile => {
                restore_terminal();
                let path = rfd::FileDialog::new()
                    .set_file_name("pcs-migrate.json")
                    .add_filter("JSON", &["json"])
                    .save_file()
                    .map(|p| p.to_string_lossy().trim().to_string());
                match path {
                    Some(path) => {
                        let pack = crate::persist::MigratePack::pack(data, config);
                        match crate::persist::write_pack(std::path::Path::new(&path), &pack) {
                            Ok(()) => app.flash(format!("已导出: {path}")),
                            Err(e) => app.flash(e.to_string()),
                        }
                    }
                    None => app.flash("已取消导出"),
                }
                *terminal = ratatui::try_init()?;
            }
            Outcome::PickMigrateFile => {
                restore_terminal();
                let path = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .add_filter("所有文件", &["*"])
                    .pick_file()
                    .map(|p| p.to_string_lossy().trim().to_string());
                match path {
                    Some(path) => match crate::persist::read_pack(std::path::Path::new(&path)) {
                        Ok(pack) => {
                            app.mode = Mode::Confirm {
                                message:
                                    "导入将替换当前项目与启动工具配置（不含秘密/PIN）。确认？ y/N"
                                        .into(),
                                kind: crate::tui::app::ConfirmKind::ImportMigrate { pack },
                            };
                        }
                        Err(e) => app.flash(e.to_string()),
                    },
                    None => app.flash("已取消导入"),
                }
                *terminal = ratatui::try_init()?;
            }
        }
    }
}

fn do_launch(
    data: &ProjectData,
    project: &Project,
    group: &str,
    option: &crate::launch::LaunchOption,
) -> Result<i32> {
    let spawned = launcher::spawn_direct(data, project, group, option.env, &option.command)
        .map_err(anyhow::Error::msg)?;
    launcher::wait_spawned(spawned).map_err(anyhow::Error::msg)
}

/// ssh 非零退出时在控制台暂停，让报错可读；按 Enter 返回 TUI。
/// 退出码为负（被信号终止）显示「异常终止」。期间 Ctrl+C 可直接杀掉本进程。
fn pause_after_ssh_failure(code: i32) {
    let detail = if code < 0 {
        "异常终止".to_string()
    } else {
        format!("退出码 {code}")
    };
    println!("\nSSH 连接失败（{detail}）。按 Enter 返回…");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
