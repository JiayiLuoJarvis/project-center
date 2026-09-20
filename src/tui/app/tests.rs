use super::*;
use crate::domain::models::{
    Connection, DeletedItem, Endpoint, Group, Project, ProjectCommand, rfc3339_now,
};
use crate::persist::RecentRecord;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn sample() -> ProjectData {
    ProjectData {
        groups: vec![
            Group {
                name: "dev".into(),
                alias: String::new(),
                projects: vec![Project::new("pcs", r"E:\pcs", "")],
            },
            Group {
                name: "tools".into(),
                alias: String::new(),
                projects: vec![],
            },
        ],
        ..Default::default()
    }
}

fn ctrl_c() -> KeyEvent {
    let mut ev = key(KeyCode::Char('c'));
    ev.modifiers = KeyModifiers::CONTROL;
    ev
}

#[test]
fn quit_on_q() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    assert!(matches!(
        app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
        Outcome::Continue
    ));
    assert!(app.quit_confirm);
    assert!(matches!(
        app.handle(key(KeyCode::Char('y')), &mut data, &mut config),
        Outcome::Quit
    ));
    assert!(!app.quit_confirm);
}

#[test]
fn ctrl_c_quits() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    assert!(matches!(
        app.handle(ctrl_c(), &mut data, &mut config),
        Outcome::Continue
    ));
    assert!(app.quit_confirm);
    assert!(matches!(
        app.handle(ctrl_c(), &mut data, &mut config),
        Outcome::Quit
    ));
}

#[test]
fn quit_confirm_cancel_keeps_mode() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.mode = Mode::Confirm {
        message: "确认清空回收站？ y/N".into(),
        kind: ConfirmKind::EmptyTrash,
    };
    app.handle(ctrl_c(), &mut data, &mut config);
    assert!(app.quit_confirm);
    assert!(matches!(
        app.handle(key(KeyCode::Esc), &mut data, &mut config),
        Outcome::Continue
    ));
    assert!(!app.quit_confirm);
    assert!(matches!(
        app.mode,
        Mode::Confirm {
            kind: ConfirmKind::EmptyTrash,
            ..
        }
    ));
}

#[test]
fn help_q_closes_without_quit_confirm() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.mode = Mode::Help;
    assert!(matches!(
        app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
        Outcome::Continue
    ));
    assert!(!app.quit_confirm);
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn filter_q_is_literal() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.mode = Mode::Filter;
    app.handle(key(KeyCode::Char('q')), &mut data, &mut config);
    assert_eq!(app.filter, "q");
    assert!(!app.quit_confirm);
    assert!(matches!(app.mode, Mode::Filter));
}

#[test]
fn quit_confirm_second_q_quits() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char('q')), &mut data, &mut config);
    assert!(matches!(
        app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
        Outcome::Quit
    ));
}

#[test]
fn filter_narrows_and_esc_clears() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
    assert_eq!(app.filter, "t");
    assert_eq!(app.filtered_group_indices(&data).len(), 1);
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(app.mode, Mode::Browse));
}

#[test]
fn filter_reselects_first_match() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.left_sel = 1;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
    assert_eq!(app.left_sel, 0);
    assert_eq!(app.left_is_group(&data, 0), Some(1)); // tools
    app.handle(key(KeyCode::Backspace), &mut data, &mut config);
    assert_eq!(app.left_is_group(&data, 0), Some(0)); // dev
}

#[test]
fn filter_enter_opens_group() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.left_sel = 1;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(app.focus, Focus::Projects);
    assert_eq!(app.left_is_group(&data, app.left_sel), Some(1));
    assert!(matches!(app.right_pane, RightPane::Projects));
}

#[test]
fn filter_resets_project_selection() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.right_sel = 5;
    app.mode = Mode::Filter;
    app.handle(key(KeyCode::Char('p')), &mut data, &mut config);
    assert_eq!(app.right_sel, 0);
    assert!(matches!(app.mode, Mode::Filter));
}

