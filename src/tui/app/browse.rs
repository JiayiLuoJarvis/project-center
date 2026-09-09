use super::*;

impl App {
    pub(crate) fn handle_browse(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.quit_confirm = true;
                return Outcome::Continue;
            }
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                return Outcome::Continue;
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Filter;
                return Outcome::Continue;
            }
            KeyCode::Char('v') => {
                self.open_secret_viewer(data, config);
                return Outcome::Continue;
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.clamp_selection(data, config);
                } else {
                    self.pop_right_pane(data, config);
                }
                return Outcome::Continue;
            }
            KeyCode::Tab | KeyCode::Char('h') | KeyCode::Left => {
                self.focus = if self.focus == Focus::Groups {
                    Focus::Projects
                } else {
                    Focus::Groups
                };
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.focus = if self.focus == Focus::Groups {
                    Focus::Projects
                } else {
                    Focus::Groups
                };
                return Outcome::Continue;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_sel(1, data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_sel(-1, data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('g') => {
                if self.focus == Focus::Groups {
                    self.left_sel = 0;
                } else {
                    self.right_sel = 0;
                }
                return Outcome::Continue;
            }
            KeyCode::Char('G') => {
                if self.focus == Focus::Groups {
                    self.left_sel = self.left_count(data).saturating_sub(1);
                } else {
                    self.right_sel = self.right_len(data, config).saturating_sub(1);
                }
                return Outcome::Continue;
            }
            KeyCode::Enter => return self.on_enter(data, config),
            KeyCode::Char('o') => {
                self.open_action_menu(data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('a') => {
                self.on_add(data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('e') => {
                self.on_edit(data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('d') => {
                self.on_delete(data, config);
                return Outcome::Continue;
            }
            KeyCode::Char('m') => {
                self.on_move(data);
                return Outcome::Continue;
            }
            KeyCode::Char('r') if matches!(self.right_pane, RightPane::Trash) => {
                self.restore_selected_trash(data);
                return Outcome::Continue;
            }
            KeyCode::Char('D') if matches!(self.right_pane, RightPane::Trash) => {
                self.mode = Mode::Confirm {
                    message: "确认清空回收站？ y/N".into(),
                    kind: ConfirmKind::EmptyTrash,
                };
                return Outcome::Continue;
            }
            _ => {}
        }
        Outcome::Continue
    }

    pub(crate) fn move_sel(&mut self, delta: i32, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            let n = self.left_count(data) as i32;
            if n == 0 {
                return;
            }
            let next = (self.left_sel as i32 + delta).rem_euclid(n) as usize;
            self.left_sel = next;
            self.right_sel = 0;
            if matches!(
                self.right_pane,
                RightPane::Commands { .. } | RightPane::ConfigTools { .. }
            ) {
                self.right_pane = RightPane::Projects;
            }
            self.sync_right_pane(data);
        } else {
            let n = self.right_len(data, config) as i32;
            if n == 0 {
                return;
            }
            self.right_sel = (self.right_sel as i32 + delta).rem_euclid(n) as usize;
        }
    }

    pub(crate) fn on_enter(&mut self, data: &mut ProjectData, config: &mut AppConfig) -> Outcome {
        if self.focus == Focus::Groups {
            self.sync_right_pane(data);
            self.focus = Focus::Projects;
            self.right_sel = 0;
            return Outcome::Continue;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let indices = self.filtered_project_indices(data, gi);
                    if let Some(&pi) = indices.get(self.right_sel) {
                        let group = data.groups[gi].name.clone();
                        let id = data.groups[gi].projects[pi].id.clone();
                        self.open_launch_picker(data, config, &group, &id);
                    }
                }
            }
            RightPane::Trash => {
                self.open_action_menu(data, config);
            }
            RightPane::ConfigEnvs => match self.right_sel {
                0 => {
                    self.right_pane = RightPane::ConfigTools {
                        env: ConfigEnv::Wsl,
                    };
                    self.right_sel = 0;
                }
                1 => {
                    self.right_pane = RightPane::ConfigTools {
                        env: ConfigEnv::PowerShell,
                    };
                    self.right_sel = 0;
                }
                2 => {
                    self.right_pane = RightPane::ConfigTools {
                        env: ConfigEnv::Ide,
                    };
                    self.right_sel = 0;
                }
                3 => {
                    self.mode = Mode::Confirm {
                        message: "确认恢复默认配置？ y/N".into(),
                        kind: ConfirmKind::ResetConfig,
                    };
                }
                _ => {}
            },
            RightPane::ConfigTools { .. } => self.open_action_menu(data, config),
            RightPane::Commands { .. } => self.open_action_menu(data, config),
        }
        Outcome::Continue
    }

    pub(crate) fn open_action_menu(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = self.left_is_group(data, self.left_sel) {
                let name = data.groups[gi].name.clone();
                let count = data.groups[gi].projects.len();
                self.mode = Mode::ActionMenu {
                    kind: ActionKind::Project {
                        group: name.clone(),
                        project_id: String::new(),
                    },
                    items: vec![
                        "重命名分组".into(),
                        format!("删除分组 ({count} 个项目)"),
                        "返回".into(),
                    ],
                    selected: 0,
                };
                // Reuse ActionKind with empty project_id for group menu via special handling
                let _ = name;
                return;
            }
            self.flash("请选择分组或右栏项目");
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let indices = self.filtered_project_indices(data, gi);
                    let Some(&pi) = indices.get(self.right_sel) else {
                        self.flash("无选中项目");
                        return;
                    };
                    let group = data.groups[gi].name.clone();
                    let project_id = data.groups[gi].projects[pi].id.clone();
                    self.mode = Mode::ActionMenu {
                        kind: ActionKind::Project { group, project_id },
                        items: vec![
                            "打开".into(),
                            "默认启动方式".into(),
                            "自定义命令".into(),
                            "编辑".into(),
                            "删除".into(),
                            "移动到其他分组".into(),
                            "返回".into(),
                        ],
                        selected: 0,
                    };
                }
            }
            RightPane::Trash => {
                let indices = self.filtered_trash_indices(data);
                let Some(&ti) = indices.get(self.right_sel) else {
                    self.flash("回收站为空");
                    return;
                };
                let id = data.trash[ti].id.clone();
                self.mode = Mode::ActionMenu {
                    kind: ActionKind::TrashItem { id },
                    items: vec!["恢复".into(), "彻底删除".into(), "返回".into()],
                    selected: 0,
                };
            }
            RightPane::ConfigTools { env } => {
                let indices = self.filtered_tool_indices(config, env);
                let Some(&ti) = indices.get(self.right_sel) else {
                    self.flash("无工具");
                    return;
                };
                self.mode = Mode::ActionMenu {
                    kind: ActionKind::ConfigTool { env, index: ti },
                    items: vec!["编辑".into(), "删除".into(), "返回".into()],
                    selected: 0,
                };
            }
            RightPane::Commands { group, project_id } => {
                let Some(project) = actions::find_project_ref(data, &group, &project_id) else {
                    return;
                };
                let indices = self.filtered_command_indices(project);
                let Some(&ci) = indices.get(self.right_sel) else {
                    self.flash("无命令");
                    return;
                };
                self.mode = Mode::ActionMenu {
                    kind: ActionKind::Command {
                        group,
                        project_id,
                        index: ci,
                    },
                    items: vec!["编辑".into(), "删除".into(), "返回".into()],
                    selected: 0,
                };
            }
            RightPane::ConfigEnvs => {}
        }
    }

    pub(crate) fn on_add(&mut self, data: &ProjectData, _config: &AppConfig) {
        if self.focus == Focus::Groups {
            self.open_form(
                "新增分组",
                vec![Self::text_field("分组名", ""), Self::text_field("别名", "")],
                FormKind::AddGroup,
            );
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let group = data.groups[gi].name.clone();
                    self.open_form(
                        "新增项目",
                        Self::project_add_fields(),
                        FormKind::AddProject { group },
                    );
                }
            }
            RightPane::ConfigTools { env } => {
                self.open_form(
                    "新增工具",
                    vec![
                        Self::text_field("工具名称", ""),
                        Self::text_field("启动命令（不含参数）", ""),
                    ],
                    FormKind::AddTool { env },
                );
            }
            RightPane::Commands { group, project_id } => {
                self.mode = Mode::ListPicker {
                    title: "运行环境".into(),
                    items: vec!["WSL".into(), "PowerShell".into(), "IDE".into()],
                    selected: 0,
                    kind: ListKind::CommandEnv {
                        group,
                        project_id,
                        edit_index: None,
                        name: String::new(),
                        current_env: String::new(),
                    },
                };
            }
            RightPane::Trash => self.flash("回收站不支持新增"),
            RightPane::ConfigEnvs => self.flash("请先进入环境工具列表"),
        }
    }

    pub(crate) fn on_edit(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = self.left_is_group(data, self.left_sel) {
                let old = data.groups[gi].name.clone();
                let alias = data.groups[gi].alias.clone();
                self.open_form(
                    "编辑分组",
                    vec![
                        Self::text_field("分组名", old.clone()),
                        Self::text_field("别名", alias),
                    ],
                    FormKind::RenameGroup { old },
                );
            }
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let indices = self.filtered_project_indices(data, gi);
                    let Some(&pi) = indices.get(self.right_sel) else {
                        return;
                    };
                    let p = &data.groups[gi].projects[pi];
                    self.open_form(
                        "编辑项目",
                        Self::project_edit_fields(p),
                        FormKind::EditProject {
                            group: data.groups[gi].name.clone(),
                            project_id: p.id.clone(),
                            old_name: p.name.clone(),
                        },
                    );
                }
            }
            RightPane::ConfigTools { env } => {
                let indices = self.filtered_tool_indices(config, env);
                let Some(&ti) = indices.get(self.right_sel) else {
                    return;
                };
                let tool = &env.tools(config)[ti];
                self.open_form(
                    "编辑工具",
                    vec![
                        Self::text_field("工具名称", tool.name.clone()),
                        Self::text_field("启动命令", tool.command.clone()),
                    ],
                    FormKind::EditTool { env, index: ti },
                );
            }
            RightPane::Commands { group, project_id } => {
                let Some(project) = actions::find_project_ref(data, &group, &project_id) else {
                    return;
                };
                let indices = self.filtered_command_indices(project);
                let Some(&ci) = indices.get(self.right_sel) else {
                    return;
                };
                let cmd = &project.commands[ci];
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
                        edit_index: Some(ci),
                        name: cmd.name.clone(),
                        current_env: cmd.env.clone(),
                    },
                };
            }
            _ => self.flash("当前上下文不支持编辑"),
        }
    }

    pub(crate) fn on_delete(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = self.left_is_group(data, self.left_sel) {
                let name = data.groups[gi].name.clone();
                let count = data.groups[gi].projects.len();
                let message = if count == 0 {
                    format!("确认删除分组 `{name}`？ y/N")
                } else {
                    format!("确认删除分组 `{name}` 及其中 {count} 个项目？ y/N")
                };
                self.mode = Mode::Confirm {
                    message,
                    kind: ConfirmKind::DeleteGroup { name, count },
                };
            }
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let indices = self.filtered_project_indices(data, gi);
                    let Some(&pi) = indices.get(self.right_sel) else {
                        return;
                    };
                    let p = &data.groups[gi].projects[pi];
                    self.mode = Mode::Confirm {
                        message: format!("确认删除项目 `{}`？ y/N", p.name),
                        kind: ConfirmKind::DeleteProject {
                            group: data.groups[gi].name.clone(),
                            project_id: p.id.clone(),
                            name: p.name.clone(),
                        },
                    };
                }
            }
            RightPane::Trash => {
                let indices = self.filtered_trash_indices(data);
                let Some(&ti) = indices.get(self.right_sel) else {
                    return;
                };
                let item = &data.trash[ti];
                self.mode = Mode::Confirm {
                    message: format!("确认彻底删除 `{}`？此操作不可恢复。 y/N", item.name),
                    kind: ConfirmKind::PurgeTrash {
                        id: item.id.clone(),
                        name: item.name.clone(),
                    },
                };
            }
            RightPane::ConfigTools { env } => {
                let indices = self.filtered_tool_indices(config, env);
                let Some(&ti) = indices.get(self.right_sel) else {
                    return;
                };
                let name = env.tools(config)[ti].name.clone();
                self.mode = Mode::Confirm {
                    message: format!("确认删除工具 `{name}`？ y/N"),
                    kind: ConfirmKind::DeleteTool {
                        env,
                        index: ti,
                        name,
                    },
                };
            }
            RightPane::Commands { group, project_id } => {
                let Some(project) = actions::find_project_ref(data, &group, &project_id) else {
                    return;
                };
                let indices = self.filtered_command_indices(project);
                let Some(&ci) = indices.get(self.right_sel) else {
                    return;
                };
                let name = project.commands[ci].name.clone();
                self.mode = Mode::Confirm {
                    message: format!("确认删除命令 `{name}`？ y/N"),
                    kind: ConfirmKind::DeleteCommand {
                        group,
                        project_id,
                        index: ci,
                        name,
                    },
                };
            }
            _ => {}
        }
    }

    pub(crate) fn on_move(&mut self, data: &ProjectData) {
        if !matches!(self.right_pane, RightPane::Projects) || self.focus != Focus::Projects {
            self.flash("仅可在项目列表中移动");
            return;
        }
        let Some(gi) = self.left_is_group(data, self.left_sel) else {
            return;
        };
        let indices = self.filtered_project_indices(data, gi);
        let Some(&pi) = indices.get(self.right_sel) else {
            return;
        };
        let group = data.groups[gi].name.clone();
        let project_id = data.groups[gi].projects[pi].id.clone();
        let targets: Vec<String> = data
            .groups
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != gi)
            .map(|(_, g)| g.name.clone())
            .collect();
        if targets.is_empty() {
            self.flash("暂无其他分组可移动");
            return;
        }
        self.mode = Mode::ListPicker {
            title: "移动到哪个分组".into(),
            items: targets.clone(),
            selected: 0,
            kind: ListKind::MoveTarget {
                group,
                project_id,
                targets,
            },
        };
    }

    pub(crate) fn restore_selected_trash(&mut self, data: &mut ProjectData) {
        let indices = self.filtered_trash_indices(data);
        let Some(&ti) = indices.get(self.right_sel) else {
            self.flash("回收站为空");
            return;
        };
        let id = data.trash[ti].id.clone();
        match actions::restore_trash(data, &id) {
            Ok(msg) => {
                self.flash(msg);
                self.clamp_selection(data, &AppConfig::defaults());
            }
            Err(e) => self.flash(e),
        }
    }
}
