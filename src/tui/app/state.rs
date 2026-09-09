use crate::domain::models::Project;
use crate::launch::LaunchOption;
use crate::persist::ConfigEnv;

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