#[test]
fn filter_enter_trash_maps_selection() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    data.trash = vec![
        DeletedItem {
            id: "id-alpha".into(),
            kind: "project".into(),
            group: "dev".into(),
            name: "alpha".into(),
            ..Default::default()
        },
        DeletedItem {
            id: "id-beta".into(),
            kind: "project".into(),
            group: "dev".into(),
            name: "beta".into(),
            ..Default::default()
        },
    ];
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.right_pane = RightPane::Trash;
    app.mode = Mode::Filter;
    app.handle(key(KeyCode::Char('b')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(app.mode, Mode::ActionMenu { .. }));
    match &app.mode {
        Mode::ActionMenu {
            kind: ActionKind::TrashItem { id },
            ..
        } => assert_eq!(id, "id-beta"),
        _ => unreachable!(),
    }
}

#[test]
fn settings_comma_opens_overlay() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::SettingsMenu { selected: 0 }));
}

#[test]
fn settings_enter_connections_and_esc_stack() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::Browse));
    assert!(matches!(app.right_pane, RightPane::Connections));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::SettingsMenu { selected: 0 }));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::Browse));
    assert!(matches!(app.right_pane, RightPane::Projects));
}

#[test]
fn settings_opens_trash_and_config() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(app.right_pane, RightPane::Trash));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
}

#[test]
fn left_list_has_no_trash_or_config() {
    let data = sample();
    let app = App::new(&data);
    let items = app.left_items(&data);
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], LeftItem::Group(0)));
    assert!(matches!(items[1], LeftItem::Group(1)));
}

#[test]
fn filter_enter_zero_match_stays_on_groups() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('z')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(app.mode, Mode::Browse));
    assert!(matches!(app.right_pane, RightPane::Projects));
}

#[test]
fn filter_enter_project_opens_launch_picker() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.mode = Mode::Filter;
    app.handle(key(KeyCode::Char('p')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(app.mode, Mode::LaunchPicker { .. }));
}

#[test]
fn filter_enter_tool_maps_selection() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.right_pane = RightPane::ConfigTools {
        env: ConfigEnv::Wsl,
    };
    app.mode = Mode::Filter;
    // 默认 WSL 工具为 [opencode, cursor-agent]，"u" 只匹配真实下标 1 的 cursor-agent
    app.handle(key(KeyCode::Char('u')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    match &app.mode {
        Mode::ActionMenu {
            kind: ActionKind::ConfigTool { index, .. },
            ..
        } => assert_eq!(*index, 1),
        _ => unreachable!(),
    }
}

#[test]
fn filter_enter_command_maps_selection() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    data.groups[0].projects[0].commands = vec![
        ProjectCommand::new("build", "wsl", "cargo build"),
        ProjectCommand::new("serve", "ide", "code ."),
    ];
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.right_pane = RightPane::Commands {
        group: "dev".into(),
        project_id: data.groups[0].projects[0].id.clone(),
    };
    app.mode = Mode::Filter;
    // "se" 只匹配真实下标 1 的 serve
    app.handle(key(KeyCode::Char('s')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    match &app.mode {
        Mode::ActionMenu {
            kind: ActionKind::Command { index, .. },
            ..
        } => assert_eq!(*index, 1),
        _ => unreachable!(),
    }
}

#[test]
fn esc_pops_config_tools_to_envs() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.focus = Focus::Projects;
    assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
    app.right_sel = 0;
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(
        app.right_pane,
        RightPane::ConfigTools {
            env: ConfigEnv::Wsl
        }
    ));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
    assert_eq!(app.right_sel, 0);
}

#[test]
fn esc_clears_filter_before_popping_config_tools() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.focus = Focus::Projects;
    app.right_sel = 1;
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(
        app.right_pane,
        RightPane::ConfigTools {
            env: ConfigEnv::PowerShell
        }
    ));
    app.filter = "x".into();
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(matches!(
        app.right_pane,
        RightPane::ConfigTools {
            env: ConfigEnv::PowerShell
        }
    ));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
    assert_eq!(app.right_sel, 1);
}

