use super::*;

impl App {
    pub(crate) fn text_field(label: &str, value: impl Into<String>) -> FormField {
        FormField::Text {
            label: label.into(),
            value: value.into(),
        }
    }

    pub(crate) fn password_field(label: &str) -> FormField {
        Self::password_field_with_hint(label, "（未设置）")
    }

    pub(crate) fn password_field_with_hint(label: &str, empty_hint: &str) -> FormField {
        FormField::Password {
            label: label.into(),
            value: String::new(),
            empty_hint: empty_hint.into(),
        }
    }

    pub(crate) fn button_field(label: &str) -> FormField {
        FormField::Button {
            label: label.into(),
        }
    }

    pub(crate) fn open_form(
        &mut self,
        title: impl Into<String>,
        fields: Vec<FormField>,
        kind: FormKind,
    ) {
        self.mode = Mode::Form {
            title: title.into(),
            fields,
            focus: 0,
            kind,
            error: None,
        };
    }

    /// 基础表单字段索引：0 项目名 / 1 别名 / 2 Windows 路径 / 3 浏览按钮 /
    /// 4 WSL 路径 / 5 登录用户 / 6 主机 / 7 端口 / 8 远程 Linux 路径。
    pub(crate) fn project_form_fields(
        name: &str,
        alias: &str,
        path: &str,
        wsl_path: &str,
    ) -> Vec<FormField> {
        vec![
            Self::text_field("项目名", name),
            Self::text_field("别名", alias),
            Self::text_field("Windows 路径", path),
            Self::button_field("浏览文件夹…"),
            Self::text_field("WSL 路径", wsl_path),
            Self::text_field("登录用户（SSH 项目，可选，留空用当前用户）", ""),
            Self::text_field("主机（SSH 项目，仅域名/IPv4，留空为普通项目）", ""),
            Self::text_field("端口（SSH 项目，默认 22）", "22"),
            Self::text_field("远程 Linux 路径", ""),
        ]
    }

    /// 新增表单：基础字段 + SSH 秘密字段（9 密钥来源 / 10 密码 / 11 口令，
    /// 仅主机非空时生效），新增即可直接保存秘密，无需二次编辑。
    pub(crate) fn project_add_fields() -> Vec<FormField> {
        let mut fields = Self::project_form_fields("", "", "", "");
        fields.push(Self::text_field("私钥来源路径（SSH 项目，可选）", ""));
        fields.push(Self::password_field("登录密码（SSH 项目，可选）"));
        fields.push(Self::password_field("私钥口令（SSH 项目，可选）"));
        fields
    }

    /// 编辑表单：SSH 项目用 SSH 专用字段；普通项目附 SSH 转换字段。
    /// SSH 表单字段索引：0 登录用户 / 1 主机 / 2 端口 / 3 远程路径 /
    /// 4 密钥来源 / 5 密码 / 6 口令。
    pub(crate) fn project_edit_fields(p: &Project) -> Vec<FormField> {
        if p.is_ssh_project() {
            let (user, host, port) = Self::split_ssh_target(&p.ssh_target);
            // 密码框恒为空值（留空不改语义）：label 简短，空值占位用 empty_hint
            // 标明已存/未存，避免值行被画成「未设置」与「已保存」矛盾。
            let (pw_label, pw_hint) = if p.ssh_password_enc.trim().is_empty() {
                ("登录密码", "（未设置）")
            } else {
                ("登录密码", "（已保存，留空不改）")
            };
            let (kp_label, kp_hint) = if p.ssh_key_pass_enc.trim().is_empty() {
                ("私钥口令", "（未设置）")
            } else {
                ("私钥口令", "（已保存，留空不改）")
            };
            return vec![
                Self::text_field("登录用户（留空用当前用户）", &user),
                Self::text_field("主机（域名/IPv4）", &host),
                Self::text_field("端口（默认 22）", &port),
                Self::text_field("远程 Linux 路径", &p.path),
                Self::text_field("私钥来源路径（填路径导入替换，留空不变）", &p.ssh_key_path),
                Self::password_field_with_hint(pw_label, pw_hint),
                Self::password_field_with_hint(kp_label, kp_hint),
            ];
        }
        let mut fields = Self::project_form_fields(&p.name, &p.alias, &p.path, &p.wsl_path);
        fields[5] = Self::text_field("登录用户（填主机后生效，留空用当前用户）", "");
        fields[6] = Self::text_field("主机（填入即转为 SSH 项目）", "");
        fields[7] = Self::text_field("端口（填主机后生效，默认 22）", "22");
        fields
    }

