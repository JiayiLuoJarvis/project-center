use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// 可选别名，便于英文过滤 / CLI 查找；空表示无。
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "wslPath", default)]
    pub wsl_path: String,
    #[serde(rename = "defaultTool", default)]
    pub default_tool: String,
    /// 项目级自定义命令。
    #[serde(rename = "commands", default)]
    pub commands: Vec<ProjectCommand>,
    /// SSH 目标（如 `user@host` 或 `user@host:2222`）；非空即 SSH 远程项目。
    #[serde(rename = "sshTarget", default)]
    pub ssh_target: String,
    /// 私钥 sidecar 文件的相对路径（相对数据根，如 `keys/<uuid>.key`）；
    /// 非空且文件存在即有密钥。空表示未导入密钥。
    #[serde(rename = "sshKeyFile", default)]
    pub ssh_key_file: String,
    /// 私钥导入来源路径，仅显示用，明文无秘密。
    #[serde(rename = "sshKeyPath", default)]
    pub ssh_key_path: String,
    /// 登录密码 DPAPI 密文（base64）；空表示未保存。
    #[serde(rename = "sshPasswordEnc", default)]
    pub ssh_password_enc: String,
    /// 私钥口令 DPAPI 密文（base64）；空表示未保存。
    #[serde(rename = "sshKeyPassEnc", default)]
    pub ssh_key_pass_enc: String,
}

impl Project {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<String>,
        wsl_path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            alias: String::new(),
            path: path.into(),
            wsl_path: wsl_path.into(),
            default_tool: String::new(),
            commands: Vec::new(),
            ssh_target: String::new(),
            ssh_key_file: String::new(),
            ssh_key_path: String::new(),
            ssh_password_enc: String::new(),
            ssh_key_pass_enc: String::new(),
        }
    }

    /// 是否 SSH 远程项目：`sshTarget` 非空即判定，与本地/WSL 路径互斥。
    pub fn is_ssh_project(&self) -> bool {
        !self.ssh_target.trim().is_empty()
    }

    /// 把项目切换为 SSH 项目（add 入口使用）。
    pub fn with_ssh_target(mut self, target: impl Into<String>) -> Self {
        self.ssh_target = target.into();
        self
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = alias.into();
        self
    }

    /// 为旧数据中缺失 id 的项目补生成 id。
    pub fn ensure_id(&mut self) {
        if self.id.trim().is_empty() {
            self.id = Uuid::new_v4().to_string();
        }
    }

    /// 无条件重新生成 id（回收站恢复时遇到 id 冲突）。
    pub fn regenerate_id(&mut self) {
        self.id = Uuid::new_v4().to_string();
    }

    /// 计算可用于 WSL 的 Linux 路径：优先手填 wsl_path；
    /// 否则 path 本身是 Linux 路径则归一化；否则转换。
    pub fn linux_path(&self) -> String {
        let manual = self.wsl_path.trim();
        if !manual.is_empty() {
            return manual.to_string();
        }
        if is_linux_path(&self.path) {
            return normalize(&self.path);
        }
        win_path_to_linux(&self.path)
    }

    /// 是否存在可用于 PowerShell / VS Code 的 Windows 路径。
    pub fn has_windows_path(&self) -> bool {
        !self.path.trim().is_empty() && !is_linux_path(&self.path)
    }

    /// 是否设置了默认启动工具。
    pub fn has_default_tool(&self) -> bool {
        !self.default_tool.trim().is_empty()
    }
}

/// 项目自定义命令：项目级覆盖启动工具，命令原样交给 shell。
/// `env` 取值 `wsl` / `powershell` / `ide`（解析大小写不敏感），空或非法视为 `ide`。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectCommand {
    pub name: String,
    pub env: String,
    pub command: String,
}

