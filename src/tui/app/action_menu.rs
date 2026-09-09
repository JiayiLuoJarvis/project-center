use super::*;

impl App {
    pub(crate) fn handle_action_menu(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::ActionMenu {
            kind,
            items,
            selected,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Esc => self.back_to_browse(),
            KeyCode::Char('j') | KeyCode::Down => {
                let n = items.len();
                if n > 0 {
                    self.mode = Mode::ActionMenu {
                        kind,
                        items,
                        selected: (selected + 1) % n,
                    };
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = items.len();
                if n > 0 {
                    self.mode = Mode::ActionMenu {
                        kind,
                        items,
                        selected: (selected + n - 1) % n,
                    };
                }
            }
            KeyCode::Enter => {
                self.back_to_browse();
                return self.apply_action(kind, selected, data, config);
            }
            _ => {}
        }
        Outcome::Continue
    }

    pub(crate) fn apply_action(
        &mut self,
        kind: ActionKind,
        selected: usize,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match kind {
            ActionKind::Project { group, project_id } if project_id.is_empty() => {
                // group menu
                match selected {
                    0 => {
                        let alias = data
                            .groups
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(&group))
                            .map(|g| g.alias.clone())
                            .unwrap_or_default();
                        self.open_form(
                            "编辑分组",
                            vec![
                                Self::text_field("分组名", group.clone()),
                                Self::text_field("别名", alias),
                            ],
                            FormKind::RenameGroup { old: group },
                        );
                    }
                    1 => {
                        let count = data
                            .groups
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(&group))
                            .map(|g| g.projects.len())
                            .unwrap_or(0);
                        let message = if count == 0 {
                            format!("确认删除分组 `{group}`？ y/N")
                        } else {
                            format!("确认删除分组 `{group}` 及其中 {count} 个项目？ y/N")
                        };
                        self.mode = Mode::Confirm {
                            message,
                            kind: ConfirmKind::DeleteGroup { name: group, count },
                        };
                    }
                    _ => {}
                }
            }
            ActionKind::Project { group, project_id } => match selected {
                0 => self.open_launch_picker(data, config, &group, &project_id),
                1 => {
                    if let Some(project) = actions::find_project_ref(data, &group, &project_id) {
                        let (options, mut labels) = actions::launch_labels(project, config);
                        labels.push("不设默认".into());
                        self.mode = Mode::ListPicker {
                            title: "选择默认启动方式".into(),
                            items: labels,
                            selected: 0,
                            kind: ListKind::DefaultTool {
                                group,
                                project_id,
                                options,
                            },
                        };
                    }
                }
                2 => {
                    self.right_pane = RightPane::Commands { group, project_id };
                    self.right_sel = 0;
                    self.focus = Focus::Projects;
                }
                3 => {
                    if let Some(project) = actions::find_project_ref(data, &group, &project_id) {
                        self.open_form(
                            "编辑项目",
                            Self::project_edit_fields(project),
                            FormKind::EditProject {
                                group,
                                project_id,
                                old_name: project.name.clone(),
                            },
                        );
                    }
                }
                4 => {
                    if let Some(project) = actions::find_project_ref(data, &group, &project_id) {
                        self.mode = Mode::Confirm {
                            message: format!("确认删除项目 `{}`？ y/N", project.name),
                            kind: ConfirmKind::DeleteProject {
                                group,
                                project_id,
                                name: project.name.clone(),
                            },
                        };
                    }
                }
                5 => {
                    self.focus = Focus::Projects;
                    if let Some(gi) = data
                        .groups
                        .iter()
                        .position(|g| g.name.eq_ignore_ascii_case(&group))
                    {
                        self.left_sel = gi;
                        if let Some(pi) = data.groups[gi]
                            .projects
                            .iter()
                            .position(|p| p.id.eq_ignore_ascii_case(&project_id))
                        {
                            self.right_sel = pi;
                        }
                    }
                    self.on_move(data);
                }
                _ => {}
            },
            ActionKind::TrashItem { id } => match selected {
                0 => match actions::restore_trash(data, &id) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                },
                1 => {
                    let name = data
                        .trash
                        .iter()
                        .find(|t| t.id.eq_ignore_ascii_case(&id))
                        .map(|t| t.name.clone())
                        .unwrap_or_default();
                    self.mode = Mode::Confirm {
                        message: format!("确认彻底删除 `{name}`？此操作不可恢复。 y/N"),
                        kind: ConfirmKind::PurgeTrash { id, name },
                    };
                }
                _ => {}
            },
            ActionKind::ConfigTool { env, index } => match selected {
                0 => {
                    if let Some(tool) = env.tools(config).get(index) {
                        self.open_form(
                            "编辑工具",
                            vec![
                                Self::text_field("工具名称", tool.name.clone()),
                                Self::text_field("启动命令", tool.command.clone()),
                            ],
                            FormKind::EditTool { env, index },
                        );
                    }
                }
                1 => {
                    let name = env
                        .tools(config)
                        .get(index)
                        .map(|t| t.name.clone())
                        .unwrap_or_default();
                    self.mode = Mode::Confirm {
                        message: format!("确认删除工具 `{name}`？ y/N"),
                        kind: ConfirmKind::DeleteTool { env, index, name },
                    };
                }
                _ => {}
            },
            ActionKind::Command {
                group,
                project_id,
                index,
            } => match selected {
                0 => {
                    if let Some(project) = actions::find_project_ref(data, &group, &project_id)
                        && let Some(cmd) = project.commands.get(index)
                    {
                        self.mode = Mode::ListPicker {
                            title: "运行环境".into(),
                            items: vec!["WSL".into(), "PowerShell".into(), "IDE".into()],
                            selected: match cmd.env.as_str() {
                                "powershell" => 1,
                                "ide" => 2,
                                _ => 0,
                            },
                            kind: ListKind::CommandEnv {
                                group,
                                project_id,
                                edit_index: Some(index),
                                name: cmd.name.clone(),
                                current_env: cmd.env.clone(),
                            },
                        };
                    }
                }
                1 => {
                    if let Some(project) = actions::find_project_ref(data, &group, &project_id) {
                        let name = project
                            .commands
                            .get(index)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        self.mode = Mode::Confirm {
                            message: format!("确认删除命令 `{name}`？ y/N"),
                            kind: ConfirmKind::DeleteCommand {
                                group,
                                project_id,
                                index,
                                name,
                            },
                        };
                    }
                }
                _ => {}
            },
        }
        Outcome::Continue
    }

    pub(crate) fn handle_list_picker(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::ListPicker {
            title,
            items,
            selected,
            kind,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Esc => self.back_to_browse(),
            KeyCode::Char('j') | KeyCode::Down => {
                let n = items.len();
                if n > 0 {
                    self.mode = Mode::ListPicker {
                        title,
                        items,
                        selected: (selected + 1) % n,
                        kind,
                    };
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = items.len();
                if n > 0 {
                    self.mode = Mode::ListPicker {
                        title,
                        items,
                        selected: (selected + n - 1) % n,
                        kind,
                    };
                }
            }
            KeyCode::Enter => return self.apply_list_pick(kind, selected, data, config),
            _ => {}
        }
        Outcome::Continue
    }

    pub(crate) fn apply_list_pick(
        &mut self,
        kind: ListKind,
        selected: usize,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match kind {
            ListKind::CommandEnv {
                group,
                project_id,
                edit_index,
                name,
                ..
            } => {
                let env = match selected {
                    1 => "powershell",
                    2 => "ide",
                    _ => "wsl",
                }
                .to_string();
                if let Some(index) = edit_index {
                    let cmd = actions::find_project_ref(data, &group, &project_id)
                        .and_then(|p| p.commands.get(index).cloned());
                    if let Some(cmd) = cmd {
                        self.open_form(
                            "编辑命令",
                            vec![
                                Self::text_field("命令名称", name),
                                Self::text_field("启动命令", cmd.command),
                            ],
                            FormKind::EditCommand {
                                group,
                                project_id,
                                index,
                                env,
                            },
                        );
                    } else {
                        self.back_to_browse();
                    }
                } else {
                    self.open_form(
                        "新增命令",
                        vec![
                            Self::text_field("命令名称", name),
                            Self::text_field("启动命令", ""),
                        ],
                        FormKind::AddCommand {
                            group,
                            project_id,
                            env,
                        },
                    );
                }
            }
            ListKind::MoveTarget {
                group,
                project_id,
                targets,
            } => {
                self.back_to_browse();
                if let Some(dest) = targets.get(selected) {
                    match actions::move_project(data, &project_id, &group, dest) {
                        Ok(msg) => self.flash(msg),
                        Err(e) => self.flash(e),
                    }
                    self.clamp_selection(data, config);
                }
            }
            ListKind::DefaultTool {
                group,
                project_id,
                options,
            } => {
                self.back_to_browse();
                let tool = if selected < options.len() {
                    Some(options[selected].tool_name.as_str())
                } else {
                    None
                };
                match actions::set_default_tool(data, &project_id, &group, tool) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
        }
        Outcome::Continue
    }
}
