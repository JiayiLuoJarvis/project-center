use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{AppConfig, ConfigEnv};
use crate::menu::LaunchOption;
use crate::models::{Project, ProjectData};
use crate::tui::actions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Groups,
    Projects,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPane {
    Projects,
    Trash,
    ConfigEnvs,
    ConfigTools { env: ConfigEnv },
    Commands { group: String, project_id: String },
}

#[derive(Clone, Debug)]
pub enum ListKind {
    PathType {
        group: String,
        name: String,
    },
    CommandEnv {
        group: String,
        project_id: String,
        edit_index: Option<usize>,
        name: String,
        #[allow(dead_code)]
        current_env: String,
    },
    MoveTarget {
        group: String,
        project_id: String,
        targets: Vec<String>,
    },
    DefaultTool {
        group: String,
        project_id: String,
        options: Vec<LaunchOption>,
    },
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    Project {
        group: String,
        project_id: String,
    },
    TrashItem {
        id: String,
    },
    ConfigTool {
        env: ConfigEnv,
        index: usize,
    },
    Command {
        group: String,
        project_id: String,
        index: usize,
    },
}

#[derive(Clone, Debug)]
pub enum ConfirmKind {
    DeleteProject {
        group: String,
        project_id: String,
        #[allow(dead_code)]
        name: String,
    },
    DeleteGroup {
        name: String,
        #[allow(dead_code)]
        count: usize,
    },
    DeleteCommand {
        group: String,
        project_id: String,
        index: usize,
        #[allow(dead_code)]
        name: String,
    },
    DeleteTool {
        env: ConfigEnv,
        index: usize,
        #[allow(dead_code)]
        name: String,
    },
    PurgeTrash {
        id: String,
        #[allow(dead_code)]
        name: String,
    },
    EmptyTrash,
    ResetConfig,
}

#[derive(Clone, Debug)]
pub enum FormKind {
    AddGroup,
    RenameGroup {
        old: String,
    },
    AddProjectName {
        group: String,
    },
    AddProjectWsl {
        group: String,
        name: String,
    },
    EditProject {
        group: String,
        project_id: String,
        old_name: String,
        step: u8,
        name: String,
        path: String,
        wsl_path: String,
    },
    AddCommandName {
        group: String,
        project_id: String,
        env: String,
    },
    AddCommandText {
        group: String,
        project_id: String,
        name: String,
        env: String,
    },
    EditCommandName {
        group: String,
        project_id: String,
        index: usize,
        env: String,
        command: String,
    },
    EditCommandText {
        group: String,
        project_id: String,
        index: usize,
        name: String,
        env: String,
    },
    AddToolName {
        env: ConfigEnv,
    },
    AddToolCmd {
        env: ConfigEnv,
        name: String,
    },
    EditToolName {
        env: ConfigEnv,
        index: usize,
        command: String,
    },
    EditToolCmd {
        env: ConfigEnv,
        index: usize,
        name: String,
    },
}

#[derive(Clone, Debug)]
pub enum Mode {
    Browse,
    LaunchPicker {
        group: String,
        project_id: String,
        options: Vec<LaunchOption>,
        labels: Vec<String>,
        selected: usize,
    },
    ActionMenu {
        kind: ActionKind,
        items: Vec<String>,
        selected: usize,
    },
    ListPicker {
        title: String,
        items: Vec<String>,
        selected: usize,
        kind: ListKind,
    },
    InlineInput {
        prompt: String,
        buffer: String,
        kind: FormKind,
    },
    Confirm {
        message: String,
        kind: ConfirmKind,
    },
    Filter,
    Help,
}

#[derive(Clone, Debug)]
pub enum Outcome {
    Continue,
    Quit,
    Launch {
        project: Project,
        group: String,
        option: LaunchOption,
        exit_after: bool,
    },
    PickFolder {
        group: String,
        name: String,
    },
}

