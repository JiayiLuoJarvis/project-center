use super::*;
use crate::domain::models::{DeletedItem, Group, Project, ProjectCommand};

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
    app.left_sel = 2; // 配置项
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
    app.left_sel = 2;
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
fn filter_enter_keeps_tail_item_position() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
    // 过滤视图 [tools, 回收站, 配置]，移动到配置
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
    assert!(app.left_is_config(&data, app.left_sel));
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(app.left_is_config(&data, app.left_sel));
    assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
}

#[test]
fn filter_enter_zero_match_lands_on_trash() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
    app.handle(key(KeyCode::Char('z')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    assert!(app.filter.is_empty());
    assert!(app.left_is_trash(&data, app.left_sel));
    assert!(matches!(app.right_pane, RightPane::Trash));
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
    app.left_sel = app.left_count(&data) - 1;
    app.sync_right_pane(&data);
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
    app.left_sel = app.left_count(&data) - 1;
    app.sync_right_pane(&data);
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
    assert!(app.left_is_trash(&data, app.left_sel));
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
fn add_group_when_left_sel_on_trash() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Groups;
    app.left_sel = data.groups.len();
    app.sync_right_pane(&data);
    assert!(app.left_is_trash(&data, app.left_sel));
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
        Outcome::PickFolder
    ));
}

#[test]
fn add_ssh_project_via_form() {
    // 提交会经 actions::save_data 写真实数据根：串行 + 把 APPDATA 指向
    // 临时目录，避免污染 projects.json（并发下 rename 也可能冲突导致保存失败）。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    // 字段 0：项目名
    for c in "srv".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // Tab 到字段 5（登录用户）
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 6：主机
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "172.16.14.10".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 7：端口预填 22，直接留用
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    // 字段 8：远程路径
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "/opt/x".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[1];
    assert!(p.is_ssh_project());
    assert_eq!(p.ssh_target, "abc@172.16.14.10:22");
    assert_eq!(p.path, "/opt/x");
    assert!(p.wsl_path.is_empty());

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[cfg(windows)]
#[test]
fn add_ssh_project_with_secrets_via_form() {
    // 新增表单直接携带密钥/密码/口令：一次提交完成秘密保存（设计 §9）。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    // 准备一个可导入的私钥源文件
    let key_source = temp_appdata.join("source_key");
    std::fs::write(&key_source, "-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    for c in "srv".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // Tab 到字段 5（登录用户）
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 6：主机
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "h".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 7 端口预填 22 留用，字段 8 远程路径留空：Tab 到字段 9（私钥来源路径）
    for _ in 0..3 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in key_source.to_string_lossy().chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 10：登录密码
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "pw123".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 11：私钥口令留空，直接提交
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[1];
    assert!(p.is_ssh_project());
    assert_eq!(p.ssh_target, "abc@h:22");
    assert!(!p.ssh_password_enc.is_empty(), "密码应已加密保存");
    assert_eq!(
        crate::persist::unprotect(&p.ssh_password_enc).unwrap(),
        "pw123"
    );
    assert!(!p.ssh_key_file.is_empty(), "密钥应已导入 sidecar");
    assert_eq!(
        crate::persist::read_key_file(&p.ssh_key_file).unwrap(),
        "-----BEGIN OPENSSH PRIVATE KEY-----"
    );
    assert_eq!(p.ssh_key_path, key_source.to_string_lossy());
    // 普通项目路径未受影响
    assert!(p.wsl_path.is_empty());

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn edit_ssh_project_uses_ssh_form() {
    let mut data = sample();
    {
        let p = &mut data.groups[0].projects[0];
        p.ssh_target = "abc@h".into();
        p.path = "/opt/x".into();
        p.ssh_password_enc = "PWENC".into();
    }
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, .. } => {
            assert_eq!(fields.len(), 7, "SSH 编辑表单 7 字段");
            assert_eq!(App::field_value(fields, 0), "abc");
            assert_eq!(App::field_value(fields, 1), "h");
            assert_eq!(App::field_value(fields, 2), "");
            assert_eq!(App::field_value(fields, 3), "/opt/x");
            assert!(App::field_value(fields, 5).is_empty(), "密码初始为空");
            // empty_hint 标明已存/未存；值行不再硬编码「未设置」。
            let FormField::Password {
                label, empty_hint, ..
            } = &fields[5]
            else {
                panic!("字段 5 应为 Password，实际 {:?}", fields.get(5));
            };
            assert_eq!(label, "登录密码");
            assert!(empty_hint.contains("已保存"), "实际 {empty_hint}");
            let FormField::Password {
                label, empty_hint, ..
            } = &fields[6]
            else {
                panic!("字段 6 应为 Password，实际 {:?}", fields.get(6));
            };
            assert_eq!(label, "私钥口令");
            assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
        }
        other => panic!("expected SSH form, got {other:?}"),
    }
}

#[test]
fn edit_ssh_form_labels_unsaved_when_no_secrets() {
    // 无任何秘密的 SSH 项目：密码/口令 empty_hint 均标「未设置」。
    let mut data = sample();
    {
        let p = &mut data.groups[0].projects[0];
        p.ssh_target = "abc@h".into();
    }
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, .. } => {
            let FormField::Password { empty_hint, .. } = &fields[5] else {
                panic!("字段 5 应为 Password，实际 {:?}", fields.get(5));
            };
            assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
            let FormField::Password { empty_hint, .. } = &fields[6] else {
                panic!("字段 6 应为 Password，实际 {:?}", fields.get(6));
            };
            assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
        }
        other => panic!("expected SSH form, got {other:?}"),
    }
}

