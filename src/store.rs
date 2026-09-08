use std::path::{Path, PathBuf};

use crate::models::{ProjectData, current_unix_ts, format_utc_compact};

/// 备份文件轮转上限：超出后删除最旧的备份。
pub const MAX_BACKUPS: usize = 10;

/// 备份文件名前缀（后接 UTC `YYYYMMDD-HHMMSS.json`，同秒冲突追加 `-N`）。
const BACKUP_PREFIX: &str = "projects-";

pub struct Store;

/// APPDATA 下数据目录名：release → 生产 `project_center`；debug → `project_center_dev`。
fn data_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "project_center_dev"
    } else {
        "project_center"
    }
}

/// 无 APPDATA 时的 HOME 回退目录名（带点前缀）。
fn home_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        ".project_center_dev"
    } else {
        ".project_center"
    }
}

impl Store {
    /// release：`%APPDATA%\project_center\projects.json`；
    /// debug：`%APPDATA%\project_center_dev\projects.json`。
    /// APPDATA 缺失时回退 `%USERPROFILE%`（或 `HOME`）下对应的点目录。
    pub fn file_path() -> PathBuf {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let s = appdata.to_string_lossy();
            if !s.trim().is_empty() {
                return PathBuf::from(appdata)
                    .join(data_dir_name())
                    .join("projects.json");
            }
        }
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default();
        PathBuf::from(home)
            .join(home_dir_name())
            .join("projects.json")
    }

    pub fn load() -> ProjectData {
        let path = Self::file_path();
        let (mut data, backfilled, recovered) = Self::load_from_inner(&path);
        // 超过保留期的回收站项自动清理。
        let purged = crate::ops::purge_expired_trash(&mut data);
        // 秘密维护：pendingKeyDeletes 重试、孤儿 key 清理、keys_tmp 残留清理。
        let maintained = crate::secret::startup_maintenance(&mut data);
        // 回填 id / 损坏恢复 / 清理过期项后立即写回，避免只读命令丢恢复结果。
        if backfilled || recovered || purged || maintained {
            let _ = Self::save_to(&data, &path);
        }
        data
    }

    /// 只读加载（`__askpass` 专用）：不写回、不做维护，
    /// 避免与正在运行的父进程产生 projects.json 写竞态。
    pub fn load_readonly() -> ProjectData {
        Self::load_from_inner(&Self::file_path()).0
    }

    pub fn save(data: &ProjectData) -> bool {
        Self::save_to(data, &Self::file_path())
    }

    #[cfg(test)]
    pub fn load_from(p: &Path) -> ProjectData {
        Self::load_from_inner(p).0
    }

    /// 加载数据并回填缺失的项目 id；返回（数据，是否回填了 id，是否从备份恢复）。
    fn load_from_inner(p: &Path) -> (ProjectData, bool, bool) {
        let Ok(text) = std::fs::read_to_string(p) else {
            return (ProjectData::default(), false, false);
        };
        match parse_projects(&text) {
            Some(parsed) => (parsed.0, parsed.1, false),
            None => {
                // 损坏：改名保留现场，依次尝试「最新可用备份 -> 空数据」。
                Self::quarantine_corrupt(p);
                match recover_from_backups(p) {
                    Some(parsed) => {
                        eprintln!(
                            "projects.json 损坏，已改名保存，并从备份恢复: {}",
                            parsed.1.display()
                        );
                        (parsed.0, parsed.2, true)
                    }
                    None => {
                        eprintln!("projects.json 损坏，无可用备份，已使用空数据。");
                        (ProjectData::default(), false, false)
                    }
                }
            }
        }
    }

    /// 原子写：写 `.json.tmp` -> 删除旧文件 -> rename。
    /// 写入前把旧文件轮转为带时间戳的备份（仅当旧文件存在且非空）。
    pub fn save_to(data: &ProjectData, p: &Path) -> bool {
        let Some(parent) = p.parent() else {
            return false;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
        // 备份失败仅警告，不中断保存（延续「仅警告」哲学）。
        if !backup_to(p) {
            eprintln!("警告：无法备份旧数据文件，已跳过备份。");
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

/// 解析文本并回填缺失的项目 id（含回收站项与分组快照内项目）；
/// 解析失败返回 None。
fn parse_projects(text: &str) -> Option<(ProjectData, bool)> {
    let mut data: ProjectData = serde_json::from_str(text).ok()?;
    let mut backfilled = false;
    for group in &mut data.groups {
        for project in &mut group.projects {
            if project.id.trim().is_empty() {
                project.ensure_id();
                backfilled = true;
            }
        }
    }
    for item in &mut data.trash {
        if item.id.trim().is_empty() {
            item.ensure_id();
            backfilled = true;
        }
        for project in &mut item.projects {
            if project.id.trim().is_empty() {
                project.ensure_id();
                backfilled = true;
            }
        }
    }
    Some((data, backfilled))
}

/// 备份目录：与数据文件同目录下的 `backups\`。
fn backup_dir(p: &Path) -> Option<PathBuf> {
    p.parent().map(|parent| parent.join("backups"))
}

/// 列出备份文件名（按文件名时间序）。
fn backup_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.starts_with(BACKUP_PREFIX) && name.ends_with(".json")).then_some(name)
        })
        .collect();
    names.sort();
    names
}

