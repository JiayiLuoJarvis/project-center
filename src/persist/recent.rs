use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::models::current_unix_ts;

use super::Store;

/// 最近列表上限。
pub const MAX_RECENT: usize = 20;

/// 最近一次成功启动。按 `id` 去重，新的在前。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentRecord {
    pub id: String,
    pub ts: i64,
    /// `wsl` / `powershell` / `ide` / `explorer` / `ssh`
    pub env: String,
    #[serde(rename = "toolName", default)]
    pub tool_name: String,
    #[serde(default)]
    pub command: String,
}

/// `Store::file_path()` 同目录下的 `recent.json`。跟随 `PCS_DATA_DIR` 与 debug/release 数据根。
pub fn recent_file_path() -> PathBuf {
    Store::file_path().with_file_name("recent.json")
}

/// 缺文件、读失败、坏 JSON → 空列表。不隔离、不备份、不写回。
pub fn load_recent() -> Vec<RecentRecord> {
    load_recent_from(&recent_file_path())
}

pub fn load_recent_from(path: &Path) -> Vec<RecentRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_recent(records: &[RecentRecord]) -> Result<(), super::Error> {
    save_recent_to(records, &recent_file_path())
}

/// 原子写：写 `recent.json.tmp` → 删旧文件 → rename。不轮转 `backups\`。
pub fn save_recent_to(records: &[RecentRecord], path: &Path) -> Result<(), super::Error> {
    let Some(parent) = path.parent() else {
        return Err(super::Error::SaveFailed {
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "路径没有父目录"),
        });
    };
    std::fs::create_dir_all(parent).map_err(|source| super::Error::SaveFailed { source })?;
    let json = serde_json::to_string_pretty(records).map_err(|e| super::Error::SaveFailed {
        source: super::error::io_invalid(e.to_string()),
    })?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|source| super::Error::SaveFailed { source })?;
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path).map_err(|source| super::Error::SaveFailed { source })?;
    Ok(())
}

/// 同 id（大小写不敏感）去掉旧条 → 插到队首 → `truncate(MAX_RECENT)`。
pub fn push(items: &mut Vec<RecentRecord>, next: RecentRecord) {
    items.retain(|item| !item.id.eq_ignore_ascii_case(&next.id));
    items.insert(0, next);
    items.truncate(MAX_RECENT);
}

/// 启动成功后写入：load → push → save。空 id 忽略；保存失败只 eprintln。
pub fn record(id: &str, env: &str, tool_name: &str, command: &str) {
    if id.trim().is_empty() {
        return;
    }
    let mut items = load_recent();
    push(
        &mut items,
        RecentRecord {
            id: id.to_string(),
            ts: current_unix_ts(),
            env: env.to_string(),
            tool_name: tool_name.to_string(),
            command: command.to_string(),
        },
    );
    if let Err(e) = save_recent(&items) {
        eprintln!("警告：无法写入最近打开记录: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn rec(id: &str, env: &str, tool: &str, command: &str, ts: i64) -> RecentRecord {
        RecentRecord {
            id: id.into(),
            ts,
            env: env.into(),
            tool_name: tool.into(),
            command: command.into(),
        }
    }

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pcs_recent_test_{stamp}"))
    }

    #[test]
    fn push_same_id_keeps_one_row_at_front_and_refreshes_fields() {
        let mut items = vec![
            rec("aaa", "wsl", "终端", "", 1),
            rec("bbb", "ide", "VS Code", "code", 2),
        ];
        push(&mut items, rec("AAA", "powershell", "终端", "", 9));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "AAA");
        assert_eq!(items[0].env, "powershell");
        assert_eq!(items[0].tool_name, "终端");
        assert_eq!(items[0].command, "");
        assert_eq!(items[0].ts, 9);
        assert_eq!(items[1].id, "bbb");
    }

    #[test]
    fn push_21st_distinct_id_truncates_to_20_and_drops_oldest() {
        let mut items = Vec::new();
        for i in 0..20 {
            push(&mut items, rec(&format!("id-{i}"), "wsl", "终端", "", i));
        }
        assert_eq!(items.len(), 20);
        assert_eq!(items[0].id, "id-19");
        assert_eq!(items[19].id, "id-0");
        push(&mut items, rec("id-20", "ide", "Cursor", "cursor", 20));
        assert_eq!(items.len(), 20);
        assert_eq!(items[0].id, "id-20");
        assert_eq!(items[19].id, "id-1");
        assert!(!items.iter().any(|item| item.id == "id-0"));
    }

    #[test]
    fn load_recent_from_missing_path_is_empty() {
        let path = temp_dir().join("missing").join("recent.json");
        assert!(load_recent_from(&path).is_empty());
    }

    #[test]
    fn load_recent_from_corrupt_json_is_empty_and_does_not_rename() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("recent.json");
        std::fs::write(&path, "{ not valid json").unwrap();
        assert!(load_recent_from(&path).is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not valid json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_recent_to_leaves_dest_and_no_leftover_tmp() {
        let dir = temp_dir();
        let path = dir.join("recent.json");
        let records = vec![rec("aaa", "wsl", "终端", "", 1)];
        assert!(save_recent_to(&records, &path).is_ok());
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let loaded = load_recent_from(&path);
        assert_eq!(loaded, records);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
