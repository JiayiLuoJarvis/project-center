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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RightPane {
    Projects,
    Connections,
    Trash,
    ConfigEnvs,
    ConfigTools { env: ConfigEnv },
    Commands { group: String, project_id: String },
}

impl RightPane {
    /// Esc 返回表：设置子页回设置弹窗；工具列表回环境；命令回项目。
    pub fn parent(&self) -> Option<RightPane> {
        match self {
            RightPane::Connections | RightPane::Trash | RightPane::ConfigEnvs => None,
            RightPane::ConfigTools { .. } => Some(RightPane::ConfigEnvs),
            RightPane::Commands { .. } => Some(RightPane::Projects),
            RightPane::Projects => None,
        }
    }

    pub fn is_settings_child(&self) -> bool {
        matches!(
            self,
            RightPane::Connections | RightPane::Trash | RightPane::ConfigEnvs
        )
    }

    pub fn settings_index(&self) -> usize {
        match self {
            RightPane::Connections => 0,
            RightPane::Trash => 1,
            RightPane::ConfigEnvs | RightPane::ConfigTools { .. } => 2,
            _ => 0,
        }
    }
}

pub const SETTINGS_ITEMS: [&str; 3] = ["远程连接", "回收站", "启动工具"];
pub const NEW_CONNECTION_LABEL: &str = "＋ 新建连接…";

#[derive(Clone, Debug)]
pub enum ListKind {
    CommandEnv {
        group: String,
        project_id: String,
        edit_index: Option<usize>,
        name: String,
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
    PickConnection {
        ids: Vec<String>,
        suspended: Box<SuspendedForm>,
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
    Connection {
        id: String,
    },
}

#[derive(Clone, Debug)]
pub enum ConfirmKind {
    DeleteProject {
        group: String,
        project_id: String,
    },
    DeleteGroup {
        name: String,
    },
    DeleteCommand {
        group: String,
        project_id: String,
        index: usize,
    },
    DeleteTool {
        env: ConfigEnv,
        index: usize,
    },
    DeleteConnection {
        id: String,
    },
    PurgeTrash {
        id: String,
    },
    EmptyTrash,
    ResetConfig,
}

/// 表单按钮动作；`target` 为要回填/清空的文本字段索引。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonAction {
    PickFolder { target: usize },
    PickFile { target: usize },
    ClearKey { target: usize },
}

/// 项目表单的查看/插入；其它表单种类固定为 Insert。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormInteraction {
    View,
    Insert,
}

#[derive(Clone, Debug)]
pub enum FormField {
    Text {
        label: String,
        value: String,
    },
    Password {
        label: String,
        value: String,
        empty_hint: String,
    },
    Button {
        label: String,
        action: ButtonAction,
    },
    /// 只读选择：Enter 打开选择器；Backspace/Ctrl+U 清空。
    Select {
        label: String,
        display: String,
        value: String,
    },
}

#[derive(Clone, Debug)]
pub struct SuspendedForm {
    pub title: String,
    pub fields: Vec<FormField>,
    pub focus: usize,
    /// 当前焦点文本/密码框内的字符光标下标。
    pub cursor: usize,
    pub kind: FormKind,
    pub interaction: FormInteraction,
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
    AddConnection {
        resume: Option<Box<SuspendedForm>>,
    },
    EditConnection {
        id: String,
    },
}

/// 项目表单固定布局；`as usize` 即字段下标。
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectField {
    Name = 0,
    Alias = 1,
    WinPath = 2,
    BrowseFolder = 3,
    WslPath = 4,
    Connection = 5,
    RemotePath = 6,
}

impl ProjectField {
    pub const COUNT: usize = 7;
}

impl From<ProjectField> for usize {
    fn from(field: ProjectField) -> usize {
        field as usize
    }
}

/// 连接表单固定布局（含编辑态「清除密钥」）。
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnField {
    Name = 0,
    User = 1,
    Host = 2,
    Port = 3,
    KeySource = 4,
    BrowseKey = 5,
    ClearKey = 6,
    Password = 7,
    KeyPass = 8,
}

impl ConnField {
    pub const COUNT: usize = 9;
}

impl From<ConnField> for usize {
    fn from(field: ConnField) -> usize {
        field as usize
    }
}

#[derive(Clone, Debug)]
pub enum Mode {
    Browse,
    SettingsMenu {
        selected: usize,
    },
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
        /// 当前焦点文本/密码框内的字符光标下标。
        cursor: usize,
        kind: FormKind,
        interaction: FormInteraction,
        error: Option<String>,
    },
    Confirm {
        message: String,
        kind: ConfirmKind,
    },
    SecretViewer {
        connection_id: String,
        pin_input: String,
        attempts: u8,
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
    PickFolder {
        target: usize,
    },
    PickFile {
        target: usize,
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
    pub quit_confirm: bool,
    /// 连接编辑表单点了「清除已导入密钥」：提交时移除密钥（Esc 取消即丢弃）。
    pub clear_key: bool,
}