    pub(crate) fn field_value(fields: &[FormField], index: usize) -> String {
        match fields.get(index) {
            Some(FormField::Text { value, .. } | FormField::Password { value, .. }) => {
                value.clone()
            }
            _ => String::new(),
        }
    }

    /// 拼接 SSH 目标：`[user@]host[:port]`。user/host 禁止含 `@`（防拼出
    /// `a@b@c` 这类坏目标）；port 非空时必须为纯数字，留空即 ssh 默认 22。
    pub(crate) fn compose_ssh_target(user: &str, host: &str, port: &str) -> Result<String, String> {
        let user = user.trim();
        let host = host.trim();
        let port = port.trim();
        if user.contains('@') {
            return Err("登录用户不要包含 @".into());
        }
        if host.contains('@') {
            return Err("主机不要包含 @，用户名请填在“登录用户”字段".into());
        }
        if !port.is_empty() && !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err("端口必须是纯数字".into());
        }
        let mut target = String::new();
        if !user.is_empty() {
            target.push_str(user);
            target.push('@');
        }
        target.push_str(host);
        if !port.is_empty() {
            target.push(':');
            target.push_str(port);
        }
        Ok(target)
    }

    /// 反拆已存 SSH 目标为（用户， 主机， 端口），供编辑表单预填；
    /// 缺失部分为空串（与 `compose_ssh_target` 互为不动点）。
    pub(crate) fn split_ssh_target(target: &str) -> (String, String, String) {
        let target = target.trim();
        let (user, rest) = match target.split_once('@') {
            Some((u, r)) => (u, r),
            None => ("", target),
        };
        let (host, port) = crate::launch::split_host_port(rest);
        (
            user.to_string(),
            host.to_string(),
            port.unwrap_or_default().to_string(),
        )
    }

    pub(crate) fn set_form_error(&mut self, error: impl Into<String>) {
        if let Mode::Form { error: slot, .. } = &mut self.mode {
            *slot = Some(error.into());
        }
    }

    /// 打开「查看保存的秘密」对话框（仅 SSH 项目且已存秘密、已设 PIN）。
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

        match key.code {
            KeyCode::Esc => {
                self.back_to_browse();
                return Outcome::Continue;
            }
            KeyCode::Tab | KeyCode::Down => {
                focus = (focus + 1) % fields.len();
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: None,
                };
            }
            KeyCode::BackTab | KeyCode::Up => {
                focus = (focus + fields.len() - 1) % fields.len();
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: None,
                };
            }
            KeyCode::Backspace => {
                if let Some(FormField::Text { value, .. } | FormField::Password { value, .. }) =
                    fields.get_mut(focus)
                {
                    value.pop();
                }
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: None,
                };
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(FormField::Text { value, .. } | FormField::Password { value, .. }) =
                    fields.get_mut(focus)
                {
                    value.push(c);
                }
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: None,
                };
            }
            KeyCode::Enter => match fields.get(focus) {
                Some(FormField::Button { .. }) => {
                    self.mode = Mode::Form {
                        title,
                        fields,
                        focus,
                        kind,
                        error: None,
                    };
                    return Outcome::PickFolder;
                }
                _ => {
                    self.mode = Mode::Form {
                        title,
                        fields: fields.clone(),
                        focus,
                        kind: kind.clone(),
                        error: None,
                    };
                    return self.submit_form(kind, &fields, data, config);
                }
            },
            _ => {
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: None,
                };
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
        let result = match kind {
            FormKind::AddGroup => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                actions::add_group(data, &name, &alias).inspect(|_| {
                    self.left_sel = data.groups.len().saturating_sub(1);
                    self.sync_right_pane(data);
                })
            }
            FormKind::RenameGroup { old } => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                actions::rename_group(data, &old, &name, &alias)
            }
            FormKind::AddProject { group } => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                // 基础表单索引：5 用户 / 6 主机 / 7 端口 / 8 远程路径 /
                // 9 密钥来源 / 10 密码 / 11 口令
                let ssh_host = Self::field_value(fields, 6);
                if !ssh_host.trim().is_empty() {
                    let ssh_user = Self::field_value(fields, 5);
                    let ssh_port = Self::field_value(fields, 7);
                    let remote = Self::field_value(fields, 8);
                    let key_source = Self::field_value(fields, 9);
                    let password = Self::field_value(fields, 10);
                    let key_pass = Self::field_value(fields, 11);
                    match Self::compose_ssh_target(&ssh_user, &ssh_host, &ssh_port) {
                        Ok(ssh_target) => actions::add_project_ssh(
                            data,
                            &group,
                            &name,
                            &alias,
                            &ssh_target,
                            &remote,
                            &key_source,
                            &password,
                            &key_pass,
                        ),
                        Err(e) => Err(e),
                    }
                } else {
                    let path = Self::field_value(fields, 2);
                    let wsl = Self::field_value(fields, 4);
                    actions::add_project_paths(data, &group, &name, &alias, &path, &wsl)
                }
            }
            FormKind::EditProject {
                group,
                project_id,
                old_name,
            } => {
                let is_ssh = actions::find_project_ref(data, &group, &project_id)
                    .map(|p| p.is_ssh_project())
                    .unwrap_or(false);
                if is_ssh {
                    // SSH 编辑表单：0 用户 1 主机 2 端口 3 远程路径 4 密钥来源 5 密码 6 口令
                    let ssh_user = Self::field_value(fields, 0);
                    let ssh_host = Self::field_value(fields, 1);
                    let ssh_port = Self::field_value(fields, 2);
                    let remote = Self::field_value(fields, 3);
                    let key_source = Self::field_value(fields, 4);
                    let password = Self::field_value(fields, 5);
                    let key_pass = Self::field_value(fields, 6);
                    match Self::compose_ssh_target(&ssh_user, &ssh_host, &ssh_port) {
                        Ok(target) => actions::edit_project_ssh(
                            data,
                            &group,
                            &project_id,
                            &target,
                            &remote,
                            &key_source,
                            &password,
                            &key_pass,
                        ),
                        Err(e) => Err(e),
                    }
                } else {
                    let name = Self::field_value(fields, 0);
                    let alias = Self::field_value(fields, 1);
                    let path = Self::field_value(fields, 2);
                    let wsl = Self::field_value(fields, 4);
                    let base =
                        actions::edit_project(data, &group, &old_name, &name, &alias, &path, &wsl);
                    // 普通项目表单填了主机即转换为 SSH 项目
                    let ssh_host = Self::field_value(fields, 6);
                    if base.is_ok() && !ssh_host.trim().is_empty() {
                        let ssh_user = Self::field_value(fields, 5);
                        let ssh_port = Self::field_value(fields, 7);
                        let remote = Self::field_value(fields, 8);
                        match Self::compose_ssh_target(&ssh_user, &ssh_host, &ssh_port) {
                            Ok(ssh_target) => actions::set_ssh_target(
                                data,
                                &group,
                                &project_id,
                                &ssh_target,
                                &remote,
                            ),
                            Err(e) => Err(e),
                        }
                    } else {
                        base
                    }
                }
            }
            FormKind::AddCommand {
                group,
                project_id,
                env,
            } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                actions::add_command(data, &project_id, &group, &name, &env, &command)
            }
            FormKind::EditCommand {
                group,
                project_id,
                index,
                env,
            } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                actions::edit_command(data, &project_id, &group, index, &name, &env, &command)
            }
            FormKind::AddTool { env } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                actions::add_config_tool(config, env, &name, &command)
            }
            FormKind::EditTool { env, index } => {
                let name = Self::field_value(fields, 0);
                let command = Self::field_value(fields, 1);
                actions::edit_config_tool(config, env, index, &name, &command)
            }
        };

        match result {
            Ok(msg) => {
                self.back_to_browse();
                self.flash(msg);
            }
            Err(e) => self.set_form_error(e),
        }
        Outcome::Continue
    }

    pub fn resume_after_folder_pick(&mut self, path: Option<String>) {
        let Mode::Form {
            title,
            mut fields,
            focus,
            kind,
            ..
        } = self.mode.clone()
        else {
            return;
        };
        match path {
            Some(path) => {
                // 项目表单：0 名 1 别名 2 Windows 路径 3 浏览 4 WSL
                if let Some(FormField::Text { value, .. }) = fields.get_mut(2) {
                    *value = path.clone();
                }
                let wsl_empty = matches!(
                    fields.get(4),
                    Some(FormField::Text { value, .. }) if value.trim().is_empty()
                );
                if wsl_empty {
                    let linux = crate::domain::models::win_path_to_linux(&path);
                    if let Some(FormField::Text { value, .. }) = fields.get_mut(4) {
                        *value = linux;
                    }
                }
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus: 2,
                    kind,
                    error: None,
                };
            }
            None => {
                self.mode = Mode::Form {
                    title,
                    fields,
                    focus,
                    kind,
                    error: Some("已取消选择文件夹".into()),
                };
            }
        }
    }
}