/// 目录内不存在的 `<base>[-N]<extension>` 文件名（同秒多次写入自动追加序号）。
fn unique_name_in(dir: &Path, base: &str, extension: &str) -> PathBuf {
    let mut name = format!("{base}{extension}");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}{extension}");
        n += 1;
    }
    dir.join(name)
}

/// 当前数据文件复制为带时间戳的备份，随后按上限轮转。
/// 文件缺失或为空（含首次创建）时直接返回 true，不产生备份。
/// 同秒多次备份追加 `-N` 后缀，避免快照互相覆盖。
fn backup_to(p: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(p) else {
        return true;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return true;
    }
    let Some(dir) = backup_dir(p) else {
        return false;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let base = format!("{BACKUP_PREFIX}{}", format_utc_compact(current_unix_ts()));
    let target = unique_name_in(&dir, &base, ".json");
    if std::fs::copy(p, target).is_err() {
        return false;
    }
    prune_backups(&dir, MAX_BACKUPS);
    true
}

/// 按文件名时间序删除最旧的多余备份。
fn prune_backups(dir: &Path, max: usize) {
    let names = backup_names(dir);
    if names.len() <= max {
        return;
    }
    for name in names.iter().take(names.len() - max) {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// 从新到旧遍历备份，返回第一个可解析的（数据，备份路径，是否回填 id）；
/// 最新备份损坏/截断时仍可回退到更早的可用快照。
fn recover_from_backups(p: &Path) -> Option<(ProjectData, PathBuf, bool)> {
    let dir = backup_dir(p)?;
    let names = backup_names(&dir);
    for name in names.iter().rev() {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        if let Some(parsed) = parse_projects(&text) {
            return Some((parsed.0, dir.join(name), parsed.1));
        }
    }
    None
}

impl Store {
    /// 损坏文件改名为带时间戳的 `projects-<utc>.corrupt` 保留现场
    /// （多份损坏原件各自保留，不互相覆盖）；失败仅警告。
    fn quarantine_corrupt(p: &Path) {
        let Some(parent) = p.parent() else {
            return;
        };
        let base = format!("{BACKUP_PREFIX}{}", format_utc_compact(current_unix_ts()));
        let corrupt = unique_name_in(parent, &base, ".corrupt");
        if std::fs::rename(p, &corrupt).is_err() {
            eprintln!("警告：无法把损坏文件改名以保留现场。");
        }
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
    fn file_path_matches_build_profile() {
        let path = Store::file_path();
        assert!(
            path.ends_with("projects.json"),
            "expected projects.json suffix, got {}",
            path.display()
        );
        if cfg!(debug_assertions) {
            assert_eq!(data_dir_name(), "project_center_dev");
            assert_eq!(home_dir_name(), ".project_center_dev");
            assert!(
                path.components()
                    .any(|c| c.as_os_str() == "project_center_dev"
                        || c.as_os_str() == ".project_center_dev"),
                "debug path must use project_center_dev, got {}",
                path.display()
            );
        } else {
            assert_eq!(data_dir_name(), "project_center");
            assert_eq!(home_dir_name(), ".project_center");
            assert!(
                path.components().any(
                    |c| c.as_os_str() == "project_center" || c.as_os_str() == ".project_center"
                ),
                "release path must use project_center, got {}",
                path.display()
            );
            assert!(
                !path
                    .components()
                    .any(|c| c.as_os_str() == "project_center_dev"
                        || c.as_os_str() == ".project_center_dev"),
                "release path must not use project_center_dev, got {}",
                path.display()
            );
        }
    }

    #[test]
    fn round_trip() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        let data = ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project::new("app", r"E:\dev\app", "/mnt/e/dev/app")],
            }],
            ..Default::default()
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
        let (data, backfilled, _) = Store::load_from_inner(&path);
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
                alias: String::new(),
                projects: vec![Project::new("x", r"E:\x", "")],
            }],
            ..Default::default()
        };
        std::fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();
        let (data, backfilled, _) = Store::load_from_inner(&path);
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

    #[test]
    fn backup_created_on_second_save() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        let data = ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project::new("app", r"E:\dev\app", "")],
            }],
            ..Default::default()
        };
        // 首次保存：文件不存在，不产生备份
        assert!(Store::save_to(&data, &path));
        assert!(backup_names(&dir.join("backups")).is_empty());
        // 再次保存：旧文件轮转为备份
        assert!(Store::save_to(&data, &path));
        let names = backup_names(&dir.join("backups"));
        assert_eq!(names.len(), 1);
        assert!(names[0].starts_with(BACKUP_PREFIX));
        let backup_content = std::fs::read_to_string(dir.join("backups").join(&names[0])).unwrap();
        let backed_up: ProjectData = serde_json::from_str(&backup_content).unwrap();
        assert_eq!(backed_up, data);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_skips_empty_file() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "").unwrap();
        let data = ProjectData::default();
        assert!(Store::save_to(&data, &path));
        assert!(backup_names(&dir.join("backups")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_backups_keeps_newest_max() {
        let dir = temp_path().join("backups");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..15 {
            std::fs::write(
                dir.join(format!("{BACKUP_PREFIX}20260810-{i:06}.json")),
                format!("backup-{i}"),
            )
            .unwrap();
        }
        prune_backups(&dir, MAX_BACKUPS);
        let names = backup_names(&dir);
        assert_eq!(names.len(), MAX_BACKUPS);
        assert!(!names.contains(&format!("{BACKUP_PREFIX}20260810-000000.json")));
        assert!(names.contains(&format!("{BACKUP_PREFIX}20260810-000014.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_prunes_backups_over_limit() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        let data = ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project::new("app", r"E:\dev\app", "")],
            }],
            ..Default::default()
        };
        std::fs::create_dir_all(&dir).unwrap();
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        // 预置 10 份旧备份，本次保存再产生 1 份 -> 轮转后仍为 10 份
        for i in 0..MAX_BACKUPS {
            std::fs::write(
                backups.join(format!("{BACKUP_PREFIX}20200101-{i:06}.json")),
                "old",
            )
            .unwrap();
        }
        assert!(Store::save_to(&data, &path));
        let names = backup_names(&backups);
        assert_eq!(names.len(), MAX_BACKUPS);
        assert!(!names[0].starts_with("20200101"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn corrupt_files(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with(BACKUP_PREFIX) && name.ends_with(".corrupt")
            })
            .map(|entry| entry.path())
            .collect()
    }

    #[test]
    fn corrupt_recovers_from_latest_backup() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        // 手工造一份旧数据文件，让第一次保存产生备份
        std::fs::write(&path, r#"{"groups":[{"name":"Legacy","projects":[]}]}"#).unwrap();
        let first = ProjectData {
            groups: vec![Group {
                name: "First".into(),
                alias: String::new(),
                projects: vec![Project::new("a", r"E:\a", "")],
            }],
            ..Default::default()
        };
        assert!(Store::save_to(&first, &path));
        // 把第一份备份改名到旧时间，确保第二次保存的备份名更新
        let first_backup = backup_names(&dir.join("backups"))[0].clone();
        std::fs::rename(
            dir.join("backups").join(&first_backup),
            dir.join("backups")
                .join(format!("{BACKUP_PREFIX}20200101-000000.json")),
        )
        .unwrap();
        let second = ProjectData {
            groups: vec![Group {
                name: "Second".into(),
                alias: String::new(),
                projects: vec![Project::new("b", r"E:\b", "")],
            }],
            ..Default::default()
        };
        assert!(Store::save_to(&second, &path));
        // 主文件损坏
        std::fs::write(&path, "{ corrupted !!!").unwrap();
        // 恢复取最新备份：最新备份是第二次保存前的快照（First），
        // 而非改名到旧时间的首份备份（Legacy）。
        let data = Store::load_from(&path);
        assert_eq!(data.groups[0].name, "First");
        // 损坏原件改名保留
        let corrupts = corrupt_files(&dir);
        assert_eq!(corrupts.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&corrupts[0]).unwrap(),
            "{ corrupted !!!"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_falls_back_to_older_backup_when_newest_broken() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"groups":[{"name":"Legacy","projects":[]}]}"#).unwrap();
        let first = ProjectData {
            groups: vec![Group {
                name: "First".into(),
                alias: String::new(),
                projects: vec![Project::new("a", r"E:\a", "")],
            }],
            ..Default::default()
        };
        assert!(Store::save_to(&first, &path));
        // 第二次保存产生更新备份；两份备份都存在
        let second = ProjectData {
            groups: vec![Group {
                name: "Second".into(),
                alias: String::new(),
                projects: vec![Project::new("b", r"E:\b", "")],
            }],
            ..Default::default()
        };
        assert!(Store::save_to(&second, &path));
        let backups = dir.join("backups");
        let mut names = backup_names(&backups);
        assert_eq!(names.len(), 2);
        // 最新备份（Second 保存前的快照，First 内容）改名到未来时间并写坏，
        // 旧备份保持完好
        let newest = names.pop().unwrap();
        std::fs::rename(
            backups.join(&newest),
            backups.join(format!("{BACKUP_PREFIX}20990101-000000.json")),
        )
        .unwrap();
        std::fs::write(
            backups.join(format!("{BACKUP_PREFIX}20990101-000000.json")),
            "{|",
        )
        .unwrap();
        std::fs::write(&path, "{ corrupted !!!").unwrap();
        // 最新备份损坏时应回退到更早的可用备份（First 快照）
        let data = Store::load_from(&path);
        assert_eq!(data.groups[0].name, "First");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_second_saves_create_distinct_backups() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"groups":[{"name":"Legacy","projects":[]}]}"#).unwrap();
        let data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![Project::new("a", r"E:\a", "")],
            }],
            ..Default::default()
        };
        // 同一秒内的两次保存各自产生独立备份，互不覆盖
        assert!(Store::save_to(&data, &path));
        assert!(Store::save_to(&data, &path));
        assert_eq!(backup_names(&dir.join("backups")).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backfills_trash_ids_at_load() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"{"groups":[],"trash":[{"type":"project","name":"x"},{"type":"group","name":"g","projects":[{"name":"inner"}]}]}"#,
        )
        .unwrap();
        let (data, backfilled, _) = Store::load_from_inner(&path);
        assert!(backfilled);
        assert!(!data.trash[0].id.is_empty());
        assert!(!data.trash[1].id.is_empty());
        assert!(!data.trash[1].projects[0].id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_without_backup_returns_empty() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ not valid json").unwrap();
        let data = Store::load_from(&path);
        assert!(data.groups.is_empty());
        assert_eq!(corrupt_files(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_quarantine_keeps_each_original() {
        let dir = temp_path();
        let path = dir.join("projects.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ first corruption").unwrap();
        let _ = Store::load_from(&path);
        std::fs::write(&path, "{ second corruption").unwrap();
        let _ = Store::load_from(&path);
        // 两份损坏原件各自保留（带时间戳命名，不互相覆盖）
        let corrupts = corrupt_files(&dir);
        assert_eq!(corrupts.len(), 2);
        let contents: Vec<String> = corrupts
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        assert!(contents.contains(&"{ first corruption".to_string()));
        assert!(contents.contains(&"{ second corruption".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_does_not_quarantine() {
        let dir = temp_path();
        let path = dir.join("nope").join("projects.json");
        let data = Store::load_from(&path);
        assert!(data.groups.is_empty());
        assert!(corrupt_files(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