#[test]
fn edit_normal_project_shows_ssh_conversion_fields() {
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    match &app.mode {
        Mode::Form { fields, .. } => {
            assert_eq!(fields.len(), 9, "普通编辑表单含 SSH 转换字段");
            assert_eq!(App::field_value(fields, 5), "");
            assert_eq!(App::field_value(fields, 6), "");
            assert_eq!(App::field_value(fields, 7), "22", "端口默认 22");
        }
        other => panic!("expected form, got {other:?}"),
    }
}

#[test]
fn compose_ssh_target_cases() {
    assert_eq!(
        App::compose_ssh_target("abc", "h", "22").unwrap(),
        "abc@h:22"
    );
    assert_eq!(App::compose_ssh_target("abc", "h", "").unwrap(), "abc@h");
    assert_eq!(App::compose_ssh_target("", "h", "").unwrap(), "h");
    assert_eq!(
        App::compose_ssh_target(" abc ", " h ", " 22 ").unwrap(),
        "abc@h:22"
    );
    assert!(App::compose_ssh_target("a@b", "h", "").is_err());
    assert!(App::compose_ssh_target("abc", "a@h", "").is_err());
    assert!(App::compose_ssh_target("abc", "h", "22x").is_err());
}

#[test]
fn split_ssh_target_roundtrip() {
    assert_eq!(
        App::split_ssh_target("abc@h:2222"),
        ("abc".to_string(), "h".to_string(), "2222".to_string())
    );
    assert_eq!(
        App::split_ssh_target("abc@h"),
        ("abc".to_string(), "h".to_string(), String::new())
    );
    assert_eq!(
        App::split_ssh_target("h"),
        (String::new(), "h".to_string(), String::new())
    );
    // 往返恒等
    for t in ["abc@h:2222", "abc@h", "h", "h:22"] {
        let (u, h, p) = App::split_ssh_target(t);
        assert_eq!(App::compose_ssh_target(&u, &h, &p).unwrap(), t);
    }
}

#[test]
fn add_ssh_project_custom_port_via_form() {
    // 端口预填 22：Backspace 清空后可填自定义端口。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    for c in "srvp".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 5：登录用户
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 6：主机
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "h".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 7：清空预填 22，改填 2222
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    app.handle(key(KeyCode::Backspace), &mut data, &mut config);
    app.handle(key(KeyCode::Backspace), &mut data, &mut config);
    for c in "2222".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[1];
    assert!(p.is_ssh_project());
    assert_eq!(p.ssh_target, "abc@h:2222");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn edit_ssh_project_save_unchanged_keeps_target() {
    // 编辑表单零修改直接保存：反拆再拼接必须恒等，不改写 ssh_target。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    {
        let p = &mut data.groups[0].projects[0];
        p.ssh_target = "abc@h:2222".into();
        p.path = "/opt/x".into();
    }
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    assert_eq!(data.groups[0].projects[0].ssh_target, "abc@h:2222");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn add_ssh_project_rejects_at_in_host() {
    // 主机含 @ 时拼接失败：表单报错不关闭，不产生项目。
    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    for c in "srv".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "a@h".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Form { error, .. } => {
            assert!(error.as_ref().is_some_and(|e| e.contains('@')));
        }
        other => panic!("应留在表单并报错，实际 {other:?}"),
    }
    assert_eq!(data.groups[0].projects.len(), 1);
}

#[test]
fn edit_normal_project_converts_to_ssh_via_form() {
    // 普通项目编辑表单填写用户/主机即转为 SSH 项目（第三条保存路径）。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
    // 字段 5：登录用户
    for _ in 0..5 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 6：主机
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "h".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 7 端口预填 22 留用；字段 8 远程路径
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "/opt/r".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    let p = &data.groups[0].projects[0];
    assert!(p.is_ssh_project());
    assert_eq!(p.ssh_target, "abc@h:22");
    assert_eq!(p.path, "/opt/r");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}

#[test]
fn add_project_ignores_ssh_user_when_host_empty() {
    // 主机留空时即便填了登录用户也按普通项目保存（user 字段静默忽略）。
    let _guard = crate::persist::test_env::lock_appdata();
    let temp_appdata = std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_appdata).unwrap();
    let _appdata = crate::persist::test_env::AppdataGuard::redirect(&temp_appdata);

    let mut data = sample();
    let mut config = AppConfig::defaults();
    let mut app = App::new(&data);
    app.focus = Focus::Projects;
    app.left_sel = 0;
    app.sync_right_pane(&data);
    app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
    for c in "srv".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // Tab 到字段 4（WSL 路径，普通项目路径二选一）
    for _ in 0..4 {
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
    }
    for c in "/srv".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    // 字段 5：登录用户（主机留空，应被忽略）
    app.handle(key(KeyCode::Tab), &mut data, &mut config);
    for c in "abc".chars() {
        app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
    }
    app.handle(key(KeyCode::Enter), &mut data, &mut config);
    match &app.mode {
        Mode::Browse => {}
        other => panic!("提交成功应回浏览模式，实际 {other:?}"),
    }
    assert_eq!(data.groups[0].projects.len(), 2);
    let p = &data.groups[0].projects[1];
    assert!(!p.is_ssh_project());
    assert!(p.ssh_target.is_empty());
    assert_eq!(p.wsl_path, "/srv");

    let _ = std::fs::remove_dir_all(&temp_appdata);
}