#[test]
fn esc_pops_commands_to_projects() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    let id = data.groups[0].projects[0].id.clone();
    app.right_pane = RightPane::Commands {
        group: "dev".into(),
        project_id: id,
    };
    app.focus = Focus::Projects;
    app.right_sel = 0;
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(app.right_pane, RightPane::Projects));
}

#[test]
fn confirm_y_deletes_empty_group() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.left_sel = 1;
    app.focus = Focus::Groups;
    app.handle(key(KeyCode::Char('d')), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::Confirm { .. }));
    app.handle(key(KeyCode::Char('y')), &mut data, &mut config);
    assert_eq!(data.groups.len(), 1);
    assert_eq!(data.trash.len(), 1);
}

#[test]
fn tab_switches_focus() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    assert_eq!(app.focus, Focus::Groups);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    assert_eq!(app.focus, Focus::Projects);
}

#[test]
fn add_group_when_no_groups_left() {
    let mut data = ProjectData::default();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Form {
            kind: FormKind::AddGroup,
            ..
        }
    ));
}

#[test]
fn add_group_when_focus_on_groups() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Form {
            kind: FormKind::AddGroup,
            ..
        }
    ));
}

#[test]
fn add_project_opens_form_and_keeps_error() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Form {
            kind: FormKind::AddProject { .. },
            ..
        }
    ));
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Form { error, .. } => {
            assert!(error.as_ref().is_some_and(|e| e.contains("项目名")));
        }
        other => panic!("expected form with error, got {other:?}"),
    }
    assert_eq!(data.groups[0].projects.len(), 1);
}

#[test]
fn launch_picker_filters_by_typing() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.right_sel = 0;
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::LaunchPicker { .. }));
    app.handle(key(KeyCode::Char('w')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('s')), &mut data, &mut config);
    match &app.mode {
        Mode::LaunchPicker {
            filter,
            options,
            labels,
            selected,
            ..
        } => {
            assert_eq!(filter, "ws");
            let indices = App::launch_filter_indices(options, labels, filter);
            assert!(!indices.is_empty());
            assert!(indices.contains(selected));
        }
        other => panic!("expected launch picker, got {other:?}"),
    }
}

#[test]
fn form_tab_moves_focus_to_browse_button() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    match &app.mode {
        Mode::Form { focus, fields, .. } => {
            assert_eq!(*focus, 3);
            assert!(matches!(fields[*focus], FormField::Button { .. }));
        }
        other => panic!("expected form, got {other:?}"),
    }
    assert!(matches!(
        app.handle(key(KeyCode::Enter), &mut data, &mut config),
        Outcome::PickFolder { target: 2 }
    ));
}

#[test]
fn form_text_cursor_moves_and_edits_mid_string() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "abcd");
    app.handle(key(KeyCode::Left), &mut data, &mut config);
    app.handle(key(KeyCode::Left), &mut data, &mut config);
    app.handle(key(KeyCode::Backspace), &mut data, &mut config);
    app.handle(key(KeyCode::Char('X')), &mut data, &mut config);
    app.handle(key(KeyCode::Delete), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, cursor, .. } => {
            // abcd → ←← → 光标在 c 前 → Backspace 删 b → aXcd → Delete 删 c → aXd
            assert_eq!(App::field_value(fields, 0), "aXd");
            assert_eq!(*cursor, 2);
        }
        other => panic!("expected form, got {other:?}"),
    }
    app.handle(key(KeyCode::Home), &mut data, &mut config);
    match &app.mode {
        Mode::Form { cursor, .. } => assert_eq!(*cursor, 0),
        other => panic!("expected form, got {other:?}"),
    }
    app.handle(key(KeyCode::End), &mut data, &mut config);
    match &app.mode {
        Mode::Form { cursor, .. } => assert_eq!(*cursor, 3),
        other => panic!("expected form, got {other:?}"),
    }
}

#[test]
fn form_text_cursor_handles_unicode() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "中文路径");
    app.handle(key(KeyCode::Home), &mut data, &mut config);
    app.handle(key(KeyCode::Right), &mut data, &mut config);
    app.handle(key(KeyCode::Delete), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, cursor, .. } => {
            assert_eq!(App::field_value(fields, 0), "中路径");
            assert_eq!(*cursor, 1);
        }
        other => panic!("expected form, got {other:?}"),
    }
}

