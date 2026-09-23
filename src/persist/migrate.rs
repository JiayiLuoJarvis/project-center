//! 跨机迁移：剥离秘密后的导出/导入包。

use std::path::{Path, PathBuf};

use crate::domain::models::ProjectData;
use crate::persist::{AppConfig, Config, Store};

use super::error::io_invalid;

/// 迁移包：项目数据 + 启动工具配置（不含 PIN / DPAPI / keys）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigratePack {
    pub projects: ProjectData,
    pub config: AppConfig,
}

impl MigratePack {
    /// 从当前内存状态打包；剥离连接密文、密钥引用与 PIN。
    pub fn pack(data: &ProjectData, config: &AppConfig) -> Self {
        Self {
            projects: strip_project_secrets(data),
            config: strip_config_pin(config),
        }
    }

    /// 再剥离一次（防御导入侧未清洗的文件），返回可替换的数据与配置。
    pub fn unpack(self) -> Self {
        Self {
            projects: strip_project_secrets(&self.projects),
            config: strip_config_pin(&self.config),
        }
    }
}

/// 清空 DPAPI 密文字段、密钥 sidecar 引用与待删密钥列表。
pub fn strip_project_secrets(data: &ProjectData) -> ProjectData {
    let mut out = data.clone();
    for conn in &mut out.connections {
        conn.ssh_password_enc.clear();
        conn.ssh_key_pass_enc.clear();
        conn.ssh_key_file.clear();
    }
    out.pending_key_deletes.clear();
    out
}

/// 导出配置不含 PIN 校验记录。
pub fn strip_config_pin(config: &AppConfig) -> AppConfig {
    let mut out = config.clone();
    out.pin = None;
    out
}

/// 写入迁移 JSON（pretty）。
pub fn write_pack(path: &Path, pack: &MigratePack) -> Result<(), super::Error> {
    let json = serde_json::to_string_pretty(pack).map_err(|e| super::Error::SaveFailed {
        source: io_invalid(e.to_string()),
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| super::Error::SaveFailed { source })?;
    }
    std::fs::write(path, json).map_err(|source| super::Error::SaveFailed { source })?;
    Ok(())
}

/// 读取并校验迁移 JSON。
pub fn read_pack(path: &Path) -> Result<MigratePack, super::Error> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| super::Error::Message(format!("无法读取迁移文件: {source}")))?;
    let pack: MigratePack = serde_json::from_str(&text)
        .map_err(|e| super::Error::Message(format!("迁移文件无效: {e}")))?;
    Ok(pack.unpack())
}

/// 替换内存中的数据与配置并落盘。
/// `Store::save` 写入前会把当前非空 `projects.json` 轮转入 `backups/`。
pub fn import_replace(
    data: &mut ProjectData,
    config: &mut AppConfig,
    pack: MigratePack,
) -> Result<String, super::Error> {
    let pack = pack.unpack();
    *data = pack.projects;
    *config = pack.config;
    Store::save(data)?;
    Config::save(config)?;
    Ok("已导入配置（秘密与 PIN 未迁移）".into())
}

/// 备份条目：文件名 + 分组/项目计数摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub name: String,
    pub path: PathBuf,
    pub group_count: usize,
    pub project_count: usize,
}

impl BackupEntry {
    pub fn summary_line(&self) -> String {
        format!(
            "{} · {} 组 · {} 项目",
            self.name
                .trim_start_matches("projects-")
                .trim_end_matches(".json"),
            self.group_count,
            self.project_count
        )
    }
}

/// 列出 `backups/projects-*.json`，最新在前；无法解析的仍列出但计数为 0。
pub fn list_backups(data_root: &Path) -> Vec<BackupEntry> {
    Store::list_backups_in(&data_root.join("backups"))
}

