use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::{AppConfig, ConfigEnv};
use crate::menu::LaunchOption;
use crate::models::{Project, ProjectData};
use crate::secret;
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
    Text {
        label: String,
        value: String,
    },
    /// 密码输入：value 存明文，渲染为星号掩码。
    /// `empty_hint`：value 为空时显示的占位（如「未设置」/「已保存，留空不改」）。
    Password {
        label: String,
        value: String,
        empty_hint: String,
    },
    Button {
        label: String,
    },
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
    /// 查看项目的保存密码/口令：PIN 验证通过后显示明文。
    SecretViewer {
        group: String,
        project_id: String,
        pin_input: String,
        attempts: u8,
        /// 验证通过后的 (登录密码, 私钥口令)；未验证为 None。
        revealed: Option<(Option<String>, Option<String>)>,
        error: Option<String>,
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
    pub quit_confirm: bool,
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

    fn pop_right_pane(&mut self, data: &ProjectData, config: &AppConfig) {
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

    fn text_field(label: &str, value: impl Into<String>) -> FormField {
        FormField::Text {
            label: label.into(),
            value: value.into(),
        }
    }

    fn password_field(label: &str) -> FormField {
        Self::password_field_with_hint(label, "（未设置）")
    }

    fn password_field_with_hint(label: &str, empty_hint: &str) -> FormField {
        FormField::Password {
            label: label.into(),
            value: String::new(),
            empty_hint: empty_hint.into(),
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

    /// 基础表单字段索引：0 项目名 / 1 别名 / 2 Windows 路径 / 3 浏览按钮 /
    /// 4 WSL 路径 / 5 登录用户 / 6 主机 / 7 端口 / 8 远程 Linux 路径。
    fn project_form_fields(name: &str, alias: &str, path: &str, wsl_path: &str) -> Vec<FormField> {
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
    fn project_add_fields() -> Vec<FormField> {
        let mut fields = Self::project_form_fields("", "", "", "");
        fields.push(Self::text_field("私钥来源路径（SSH 项目，可选）", ""));
        fields.push(Self::password_field("登录密码（SSH 项目，可选）"));
        fields.push(Self::password_field("私钥口令（SSH 项目，可选）"));
        fields
    }

    /// 编辑表单：SSH 项目用 SSH 专用字段；普通项目附 SSH 转换字段。
    /// SSH 表单字段索引：0 登录用户 / 1 主机 / 2 端口 / 3 远程路径 /
    /// 4 密钥来源 / 5 密码 / 6 口令。
    fn project_edit_fields(p: &Project) -> Vec<FormField> {
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

    fn field_value(fields: &[FormField], index: usize) -> String {
        match fields.get(index) {
            Some(FormField::Text { value, .. } | FormField::Password { value, .. }) => {
                value.clone()
            }
            _ => String::new(),
        }
    }

    /// 拼接 SSH 目标：`[user@]host[:port]`。user/host 禁止含 `@`（防拼出
    /// `a@b@c` 这类坏目标）；port 非空时必须为纯数字，留空即 ssh 默认 22。
    fn compose_ssh_target(user: &str, host: &str, port: &str) -> Result<String, String> {
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
    fn split_ssh_target(target: &str) -> (String, String, String) {
        let target = target.trim();
        let (user, rest) = match target.split_once('@') {
            Some((u, r)) => (u, r),
            None => ("", target),
        };
        let (host, port) = crate::launcher::split_host_port(rest);
        (
            user.to_string(),
            host.to_string(),
            port.unwrap_or_default().to_string(),
        )
    }

    fn set_form_error(&mut self, error: impl Into<String>) {
        if let Mode::Form { error: slot, .. } = &mut self.mode {
            *slot = Some(error.into());
        }
    }

    /// 打开「查看保存的秘密」对话框（仅 SSH 项目且已存秘密、已设 PIN）。
    fn open_secret_viewer(&mut self, data: &ProjectData, config: &AppConfig) {
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

    fn handle_secret_viewer(
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

    fn handle_browse(
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

    fn handle_filter(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Browse;
                self.clamp_selection(data, config);
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                // 清空过滤串前，把过滤视图中的选中位置换算成完整列表下标，
                // 避免清空后高亮错位。
                match self.focus {
                    Focus::Groups => {
                        let pos = self.map_left_selection(data);
                        self.filter.clear();
                        self.left_sel = pos;
                    }
                    Focus::Projects => {
                        let pi = self.map_right_selection(data, config);
                        self.filter.clear();
                        self.right_sel = pi.unwrap_or(0);
                    }
                }
                self.clamp_selection(data, config);
                return self.on_enter(data, config);
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.reselect_after_filter(data, config);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_sel(1, data, config);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_sel(-1, data, config);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.reselect_after_filter(data, config);
            }
            _ => {}
        }
        Outcome::Continue
    }

    /// 过滤视图中左栏选中项在完整列表中的下标；
    /// 回收站与配置固定在末尾，无匹配时回退到回收站。
    fn map_left_selection(&self, data: &ProjectData) -> usize {
        match self.left_items(data).get(self.left_sel) {
            Some(LeftItem::Group(gi)) => *gi,
            Some(LeftItem::Config) => data.groups.len() + 1,
            _ => data.groups.len(),
        }
    }

    /// 过滤视图中右栏选中项在完整列表中的下标；
    /// 环境列表不参与过滤，保持原选中。
    fn map_right_selection(&self, data: &ProjectData, config: &AppConfig) -> Option<usize> {
        match self.right_pane.clone() {
            RightPane::Projects => self.left_is_group(data, self.left_sel).and_then(|gi| {
                self.filtered_project_indices(data, gi)
                    .get(self.right_sel)
                    .copied()
            }),
            RightPane::Trash => self
                .filtered_trash_indices(data)
                .get(self.right_sel)
                .copied(),
            RightPane::ConfigTools { env } => self
                .filtered_tool_indices(config, env)
                .get(self.right_sel)
                .copied(),
            RightPane::Commands { group, project_id } => {
                actions::find_project_ref(data, &group, &project_id).and_then(|p| {
                    self.filtered_command_indices(p)
                        .get(self.right_sel)
                        .copied()
                })
            }
            RightPane::ConfigEnvs => Some(self.right_sel),
        }
    }

    /// 过滤串变化后，把选中项重定位到首个匹配结果。
    fn reselect_after_filter(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            self.left_sel = 0;
            self.right_sel = 0;
            self.sync_right_pane(data);
        } else {
            self.right_sel = 0;
        }
        self.clamp_selection(data, config);
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
    use crate::models::{DeletedItem, Group, Project, ProjectCommand};

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

    fn ctrl_c() -> KeyEvent {
        let mut ev = key(KeyCode::Char('c'));
        ev.modifiers = KeyModifiers::CONTROL;
        ev
    }

    #[test]
    fn quit_on_q() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        assert!(matches!(
            app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
            Outcome::Continue
        ));
        assert!(app.quit_confirm);
        assert!(matches!(
            app.handle(key(KeyCode::Char('y')), &mut data, &mut config),
            Outcome::Quit
        ));
        assert!(!app.quit_confirm);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        assert!(matches!(
            app.handle(ctrl_c(), &mut data, &mut config),
            Outcome::Continue
        ));
        assert!(app.quit_confirm);
        assert!(matches!(
            app.handle(ctrl_c(), &mut data, &mut config),
            Outcome::Quit
        ));
    }

    #[test]
    fn quit_confirm_cancel_keeps_mode() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.mode = Mode::Confirm {
            message: "确认清空回收站？ y/N".into(),
            kind: ConfirmKind::EmptyTrash,
        };
        app.handle(ctrl_c(), &mut data, &mut config);
        assert!(app.quit_confirm);
        assert!(matches!(
            app.handle(key(KeyCode::Esc), &mut data, &mut config),
            Outcome::Continue
        ));
        assert!(!app.quit_confirm);
        assert!(matches!(
            app.mode,
            Mode::Confirm {
                kind: ConfirmKind::EmptyTrash,
                ..
            }
        ));
    }

    #[test]
    fn help_q_closes_without_quit_confirm() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.mode = Mode::Help;
        assert!(matches!(
            app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
            Outcome::Continue
        ));
        assert!(!app.quit_confirm);
        assert!(matches!(app.mode, Mode::Browse));
    }

    #[test]
    fn filter_q_is_literal() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.mode = Mode::Filter;
        app.handle(key(KeyCode::Char('q')), &mut data, &mut config);
        assert_eq!(app.filter, "q");
        assert!(!app.quit_confirm);
        assert!(matches!(app.mode, Mode::Filter));
    }

    #[test]
    fn quit_confirm_second_q_quits() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.handle(key(KeyCode::Char('q')), &mut data, &mut config);
        assert!(matches!(
            app.handle(key(KeyCode::Char('q')), &mut data, &mut config),
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
    fn filter_reselects_first_match() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.left_sel = 2; // 配置项
        app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
        assert_eq!(app.left_sel, 0);
        assert_eq!(app.left_is_group(&data, 0), Some(1)); // tools
        app.handle(key(KeyCode::Backspace), &mut data, &mut config);
        assert_eq!(app.left_is_group(&data, 0), Some(0)); // dev
    }

    #[test]
    fn filter_enter_opens_group() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.left_sel = 2;
        app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(matches!(app.mode, Mode::Browse));
        assert_eq!(app.focus, Focus::Projects);
        assert_eq!(app.left_is_group(&data, app.left_sel), Some(1));
        assert!(matches!(app.right_pane, RightPane::Projects));
    }

    #[test]
    fn filter_resets_project_selection() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.right_sel = 5;
        app.mode = Mode::Filter;
        app.handle(key(KeyCode::Char('p')), &mut data, &mut config);
        assert_eq!(app.right_sel, 0);
        assert!(matches!(app.mode, Mode::Filter));
    }

    #[test]
    fn filter_enter_trash_maps_selection() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        data.trash = vec![
            DeletedItem {
                id: "id-alpha".into(),
                kind: "project".into(),
                group: "dev".into(),
                name: "alpha".into(),
                ..Default::default()
            },
            DeletedItem {
                id: "id-beta".into(),
                kind: "project".into(),
                group: "dev".into(),
                name: "beta".into(),
                ..Default::default()
            },
        ];
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.right_pane = RightPane::Trash;
        app.mode = Mode::Filter;
        app.handle(key(KeyCode::Char('b')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(matches!(app.mode, Mode::ActionMenu { .. }));
        match &app.mode {
            Mode::ActionMenu {
                kind: ActionKind::TrashItem { id },
                ..
            } => assert_eq!(id, "id-beta"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn filter_enter_keeps_tail_item_position() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('t')), &mut data, &mut config);
        // 过滤视图 [tools, 回收站, 配置]，移动到配置
        app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('j')), &mut data, &mut config);
        assert!(app.left_is_config(&data, app.left_sel));
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(app.left_is_config(&data, app.left_sel));
        assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
    }

    #[test]
    fn filter_enter_zero_match_lands_on_trash() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.handle(key(KeyCode::Char('/')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('z')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(app.left_is_trash(&data, app.left_sel));
        assert!(matches!(app.right_pane, RightPane::Trash));
    }

    #[test]
    fn filter_enter_project_opens_launch_picker() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.mode = Mode::Filter;
        app.handle(key(KeyCode::Char('p')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(matches!(app.mode, Mode::LaunchPicker { .. }));
    }

    #[test]
    fn filter_enter_tool_maps_selection() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.right_pane = RightPane::ConfigTools {
            env: ConfigEnv::Wsl,
        };
        app.mode = Mode::Filter;
        // 默认 WSL 工具为 [opencode, cursor-agent]，"u" 只匹配真实下标 1 的 cursor-agent
        app.handle(key(KeyCode::Char('u')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        match &app.mode {
            Mode::ActionMenu {
                kind: ActionKind::ConfigTool { index, .. },
                ..
            } => assert_eq!(*index, 1),
            _ => unreachable!(),
        }
    }

    #[test]
    fn filter_enter_command_maps_selection() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        data.groups[0].projects[0].commands = vec![
            ProjectCommand::new("build", "wsl", "cargo build"),
            ProjectCommand::new("serve", "ide", "code ."),
        ];
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.right_pane = RightPane::Commands {
            group: "dev".into(),
            project_id: data.groups[0].projects[0].id.clone(),
        };
        app.mode = Mode::Filter;
        // "se" 只匹配真实下标 1 的 serve
        app.handle(key(KeyCode::Char('s')), &mut data, &mut config);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(app.filter.is_empty());
        match &app.mode {
            Mode::ActionMenu {
                kind: ActionKind::Command { index, .. },
                ..
            } => assert_eq!(*index, 1),
            _ => unreachable!(),
        }
    }

    #[test]
    fn esc_pops_config_tools_to_envs() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.left_sel = app.left_count(&data) - 1;
        app.sync_right_pane(&data);
        app.focus = Focus::Projects;
        assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
        app.right_sel = 0;
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(matches!(
            app.right_pane,
            RightPane::ConfigTools {
                env: ConfigEnv::Wsl
            }
        ));
        app.handle(key(KeyCode::Esc), &mut data, &mut config);
        assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
        assert_eq!(app.right_sel, 0);
    }

    #[test]
    fn esc_clears_filter_before_popping_config_tools() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.left_sel = app.left_count(&data) - 1;
        app.sync_right_pane(&data);
        app.focus = Focus::Projects;
        app.right_sel = 1;
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        assert!(matches!(
            app.right_pane,
            RightPane::ConfigTools {
                env: ConfigEnv::PowerShell
            }
        ));
        app.filter = "x".into();
        app.handle(key(KeyCode::Esc), &mut data, &mut config);
        assert!(app.filter.is_empty());
        assert!(matches!(
            app.right_pane,
            RightPane::ConfigTools {
                env: ConfigEnv::PowerShell
            }
        ));
        app.handle(key(KeyCode::Esc), &mut data, &mut config);
        assert!(matches!(app.right_pane, RightPane::ConfigEnvs));
        assert_eq!(app.right_sel, 1);
    }

    #[test]
    fn esc_pops_commands_to_projects() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        let id = data.groups[0].projects[0].id.clone();
        app.right_pane = RightPane::Commands {
            group: "dev".into(),
            project_id: id,
        };
        app.focus = Focus::Projects;
        app.right_sel = 0;
        app.handle(key(KeyCode::Esc), &mut data, &mut config);
        assert!(matches!(app.right_pane, RightPane::Projects));
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
    fn add_group_when_no_groups_left() {
        let mut data = ProjectData::default();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        assert!(app.left_is_trash(&data, app.left_sel));
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        assert!(matches!(
            app.mode,
            Mode::Form {
                kind: FormKind::AddGroup,
                ..
            }
        ));
    }

    #[test]
    fn add_group_when_left_sel_on_trash() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Groups;
        app.left_sel = data.groups.len();
        app.sync_right_pane(&data);
        assert!(app.left_is_trash(&data, app.left_sel));
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        assert!(matches!(
            app.mode,
            Mode::Form {
                kind: FormKind::AddGroup,
                ..
            }
        ));
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

    #[test]
    fn add_ssh_project_via_form() {
        // 提交会经 actions::save_data 写真实数据根：串行 + 把 APPDATA 指向
        // 临时目录，避免污染 projects.json（并发下 rename 也可能冲突导致保存失败）。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        // 字段 0：项目名
        for c in "srv".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // Tab 到字段 5（登录用户）
        for _ in 0..5 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 6：主机
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "172.16.14.10".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 7：端口预填 22，直接留用
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        // 字段 8：远程路径
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "/opt/x".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        let p = &data.groups[0].projects[1];
        assert!(p.is_ssh_project());
        assert_eq!(p.ssh_target, "abc@172.16.14.10:22");
        assert_eq!(p.path, "/opt/x");
        assert!(p.wsl_path.is_empty());

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }

    #[cfg(windows)]
    #[test]
    fn add_ssh_project_with_secrets_via_form() {
        // 新增表单直接携带密钥/密码/口令：一次提交完成秘密保存（设计 §9）。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        // 准备一个可导入的私钥源文件
        let key_source = temp_appdata.join("source_key");
        std::fs::write(&key_source, "-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();

        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        for c in "srv".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // Tab 到字段 5（登录用户）
        for _ in 0..5 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 6：主机
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "h".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 7 端口预填 22 留用，字段 8 远程路径留空：Tab 到字段 9（私钥来源路径）
        for _ in 0..3 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in key_source.to_string_lossy().chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 10：登录密码
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "pw123".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 11：私钥口令留空，直接提交
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        let p = &data.groups[0].projects[1];
        assert!(p.is_ssh_project());
        assert_eq!(p.ssh_target, "abc@h:22");
        assert!(!p.ssh_password_enc.is_empty(), "密码应已加密保存");
        assert_eq!(
            crate::secret::unprotect(&p.ssh_password_enc).unwrap(),
            "pw123"
        );
        assert!(!p.ssh_key_file.is_empty(), "密钥应已导入 sidecar");
        assert_eq!(
            crate::secret::read_key_file(&p.ssh_key_file).unwrap(),
            "-----BEGIN OPENSSH PRIVATE KEY-----"
        );
        assert_eq!(p.ssh_key_path, key_source.to_string_lossy());
        // 普通项目路径未受影响
        assert!(p.wsl_path.is_empty());

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }

    #[test]
    fn edit_ssh_project_uses_ssh_form() {
        let mut data = sample();
        {
            let p = &mut data.groups[0].projects[0];
            p.ssh_target = "abc@h".into();
            p.path = "/opt/x".into();
            p.ssh_password_enc = "PWENC".into();
        }
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        match &app.mode {
            Mode::Form { fields, .. } => {
                assert_eq!(fields.len(), 7, "SSH 编辑表单 7 字段");
                assert_eq!(App::field_value(fields, 0), "abc");
                assert_eq!(App::field_value(fields, 1), "h");
                assert_eq!(App::field_value(fields, 2), "");
                assert_eq!(App::field_value(fields, 3), "/opt/x");
                assert!(App::field_value(fields, 5).is_empty(), "密码初始为空");
                // empty_hint 标明已存/未存；值行不再硬编码「未设置」。
                let FormField::Password {
                    label, empty_hint, ..
                } = &fields[5]
                else {
                    panic!("字段 5 应为 Password，实际 {:?}", fields.get(5));
                };
                assert_eq!(label, "登录密码");
                assert!(empty_hint.contains("已保存"), "实际 {empty_hint}");
                let FormField::Password {
                    label, empty_hint, ..
                } = &fields[6]
                else {
                    panic!("字段 6 应为 Password，实际 {:?}", fields.get(6));
                };
                assert_eq!(label, "私钥口令");
                assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
            }
            other => panic!("expected SSH form, got {other:?}"),
        }
    }

    #[test]
    fn edit_ssh_form_labels_unsaved_when_no_secrets() {
        // 无任何秘密的 SSH 项目：密码/口令 empty_hint 均标「未设置」。
        let mut data = sample();
        {
            let p = &mut data.groups[0].projects[0];
            p.ssh_target = "abc@h".into();
        }
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        match &app.mode {
            Mode::Form { fields, .. } => {
                let FormField::Password { empty_hint, .. } = &fields[5] else {
                    panic!("字段 5 应为 Password，实际 {:?}", fields.get(5));
                };
                assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
                let FormField::Password { empty_hint, .. } = &fields[6] else {
                    panic!("字段 6 应为 Password，实际 {:?}", fields.get(6));
                };
                assert!(empty_hint.contains("未设置"), "实际 {empty_hint}");
            }
            other => panic!("expected SSH form, got {other:?}"),
        }
    }

    #[test]
    fn edit_normal_project_shows_ssh_conversion_fields() {
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        match &app.mode {
            Mode::Form { fields, .. } => {
                assert_eq!(fields.len(), 9, "普通编辑表单含 SSH 转换字段");
                assert_eq!(App::field_value(fields, 5), "");
                assert_eq!(App::field_value(fields, 6), "");
                assert_eq!(App::field_value(fields, 7), "22", "端口默认 22");
            }
            other => panic!("expected form, got {other:?}"),
        }
    }

    #[test]
    fn compose_ssh_target_cases() {
        assert_eq!(
            App::compose_ssh_target("abc", "h", "22").unwrap(),
            "abc@h:22"
        );
        assert_eq!(App::compose_ssh_target("abc", "h", "").unwrap(), "abc@h");
        assert_eq!(App::compose_ssh_target("", "h", "").unwrap(), "h");
        assert_eq!(
            App::compose_ssh_target(" abc ", " h ", " 22 ").unwrap(),
            "abc@h:22"
        );
        assert!(App::compose_ssh_target("a@b", "h", "").is_err());
        assert!(App::compose_ssh_target("abc", "a@h", "").is_err());
        assert!(App::compose_ssh_target("abc", "h", "22x").is_err());
    }

    #[test]
    fn split_ssh_target_roundtrip() {
        assert_eq!(
            App::split_ssh_target("abc@h:2222"),
            ("abc".to_string(), "h".to_string(), "2222".to_string())
        );
        assert_eq!(
            App::split_ssh_target("abc@h"),
            ("abc".to_string(), "h".to_string(), String::new())
        );
        assert_eq!(
            App::split_ssh_target("h"),
            (String::new(), "h".to_string(), String::new())
        );
        // 往返恒等
        for t in ["abc@h:2222", "abc@h", "h", "h:22"] {
            let (u, h, p) = App::split_ssh_target(t);
            assert_eq!(App::compose_ssh_target(&u, &h, &p).unwrap(), t);
        }
    }

    #[test]
    fn add_ssh_project_custom_port_via_form() {
        // 端口预填 22：Backspace 清空后可填自定义端口。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        for c in "srvp".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 5：登录用户
        for _ in 0..5 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 6：主机
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "h".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 7：清空预填 22，改填 2222
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        app.handle(key(KeyCode::Backspace), &mut data, &mut config);
        app.handle(key(KeyCode::Backspace), &mut data, &mut config);
        for c in "2222".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        let p = &data.groups[0].projects[1];
        assert!(p.is_ssh_project());
        assert_eq!(p.ssh_target, "abc@h:2222");

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }

    #[test]
    fn edit_ssh_project_save_unchanged_keeps_target() {
        // 编辑表单零修改直接保存：反拆再拼接必须恒等，不改写 ssh_target。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        let mut data = sample();
        {
            let p = &mut data.groups[0].projects[0];
            p.ssh_target = "abc@h:2222".into();
            p.path = "/opt/x".into();
        }
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        assert_eq!(data.groups[0].projects[0].ssh_target, "abc@h:2222");

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }

    #[test]
    fn add_ssh_project_rejects_at_in_host() {
        // 主机含 @ 时拼接失败：表单报错不关闭，不产生项目。
        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        for c in "srv".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        for _ in 0..5 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "a@h".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Form { error, .. } => {
                assert!(error.as_ref().is_some_and(|e| e.contains('@')));
            }
            other => panic!("应留在表单并报错，实际 {other:?}"),
        }
        assert_eq!(data.groups[0].projects.len(), 1);
    }

    #[test]
    fn edit_normal_project_converts_to_ssh_via_form() {
        // 普通项目编辑表单填写用户/主机即转为 SSH 项目（第三条保存路径）。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('e')), &mut data, &mut config);
        // 字段 5：登录用户
        for _ in 0..5 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 6：主机
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "h".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 7 端口预填 22 留用；字段 8 远程路径
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "/opt/r".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        let p = &data.groups[0].projects[0];
        assert!(p.is_ssh_project());
        assert_eq!(p.ssh_target, "abc@h:22");
        assert_eq!(p.path, "/opt/r");

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }

    #[test]
    fn add_project_ignores_ssh_user_when_host_empty() {
        // 主机留空时即便填了登录用户也按普通项目保存（user 字段静默忽略）。
        let _guard = crate::store::test_env::lock_appdata();
        let temp_appdata =
            std::env::temp_dir().join(format!("pcs_tui_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_appdata).unwrap();
        let _appdata = crate::store::test_env::AppdataGuard::redirect(&temp_appdata);

        let mut data = sample();
        let mut config = AppConfig::defaults();
        let mut app = App::new(&data);
        app.focus = Focus::Projects;
        app.left_sel = 0;
        app.sync_right_pane(&data);
        app.handle(key(KeyCode::Char('a')), &mut data, &mut config);
        for c in "srv".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // Tab 到字段 4（WSL 路径，普通项目路径二选一）
        for _ in 0..4 {
            app.handle(key(KeyCode::Tab), &mut data, &mut config);
        }
        for c in "/srv".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        // 字段 5：登录用户（主机留空，应被忽略）
        app.handle(key(KeyCode::Tab), &mut data, &mut config);
        for c in "abc".chars() {
            app.handle(key(KeyCode::Char(c)), &mut data, &mut config);
        }
        app.handle(key(KeyCode::Enter), &mut data, &mut config);
        match &app.mode {
            Mode::Browse => {}
            other => panic!("提交成功应回浏览模式，实际 {other:?}"),
        }
        assert_eq!(data.groups[0].projects.len(), 2);
        let p = &data.groups[0].projects[1];
        assert!(!p.is_ssh_project());
        assert!(p.ssh_target.is_empty());
        assert_eq!(p.wsl_path, "/srv");

        let _ = std::fs::remove_dir_all(&temp_appdata);
    }
}
