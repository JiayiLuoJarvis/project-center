mod actions;
mod app;
mod theme;
mod ui;
mod widgets;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::config::AppConfig;
use crate::launcher;
use crate::models::{Project, ProjectData};
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

fn run_with(data: &mut ProjectData, config: &mut AppConfig, mut app: App) -> Result<()> {
    let mut terminal = ratatui::try_init()?;
    let result = loop_ui(&mut terminal, data, config, &mut app);
    ratatui::restore();
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
                ratatui::restore();
                let launch_result = do_launch(&project, &group, &option);
                if exit_after || app.short_session {
                    return launch_result.map(|_| ());
                }
                match launch_result {
                    Ok(code) => {
                        // ssh 失败退出时控制台报错一闪而过，返回 TUI 前暂停让用户看清。
                        if code != 0 && option.env == crate::launcher::LaunchEnv::Ssh {
                            pause_after_ssh_failure(code);
                        }
                    }
                    Err(e) => app.flash = Some(e.to_string()),
                }
                *terminal = ratatui::try_init()?;
                app.mode = Mode::Browse;
            }
            Outcome::PickFolder => {
                ratatui::restore();
                let path = rfd::FileDialog::new()
                    .pick_folder()
                    .map(|p| p.to_string_lossy().trim().to_string());
                app.resume_after_folder_pick(path);
                *terminal = ratatui::try_init()?;
            }
        }
    }
}

fn do_launch(project: &Project, group: &str, option: &crate::menu::LaunchOption) -> Result<i32> {
    let spawned = launcher::spawn_direct(project, group, option.env, &option.command)
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
