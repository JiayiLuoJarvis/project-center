use crate::domain::models::Connection;
use crate::tui::actions::{ProjectInput, SecretInput};

use super::*;

impl App {
    pub(crate) fn text_field(label: &str, value: impl Into<String>) -> FormField {
        FormField::Text {
            label: label.into(),
            value: value.into(),
        }
    }

    pub(crate) fn password_field_with_hint(label: &str, empty_hint: &str) -> FormField {
        FormField::Password {
            label: label.into(),
            value: String::new(),
            empty_hint: empty_hint.into(),
        }
    }

    pub(crate) fn button_field(label: &str, action: ButtonAction) -> FormField {
        FormField::Button {
            label: label.into(),
            action,
        }
    }

    pub(crate) fn select_field(
        label: &str,
        display: impl Into<String>,
        value: impl Into<String>,
    ) -> FormField {
        FormField::Select {
            label: label.into(),
            display: display.into(),
            value: value.into(),
        }
    }

    pub(crate) fn open_form(
        &mut self,
        title: impl Into<String>,
        fields: Vec<FormField>,
        kind: FormKind,
    ) {
        self.clear_key = false;
        let cursor = Self::field_end_cursor(&fields, 0);
        self.mode = Mode::Form {
            title: title.into(),
            fields,
            focus: 0,
            cursor,
            kind,
            error: None,
        };
    }

    /// 文本/密码字段的字符长度；非编辑字段为 0。
    pub(crate) fn field_char_len(fields: &[FormField], index: usize) -> usize {
        match fields.get(index) {
            Some(FormField::Text { value, .. } | FormField::Password { value, .. }) => {
                value.chars().count()
            }
            _ => 0,
        }
    }

    pub(crate) fn field_end_cursor(fields: &[FormField], index: usize) -> usize {
        Self::field_char_len(fields, index)
    }

    fn clamp_cursor(fields: &[FormField], index: usize, cursor: usize) -> usize {
        cursor.min(Self::field_char_len(fields, index))
    }

    fn insert_char_at(value: &mut String, cursor: usize, c: char) -> usize {
        let mut chars: Vec<char> = value.chars().collect();
        let at = cursor.min(chars.len());
        chars.insert(at, c);
        *value = chars.into_iter().collect();
        at + 1
    }

    fn delete_char_before(value: &mut String, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let mut chars: Vec<char> = value.chars().collect();
        let at = cursor.min(chars.len());
        chars.remove(at - 1);
        *value = chars.into_iter().collect();
        at - 1
    }

    fn delete_char_at(value: &mut String, cursor: usize) -> usize {
        let mut chars: Vec<char> = value.chars().collect();
        if cursor >= chars.len() {
            return cursor.min(chars.len());
        }
        chars.remove(cursor);
        *value = chars.into_iter().collect();
        cursor
    }

    /// 新增/编辑同一布局：本地路径 + 远程连接 + 远程路径。
    pub(crate) fn project_fields(data: &ProjectData, p: Option<&Project>) -> Vec<FormField> {
        let (name, alias, path, wsl, cid, remote) = match p {
            Some(p) if p.is_ssh_project() => (
                p.name.as_str(),
                p.alias.as_str(),
                "",
                "",
                p.connection_id.trim(),
                p.path.as_str(),
            ),
            Some(p) => (
                p.name.as_str(),
                p.alias.as_str(),
                p.path.as_str(),
                p.wsl_path.as_str(),
                "",
                "",
            ),
            None => ("", "", "", "", "", ""),
        };
        let (display, value) = if cid.is_empty() {
            ("（未选择）".to_string(), String::new())
        } else if let Some(conn) = data.connection(cid) {
            (format!("{}  {}", conn.name, conn.label()), conn.id.clone())
        } else {
            ("（连接缺失，请重选）".to_string(), cid.to_string())
        };
        vec![
            Self::text_field("项目名", name),
            Self::text_field("别名", alias),
            Self::text_field("Windows 路径", path),
            Self::button_field(
                "浏览文件夹…",
                ButtonAction::PickFolder {
                    target: ProjectField::WinPath as usize,
                },
            ),
            Self::text_field("WSL 路径", wsl),
            Self::select_field("远程连接", display, value),
            Self::text_field("远程路径", remote),
        ]
    }