fn seed_connection(
    data: &mut ProjectData,
    name: &str,
    user: &str,
    host: &str,
    port: u16,
) -> String {
    let endpoint = Endpoint {
        user: user.into(),
        host: host.into(),
        port,
    };
    let conn = Connection::new(name, &endpoint, &rfc3339_now());
    let id = conn.id.clone();
    data.connections.push(conn);
    id
}

fn type_chars(app: &mut App, data: &mut ProjectData, config: &mut AppConfig, text: &str) {
    for c in text.chars() {
        app.handle(key(KeyCode::Char(c)), data, config);
    }
}

#[test]
fn project_form_layout_is_named_fields() {
    let data = sample();
    let fields = App::project_fields(&data, None);
    assert_eq!(fields.len(), ProjectField::COUNT);
    assert!(matches!(
        fields[ProjectField::Connection as usize],
        FormField::Select { .. }
    ));
    assert!(matches!(
        fields[ProjectField::RemotePath as usize],
        FormField::Text { .. }
    ));
}

#[test]
fn connection_form_layout_is_named_fields() {
    let fields = App::connection_fields(None);
    assert_eq!(fields.len(), ConnField::COUNT);
    assert!(matches!(
        fields[ConnField::Password as usize],
        FormField::Password { .. }
    ));
    match &fields[ConnField::BrowseKey as usize] {
        FormField::Button {
            action: ButtonAction::PickFile { target },
            ..
        } => assert_eq!(*target, ConnField::KeySource as usize),
        other => panic!("expected PickFile, got {other:?}"),
    }
}

#[test]
fn add_ssh_project_picks_existing_connection() {
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let cid = seed_connection(&mut data, "14.10", "abc", "192.0.2.10", 22);
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "srv");
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::ListPicker {
            kind: ListKind::PickConnection { .. },
            ..
        }
    ));
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, .. } => {
            assert_eq!(
                App::field_value(fields, ProjectField::Connection as usize),
                cid
            );
        }
        other => panic!("expected project form, got {other:?}"),
    }
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "/opt/x");
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[1];
    assert!(p.is_ssh_project());
    assert_eq!(p.connection_id, cid);
    assert_eq!(p.path, "/opt/x");
    assert!(p.wsl_path.is_empty());

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn new_connection_from_picker_resumes_project_form() {
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "srv");
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Form {
            kind: FormKind::AddConnection { resume: Some(_) },
            ..
        } => {}
        other => panic!("expected nested connection form, got {other:?}"),
    }
    type_chars(&mut app, &mut data, &mut config, "lab");
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "abc");
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "h");
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Form {
            kind: FormKind::AddProject { .. },
            fields,
            ..
        } => {
            let cid = App::field_value(fields, ProjectField::Connection as usize);
            assert!(!cid.is_empty());
            assert_eq!(data.connections.len(), 1);
            assert_eq!(data.connections[0].host, "h");
        }
        other => panic!("expected resumed project form, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn esc_from_nested_connection_form_returns_to_picker() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Form {
            kind: FormKind::AddConnection { resume: Some(_) },
            ..
        }
    ));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::ListPicker {
            kind: ListKind::PickConnection { .. },
            ..
        }
    ));
    app.handle(key(KeyCode::Esc), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Form {
            kind: FormKind::AddProject { .. },
            ..
        }
    ));
}

#[test]
fn delete_connection_refuses_when_referenced() {
    let mut data = sample();
    let cid = seed_connection(&mut data, "lab", "abc", "h", 22);
    data.groups[0].projects[0].connection_id = cid;
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Char('d')), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::Browse));
    assert!(
        app.flash
            .as_ref()
            .is_some_and(|m| m.contains("引用") && m.contains("lab"))
    );
    assert_eq!(data.connections.len(), 1);
}

