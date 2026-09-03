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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftItem {
    Group(usize),
    Trash,
    Config,
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
pub enum FormField {
    Text { label: String, value: String },
    Button { label: String },
}

#[derive(Clone, Debug)]
pub enum FormKind {
    AddGroup,
    RenameGroup {
        old: String,
    },
    AddProject {
        group: String,
    },
    EditProject {
        group: String,
        project_id: String,
        old_name: String,
    },
    AddCommand {
        group: String,
        project_id: String,
        env: String,
    },
    EditCommand {
        group: String,
        project_id: String,
        index: usize,
        env: String,
    },
    AddTool {
        env: ConfigEnv,
    },
    EditTool {
        env: ConfigEnv,
        index: usize,
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
        filter: String,
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
    Form {
        title: String,
        fields: Vec<FormField>,
        focus: usize,
        kind: FormKind,
        error: Option<String>,
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
        project: Box<Project>,
        group: String,
        option: LaunchOption,
        exit_after: bool,
    },
    PickFolder,
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

    fn matches_filter(&self, text: &str) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        text.to_lowercase().contains(&self.filter.to_lowercase())
    }

    #[cfg(test)]
    pub fn filtered_group_indices(&self, data: &ProjectData) -> Vec<usize> {
        self.left_items(data)
            .into_iter()
            .filter_map(|item| match item {
                LeftItem::Group(i) => Some(i),
                _ => None,
            })
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
                            || self.matches_filter(&p.alias)
                            || self.matches_filter(&p.path)
                            || self.matches_filter(&p.linux_path())
                            || (!p.id.is_empty()
                                && p.id
                                    .get(..self.filter.len())
                                    .is_some_and(|prefix| self.matches_filter(prefix)))
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

    fn text_field(label: &str, value: impl Into<String>) -> FormField {
        FormField::Text {
            label: label.into(),
            value: value.into(),
        }
    }

    fn button_field(label: &str) -> FormField {
        FormField::Button {
            label: label.into(),
        }
    }

    fn open_form(&mut self, title: impl Into<String>, fields: Vec<FormField>, kind: FormKind) {
        self.mode = Mode::Form {
            title: title.into(),
            fields,
            focus: 0,
            kind,
            error: None,
        };
    }

    fn project_form_fields(name: &str, alias: &str, path: &str, wsl_path: &str) -> Vec<FormField> {
        vec![
            Self::text_field("项目名", name),
            Self::text_field("别名", alias),
            Self::text_field("Windows 路径", path),
            Self::button_field("浏览文件夹…"),
            Self::text_field("WSL 路径", wsl_path),
        ]
    }

    fn field_value(fields: &[FormField], index: usize) -> String {
        match fields.get(index) {
            Some(FormField::Text { value, .. }) => value.clone(),
            _ => String::new(),
        }
    }

    fn set_form_error(&mut self, error: impl Into<String>) {
        if let Mode::Form { error: slot, .. } = &mut self.mode {
            *slot = Some(error.into());
        }
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
            filter: String::new(),
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
            Mode::Form { .. } => self.handle_form(key, data, config),
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

    fn move_sel(&mut self, delta: i32, data: &ProjectData, config: &AppConfig) {
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

    fn on_enter(&mut self, data: &mut ProjectData, config: &mut AppConfig) -> Outcome {
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

    fn open_action_menu(&mut self, data: &ProjectData, config: &AppConfig) {
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

    fn on_add(&mut self, data: &ProjectData, _config: &AppConfig) {
        if self.focus == Focus::Groups
            && !self.left_is_trash(data, self.left_sel)
            && !self.left_is_config(data, self.left_sel)
        {
            self.open_form(
                "新增分组",
                vec![Self::text_field("分组名", ""), Self::text_field("别名", "")],
                FormKind::AddGroup,
            );
            return;
        }
        if self.focus == Focus::Groups {
            self.flash("请在分组区按 a 新增分组");
            return;
        }
        match self.right_pane.clone() {
            RightPane::Projects => {
                if let Some(gi) = self.left_is_group(data, self.left_sel) {
                    let group = data.groups[gi].name.clone();
                    self.open_form(
                        "新增项目",
                        Self::project_form_fields("", "", "", ""),
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

    fn on_edit(&mut self, data: &ProjectData, config: &AppConfig) {
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
                        Self::project_form_fields(&p.name, &p.alias, &p.path, &p.wsl_path),
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

    fn on_delete(&mut self, data: &ProjectData, config: &AppConfig) {
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

    fn on_move(&mut self, data: &ProjectData) {
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

    pub fn launch_filter_indices(
        options: &[LaunchOption],
        labels: &[String],
        filter: &str,
    ) -> Vec<usize> {
        if filter.is_empty() {
            return (0..options.len()).collect();
        }
        let q = filter.to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(i, opt)| {
                let label = labels.get(*i).map(|s| s.as_str()).unwrap_or("");
                label.to_lowercase().contains(&q)
                    || opt.tool_name.to_lowercase().contains(&q)
                    || opt.command.to_lowercase().contains(&q)
                    || opt.env.label().to_lowercase().contains(&q)
                    || opt.env.short_label().to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn handle_launch_picker(&mut self, key: KeyEvent, data: &ProjectData) -> Outcome {
        let Mode::LaunchPicker {
            group,
            project_id,
            options,
            labels,
            selected,
            filter,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        let indices = Self::launch_filter_indices(&options, &labels, &filter);
        let set = |filter: String, selected: usize| Mode::LaunchPicker {
            group: group.clone(),
            project_id: project_id.clone(),
            options: options.clone(),
            labels: labels.clone(),
            selected,
            filter,
        };
        match key.code {
            KeyCode::Esc => {
                if !filter.is_empty() {
                    self.mode = set(String::new(), 0);
                } else if self.short_session {
                    return Outcome::Quit;
                } else {
                    self.back_to_browse();
                }
            }
            KeyCode::Backspace => {
                let mut filter = filter;
                filter.pop();
                self.mode = set(filter, 0);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let n = indices.len();
                if n > 0 {
                    let pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
                    let next = indices[(pos + 1) % n];
                    self.mode = set(filter, next);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = indices.len();
                if n > 0 {
                    let pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
                    let next = indices[(pos + n - 1) % n];
                    self.mode = set(filter, next);
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut filter = filter;
                filter.push(c);
                let indices = Self::launch_filter_indices(&options, &labels, &filter);
                let selected = indices.first().copied().unwrap_or(0);
                self.mode = set(filter, selected);
            }
            KeyCode::Enter => {
                if indices.is_empty() {
                    return Outcome::Continue;
                }
                let idx = if indices.contains(&selected) {
                    selected
                } else {
                    indices[0]
                };
                if let Some(option) = options.get(idx).cloned()
                    && let Some(project) = actions::find_project_ref(data, &group, &project_id)
                {
                    let exit_after = self.exit_after_launch || self.short_session;
                    if !self.short_session {
                        self.back_to_browse();
                    }
                    return Outcome::Launch {
                        project: Box::new(project.clone()),
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
                            Self::project_form_fields(
                                &project.name,
                                &project.alias,
                                &project.path,
                                &project.wsl_path,
                            ),
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

    fn handle_form(
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
                if let Some(FormField::Text { value, .. }) = fields.get_mut(focus) {
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
                if let Some(FormField::Text { value, .. }) = fields.get_mut(focus) {
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

    fn submit_form(
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
                let path = Self::field_value(fields, 2);
                let wsl = Self::field_value(fields, 4);
                actions::add_project_paths(data, &group, &name, &alias, &path, &wsl)
            }
            FormKind::EditProject {
                group,
                project_id,
                old_name,
            } => {
                let name = Self::field_value(fields, 0);
                let alias = Self::field_value(fields, 1);
                let path = Self::field_value(fields, 2);
                let wsl = Self::field_value(fields, 4);
                let _ = project_id;
                actions::edit_project(data, &group, &old_name, &name, &alias, &path, &wsl)
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
                    let linux = crate::models::win_path_to_linux(&path);
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
}
