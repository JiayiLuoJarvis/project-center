use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(rename = "wslPath", default)]
    pub wsl_path: String,
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
            path: path.into(),
            wsl_path: wsl_path.into(),
        }
    }

    /// 为旧数据中缺失 id 的项目补生成 id。
    pub fn ensure_id(&mut self) {
        if self.id.trim().is_empty() {
            self.id = Uuid::new_v4().to_string();
        }
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            projects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData {
    #[serde(default)]
    pub groups: Vec<Group>,
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
            name: "old".into(),
            path: "".into(),
            wsl_path: "".into(),
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
}