    pub(crate) fn connection_fields(conn: Option<&Connection>) -> Vec<FormField> {
        let (name, user, host, port, key_path) = match conn {
            Some(c) => (
                c.name.as_str(),
                c.user.as_str(),
                c.host.as_str(),
                c.port.to_string(),
                c.ssh_key_path.as_str(),
            ),
            None => ("", "", "", "22".into(), ""),
        };
        let (pw_hint, kp_hint) = match conn {
            Some(c) if !c.ssh_password_enc.trim().is_empty() => (
                "（已保存，留空不改）",
                password_hint(c.ssh_key_pass_enc.trim().is_empty()),
            ),
            Some(c) => (
                "（未设置）",
                password_hint(c.ssh_key_pass_enc.trim().is_empty()),
            ),
            None => ("（未设置）", "（未设置）"),
        };
        vec![
            Self::text_field("名称", name),
            Self::text_field("登录用户（可选，留空用当前用户）", user),
            Self::text_field("主机", host),
            Self::text_field("端口（默认 22）", port.as_str()),
            Self::text_field("私钥来源路径（可选）", key_path),
            Self::button_field(
                "浏览私钥文件…",
                ButtonAction::PickFile {
                    target: ConnField::KeySource as usize,
                },
            ),
            Self::button_field(
                "清除已导入密钥",
                ButtonAction::ClearKey {
                    target: ConnField::KeySource as usize,
                },
            ),
            Self::password_field_with_hint("登录密码", pw_hint),
            Self::password_field_with_hint("私钥口令", kp_hint),
        ]
    }

    pub(crate) fn field_value(fields: &[FormField], index: usize) -> String {
        match fields.get(index) {
            Some(
                FormField::Text { value, .. }
                | FormField::Password { value, .. }
                | FormField::Select { value, .. },
            ) => value.clone(),
            _ => String::new(),
        }
    }

    fn set_select(fields: &mut [FormField], index: usize, value: String, display: String) {
        if let Some(FormField::Select {
            value: slot,
            display: shown,
            ..
        }) = fields.get_mut(index)
        {
            *slot = value;
            *shown = display;
        }
    }

    pub(crate) fn set_form_error(&mut self, error: impl Into<String>) {
        if let Mode::Form { error: slot, .. } = &mut self.mode {
            *slot = Some(error.into());
        }
    }

    pub(crate) fn open_connection_picker(&mut self, data: &ProjectData, suspended: SuspendedForm) {
        let ids: Vec<String> = data.connections.iter().map(|c| c.id.clone()).collect();
        let mut items: Vec<String> = data
            .connections
            .iter()
            .map(|c| format!("{}  {}", c.name, c.label()))
            .collect();
        items.push(NEW_CONNECTION_LABEL.into());
        self.mode = Mode::ListPicker {
            title: "选择远程连接".into(),
            items,
            selected: 0,
            kind: ListKind::PickConnection {
                ids,
                suspended: Box::new(suspended),
            },
        };
    }

    pub(crate) fn resume_form(
        &mut self,
        mut suspended: SuspendedForm,
        picked: Option<&str>,
        data: &ProjectData,
    ) {
        if let Some(id) = picked {
            let display = data
                .connection(id)
                .map(|c| format!("{}  {}", c.name, c.label()))
                .unwrap_or_else(|| id.to_string());
            Self::set_select(
                &mut suspended.fields,
                ProjectField::Connection as usize,
                id.to_string(),
                display,
            );
        }
        let cursor = Self::clamp_cursor(&suspended.fields, suspended.focus, suspended.cursor);
        self.mode = Mode::Form {
            title: suspended.title,
            fields: suspended.fields,
            focus: suspended.focus,
            cursor,
            kind: suspended.kind,
            error: None,
        };
    }

    fn set_form_state(
        &mut self,
        title: String,
        fields: Vec<FormField>,
        focus: usize,
        kind: FormKind,
        cursor: usize,
        error: Option<String>,
    ) {
        let focus = if fields.is_empty() {
            0
        } else {
            focus.min(fields.len() - 1)
        };
        let cursor = Self::clamp_cursor(&fields, focus, cursor);
        self.mode = Mode::Form {
            title,
            fields,
            focus,
            cursor,
            kind,
            error,
        };
    }

