use super::*;

impl App {
    pub(crate) fn handle_confirm(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::Confirm { kind, .. } = self.mode.clone() else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.back_to_browse();
                match kind {
                    ConfirmKind::DeleteProject {
                        group, project_id, ..
                    } => match actions::remove_project(data, &project_id, &group) {
                        Ok(msg) => {
                            self.flash(msg);
                            self.clamp_selection(data, config);
                        }
                        Err(e) => self.flash(e),
                    },
                    ConfirmKind::DeleteGroup { name, .. } => {
                        match actions::remove_group(data, &name) {
                            Ok(msg) => {
                                self.flash(msg);
                                self.left_sel = 0;
                                self.sync_right_pane(data);
                                self.clamp_selection(data, config);
                            }
                            Err(e) => self.flash(e),
                        }
                    }
                    ConfirmKind::DeleteCommand {
                        group,
                        project_id,
                        index,
                        ..
                    } => match actions::remove_command(data, &project_id, &group, index) {
                        Ok(msg) => {
                            self.flash(msg);
                            self.clamp_selection(data, config);
                        }
                        Err(e) => self.flash(e),
                    },
                    ConfirmKind::DeleteTool { env, index, .. } => {
                        match actions::remove_config_tool(config, env, index) {
                            Ok(msg) => {
                                self.flash(msg);
                                self.clamp_selection(data, config);
                            }
                            Err(e) => self.flash(e),
                        }
                    }
                    ConfirmKind::PurgeTrash { id, .. } => {
                        match actions::purge_trash_item(data, &id) {
                            Ok(msg) => {
                                self.flash(msg);
                                self.clamp_selection(data, config);
                            }
                            Err(e) => self.flash(e),
                        }
                    }
                    ConfirmKind::EmptyTrash => match actions::empty_trash(data) {
                        Ok(msg) => {
                            self.flash(msg);
                            self.right_sel = 0;
                        }
                        Err(e) => self.flash(e),
                    },
                    ConfirmKind::ResetConfig => match actions::reset_app_config(config) {
                        Ok(msg) => self.flash(msg),
                        Err(e) => self.flash(e),
                    },
                }
            }
            _ => self.back_to_browse(),
        }
        Outcome::Continue
    }
}
