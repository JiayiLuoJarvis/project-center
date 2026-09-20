use std::path::{Path, PathBuf};

use crate::domain::KeyRename;
use crate::domain::models::{ProjectData, current_unix_ts, format_utc_compact, rfc3339_now};

use super::legacy::harvest_legacy_ssh;

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
    /// `PCS_DATA_DIR` 若非空，则该目录即为数据根（测试/CI 用，不改变默认行为）。
    pub fn file_path() -> PathBuf {
        if let Some(dir) = std::env::var_os("PCS_DATA_DIR") {
            let s = dir.to_string_lossy();
            if !s.trim().is_empty() {
                return PathBuf::from(dir).join("projects.json");
            }
        }
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
        let (mut data, backfilled, recovered, migrated) = Self::load_from_inner(&path, true);
        // 超过保留期的回收站项自动清理。
        let purged = crate::domain::purge_expired_trash(&mut data);
        // 数据退化（损坏且无备份、或从备份恢复）时引用集不可信，
        // 绝不做孤儿 key 清理，否则会把全部密文 sidecar 当孤儿销毁。
        let allow_orphan_sweep = !recovered && orphan_sweep_allowed(&data);
        // 秘密维护：pendingKeyDeletes 重试、孤儿 key 清理、tmp 残留清理。
        let maintained = crate::persist::startup_maintenance(&mut data, allow_orphan_sweep);
        // 回填 id / 损坏恢复 / SSH 迁移 / 清理过期项后立即写回，避免只读命令丢恢复结果。
        if backfilled || recovered || purged || maintained || migrated {
            let _ = Self::save_to(&data, &path);
        }
        data
    }

    /// 只读加载（`__askpass` 专用）：不写回、不改名密钥、不做维护，
    /// 避免与正在运行的父进程产生 projects.json 写竞态。
    pub fn load_readonly() -> ProjectData {
        Self::load_from_inner(&Self::file_path(), false).0
    }

    pub fn save(data: &ProjectData) -> Result<(), super::Error> {
        Self::save_to(data, &Self::file_path())
    }

    #[cfg(test)]
    pub fn load_from(p: &Path) -> ProjectData {
        Self::load_from_inner(p, true).0
    }

    /// 加载数据并回填缺失的项目 id；返回（数据，是否回填了 id，是否从备份恢复，是否做了 SSH 迁移）。
    /// `apply_key_fs`：写路径改名 sidecar；只读加载必须为 false。
    fn load_from_inner(p: &Path, apply_key_fs: bool) -> (ProjectData, bool, bool, bool) {
        let Ok(text) = std::fs::read_to_string(p) else {
            return (ProjectData::default(), false, false, false);
        };
        match parse_projects(&text) {
            Some(mut parsed) => {
                if apply_key_fs {
                    apply_key_renames(p.parent(), &mut parsed);
                }
                (parsed.data, parsed.backfilled, false, parsed.migrated)
            }
            None => {
                // 损坏：改名保留现场，依次尝试「最新可用备份 -> 空数据」。
                Self::quarantine_corrupt(p);
                match recover_from_backups(p) {
                    Some((mut parsed, backup)) => {
                        eprintln!(
                            "projects.json 损坏，已改名保存，并从备份恢复: {}",
                            backup.display()
                        );
                        if apply_key_fs {
                            apply_key_renames(p.parent(), &mut parsed);
                        }
                        (parsed.data, parsed.backfilled, true, parsed.migrated)
                    }
                    None => {
                        eprintln!("projects.json 损坏，无可用备份，已使用空数据。");
                        (ProjectData::default(), false, false, false)
                    }
                }
            }
        }
    }

    /// 原子写：写 `.json.tmp` -> 删除旧文件 -> rename。
    /// 写入前把旧文件轮转为带时间戳的备份（仅当旧文件存在且非空）。
    pub fn save_to(data: &ProjectData, p: &Path) -> Result<(), super::Error> {
        let Some(parent) = p.parent() else {
            return Err(super::Error::SaveFailed {
                source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "路径没有父目录"),
            });
        };
        std::fs::create_dir_all(parent).map_err(|source| super::Error::SaveFailed { source })?;
        // 备份失败仅警告，不中断保存（延续「仅警告」哲学）。
        if !backup_to(p) {
            eprintln!("警告：无法备份旧数据文件，已跳过备份。");
        }
        let json = serde_json::to_string_pretty(data).map_err(|e| super::Error::SaveFailed {
            source: super::error::io_invalid(e.to_string()),
        })?;
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|source| super::Error::SaveFailed { source })?;
        let _ = std::fs::remove_file(p);
        std::fs::rename(&tmp, p).map_err(|source| super::Error::SaveFailed { source })?;
        Ok(())
    }
}