    pub(crate) fn handle_form(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::Form {
            title,
            mut fields,
            mut focus,
            mut cursor,
            kind,
            ..
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        if fields.is_empty() {
            self.back_to_browse();
            return Outcome::Continue;
        }
        focus = focus.min(fields.len() - 1);
        cursor = Self::clamp_cursor(&fields, focus, cursor);

        match key.code {
            KeyCode::Esc => {
                if let FormKind::AddConnection {
                    resume: Some(suspended),
                } = kind
                {
                    self.open_connection_picker(data, *suspended);
                } else {
                    self.back_to_browse();
                }
                return Outcome::Continue;
            }
            KeyCode::Tab | KeyCode::Down => {
                focus = (focus + 1) % fields.len();
                cursor = Self::field_end_cursor(&fields, focus);
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::BackTab | KeyCode::Up => {
                focus = (focus + fields.len() - 1) % fields.len();
                cursor = Self::field_end_cursor(&fields, focus);
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Left => {
                cursor = cursor.saturating_sub(1);
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Right => {
                cursor = (cursor + 1).min(Self::field_char_len(&fields, focus));
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Home => {
                cursor = 0;
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::End => {
                cursor = Self::field_end_cursor(&fields, focus);
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Backspace => {
                match fields.get_mut(focus) {
                    Some(FormField::Text { value, .. } | FormField::Password { value, .. }) => {
                        cursor = Self::delete_char_before(value, cursor);
                    }
                    Some(FormField::Select { value, display, .. }) => {
                        value.clear();
                        *display = "（未选择）".into();
                        cursor = 0;
                    }
                    _ => {}
                }
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Delete => {
                if let Some(FormField::Text { value, .. } | FormField::Password { value, .. }) =
                    fields.get_mut(focus)
                {
                    cursor = Self::delete_char_at(value, cursor);
                }
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Char('u') | KeyCode::Char('U')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                match fields.get_mut(focus) {
                    Some(FormField::Text { value, .. } | FormField::Password { value, .. }) => {
                        value.clear();
                        cursor = 0;
                    }
                    Some(FormField::Select { value, display, .. }) => {
                        value.clear();
                        *display = "（未选择）".into();
                        cursor = 0;
                    }
                    _ => {}
                }
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(FormField::Text { value, .. } | FormField::Password { value, .. }) =
                    fields.get_mut(focus)
                {
                    cursor = Self::insert_char_at(value, cursor, c);
                }
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
            KeyCode::Enter => match fields.get(focus) {
                Some(FormField::Select { .. }) => {
                    let suspended = SuspendedForm {
                        title,
                        fields,
                        focus,
                        cursor,
                        kind,
                    };
                    self.open_connection_picker(data, suspended);
                    return Outcome::Continue;
                }
                Some(FormField::Button { action, .. }) => {
                    let action = *action;
                    match action {
                        ButtonAction::ClearKey { target } => {
                            if let Some(FormField::Text { value, .. }) = fields.get_mut(target) {
                                value.clear();
                            }
                            self.clear_key = true;
                            cursor = Self::field_end_cursor(&fields, target);
                            self.set_form_state(title, fields, target, kind, cursor, None);
                        }
                        ButtonAction::PickFolder { target } => {
                            self.set_form_state(title, fields, focus, kind, cursor, None);
                            return Outcome::PickFolder { target };
                        }
                        ButtonAction::PickFile { target } => {
                            self.set_form_state(title, fields, focus, kind, cursor, None);
                            return Outcome::PickFile { target };
                        }
                    }
                }
                _ => {
                    self.set_form_state(
                        title,
                        fields.clone(),
                        focus,
                        kind.clone(),
                        cursor,
                        None,
                    );
                    return self.submit_form(kind, &fields, data, config);
                }
            },
            _ => {
                self.set_form_state(title, fields, focus, kind, cursor, None);
            }
        }
        Outcome::Continue
    }

    pub(crate) fn submit_form(
        &mut self,
        kind: FormKind,
        fields: &[FormField],
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match kind {
            FormKind::AddGroup => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                let result = actions::add_group(data, &name, &alias);
                if result.is_ok() {
                    self.left_sel = data.groups.len().saturating_sub(1);
                    self.right_pane = RightPane::Projects;
                    self.sync_right_pane(data);
                }
                self.finish_submit(result);
            }
            FormKind::RenameGroup { old } => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                self.finish_submit(actions::rename_group(data, &old, &name, &alias));
            }
            FormKind::AddProject { group } => {
                let input = project_input(fields);
                self.finish_submit(actions::save_project(data, &group, None, input));
            }
            FormKind::EditProject {
                group, project_id, ..
            } => {
                let input = project_input(fields);
                self.finish_submit(actions::save_project(
                    data,
                    &group,
                    Some(&project_id),
                    input,
                ));
            }
            FormKind::AddCommand {
                group,
                project_id,
                env,
            } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                self.finish_submit(actions::add_command(
                    data,
                    &project_id,
                    &group,
                    &name,
                    &env,
                    &command,
                ));
            }
            FormKind::EditCommand {
                group,
                project_id,
                index,
                env,
            } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                self.finish_submit(actions::edit_command(
                    data,
                    &project_id,
                    &group,
                    index,
                    &name,
                    &env,
                    &command,
                ));
            }
            FormKind::AddTool { env } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                self.finish_submit(actions::add_config_tool(config, env, &name, &command));
            }
            FormKind::EditTool { env, index } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                self.finish_submit(actions::edit_config_tool(
                    config, env, index, &name, &command,
                ));
            }
            FormKind::AddConnection { resume } => {
                let draft = connection_draft(fields);
                let secrets = secret_input(fields);
                match actions::add_connection(data, draft, secrets) {
                    Ok(id) => match resume {
                        Some(suspended) => {
                            self.resume_form(*suspended, Some(&id), data);
                        }
                        None => {
                            self.back_to_browse();
                            self.flash("连接已添加");
                        }
                    },
                    Err(e) => self.set_form_error(e),
                }
            }
            FormKind::EditConnection { id } => {
                let draft = connection_draft(fields);
                let secrets = secret_input(fields);
                self.finish_submit(actions::edit_connection(
                    data,
                    &id,
                    draft,
                    secrets,
                    self.clear_key,
                ));
            }
        }
        Outcome::Continue
    }

    fn finish_submit(&mut self, result: Result<String, crate::tui::actions::Error>) {
        match result {
            Ok(msg) => {
                self.back_to_browse();
                self.flash(msg);
            }
            Err(e) => self.set_form_error(e),
        }
    }

    pub fn resume_after_folder_pick(&mut self, path: Option<String>, target: usize) {
        let Mode::Form {
            title,
            mut fields,
            focus,
            cursor,
            kind,
            ..
        } = self.mode.clone()
        else {
            return;
        };
        match path {
            Some(path) => {
                if let Some(FormField::Text { value, .. }) = fields.get_mut(target) {
                    *value = path.clone();
                }
                if matches!(
                    kind,
                    FormKind::AddProject { .. } | FormKind::EditProject { .. }
                ) {
                    let wsl_empty = matches!(
                        fields.get(ProjectField::WslPath as usize),
                        Some(FormField::Text { value, .. }) if value.trim().is_empty()
                    );
                    if wsl_empty {
                        let linux = crate::domain::models::win_path_to_linux(&path);
                        if let Some(FormField::Text { value, .. }) =
                            fields.get_mut(ProjectField::WslPath as usize)
                        {
                            *value = linux;
                        }
                    }
                }
                let cursor = Self::field_end_cursor(&fields, target);
                self.set_form_state(title, fields, target, kind, cursor, None);
            }
            None => {
                self.set_form_state(
                    title,
                    fields,
                    focus,
                    kind,
                    cursor,
                    Some("已取消选择文件夹".into()),
                );
            }
        }
    }

    pub fn resume_after_file_pick(&mut self, path: Option<String>, target: usize) {
        let Mode::Form {
            title,
            mut fields,
            focus,
            cursor,
            kind,
            ..
        } = self.mode.clone()
        else {
            return;
        };
        match path {
            Some(path) => {
                if let Some(FormField::Text { value, .. }) = fields.get_mut(target) {
                    *value = path;
                }
                self.clear_key = false;
                let cursor = Self::field_end_cursor(&fields, target);
                self.set_form_state(title, fields, target, kind, cursor, None);
            }
            None => {
                self.set_form_state(
                    title,
                    fields,
                    focus,
                    kind,
                    cursor,
                    Some("已取消选择文件".into()),
                );
            }
        }
    }
}

fn password_hint(empty: bool) -> &'static str {
    if empty {
        "（未设置）"
    } else {
        "（已保存，留空不改）"
    }
}

fn project_input(fields: &[FormField]) -> ProjectInput {
    ProjectInput {
        name: App::field_value(fields, ProjectField::Name as usize),
        alias: App::field_value(fields, ProjectField::Alias as usize),
        win_path: App::field_value(fields, ProjectField::WinPath as usize),
        wsl_path: App::field_value(fields, ProjectField::WslPath as usize),
        connection_id: App::field_value(fields, ProjectField::Connection as usize),
        remote_path: App::field_value(fields, ProjectField::RemotePath as usize),
    }
}

fn connection_draft(fields: &[FormField]) -> crate::domain::ConnectionDraft {
    let port = App::field_value(fields, ConnField::Port as usize);
    let port = port.trim().parse::<u16>().unwrap_or(22);
    crate::domain::ConnectionDraft {
        name: App::field_value(fields, ConnField::Name as usize),
        user: App::field_value(fields, ConnField::User as usize),
        host: App::field_value(fields, ConnField::Host as usize),
        port,
    }
}

fn secret_input(fields: &[FormField]) -> SecretInput {
    SecretInput {
        key_source: App::field_value(fields, ConnField::KeySource as usize),
        password: App::field_value(fields, ConnField::Password as usize),
        key_pass: App::field_value(fields, ConnField::KeyPass as usize),
    }
}
