use super::*;

impl App {
    pub(crate) fn open_secret_viewer(&mut self, data: &ProjectData, config: &AppConfig) {
        match &self.right_pane {
            RightPane::Connections => {
                let Some(id) = self.selected_connection_id(data) else {
                    self.flash("无选中连接");
                    return;
                };
                self.open_secret_viewer_for(data, config, &id);
            }
            RightPane::Projects => {
                let Some(gi) = self.left_is_group(data, self.left_sel) else {
                    return;
                };
                let indices = self.filtered_project_indices(data, gi);
                let Some(&pi) = indices.get(self.right_sel) else {
                    return;
                };
                let p = &data.groups[gi].projects[pi];
                match data.connection_of(p) {
                    Ok(conn) => {
                        let id = conn.id.clone();
                        self.open_secret_viewer_for(data, config, &id);
                    }
                    Err(_) => {
                        if p.is_ssh_project() {
                            self.flash("项目引用的远程连接不存在，请重新选择连接");
                        } else {
                            self.flash("仅 SSH 项目支持查看保存的秘密");
                        }
                    }
                }
            }
            _ => self.flash("请先选择项目或连接"),
        }
    }

    pub(crate) fn open_secret_viewer_for(
        &mut self,
        data: &ProjectData,
        config: &AppConfig,
        connection_id: &str,
    ) {
        let Some(conn) = data.connection(connection_id) else {
            self.flash("未找到远程连接");
            return;
        };
        if !conn.has_saved_secrets() {
            self.flash("该连接未保存密码或口令");
            return;
        }
        if config.pin.is_none() {
            self.flash("尚未设置 PIN，请先在终端运行 `pcs pin set`");
            return;
        }
        self.mode = Mode::SecretViewer {
            connection_id: conn.id.clone(),
            pin_input: String::new(),
            attempts: 0,
            revealed: None,
            error: None,
        };
    }

    pub(crate) fn handle_secret_viewer(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::SecretViewer {
            connection_id,
            pin_input,
            attempts,
            revealed,
            error: _,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Esc => self.back_to_browse(),
            KeyCode::Enter => {
                if revealed.is_some() {
                    self.back_to_browse();
                    return Outcome::Continue;
                }
                let Some(record) = config.pin.clone() else {
                    self.back_to_browse();
                    return Outcome::Continue;
                };
                match secret::verify_pin(&pin_input, &record) {
                    Ok(true) => match data.connection(&connection_id) {
                        Some(conn) => {
                            let revealed = (
                                (!conn.ssh_password_enc.trim().is_empty())
                                    .then(|| secret::unprotect(&conn.ssh_password_enc).ok())
                                    .flatten(),
                                (!conn.ssh_key_pass_enc.trim().is_empty())
                                    .then(|| secret::unprotect(&conn.ssh_key_pass_enc).ok())
                                    .flatten(),
                            );
                            self.mode = Mode::SecretViewer {
                                connection_id,
                                pin_input: String::new(),
                                attempts,
                                revealed: Some(revealed),
                                error: None,
                            };
                        }
                        None => {
                            self.back_to_browse();
                            self.flash("连接已被删除");
                        }
                    },
                    Ok(false) => {
                        let attempts = attempts + 1;
                        if attempts >= 3 {
                            self.back_to_browse();
                            self.flash("PIN 验证失败");
                        } else {
                            self.mode = Mode::SecretViewer {
                                connection_id,
                                pin_input: String::new(),
                                attempts,
                                revealed: None,
                                error: Some(format!("PIN 不正确，剩余 {} 次", 3 - attempts)),
                            };
                        }
                    }
                    Err(e) => {
                        self.mode = Mode::SecretViewer {
                            connection_id,
                            pin_input,
                            attempts,
                            revealed: None,
                            error: Some(e.to_string()),
                        };
                    }
                }
            }
            KeyCode::Backspace => {
                let mut pin_input = pin_input;
                pin_input.pop();
                self.mode = Mode::SecretViewer {
                    connection_id,
                    pin_input,
                    attempts,
                    revealed: None,
                    error: None,
                };
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut pin_input = pin_input;
                pin_input.push(c);
                self.mode = Mode::SecretViewer {
                    connection_id,
                    pin_input,
                    attempts,
                    revealed: None,
                    error: None,
                };
            }
            _ => {}
        }
        Outcome::Continue
    }
}