/// 孤儿 key 清理的允许条件：groups / trash / connections 至少一个非空（引用集可信）。
/// 数据为空可能是「损坏兜底到空数据」或「首启文件缺失」，此时 `keys\` 下
/// 任何文件都可能是最后一次正常数据引用的密钥，一律不清理。
fn orphan_sweep_allowed(data: &ProjectData) -> bool {
    !data.groups.is_empty() || !data.trash.is_empty() || !data.connections.is_empty()
}

struct ParsedProjects {
    data: ProjectData,
    backfilled: bool,
    migrated: bool,
    key_renames: Vec<KeyRename>,
}

/// 解析文本：Value 采集遗留 SSH → 结构化 → 回填 id → absorb。
/// 解析失败返回 None（损坏门控不变）。
fn parse_projects(text: &str) -> Option<ParsedProjects> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let records = harvest_legacy_ssh(&value);
    let mut data: ProjectData = serde_json::from_value(value).ok()?;
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
    let now = rfc3339_now();
    let mig = data.absorb_legacy_ssh(records, &now);
    Some(ParsedProjects {
        data,
        backfilled,
        migrated: mig.changed,
        key_renames: mig.key_renames,
    })
}

/// 改名成功（或目标已在位）才 relink；失败保持连接上的旧路径。
fn apply_key_renames(root: Option<&Path>, parsed: &mut ParsedProjects) {
    let Some(root) = root else {
        return;
    };
    for rename in &parsed.key_renames {
        if rename_key_file(root, &rename.from, &rename.to) {
            parsed
                .data
                .relink_connection_key(&rename.connection_id, &rename.to);
        }
    }
}