impl ProjectCommand {
    pub fn new(
        name: impl Into<String>,
        env: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            env: env.into(),
            command: command.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    /// 可选别名，便于英文过滤 / CLI 查找；空表示无。
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            alias: String::new(),
            projects: Vec::new(),
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = alias.into();
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub trash: Vec<DeletedItem>,
    /// 删除失败的私钥 sidecar 相对路径，每次启动重试删除（成功即摘除）。
    #[serde(rename = "pendingKeyDeletes", default)]
    pub pending_key_deletes: Vec<String>,
}

/// 回收站中的一条记录：被删项目或分组在删除时刻的完整快照。
/// `kind` 为 `project` 时使用路径字段，`group` 时使用 `projects`。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletedItem {
    /// 项目项为被删项目的 id；分组项为独立生成的 id。
    /// 缺省时由 Store::load 回填（`#[serde(default)]` 保证旧数据可解析）。
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// 项目项的原分组名；分组项为空。
    #[serde(default)]
    pub group: String,
    pub name: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "wslPath", default)]
    pub wsl_path: String,
    #[serde(rename = "defaultTool", default)]
    pub default_tool: String,
    /// 项目项的自定义命令快照。
    #[serde(rename = "commands", default)]
    pub commands: Vec<ProjectCommand>,
    /// SSH 目标快照。
    #[serde(rename = "sshTarget", default)]
    pub ssh_target: String,
    /// 私钥 sidecar 相对路径快照。
    #[serde(rename = "sshKeyFile", default)]
    pub ssh_key_file: String,
    /// 私钥来源路径快照。
    #[serde(rename = "sshKeyPath", default)]
    pub ssh_key_path: String,
    /// 登录密码密文快照。
    #[serde(rename = "sshPasswordEnc", default)]
    pub ssh_password_enc: String,
    /// 私钥口令密文快照。
    #[serde(rename = "sshKeyPassEnc", default)]
    pub ssh_key_pass_enc: String,
    /// 分组项的完整项目数组。
    #[serde(default)]
    pub projects: Vec<Project>,
    /// 删除时刻的 unix 秒。
    #[serde(rename = "deletedAt", default)]
    pub deleted_at: i64,
}

impl DeletedItem {
    /// 空 id 时生成 uuid（兼容旧数据/手工编辑的回收站项）。
    pub fn ensure_id(&mut self) {
        if self.id.trim().is_empty() {
            self.id = uuid::Uuid::new_v4().to_string();
        }
    }

    /// 项目删除快照：id 沿用被删项目，恢复时项目 id 不变。
    pub fn from_project(project: &Project, group: &str, deleted_at: i64) -> Self {
        Self {
            id: project.id.clone(),
            kind: "project".into(),
            group: group.to_string(),
            name: project.name.clone(),
            alias: project.alias.clone(),
            path: project.path.clone(),
            wsl_path: project.wsl_path.clone(),
            default_tool: project.default_tool.clone(),
            commands: project.commands.clone(),
            ssh_target: project.ssh_target.clone(),
            ssh_key_file: project.ssh_key_file.clone(),
            ssh_key_path: project.ssh_key_path.clone(),
            ssh_password_enc: project.ssh_password_enc.clone(),
            ssh_key_pass_enc: project.ssh_key_pass_enc.clone(),
            projects: Vec::new(),
            deleted_at,
        }
    }

    /// 分组删除快照：整组（含内部项目）进回收站，分组项 id 独立生成。
    pub fn from_group(group: &Group, deleted_at: i64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: "group".into(),
            group: String::new(),
            name: group.name.clone(),
            alias: group.alias.clone(),
            path: String::new(),
            wsl_path: String::new(),
            default_tool: String::new(),
            commands: Vec::new(),
            ssh_target: String::new(),
            ssh_key_file: String::new(),
            ssh_key_path: String::new(),
            ssh_password_enc: String::new(),
            ssh_key_pass_enc: String::new(),
            projects: group.projects.clone(),
            deleted_at,
        }
    }

    pub fn is_group(&self) -> bool {
        self.kind == "group"
    }
}

/// 当前 unix 秒。
pub fn current_unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// 天数（自 epoch 起）转公历年月日（Hinnant civil_from_days 算法，UTC）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// unix 秒转 UTC 时间分量 (年, 月, 日, 时, 分, 秒)。
fn utc_components(ts: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (secs / 3600) as u32,
        ((secs % 3600) / 60) as u32,
        (secs % 60) as u32,
    )
}

/// 本地时间分量：Windows 下按系统时区换算，失败回退 UTC。
#[cfg(windows)]
fn local_components(ts: i64) -> Option<(i64, u32, u32, u32, u32, u32)> {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    let (year, month, day, hour, minute, second) = utc_components(ts);
    let utc = SYSTEMTIME {
        wYear: year as u16,
        wMonth: month as u16,
        wDayOfWeek: 0,
        wDay: day as u16,
        wHour: hour as u16,
        wMinute: minute as u16,
        wSecond: second as u16,
        wMilliseconds: 0,
    };
    let local = unsafe {
        let mut local = std::mem::zeroed::<SYSTEMTIME>();
        let ok = SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local);
        if ok == 0 {
            return None;
        }
        local
    };
    Some((
        local.wYear as i64,
        local.wMonth as u32,
        local.wDay as u32,
        local.wHour as u32,
        local.wMinute as u32,
        local.wSecond as u32,
    ))
}