#[test]
fn delete_unused_connection_confirms() {
    let mut data = sample();
    seed_connection(&mut data, "lab", "abc", "h", 22);
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Char('d')), &mut data, &mut config);
    assert!(matches!(
        app.mode,
        Mode::Confirm {
            kind: ConfirmKind::DeleteConnection { .. },
            ..
        }
    ));
}

#[test]
fn add_connection_from_settings_list() {
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "lab");
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "example.com");
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(data.connections.len(), 1);
    assert_eq!(data.connections[0].name, "lab");
    assert_eq!(data.connections[0].host, "example.com");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn ctrl_u_clears_focused_field() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "abc");
    let mut ctrl_u = key(KeyCode::Char('u'));
    ctrl_u.modifiers = KeyModifiers::CONTROL;
    app.handle(ctrl_u, &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, .. } => assert_eq!(App::field_value(fields, 0), ""),
        other => panic!("expected form, got {other:?}"),
    }
}

#[test]
fn file_pick_fills_connection_key_source() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    app.resume_after_file_pick(
        Some(r"C:\keys\id_ed25519".into()),
        ConnField::KeySource as usize,
    );
    match &app.mode {
        Mode::Form { fields, focus, .. } => {
            assert_eq!(
                App::field_value(fields, ConnField::KeySource as usize),
                r"C:\keys\id_ed25519"
            );
            assert_eq!(*focus, ConnField::KeySource as usize);
        }
        other => panic!("expected form, got {other:?}"),
    }
}

