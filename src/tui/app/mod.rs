use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::domain::models::{Project, ProjectData};
use crate::launch::LaunchOption;
use crate::persist as secret;
use crate::persist::{AppConfig, ConfigEnv};
use crate::tui::actions;

mod action_menu;
mod browse;
mod confirm;
mod filter;
mod form;
mod launch;
mod secret_viewer;
mod state;

pub(crate) use state::*;

impl App {
    pub fn new(data: &ProjectData) -> Self {
        let mut app = Self {
            focus: Focus::Groups,
            left_sel: 0,
            right_sel: 0,
            mode: Mode::Browse,
            right_pane: RightPane::Projects,
            filter: String::new(),
            flash: None,
            exit_after_launch: false,
            short_session: false,
            quit_confirm: false,
        };
        app.sync_right_pane(data);
        app
    }

    pub fn short_launch(
        data: &ProjectData,
        config: &AppConfig,
        group: &str,
        project_id: &str,
    ) -> Self {
        let mut app = Self::new(data);
        app.short_session = true;
        app.exit_after_launch = true;
        if let Some(gi) = data
            .groups
            .iter()
            .position(|g| g.name.eq_ignore_ascii_case(group))
        {
            app.left_sel = gi;
            app.sync_right_pane(data);
            app.focus = Focus::Projects;
            if let Some(pi) = data.groups[gi]
                .projects
                .iter()
                .position(|p| p.id.eq_ignore_ascii_case(project_id))
            {
                app.right_sel = pi;
            }
        }
        app.open_launch_picker(data, config, group, project_id);
        app
    }

    /// 左栏展示项：过滤后的分组下标，末尾固定回收站与配置。
    pub fn left_items(&self, data: &ProjectData) -> Vec<LeftItem> {
        let mut items: Vec<LeftItem> = data
            .groups
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                if self.focus != Focus::Groups || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&g.name) || self.matches_filter(&g.alias)
            })
            .map(|(i, _)| LeftItem::Group(i))
            .collect();
        items.push(LeftItem::Trash);
        items.push(LeftItem::Config);
        items
    }

    pub fn left_count(&self, data: &ProjectData) -> usize {
        self.left_items(data).len()
    }

    pub fn left_is_group(&self, data: &ProjectData, sel: usize) -> Option<usize> {
        match self.left_items(data).get(sel) {
            Some(LeftItem::Group(i)) => Some(*i),
            _ => None,
        }
    }

    pub fn left_is_trash(&self, data: &ProjectData, sel: usize) -> bool {
        matches!(self.left_items(data).get(sel), Some(LeftItem::Trash))
    }

    pub fn left_is_config(&self, data: &ProjectData, sel: usize) -> bool {
        matches!(self.left_items(data).get(sel), Some(LeftItem::Config))
    }

    pub fn sync_right_pane(&mut self, data: &ProjectData) {
        if matches!(
            self.right_pane,
            RightPane::Commands { .. } | RightPane::ConfigTools { .. }
        ) {
            return;
        }
        if self.left_is_trash(data, self.left_sel) {
            self.right_pane = RightPane::Trash;
        } else if self.left_is_config(data, self.left_sel) {
            if !matches!(self.right_pane, RightPane::ConfigTools { .. }) {
                self.right_pane = RightPane::ConfigEnvs;
            }
        } else {
            self.right_pane = RightPane::Projects;
        }
    }

    pub fn clamp_selection(&mut self, data: &ProjectData, config: &AppConfig) {
        let lc = self.left_count(data).max(1);
        if self.left_sel >= lc {
            self.left_sel = lc - 1;
        }
        let rc = self.right_len(data, config).max(1);
        if self.right_sel >= rc {
            self.right_sel = rc.saturating_sub(1);
        }
    }

    pub fn right_len(&self, data: &ProjectData, config: &AppConfig) -> usize {
        match &self.right_pane {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    self.filtered_project_indices(data, gi).len()
                } else {
                    0
                }
            }
            RightPane::Trash => self.filtered_trash_indices(data).len(),
            RightPane::ConfigEnvs => 4,
            RightPane::ConfigTools { env } => self.filtered_tool_indices(config, *env).len(),
            RightPane::Commands { group, project_id } => {
                actions::find_project_ref(data, group, project_id)
                    .map(|p| self.filtered_command_indices(p).len())
                    .unwrap_or(0)
            }
        }
    }

    pub(crate) fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
    }

    pub(crate) fn clear_flash(&mut self) {
        self.flash = None;
    }

    pub(crate) fn back_to_browse(&mut self) {
        self.mode = Mode::Browse;
    }

    pub(crate) fn pop_right_pane(&mut self, data: &ProjectData, config: &AppConfig) {
        match &self.right_pane {
            RightPane::ConfigTools { env } => {
                let sel = match env {
                    ConfigEnv::Wsl => 0,
                    ConfigEnv::PowerShell => 1,
                    ConfigEnv::Ide => 2,
                };
                self.right_pane = RightPane::ConfigEnvs;
                self.right_sel = sel;
            }
            RightPane::Commands { .. } => {
                self.right_pane = RightPane::Projects;
                self.right_sel = 0;
                self.clamp_selection(data, config);
            }
            _ => {}
        }
    }

    pub fn handle(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        self.clear_flash();
        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
        if self.quit_confirm {
            return match key.code {
                KeyCode::Char('y')
                | KeyCode::Char('Y')
                | KeyCode::Char('q')
                | KeyCode::Char('Q') => {
                    self.quit_confirm = false;
                    Outcome::Quit
                }
                _ if ctrl_c => {
                    self.quit_confirm = false;
                    Outcome::Quit
                }
                _ => {
                    self.quit_confirm = false;
                    Outcome::Continue
                }
            };
        }
        if ctrl_c {
            self.quit_confirm = true;
            return Outcome::Continue;
        }
        match self.mode.clone() {
            Mode::Browse => self.handle_browse(key, data, config),
            Mode::LaunchPicker { .. } => self.handle_launch_picker(key, data),
            Mode::ActionMenu { .. } => self.handle_action_menu(key, data, config),
            Mode::ListPicker { .. } => self.handle_list_picker(key, data, config),
            Mode::Form { .. } => self.handle_form(key, data, config),
            Mode::Confirm { .. } => self.handle_confirm(key, data, config),
            Mode::SecretViewer { .. } => self.handle_secret_viewer(key, data, config),
            Mode::Filter => self.handle_filter(key, data, config),
            Mode::Help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
                ) {
                    self.back_to_browse();
                }
                Outcome::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests;
