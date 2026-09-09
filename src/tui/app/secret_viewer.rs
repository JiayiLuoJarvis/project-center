use super::*;

impl App {
    pub(crate) fn open_secret_viewer(&mut self, data: &ProjectData, config: &AppConfig) {
        let RightPane::Projects = self.right_pane else {
            self.flash("请先选择项目");
            return;
        };
        let Some(gi) = self.left_is_group(data, self.left_sel) else {
            return;
        };
        let indices = self.filtered_project_indices(data, gi);
        let Some(&pi) = indices.get(self.right_sel) else {
            return;
        };
        let p = &data.groups[gi].projects[pi];
        if !p.is_ssh_project() {
            self.flash("仅 SSH 项目支持查看保存的秘密");
            return;
        }
        if p.ssh_password_enc.trim().is_empty() && p.ssh_key_pass_enc.trim().is_empty() {
            self.flash("该项目未保存密码或口令");
            return;
        }
        if config.pin.is_none() {
            self.flash("尚未设置 PIN，请先在终端运行 `pcs pin set`");
            return;
        }
        self.mode = Mode::SecretViewer {
            group: data.groups[gi].name.clone(),
            project_id: p.id.clone(),
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
            group,
            project_id,
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
                    Ok(true) => {
                        let revealed =
                            actions::find_project_ref(data, &group, &project_id).map(|p| {
                                (
                                    (!p.ssh_password_enc.trim().is_empty())
                                        .then(|| secret::unprotect(&p.ssh_password_enc).ok())
                                        .flatten(),
                                    (!p.ssh_key_pass_enc.trim().is_empty())
                                        .then(|| secret::unprotect(&p.ssh_key_pass_enc).ok())
                                        .flatten(),
                                )
                            });
                        match revealed {
                            Some(revealed) => {
                                self.mode = Mode::SecretViewer {
                                    group,
                                    project_id,
                                    pin_input: String::new(),
                                    attempts,
                                    revealed: Some(revealed),
                                    error: None,
                                };
                            }
                            None => {
                                self.back_to_browse();
                                self.flash("项目已被删除");
                            }
                        }
                    }
                    Ok(false) => {
                        let attempts = attempts + 1;
                        if attempts >= 3 {
                            self.back_to_browse();
                            self.flash("PIN 验证失败");
                        } else {
                            self.mode = Mode::SecretViewer {
                                group,
                                project_id,
                                pin_input: String::new(),
                                attempts,
                                revealed: None,
                                error: Some(format!("PIN 不正确，剩余 {} 次", 3 - attempts)),
                            };
                        }
                    }
                    Err(e) => {
                        self.mode = Mode::SecretViewer {
                            group,
                            project_id,
                            pin_input,
                            attempts,
                            revealed: None,
                            error: Some(e),
                        };
                    }
                }
            }
            KeyCode::Backspace => {
                let mut pin_input = pin_input;
                pin_input.pop();
                self.mode = Mode::SecretViewer {
                    group,
                    project_id,
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
                    group,
                    project_id,
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