#[test]
fn clear_key_button_marks_intent_on_connection_form() {
    let mut data = sample();
    let mut conn = Connection::new(
        "lab",
        &Endpoint {
            user: "abc".into(),
            host: "h".into(),
            port: 22,
        },
        &rfc3339_now(),
    );
    conn.ssh_key_file = "keys/x.key".into();
    conn.ssh_key_path = r"C:\keys\id".into();
    data.connections.push(conn);
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.handle(key(KeyCode::Char(',')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    for _ in 0..ConnField::ClearKey as usize {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.clear_key);
    match &app.mode {
        Mode::Form { fields, focus, .. } => {
            assert_eq!(App::field_value(fields, ConnField::KeySource as usize), "");
            assert_eq!(*focus, ConnField::KeySource as usize);
        }
        other => panic!("expected form, got {other:?}"),
    }
}

#[test]
fn add_local_project_without_connection() {
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    type_chars(&mut app, &mut data, &mut config, "srv");
    for _ in 0..4 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    type_chars(&mut app, &mut data, &mut config, "/srv");
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[1];
    assert!(!p.is_ssh_project());
    assert!(p.connection_id.is_empty());
    assert_eq!(p.wsl_path, "/srv");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn edit_ssh_project_keeps_connection_on_save() {
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let cid = seed_connection(&mut data, "lab", "abc", "h", 2222);
    {
        let p = &mut data.groups[0].projects[0];
        p.connection_id = cid.clone();
        p.path = "/opt/x".into();
    }
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[0];
    assert_eq!(p.connection_id, cid);
    assert_eq!(p.path, "/opt/x");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn filter_digit_hits_group_store_index() {
    let data = sample();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;

    app.filter = "1".into();
    assert_eq!(app.filtered_group_indices(&data), vec![0]);
    assert_eq!(app.left_items(&data), vec![LeftItem::Group(0)]);

    app.filter = "01".into();
    assert_eq!(app.filtered_group_indices(&data), vec![0]);

    app.filter = "2".into();
    assert_eq!(app.filtered_group_indices(&data), vec![1]);
    assert_eq!(app.left_items(&data), vec![LeftItem::Group(1)]);
}

#[test]
fn filter_digit_hits_project_store_index() {
    let mut data = sample();
    let mut alpha = Project::new("alpha", r"E:\a", "");
    let mut beta = Project::new("beta", r"E:\b", "");
    alpha.id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into();
    beta.id = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".into();
    data.groups[0].projects = vec![alpha, beta];
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;

    app.filter = "1".into();
    assert_eq!(app.filtered_project_indices(&data, 0), vec![0]);

    app.filter = "01".into();
    assert_eq!(app.filtered_project_indices(&data, 0), vec![0]);

    app.filter = "2".into();
    assert_eq!(app.filtered_project_indices(&data, 0), vec![1]);
}

#[test]
fn filter_alias_wk_does_not_take_index_branch() {
    let mut data = sample();
    data.groups[0].alias = "wk".into();
    data.groups[0].projects = vec![
        Project::new("alpha", r"E:\a", "").with_alias("wk"),
        Project::new("beta", r"E:\b", ""),
    ];
    let mut app = App::new(&data);

    app.focus = Focus::Groups;
    app.filter = "wk".into();
    assert_eq!(app.filtered_group_indices(&data), vec![0]);
    assert_eq!(app.left_items(&data), vec![LeftItem::Group(0)]);

    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.filter = "wk".into();
    assert_eq!(app.filtered_project_indices(&data, 0), vec![0]);
}

fn recent_record(id: &str, env: &str, tool: &str, command: &str) -> RecentRecord {
    RecentRecord {
        id: id.into(),
        ts: 1,
        env: env.into(),
        tool_name: tool.into(),
        command: command.into(),
    }
}

fn with_recent(records: &[RecentRecord], f: impl FnOnce()) {
    let _lock = crate::persist::test_env::lock_appdata();
    let dir = std::env::temp_dir().join(format!("pcs_recent_tui_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let saved = std::env::var_os("PCS_DATA_DIR");
    unsafe { std::env::set_var("PCS_DATA_DIR", &dir) };
    crate::persist::save_recent_to(records, &crate::persist::recent_file_path()).unwrap();
    f();
    unsafe {
        match saved {
            Some(value) => std::env::set_var("PCS_DATA_DIR", value),
            None => std::env::remove_var("PCS_DATA_DIR"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn browse_r_opens_recent_picker() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.handle(key(KeyCode::Char('r')), &mut data, &mut config);
    assert!(matches!(app.mode, Mode::RecentPicker { .. }));
}

#[test]
fn browse_trash_r_stays_restore() {
    let mut data = sample();
    data.trash.push(DeletedItem {
        id: "id-gone".into(),
        kind: "project".into(),
        group: "dev".into(),
        name: "gone".into(),
        ..Default::default()
    });
    with_recent(&[], || {
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.right_pane = RightPane::Trash;
        app.handle(key(KeyCode::Char('r')), &mut data, &mut config);
        assert!(matches!(app.mode, Mode::Browse));
        assert!(data.trash.is_empty());
        assert!(data.groups[0].projects.iter().any(|p| p.name == "gone"));
    });
}

#[test]
fn recent_picker_enter_launches_recorded_option() {
    let mut data = sample();
    let id = data.groups[0].projects[0].id.clone();
    let record = recent_record(&id, "wsl", "终端", "");
    with_recent(&[record], || {
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.handle(key(KeyCode::Char('r')), &mut data, &mut config);
        match app.handle(key(KeyCode::Enter), &mut data, &mut config) {
            Outcome::Launch { option, group, .. } => {
                assert_eq!(group, "dev");
                assert_eq!(option.env, crate::launch::LaunchEnv::Wsl);
                assert_eq!(option.tool_name, "终端");
                assert_eq!(option.command, "");
            }
            other => panic!("expected Launch, got {other:?}"),
        }
    });
}

#[test]
fn recent_picker_space_opens_launch_picker() {
    let mut data = sample();
    let id = data.groups[0].projects[0].id.clone();
    let record = recent_record(&id, "wsl", "终端", "");
    with_recent(&[record], || {
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.handle(key(KeyCode::Char('r')), &mut data, &mut config);
        app.handle(key(KeyCode::Char(' ')), &mut data, &mut config);
        match &app.mode {
            Mode::LaunchPicker {
                group, project_id, ..
            } => {
                assert_eq!(group, "dev");
                assert_eq!(project_id, &id);
            }
            other => panic!("expected LaunchPicker, got {other:?}"),
        }
    });
}