pub struct App {
    pub focus: Focus,
    pub left_sel: usize,
    pub right_sel: usize,
    pub mode: Mode,
    pub right_pane: RightPane,
    pub filter: String,
    pub flash: Option<String>,
    pub exit_after_launch: bool,
    pub short_session: bool,
}

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

    pub fn left_count(data: &ProjectData) -> usize {
        data.groups.len() + 2
    }

    pub fn left_is_group(data: &ProjectData, sel: usize) -> Option<usize> {
        if sel < data.groups.len() {
            Some(sel)
        } else {
            None
        }
    }

    pub fn left_is_trash(data: &ProjectData, sel: usize) -> bool {
        sel == data.groups.len()
    }

    pub fn left_is_config(data: &ProjectData, sel: usize) -> bool {
        sel == data.groups.len() + 1
    }

    pub fn sync_right_pane(&mut self, data: &ProjectData) {
        if matches!(
            self.right_pane,
            RightPane::Commands { .. } | RightPane::ConfigTools { .. }
        ) {
            return;
        }
        if Self::left_is_trash(data, self.left_sel) {
            self.right_pane = RightPane::Trash;
        } else if Self::left_is_config(data, self.left_sel) {
            if !matches!(self.right_pane, RightPane::ConfigTools { .. }) {
                self.right_pane = RightPane::ConfigEnvs;
            }
        } else {
            self.right_pane = RightPane::Projects;
        }
    }

    pub fn clamp_selection(&mut self, data: &ProjectData, config: &AppConfig) {
        let lc = Self::left_count(data).max(1);
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
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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

    fn matches_filter(&self, text: &str) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        text.to_lowercase().contains(&self.filter.to_lowercase())
    }

    #[cfg(test)]
    pub fn filtered_group_indices(&self, data: &ProjectData) -> Vec<usize> {
        data.groups
            .iter()
            .enumerate()
            .filter(|(_, g)| self.focus != Focus::Groups || self.matches_filter(&g.name))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_project_indices(&self, data: &ProjectData, gi: usize) -> Vec<usize> {
        data.groups
            .get(gi)
            .map(|g| {
                g.projects
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        if self.focus != Focus::Projects || self.filter.is_empty() {
                            return true;
                        }
                        self.matches_filter(&p.name)
                            || self.matches_filter(&p.path)
                            || self.matches_filter(&p.linux_path())
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn filtered_trash_indices(&self, data: &ProjectData) -> Vec<usize> {
        data.trash
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&actions::trash_label(item))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_tool_indices(&self, config: &AppConfig, env: ConfigEnv) -> Vec<usize> {
        env.tools(config)
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&t.name) || self.matches_filter(&t.command)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_command_indices(&self, project: &Project) -> Vec<usize> {
        project
            .commands
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&c.name) || self.matches_filter(&c.command)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
    }

    fn clear_flash(&mut self) {
        self.flash = None;
    }

    fn back_to_browse(&mut self) {
        self.mode = Mode::Browse;
    }

    pub fn open_launch_picker(
        &mut self,
        data: &ProjectData,
        config: &AppConfig,
        group: &str,
        project_id: &str,
    ) {
        let Some(project) = actions::find_project_ref(data, group, project_id) else {
            self.flash("未找到项目");
            return;
        };
        let (options, labels) = actions::launch_labels(project, config);
        if options.is_empty() {
            self.flash("无可用启动方式");
            return;
        }
        self.mode = Mode::LaunchPicker {
            group: group.to_string(),
            project_id: project_id.to_string(),
            options,
            labels,
            selected: 0,
        };
    }

    pub fn handle(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        self.clear_flash();
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Outcome::Quit;
        }
        match self.mode.clone() {
            Mode::Browse => self.handle_browse(key, data, config),
            Mode::LaunchPicker { .. } => self.handle_launch_picker(key, data),
            Mode::ActionMenu { .. } => self.handle_action_menu(key, data, config),
            Mode::ListPicker { .. } => self.handle_list_picker(key, data, config),
            Mode::InlineInput { .. } => self.handle_inline(key, data, config),
            Mode::Confirm { .. } => self.handle_confirm(key, data, config),
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

    fn handle_browse(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match key.code {
            KeyCode::Char('q') => return Outcome::Quit,
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                return Outcome::Continue;
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Filter;
                return Outcome::Continue;
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.clamp_selection(data, config);
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
                    self.left_sel = Self::left_count(data).saturating_sub(1);
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

    fn move_sel(&mut self, delta: i32, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            let n = Self::left_count(data) as i32;
            if n == 0 {
                return;
            }
            let next = (self.left_sel as i32 + delta).rem_euclid(n) as usize;
            self.left_sel = next;
            self.right_sel = 0;
            self.filter.clear();
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

    fn on_enter(&mut self, data: &mut ProjectData, config: &mut AppConfig) -> Outcome {
        if self.focus == Focus::Groups {
            self.sync_right_pane(data);
            self.focus = Focus::Projects;
            self.right_sel = 0;
            return Outcome::Continue;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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

    fn open_action_menu(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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

    fn on_add(&mut self, data: &ProjectData, _config: &AppConfig) {
        if self.focus == Focus::Groups
            && !Self::left_is_trash(data, self.left_sel)
            && !Self::left_is_config(data, self.left_sel)
        {
            self.mode = Mode::InlineInput {
                prompt: "分组名: ".into(),
                buffer: String::new(),
                kind: FormKind::AddGroup,
            };
            return;
        }
        if self.focus == Focus::Groups {
            self.flash("请在分组区按 a 新增分组");
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
                    let group = data.groups[gi].name.clone();
                    self.mode = Mode::InlineInput {
                        prompt: "项目名: ".into(),
                        buffer: String::new(),
                        kind: FormKind::AddProjectName { group },
                    };
                }
            }
            RightPane::ConfigTools { env } => {
                self.mode = Mode::InlineInput {
                    prompt: "工具名称: ".into(),
                    buffer: String::new(),
                    kind: FormKind::AddToolName { env },
                };
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

    fn on_edit(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = Self::left_is_group(data, self.left_sel) {
                let old = data.groups[gi].name.clone();
                self.mode = Mode::InlineInput {
                    prompt: "新的分组名: ".into(),
                    buffer: old.clone(),
                    kind: FormKind::RenameGroup { old },
                };
            }
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
                    let indices = self.filtered_project_indices(data, gi);
                    let Some(&pi) = indices.get(self.right_sel) else {
                        return;
                    };
                    let p = &data.groups[gi].projects[pi];
                    self.mode = Mode::InlineInput {
                        prompt: "项目名: ".into(),
                        buffer: p.name.clone(),
                        kind: FormKind::EditProject {
                            group: data.groups[gi].name.clone(),
                            project_id: p.id.clone(),
                            old_name: p.name.clone(),
                            step: 0,
                            name: p.name.clone(),
                            path: p.path.clone(),
                            wsl_path: p.wsl_path.clone(),
                        },
                    };
                }
            }
            RightPane::ConfigTools { env } => {
                let indices = self.filtered_tool_indices(config, env);
                let Some(&ti) = indices.get(self.right_sel) else {
                    return;
                };
                let tool = &env.tools(config)[ti];
                self.mode = Mode::InlineInput {
                    prompt: "工具名称: ".into(),
                    buffer: tool.name.clone(),
                    kind: FormKind::EditToolName {
                        env,
                        index: ti,
                        command: tool.command.clone(),
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

    fn on_delete(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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
                if let Some(gi) = Self::left_is_group(data, self.left_sel) {
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

    fn on_move(&mut self, data: &ProjectData) {
        if !matches!(self.right_pane, RightPane::Projects) || self.focus != Focus::Projects {
            self.flash("仅可在项目列表中移动");
            return;
        }
        let Some(gi) = Self::left_is_group(data, self.left_sel) else {
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

    fn restore_selected_trash(&mut self, data: &mut ProjectData) {
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

    fn handle_launch_picker(&mut self, key: KeyEvent, data: &ProjectData) -> Outcome {
        let Mode::LaunchPicker {
            group,
            project_id,
            options,
            labels,
            selected,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Esc => {
                if self.short_session {
                    return Outcome::Quit;
                }
                self.back_to_browse();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let n = options.len();
                if n > 0 {
                    let next = (selected + 1) % n;
                    self.mode = Mode::LaunchPicker {
                        group,
                        project_id,
                        options,
                        labels,
                        selected: next,
                    };
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = options.len();
                if n > 0 {
                    let next = (selected + n - 1) % n;
                    self.mode = Mode::LaunchPicker {
                        group,
                        project_id,
                        options,
                        labels,
                        selected: next,
                    };
                }
            }
            KeyCode::Enter => {
                if let Some(option) = options.get(selected).cloned()
                    && let Some(project) = actions::find_project_ref(data, &group, &project_id)
                {
                    let exit_after = self.exit_after_launch || self.short_session;
                    if !self.short_session {
                        self.back_to_browse();
                    }
                    return Outcome::Launch {
                        project: project.clone(),
                        group,
                        option,
                        exit_after,
                    };
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    fn handle_action_menu(
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

    fn apply_action(
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
                        self.mode = Mode::InlineInput {
                            prompt: "新的分组名: ".into(),
                            buffer: group.clone(),
                            kind: FormKind::RenameGroup { old: group },
                        };
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
                        self.mode = Mode::InlineInput {
                            prompt: "项目名: ".into(),
                            buffer: project.name.clone(),
                            kind: FormKind::EditProject {
                                group,
                                project_id,
                                old_name: project.name.clone(),
                                step: 0,
                                name: project.name.clone(),
                                path: project.path.clone(),
                                wsl_path: project.wsl_path.clone(),
                            },
                        };
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
                        self.mode = Mode::InlineInput {
                            prompt: "工具名称: ".into(),
                            buffer: tool.name.clone(),
                            kind: FormKind::EditToolName {
                                env,
                                index,
                                command: tool.command.clone(),
                            },
                        };
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

    fn handle_list_picker(
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

    fn apply_list_pick(
        &mut self,
        kind: ListKind,
        selected: usize,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match kind {
            ListKind::PathType { group, name } => {
                self.back_to_browse();
                if selected == 0 {
                    return Outcome::PickFolder { group, name };
                }
                self.mode = Mode::InlineInput {
                    prompt: "WSL 路径: ".into(),
                    buffer: String::new(),
                    kind: FormKind::AddProjectWsl { group, name },
                };
            }
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
                        self.mode = Mode::InlineInput {
                            prompt: "命令名称: ".into(),
                            buffer: name.clone(),
                            kind: FormKind::EditCommandName {
                                group,
                                project_id,
                                index,
                                env,
                                command: cmd.command,
                            },
                        };
                    } else {
                        self.back_to_browse();
                    }
                } else if name.is_empty() {
                    self.mode = Mode::InlineInput {
                        prompt: "命令名称: ".into(),
                        buffer: String::new(),
                        kind: FormKind::AddCommandName {
                            group,
                            project_id,
                            env,
                        },
                    };
                } else {
                    self.mode = Mode::InlineInput {
                        prompt: "启动命令: ".into(),
                        buffer: String::new(),
                        kind: FormKind::AddCommandText {
                            group,
                            project_id,
                            name,
                            env,
                        },
                    };
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

    fn handle_inline(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        let Mode::InlineInput {
            prompt,
            mut buffer,
            kind,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        match key.code {
            KeyCode::Esc => {
                self.back_to_browse();
                return Outcome::Continue;
            }
            KeyCode::Backspace => {
                buffer.pop();
                self.mode = Mode::InlineInput {
                    prompt,
                    buffer,
                    kind,
                };
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                buffer.push(c);
                self.mode = Mode::InlineInput {
                    prompt,
                    buffer,
                    kind,
                };
            }
            KeyCode::Enter => return self.submit_inline(kind, buffer, data, config),
            _ => {}
        }
        Outcome::Continue
    }

    fn submit_inline(
        &mut self,
        kind: FormKind,
        buffer: String,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match kind {
            FormKind::AddGroup => {
                self.back_to_browse();
                match actions::add_group(data, &buffer) {
                    Ok(msg) => {
                        self.flash(msg);
                        self.left_sel = data.groups.len().saturating_sub(1);
                        self.sync_right_pane(data);
                    }
                    Err(e) => self.flash(e),
                }
            }
            FormKind::RenameGroup { old } => {
                self.back_to_browse();
                match actions::rename_group(data, &old, &buffer) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
            FormKind::AddProjectName { group } => {
                let name = buffer.trim().to_string();
                if name.is_empty() {
                    self.flash("项目名不能为空");
                    self.back_to_browse();
                    return Outcome::Continue;
                }
                self.mode = Mode::ListPicker {
                    title: "选择项目位置".into(),
                    items: vec![
                        "Windows 路径（选择文件夹）".into(),
                        "仅 WSL 路径（手动输入）".into(),
                    ],
                    selected: 0,
                    kind: ListKind::PathType { group, name },
                };
            }
            FormKind::AddProjectWsl { group, name } => {
                self.back_to_browse();
                match actions::add_project_wsl(data, &group, &name, &buffer) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
            FormKind::EditProject {
                group,
                project_id,
                old_name,
                step,
                mut name,
                mut path,
                mut wsl_path,
            } => match step {
                0 => {
                    name = buffer;
                    self.mode = Mode::InlineInput {
                        prompt: "Windows/Linux 路径: ".into(),
                        buffer: path.clone(),
                        kind: FormKind::EditProject {
                            group,
                            project_id,
                            old_name,
                            step: 1,
                            name,
                            path,
                            wsl_path,
                        },
                    };
                }
                1 => {
                    path = buffer;
                    self.mode = Mode::InlineInput {
                        prompt: "WSL 路径: ".into(),
                        buffer: wsl_path.clone(),
                        kind: FormKind::EditProject {
                            group,
                            project_id,
                            old_name,
                            step: 2,
                            name,
                            path,
                            wsl_path,
                        },
                    };
                }
                _ => {
                    wsl_path = buffer;
                    self.back_to_browse();
                    match actions::edit_project(data, &group, &old_name, &name, &path, &wsl_path) {
                        Ok(msg) => self.flash(msg),
                        Err(e) => self.flash(e),
                    }
                    let _ = project_id;
                }
            },
            FormKind::AddCommandName {
                group,
                project_id,
                env,
            } => {
                self.mode = Mode::InlineInput {
                    prompt: "启动命令: ".into(),
                    buffer: String::new(),
                    kind: FormKind::AddCommandText {
                        group,
                        project_id,
                        name: buffer,
                        env,
                    },
                };
            }
            FormKind::AddCommandText {
                group,
                project_id,
                name,
                env,
            } => {
                self.back_to_browse();
                match actions::add_command(data, &project_id, &group, &name, &env, &buffer) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
            FormKind::EditCommandName {
                group,
                project_id,
                index,
                env,
                command,
            } => {
                self.mode = Mode::InlineInput {
                    prompt: "启动命令: ".into(),
                    buffer: command,
                    kind: FormKind::EditCommandText {
                        group,
                        project_id,
                        index,
                        name: buffer,
                        env,
                    },
                };
            }
            FormKind::EditCommandText {
                group,
                project_id,
                index,
                name,
                env,
            } => {
                self.back_to_browse();
                match actions::edit_command(data, &project_id, &group, index, &name, &env, &buffer)
                {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
            FormKind::AddToolName { env } => {
                self.mode = Mode::InlineInput {
                    prompt: "启动命令（不含参数）: ".into(),
                    buffer: String::new(),
                    kind: FormKind::AddToolCmd { env, name: buffer },
                };
            }
            FormKind::AddToolCmd { env, name } => {
                self.back_to_browse();
                match actions::add_config_tool(config, env, &name, &buffer) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
            FormKind::EditToolName {
                env,
                index,
                command,
            } => {
                self.mode = Mode::InlineInput {
                    prompt: "启动命令: ".into(),
                    buffer: command,
                    kind: FormKind::EditToolCmd {
                        env,
                        index,
                        name: buffer,
                    },
                };
            }
            FormKind::EditToolCmd { env, index, name } => {
                self.back_to_browse();
                match actions::edit_config_tool(config, env, index, &name, &buffer) {
                    Ok(msg) => self.flash(msg),
                    Err(e) => self.flash(e),
                }
            }
        }
        Outcome::Continue
    }

    fn handle_confirm(
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

    fn handle_filter(&mut self, key: KeyEvent, data: &ProjectData, config: &AppConfig) -> Outcome {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Browse;
                self.clamp_selection(data, config);
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.clamp_selection(data, config);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.mode = Mode::Browse;
                self.move_sel(1, data, config);
                self.mode = Mode::Filter;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.mode = Mode::Browse;
                self.move_sel(-1, data, config);
                self.mode = Mode::Filter;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.clamp_selection(data, config);
            }
            _ => {}
        }
        Outcome::Continue
    }

    pub fn resume_after_folder_pick(
        &mut self,
        data: &mut ProjectData,
        group: &str,
        name: &str,
        path: Option<String>,
    ) {
        self.back_to_browse();
        match path {
            Some(path) => match actions::add_project_win(data, group, name, &path) {
                Ok(msg) => self.flash(msg),
                Err(e) => self.flash(e),
            },
            None => self.flash("已取消新增项目"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Group, Project};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn sample() -> ProjectData {
        ProjectData {
            groups: vec![
                Group {
                    name: "dev".into(),
                    projects: vec![Project::new("pcs", r"E:\pcs", "")],
                },
                Group {
                    name: "tools".into(),
                    projects: vec![],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn quit_on_q() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        assert!(matches!(
            app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
            Outcome::Quit
        ));
    }

    #[test]
    fn ctrl_c_quits() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        let mut ev = key(KeyCode::Char('c'));
        ev.modifiers = KeyModifiers::CONTROL;
        assert!(matches!(
            app.handle(ev, &mut data, &mut config),
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
}