/// 从备份名恢复：`save_to` 会先轮转当前非空主文件，再写入快照。
pub fn restore_backup(data: &mut ProjectData, backup_name: &str) -> Result<String, super::Error> {
    let path = Store::file_path();
    let Some(root) = path.parent() else {
        return Err(super::Error::Message("数据路径无效".into()));
    };
    let backup_path = root.join("backups").join(backup_name);
    let text = std::fs::read_to_string(&backup_path)
        .map_err(|source| super::Error::Message(format!("无法读取备份: {source}")))?;
    let snapshot: ProjectData =
        serde_json::from_str(&text).map_err(|e| super::Error::Message(format!("备份损坏: {e}")))?;
    *data = snapshot;
    Store::save_to(data, &path)?;
    Ok(format!("已从备份恢复: {backup_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Connection, Group, Project};
    use crate::persist::{PinRecord, Tool, test_env};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pcs_migrate_test_{stamp}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn sample_data() -> ProjectData {
        let conn = Connection {
            name: "box".into(),
            host: "h".into(),
            ssh_password_enc: "DPAPI_BLOB".into(),
            ssh_key_pass_enc: "KEY_PASS_BLOB".into(),
            ssh_key_file: "keys/abc.bin".into(),
            ..Default::default()
        };
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![{
                    let mut p = Project::new("app", r"E:\dev\app", "");
                    p.git_remote = "https://example.com/app.git".into();
                    p.notes = "备注".into();
                    p
                }],
            }],
            connections: vec![conn],
            pending_key_deletes: vec!["keys/old.bin".into()],
            ..Default::default()
        }
    }

    fn sample_config() -> AppConfig {
        AppConfig {
            wsl: vec![Tool::new("x", "x")],
            powershell: vec![],
            ide: vec![],
            pin: Some(PinRecord {
                salt: "s".into(),
                iterations: 1,
                hash: "h".into(),
            }),
        }
    }

    #[test]
    fn migrate_pack_strips_secrets_and_pin() {
        let pack = MigratePack::pack(&sample_data(), &sample_config());
        let json = serde_json::to_string(&pack).unwrap();
        assert!(!json.contains("DPAPI_BLOB"));
        assert!(!json.contains("KEY_PASS_BLOB"));
        assert!(!json.contains("keys/abc.bin"));
        assert!(!json.contains("keys/old.bin"));
        assert!(!json.contains("\"pin\""));
        assert!(json.contains("gitRemote") || json.contains("https://example.com/app.git"));
        assert!(pack.projects.connections[0].ssh_password_enc.is_empty());
        assert!(pack.projects.connections[0].ssh_key_file.is_empty());
        assert!(pack.projects.pending_key_deletes.is_empty());
        assert!(pack.config.pin.is_none());
        assert_eq!(pack.config.wsl[0].name, "x");
    }

    #[test]
    fn migrate_import_backs_up_non_empty() {
        let _lock = test_env::lock_appdata();
        let dir = temp_dir();
        let _appdata = test_env::AppdataGuard::redirect(&dir);
        let data_root = dir.join(if cfg!(debug_assertions) {
            "project_center_dev"
        } else {
            "project_center"
        });
        let _ = std::fs::create_dir_all(&data_root);
        let path = data_root.join("projects.json");
        let prior = sample_data();
        Store::save_to(&prior, &path).unwrap();

        let incoming = ProjectData {
            groups: vec![Group {
                name: "Other".into(),
                alias: String::new(),
                projects: vec![Project::new("b", r"E:\b", "")],
            }],
            ..Default::default()
        };
        let pack = MigratePack::pack(&incoming, &AppConfig::defaults());
        let mut live = prior.clone();
        let mut cfg = sample_config();
        let cfg_path = dir.join("config.json");
        let _cfg_guard = test_env::ConfigPathGuard::redirect(&cfg_path);
        import_replace(&mut live, &mut cfg, pack).unwrap();

        assert_eq!(live.groups[0].name, "Other");
        assert!(cfg.pin.is_none());
        let backups = list_backups(&data_root);
        assert!(!backups.is_empty(), "non-empty prior must produce a backup");
        let newest = &backups[0];
        let text = std::fs::read_to_string(&newest.path).unwrap();
        let snapped: ProjectData = serde_json::from_str(&text).unwrap();
        assert_eq!(snapped.groups[0].name, "Work");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_list_newest_first_and_restore() {
        let _lock = test_env::lock_appdata();
        let dir = temp_dir();
        let _appdata = test_env::AppdataGuard::redirect(&dir);
        let data_root = dir.join(if cfg!(debug_assertions) {
            "project_center_dev"
        } else {
            "project_center"
        });
        let _ = std::fs::create_dir_all(&data_root);
        let path = data_root.join("projects.json");

        let first = ProjectData {
            groups: vec![Group {
                name: "A".into(),
                alias: String::new(),
                projects: vec![Project::new("a1", r"E:\a", "")],
            }],
            ..Default::default()
        };
        Store::save_to(&first, &path).unwrap();
        let second = ProjectData {
            groups: vec![Group {
                name: "B".into(),
                alias: String::new(),
                projects: vec![
                    Project::new("b1", r"E:\b1", ""),
                    Project::new("b2", r"E:\b2", ""),
                ],
            }],
            ..Default::default()
        };
        Store::save_to(&second, &path).unwrap();

        let list = list_backups(&data_root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].group_count, 1);
        assert_eq!(list[0].project_count, 1);
        assert!(list[0].name.starts_with("projects-"));

        // 再写一次，列表应 newest-first 且长度增长
        Store::save_to(&second, &path).unwrap();
        let list = list_backups(&data_root);
        assert!(list.len() >= 2);
        assert!(list[0].name >= list[1].name);

        let mut live = second.clone();
        let name = list[0].name.clone();
        restore_backup(&mut live, &name).unwrap();
        // 恢复后主文件内容等于该备份；且恢复前又多了一份当前快照
        let after = list_backups(&data_root);
        assert!(after.len() > list.len() || after.len() == list.len());
        assert_eq!(
            live.groups[0].name,
            serde_json::from_str::<ProjectData>(
                &std::fs::read_to_string(data_root.join("backups").join(&name)).unwrap()
            )
            .unwrap()
            .groups[0]
                .name
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