#[cfg(not(windows))]
fn local_components(_ts: i64) -> Option<(i64, u32, u32, u32, u32, u32)> {
    None
}

/// unix 秒 -> 本地日期 `2026-08-12`。
pub fn format_date(ts: i64) -> String {
    let (year, month, day, ..) = local_components(ts).unwrap_or_else(|| utc_components(ts));
    format!("{year:04}-{month:02}-{day:02}")
}

/// unix 秒 -> UTC 时间戳 `YYYYMMDD-HHMMSS`：单调、与本地时区/DST 无关，
/// 供备份文件命名（本地时间在夏令时回拨时会出现时间倒流导致轮转误删）。
pub fn format_utc_compact(ts: i64) -> String {
    let (year, month, day, hour, minute, second) = utc_components(ts);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// 归一化路径：`\` -> `/`、去除首尾空白、折叠连续斜杠。
pub fn normalize(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let mut prev_slash = false;
    for ch in p.replace('\\', "/").trim().chars() {
        if ch == '/' {
            if !prev_slash {
                out.push('/');
            }
            prev_slash = true;
        } else {
            out.push(ch);
            prev_slash = false;
        }
    }
    out
}

pub fn is_linux_path(p: &str) -> bool {
    normalize(p).starts_with('/')
}

/// WSL 路径校验：接受 `/...`、`~`、`~/...`。
pub fn is_wsl_path(p: &str) -> bool {
    let t = normalize(p);
    is_linux_path(&t) || t == "~" || t.starts_with("~/")
}

/// `/mnt/e/dev/foo` -> `E:\dev\foo`；不匹配返回 None。
#[allow(dead_code)]
pub fn linux_path_to_win(p: &str) -> Option<String> {
    let t = normalize(p);
    let rest = t.strip_prefix("/mnt/")?;
    let drive = rest.chars().next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    let tail = &rest[drive.len_utf8()..];
    if !tail.starts_with('/') {
        return None;
    }
    Some(format!(
        "{}:{}",
        drive.to_ascii_uppercase(),
        tail.replace('/', "\\")
    ))
}

/// `E:\dev\foo` -> `/mnt/e/dev/foo`；Linux 路径原样透传。
pub fn win_path_to_linux(p: &str) -> String {
    let t = normalize(p);
    if t.is_empty() || t.starts_with('/') {
        return t;
    }
    let mut chars = t.chars();
    let first = chars.next().unwrap_or_default();
    if first.is_ascii_alphabetic() && chars.next() == Some(':') {
        let rest = &t[2..];
        return format!("/mnt/{}{}", first.to_ascii_lowercase(), rest);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_slashes() {
        assert_eq!(normalize(r"E:\\dev\\foo"), "E:/dev/foo");
        assert_eq!(normalize("/mnt/e//foo///bar/"), "/mnt/e/foo/bar/");
        assert_eq!(normalize("  c:/x  "), "c:/x");
    }

    #[test]
    fn is_linux_path_checks_prefix() {
        assert!(is_linux_path("/mnt/e/foo"));
        assert!(!is_linux_path(r"E:\dev\foo"));
        assert!(!is_linux_path(""));
    }

    #[test]
    fn is_wsl_path_accepts_tilde() {
        assert!(is_wsl_path("/mnt/e/foo"));
        assert!(is_wsl_path("~/dev/foo"));
        assert!(is_wsl_path("~"));
        assert!(!is_wsl_path(r"E:\dev\foo"));
        assert!(!is_wsl_path(""));
    }

    #[test]
    fn win_to_linux_basic() {
        assert_eq!(win_path_to_linux(r"E:\dev\foo"), "/mnt/e/dev/foo");
        assert_eq!(win_path_to_linux(r"C:\Work\My App"), "/mnt/c/Work/My App");
        assert_eq!(win_path_to_linux("E:dev"), "/mnt/edev");
    }

    #[test]
    fn win_to_linux_passthrough() {
        assert_eq!(win_path_to_linux("/mnt/e/dev/foo"), "/mnt/e/dev/foo");
        assert_eq!(win_path_to_linux(""), "");
    }

    #[test]
    fn linux_to_win_basic() {
        assert_eq!(
            linux_path_to_win("/mnt/e/dev/foo").as_deref(),
            Some(r"E:\dev\foo")
        );
        assert_eq!(
            linux_path_to_win("/mnt/c/Work/My App").as_deref(),
            Some(r"C:\Work\My App")
        );
    }

    #[test]
    fn linux_to_win_no_match() {
        assert_eq!(linux_path_to_win("/mnt/e"), None);
        assert_eq!(linux_path_to_win("/home/user/foo"), None);
        assert_eq!(linux_path_to_win(r"E:\dev\foo"), None);
        assert_eq!(linux_path_to_win(""), None);
    }

    #[test]
    fn project_new_generates_id() {
        let p = Project::new("app", r"E:\dev\app", "");
        assert!(!p.id.is_empty());
        assert_ne!(p.id, Project::new("app", r"E:\dev\app", "").id);
    }

    #[test]
    fn project_ensure_id_backfills_missing() {
        let mut p = Project {
            id: String::new(),
            ..Project::new("old", "", "")
        };
        p.ensure_id();
        assert!(!p.id.is_empty());
        p.ensure_id();
        let kept = p.id.clone();
        p.ensure_id();
        assert_eq!(p.id, kept);
    }

    #[test]
    fn project_linux_path_manual_override() {
        let p = Project::new("app", r"E:\dev\app", "/custom/path");
        assert_eq!(p.linux_path(), "/custom/path");
    }

    #[test]
    fn project_linux_path_auto_generated() {
        let p = Project::new("app", r"E:\dev\app", "");
        assert_eq!(p.linux_path(), "/mnt/e/dev/app");
    }

    #[test]
    fn project_linux_path_linux_only() {
        let p = Project::new("app", "/mnt/e/dev/app", "");
        assert_eq!(p.linux_path(), "/mnt/e/dev/app");
    }

    #[test]
    fn project_has_windows_path() {
        assert!(Project::new("a", r"E:\dev\a", "").has_windows_path());
        assert!(!Project::new("a", "/mnt/e/a", "").has_windows_path());
        assert!(!Project::new("a", "", "").has_windows_path());
    }

    #[test]
    fn ssh_project_detection_and_windows_path() {
        let mut p = Project::new("srv", "", "");
        assert!(!p.is_ssh_project());
        p.ssh_target = "abc@192.0.2.10".into();
        p.path = "/opt/foo".into();
        assert!(p.is_ssh_project());
        // 远程 Linux 路径天然无 Windows 路径：PowerShell/IDE/Explorer 自动不可用。
        assert!(!p.has_windows_path());
        assert_eq!(p.linux_path(), "/opt/foo");
    }

    #[test]
    fn ssh_fields_legacy_json_defaults() {
        let json = r#"{"groups":[{"name":"G","projects":[{"name":"srv","sshTarget":"abc@h","path":"/opt/x"}]}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        let p = &data.groups[0].projects[0];
        assert!(p.is_ssh_project());
        assert_eq!(p.ssh_target, "abc@h");
        assert!(p.ssh_key_file.is_empty());
        assert!(p.ssh_key_path.is_empty());
        assert!(p.ssh_password_enc.is_empty());
        assert!(p.ssh_key_pass_enc.is_empty());
    }

    #[test]
    fn ssh_fields_serde_names_round_trip() {
        let mut p = Project::new("srv", "/opt/x", "");
        p.ssh_target = "abc@h:2222".into();
        p.ssh_key_file = "keys/abc.key".into();
        p.ssh_key_path = r"C:\keys\id_ed25519".into();
        p.ssh_password_enc = "PWENC".into();
        p.ssh_key_pass_enc = "KPENC".into();
        let out = serde_json::to_value(&p).unwrap();
        assert_eq!(out["sshTarget"], "abc@h:2222");
        assert_eq!(out["sshKeyFile"], "keys/abc.key");
        assert_eq!(out["sshKeyPath"], r"C:\keys\id_ed25519");
        assert_eq!(out["sshPasswordEnc"], "PWENC");
        assert_eq!(out["sshKeyPassEnc"], "KPENC");
        let back: Project = serde_json::from_value(out).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn deleted_item_keeps_ssh_snapshot() {
        let mut p = Project::new("srv", "/opt/x", "");
        p.ssh_target = "abc@h".into();
        p.ssh_key_file = "keys/k.key".into();
        p.ssh_password_enc = "PWENC".into();
        let item = DeletedItem::from_project(&p, "G", 123);
        assert_eq!(item.ssh_target, "abc@h");
        assert_eq!(item.ssh_key_file, "keys/k.key");
        assert_eq!(item.ssh_password_enc, "PWENC");
        // 分组快照不携带 SSH 秘密（projects 内部项目自带完整字段）。
        let mut g = Group::new("G");
        g.projects.push(p.clone());
        let gi = DeletedItem::from_group(&g, 123);
        assert!(gi.ssh_target.is_empty());
        let json = serde_json::to_value(&gi).unwrap();
        assert_eq!(json["projects"][0]["sshTarget"], "abc@h");
    }

    #[test]
    fn json_round_trip_matches_legacy_schema() {
        let json = r#"{"groups":[{"name":"Work","projects":[{"name":"my-app","path":"E:\\dev\\my-app","wslPath":"/mnt/e/dev/my-app"}]}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.groups[0].name, "Work");
        assert_eq!(data.groups[0].projects[0].wsl_path, "/mnt/e/dev/my-app");

        let out = serde_json::to_value(&data).unwrap();
        assert_eq!(
            out["groups"][0]["projects"][0]["wslPath"],
            "/mnt/e/dev/my-app"
        );
    }

    #[test]
    fn json_missing_fields_default() {
        let json = r#"{"groups":[{"name":"G"}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        assert!(data.groups[0].projects.is_empty());
        let json = r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        assert_eq!(data.groups[0].projects[0].path, "");
    }

    #[test]
    fn json_default_tool_round_trip_and_missing() {
        let json =
            r#"{"groups":[{"name":"G","projects":[{"name":"x","defaultTool":"opencode"}]}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        assert_eq!(data.groups[0].projects[0].default_tool, "opencode");

        let legacy = r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#;
        let data: ProjectData = serde_json::from_str(legacy).unwrap();
        assert_eq!(data.groups[0].projects[0].default_tool, "");

        let out = serde_json::to_value(&data).unwrap();
        assert_eq!(out["groups"][0]["projects"][0]["defaultTool"], "");
    }

    #[test]
    fn commands_round_trip_and_legacy_default() {
        let json = r#"{"groups":[{"name":"G","projects":[{"name":"x","commands":[{"name":"构建","env":"wsl","command":"make build"}]}]}]}"#;
        let data: ProjectData = serde_json::from_str(json).unwrap();
        let commands = &data.groups[0].projects[0].commands;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "构建");
        assert_eq!(commands[0].env, "wsl");
        assert_eq!(commands[0].command, "make build");

        let out = serde_json::to_value(&data).unwrap();
        assert_eq!(
            out["groups"][0]["projects"][0]["commands"][0]["name"],
            "构建"
        );

        let legacy: ProjectData =
            serde_json::from_str(r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#).unwrap();
        assert!(legacy.groups[0].projects[0].commands.is_empty());
    }

    #[test]
    fn project_command_new_fills_fields() {
        let cmd = ProjectCommand::new("构建", "powershell", "npm run build");
        assert_eq!(cmd.name, "构建");
        assert_eq!(cmd.env, "powershell");
        assert_eq!(cmd.command, "npm run build");
    }

    #[test]
    fn deleted_item_snapshot_includes_commands() {
        let mut project = Project::new("app", r"E:\app", "");
        project
            .commands
            .push(ProjectCommand::new("构建", "wsl", "make"));
        let item = DeletedItem::from_project(&project, "Work", 100);
        assert_eq!(item.commands.len(), 1);
        assert_eq!(item.commands[0].name, "构建");
        let group = Group {
            name: "Archive".into(),
            alias: String::new(),
            projects: vec![project],
        };
        let item = DeletedItem::from_group(&group, 200);
        assert!(item.commands.is_empty());
    }

    #[test]
    fn project_has_default_tool_helper() {
        let p = Project::new("a", r"E:\a", "");
        assert!(!p.has_default_tool());
        let mut p = Project::new("a", r"E:\a", "");
        p.default_tool = "opencode".into();
        assert!(p.has_default_tool());
    }

    #[test]
    fn project_regenerate_id_changes_id() {
        let mut p = Project::new("a", r"E:\a", "");
        let old = p.id.clone();
        p.regenerate_id();
        assert_ne!(p.id, old);
    }

    #[test]
    fn trash_round_trip_with_project_and_group() {
        let data: ProjectData = serde_json::from_str(
            r#"{"groups":[{"name":"Work","projects":[{"name":"app"}]}],"trash":[
                {"id":"11111111-0000-0000-0000-000000000000","type":"project","group":"Work",
                 "name":"old-app","path":"E:\\old","wslPath":"/mnt/e/old","defaultTool":"opencode",
                 "deletedAt":1723456789},
                {"id":"22222222-0000-0000-0000-000000000000","type":"group","name":"Archive",
                 "projects":[{"name":"inner","path":"E:\\inner","wslPath":"/mnt/e/inner"}],
                 "deletedAt":1723456790}
            ]}"#,
        )
        .unwrap();
        assert_eq!(data.trash.len(), 2);
        let item = &data.trash[0];
        assert!(!item.is_group());
        assert_eq!(item.group, "Work");
        assert_eq!(item.default_tool, "opencode");
        assert_eq!(item.deleted_at, 1723456789);
        let group = &data.trash[1];
        assert!(group.is_group());
        assert_eq!(group.projects.len(), 1);
        assert_eq!(group.projects[0].name, "inner");

        let out = serde_json::to_value(&data).unwrap();
        assert_eq!(out["trash"][0]["type"], "project");
        assert_eq!(out["trash"][0]["deletedAt"], 1723456789);
        assert_eq!(out["trash"][1]["projects"][0]["wslPath"], "/mnt/e/inner");
        assert_eq!(out["trash"][1]["deletedAt"], 1723456790);
    }

    #[test]
    fn trash_missing_field_defaults_empty() {
        let data: ProjectData =
            serde_json::from_str(r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#).unwrap();
        assert!(data.trash.is_empty());
        let legacy: ProjectData = serde_json::from_str(r#"{"groups":[]}"#).unwrap();
        assert!(legacy.trash.is_empty());
    }

    #[test]
    fn deleted_item_snapshots_project_and_group() {
        let mut project = Project::new("app", r"E:\app", "/mnt/e/app");
        project.default_tool = "opencode".into();
        let id = project.id.clone();
        let item = DeletedItem::from_project(&project, "Work", 100);
        assert_eq!(item.id, id);
        assert_eq!(item.name, "app");
        assert_eq!(item.group, "Work");
        assert_eq!(item.deleted_at, 100);

        let group = Group {
            name: "Archive".into(),
            alias: String::new(),
            projects: vec![project],
        };
        let item = DeletedItem::from_group(&group, 200);
        assert!(item.is_group());
        assert_ne!(item.id, id);
        assert_eq!(item.projects.len(), 1);
        assert_eq!(item.projects[0].id, id);
        assert_eq!(item.deleted_at, 200);
    }

    #[test]
    fn utc_components_known_timestamps() {
        assert_eq!(utc_components(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(utc_components(1700000000), (2023, 11, 14, 22, 13, 20));
        assert_eq!(utc_components(951782400), (2000, 2, 29, 0, 0, 0));
    }

    /// 儒略日数：用于比较日期序差。
    fn to_jdn(year: i64, month: u32, day: u32) -> i64 {
        let (year, month) = if month <= 2 {
            (year - 1, month + 12)
        } else {
            (year, month)
        };
        let a = year / 100;
        let b = 2 - a + a / 4;
        (36525 * (year + 4716)) / 100 + (306 * (i64::from(month) + 1)) / 10 + i64::from(day) + b
            - 1524
    }

    #[test]
    fn format_functions_produce_valid_dates() {
        let ts = 1700000000;
        let (year, month, day, _, _, _) = utc_components(ts);
        let date = format_date(ts);
        let compact = format_utc_compact(ts);
        assert_eq!(date.len(), 10);
        assert_eq!(compact.len(), 15);
        assert_eq!(&compact[8..9], "-");
        assert!(compact.chars().all(|c| c.is_ascii_digit() || c == '-'));
        assert_eq!(&compact[..8], &format!("{year:04}{month:02}{day:02}"));
        // UTC 版本与 utc_components 完全一致（备份命名可跨时区/夏令时）
        assert_eq!(format_utc_compact(0), "19700101-000000");
        assert_eq!(format_utc_compact(951782400), "20000229-000000");

        let parts: Vec<i64> = date.split('-').map(|s| s.parse().unwrap()).collect();
        let (d_year, d_month, d_day) = (parts[0], parts[1] as u32, parts[2] as u32);
        assert!(d_year >= 1970 && (1..=12).contains(&d_month) && (1..=31).contains(&d_day));
        // 本地时区与 UTC 最多相差一整天
        let delta = (to_jdn(d_year, d_month, d_day) - to_jdn(year, month, day)).abs();
        assert!(delta <= 1, "本地日期与 UTC 相差过大: {date}");
    }
}
