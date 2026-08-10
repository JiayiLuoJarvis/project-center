use std::path::{Path, PathBuf};

use crate::models::ProjectData;

pub struct Store;

impl Store {
    /// `%APPDATA%\project_center\projects.json`；APPDATA 缺失时回退
    /// `%USERPROFILE%\.project_center\projects.json`。
    pub fn file_path() -> PathBuf {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let s = appdata.to_string_lossy();
            if !s.trim().is_empty() {
                return PathBuf::from(appdata)
                    .join("project_center")
                    .join("projects.json");
            }
        }
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default();
        PathBuf::from(home)
            .join(".project_center")
            .join("projects.json")
    }

    pub fn load() -> ProjectData {
        let path = Self::file_path();
        let (data, backfilled) = Self::load_from_inner(&path);
        if backfilled {
            let _ = Self::save_to(&data, &path);
        }
        data
    }

    pub fn save(data: &ProjectData) -> bool {
        Self::save_to(data, &Self::file_path())
    }

    #[cfg(test)]
    pub fn load_from(p: &Path) -> ProjectData {
        Self::load_from_inner(p).0
    }

    /// 加载数据并回填缺失的项目 id；返回是否发生了回填。
    fn load_from_inner(p: &Path) -> (ProjectData, bool) {
        let Ok(text) = std::fs::read_to_string(p) else {
            return (ProjectData::default(), false);
        };
        let mut data: ProjectData = serde_json::from_str(&text).unwrap_or_default();
        let mut backfilled = false;
        for group in &mut data.groups {
            for project in &mut group.projects {
                if project.id.trim().is_empty() {
                    project.ensure_id();
                    backfilled = true;
                }
            }
        }
        (data, backfilled)
    }

    /// 原子写：写 `.json.tmp` -> 删除旧文件 -> rename。
    pub fn save_to(data: &ProjectData, p: &Path) -> bool {
        let Some(parent) = p.parent() else {
            return false;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
        let Ok(json) = serde_json::to_string_pretty(data) else {
            return false;
        };
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_err() {
            return false;
        }
        let _ = std::fs::remove_file(p);
        std::fs::rename(&tmp, p).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Group, Project, ProjectData};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pcs_test_{stamp}"))
    }

    #[test]
    fn round_trip() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        let data = ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                projects: vec![Project::new("app", r"E:\dev\app", "/mnt/e/dev/app")],
            }],
        };
        assert!(Store::save_to(&data, &path));
        let loaded = Store::load_from(&path);
        assert_eq!(loaded, data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_legacy_file() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"{"groups":[{"name":"Work","projects":[{"name":"my-app","path":"E:\\dev\\my-app","wslPath":"/mnt/e/dev/my-app"}]}]}"#,
        )
        .unwrap();
        let data = Store::load_from(&path);
        assert_eq!(data.groups[0].name, "Work");
        assert_eq!(data.groups[0].projects[0].wsl_path, "/mnt/e/dev/my-app");
        assert!(!data.groups[0].projects[0].id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backfill_reports_missing_ids() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"{"groups":[{"name":"G","projects":[{"name":"x"}]}]}"#,
        )
        .unwrap();
        let (data, backfilled) = Store::load_from_inner(&path);
        assert!(backfilled);
        assert!(!data.groups[0].projects[0].id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backfill_noop_when_ids_present() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        let data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                projects: vec![Project::new("x", r"E:\x", "")],
            }],
        };
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let (data, backfilled) = Store::load_from_inner(&path);
        assert!(!backfilled);
        assert!(!data.groups[0].projects[0].id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = temp_path();
        let path = dir.join("nope").join("projects.json");
        let data = Store::load_from(&path);
        assert!(data.groups.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_corrupt_returns_empty() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ not valid json").unwrap();
        let data = Store::load_from(&path);
        assert!(data.groups.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