fn rename_key_file(root: &Path, from: &str, to: &str) -> bool {
    let Ok(from_path) = super::key_file_path_in(root, from) else {
        return false;
    };
    let Ok(to_path) = super::key_file_path_in(root, to) else {
        return false;
    };
    if let Some(parent) = to_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if to_path.exists() {
        return true;
    }
    if !from_path.exists() {
        return false;
    }
    std::fs::rename(from_path, to_path).is_ok()
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

/// 从新到旧遍历备份，返回第一个可解析的（解析结果，备份路径）；
/// 最新备份损坏/截断时仍可回退到更早的可用快照。
fn recover_from_backups(p: &Path) -> Option<(ParsedProjects, PathBuf)> {
    let dir = backup_dir(p)?;
    let names = backup_names(&dir);
    for name in names.iter().rev() {
        let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
            continue;
        };
        if let Some(parsed) = parse_projects(&text) {
            return Some((parsed, dir.join(name)));
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
pub(crate) mod test_env {
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};

    // 需要临时改写 APPDATA 的测试必须串行执行，避免进程级全局状态互踩。
    static APPDATA_LOCK: Mutex<()> = Mutex::new(());

    pub fn lock_appdata() -> MutexGuard<'static, ()> {
        APPDATA_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 把 APPDATA 指向临时目录；Drop 时恢复原值（断言失败也不残留）。
    /// 同时清掉 `PCS_DATA_DIR`，避免测试缝盖过 APPDATA 语义。
    pub struct AppdataGuard {
        saved: Option<std::ffi::OsString>,
        saved_data_dir: Option<std::ffi::OsString>,
    }

    impl AppdataGuard {
        pub fn redirect(dir: &Path) -> Self {
            let saved = std::env::var_os("APPDATA");
            let saved_data_dir = std::env::var_os("PCS_DATA_DIR");
            unsafe {
                std::env::set_var("APPDATA", dir);
                std::env::remove_var("PCS_DATA_DIR");
            }
            Self {
                saved,
                saved_data_dir,
            }
        }
    }

    impl Drop for AppdataGuard {
        fn drop(&mut self) {
            unsafe {
                match self.saved.take() {
                    Some(value) => std::env::set_var("APPDATA", value),
                    None => std::env::remove_var("APPDATA"),
                }
                match self.saved_data_dir.take() {
                    Some(value) => std::env::set_var("PCS_DATA_DIR", value),
                    None => std::env::remove_var("PCS_DATA_DIR"),
                }
            }
        }
    }

    /// 把 `PCS_CONFIG_PATH` 指向指定文件；Drop 时恢复。须与 `lock_appdata` 同用。
    pub struct ConfigPathGuard {
        saved: Option<std::ffi::OsString>,
    }

    impl ConfigPathGuard {
        pub fn redirect(path: &Path) -> Self {
            let saved = std::env::var_os("PCS_CONFIG_PATH");
            unsafe {
                std::env::set_var("PCS_CONFIG_PATH", path);
            }
            Self { saved }
        }
    }

    impl Drop for ConfigPathGuard {
        fn drop(&mut self) {
            unsafe {
                match self.saved.take() {
                    Some(value) => std::env::set_var("PCS_CONFIG_PATH", value),
                    None => std::env::remove_var("PCS_CONFIG_PATH"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Group, Project, ProjectData};
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
        let _lock = test_env::lock_appdata();
        let _guard = test_env::AppdataGuard::redirect(&temp_path());
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
        assert!(Store::save_to(&data, &path).is_ok());
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
        let (data, backfilled, ..) = Store::load_from_inner(&path, true);
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
        let (data, backfilled, ..) = Store::load_from_inner(&path, true);
        assert!(!backfilled);
        assert!(!data.groups[0].projects[0].id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pcs_data_dir_overrides_appdata() {
        let _lock = test_env::lock_appdata();
        let dir = temp_path();
        std::fs::create_dir_all(&dir).unwrap();
        let saved = std::env::var_os("PCS_DATA_DIR");
        unsafe { std::env::set_var("PCS_DATA_DIR", &dir) };
        let path = Store::file_path();
        unsafe {
            match saved {
                Some(value) => std::env::set_var("PCS_DATA_DIR", value),
                None => std::env::remove_var("PCS_DATA_DIR"),
            }
        }
        assert_eq!(path, dir.join("projects.json"));
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
    fn orphan_sweep_allowed_requires_non_empty_data() {
        // 空数据（损坏兜底/首启缺失）不可清理
        assert!(!orphan_sweep_allowed(&ProjectData::default()));
        // 有分组即引用集可信（分组内项目可为空）
        let mut data = ProjectData::default();
        data.groups.push(Group {
            name: "G".into(),
            alias: String::new(),
            projects: Vec::new(),
        });
        assert!(orphan_sweep_allowed(&data));
        // 仅有回收站快照也可信
        let mut data = ProjectData::default();
        data.trash.push(crate::domain::models::DeletedItem {
            kind: "project".into(),
            ..Default::default()
        });
        assert!(orphan_sweep_allowed(&data));
        // 仅有连接也可信（迁移后项目级 key 引用已清空）
        let mut data = ProjectData::default();
        data.connections
            .push(crate::domain::models::Connection::default());
        assert!(orphan_sweep_allowed(&data));
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
        assert!(Store::save_to(&data, &path).is_ok());
        assert!(backup_names(&dir.join("backups")).is_empty());
        // 再次保存：旧文件轮转为备份
        assert!(Store::save_to(&data, &path).is_ok());
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
        assert!(Store::save_to(&data, &path).is_ok());
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
        assert!(Store::save_to(&data, &path).is_ok());
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
        assert!(Store::save_to(&first, &path).is_ok());
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
        assert!(Store::save_to(&second, &path).is_ok());
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
        assert!(Store::save_to(&first, &path).is_ok());
        // 第二次保存产生更新备份；两份备份都存在
        let second = ProjectData {
            groups: vec![Group {
                name: "Second".into(),
                alias: String::new(),
                projects: vec![Project::new("b", r"E:\b", "")],
            }],
            ..Default::default()
        };
        assert!(Store::save_to(&second, &path).is_ok());
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
        assert!(Store::save_to(&data, &path).is_ok());
        assert!(Store::save_to(&data, &path).is_ok());
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
        let (data, backfilled, ..) = Store::load_from_inner(&path, true);
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

    fn write_legacy_ssh_pair(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("projects.json");
        std::fs::write(
            &path,
            r#"{"groups":[{"name":"G","projects":[
              {"id":"11111111-1111-1111-1111-111111111111","name":"one","path":"/opt/a","sshTarget":"abc@h:22"},
              {"id":"22222222-2222-2222-2222-222222222222","name":"two","path":"/opt/b","sshTarget":"ABC@H"}
            ]}]}"#,
        )
        .unwrap();
        path
    }

    #[test]
    fn load_merges_two_projects_same_ssh_endpoint() {
        let dir = temp_path();
        let path = write_legacy_ssh_pair(&dir);
        let data = Store::load_from(&path);
        assert_eq!(data.connections.len(), 1);
        let cid = data.connections[0].id.clone();
        assert_eq!(data.groups[0].projects[0].connection_id, cid);
        assert_eq!(data.groups[0].projects[1].connection_id, cid);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_writes_back_cleared_ssh_target() {
        let _lock = test_env::lock_appdata();
        let dir = temp_path();
        write_legacy_ssh_pair(&dir);
        let saved = std::env::var_os("PCS_DATA_DIR");
        unsafe { std::env::set_var("PCS_DATA_DIR", &dir) };
        let loaded = Store::load();
        unsafe {
            match saved {
                Some(value) => std::env::set_var("PCS_DATA_DIR", value),
                None => std::env::remove_var("PCS_DATA_DIR"),
            }
        }
        assert_eq!(loaded.connections.len(), 1);
        let written = std::fs::read_to_string(dir.join("projects.json")).unwrap();
        assert!(!written.contains("sshTarget"));
        let disk: ProjectData = serde_json::from_str(&written).unwrap();
        assert!(!disk.groups[0].projects[0].connection_id.is_empty());
        assert_eq!(
            disk.groups[0].projects[0].connection_id,
            disk.groups[0].projects[1].connection_id
        );
        assert_eq!(disk.connections.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_readonly_does_not_rename_key_files() {
        let _lock = test_env::lock_appdata();
        let dir = temp_path();
        std::fs::create_dir_all(dir.join("keys")).unwrap();
        std::fs::write(dir.join("keys").join("old-proj.key"), "enc").unwrap();
        std::fs::write(
            dir.join("projects.json"),
            r#"{"groups":[{"name":"G","projects":[{
              "id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
              "name":"one","path":"/opt/a",
              "sshTarget":"abc@h",
              "sshKeyFile":"keys/old-proj.key"
            }]}]}"#,
        )
        .unwrap();
        let saved = std::env::var_os("PCS_DATA_DIR");
        unsafe { std::env::set_var("PCS_DATA_DIR", &dir) };
        let data = Store::load_readonly();
        let on_disk = std::fs::read_to_string(dir.join("projects.json")).unwrap();
        unsafe {
            match saved {
                Some(value) => std::env::set_var("PCS_DATA_DIR", value),
                None => std::env::remove_var("PCS_DATA_DIR"),
            }
        }
        assert_eq!(data.connections.len(), 1);
        let cid = &data.connections[0].id;
        assert!(dir.join("keys").join("old-proj.key").exists());
        assert!(!dir.join("keys").join(format!("{cid}.key")).exists());
        assert!(on_disk.contains("sshTarget"));
        assert!(!on_disk.contains("connectionId"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_renames_legacy_key_file_after_absorb() {
        let _lock = test_env::lock_appdata();
        let dir = temp_path();
        std::fs::create_dir_all(dir.join("keys")).unwrap();
        std::fs::write(dir.join("keys").join("old-proj.key"), "enc").unwrap();
        std::fs::write(
            dir.join("projects.json"),
            r#"{"groups":[{"name":"G","projects":[{
              "id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
              "name":"one","path":"/opt/a",
              "sshTarget":"abc@h",
              "sshKeyFile":"keys/old-proj.key"
            }]}]}"#,
        )
        .unwrap();
        let saved = std::env::var_os("PCS_DATA_DIR");
        unsafe { std::env::set_var("PCS_DATA_DIR", &dir) };
        let data = Store::load();
        unsafe {
            match saved {
                Some(value) => std::env::set_var("PCS_DATA_DIR", value),
                None => std::env::remove_var("PCS_DATA_DIR"),
            }
        }
        let cid = &data.connections[0].id;
        let expected = format!("keys/{cid}.key");
        assert_eq!(data.connections[0].ssh_key_file, expected);
        assert!(!dir.join("keys").join("old-proj.key").exists());
        assert!(dir.join("keys").join(format!("{cid}.key")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
