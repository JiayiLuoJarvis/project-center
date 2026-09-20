use anyhow::{Result, bail};

use crate::domain::models::ProjectData;
use crate::persist as secret;
use crate::persist::{AppConfig, Config, Store};

use super::args::PinCommand;
use super::common::{read_hidden_line, save};

/// 保存 PIN 记录；失败仅警告。
pub(crate) fn save_pin(config: &mut AppConfig, record: Option<secret::PinRecord>) {
    config.pin = record;
    if let Err(e) = Config::save(config) {
        eprintln!("警告：{e}，PIN 状态可能未持久化。");
    }
}

pub(crate) fn cmd_pin(command: PinCommand) -> Result<()> {
    let mut config = Config::load();
    match command {
        PinCommand::Set => {
            if config.pin.is_some() {
                bail!("PIN 已设置，使用 `pcs pin change` 修改或 `pcs pin reset --force` 重置");
            }
            let pin = read_hidden_line("设置 PIN (4-12 位数字): ")?;
            let confirm = read_hidden_line("确认 PIN: ")?;
            if pin != confirm {
                bail!("两次输入不一致");
            }
            let record = secret::pin_record_from(&pin).map_err(anyhow::Error::msg)?;
            save_pin(&mut config, Some(record));
            println!("PIN 已设置。查看密码请用 `pcs secret show <项目>`。");
        }
        PinCommand::Change => {
            let Some(record) = config.pin.clone() else {
                bail!("尚未设置 PIN，先 `pcs pin set`");
            };
            let old = read_hidden_line("当前 PIN: ")?;
            if !secret::verify_pin(&old, &record).map_err(anyhow::Error::msg)? {
                bail!("PIN 不正确");
            }
            let new_pin = read_hidden_line("新 PIN (4-12 位数字): ")?;
            let confirm = read_hidden_line("确认新 PIN: ")?;
            if new_pin != confirm {
                bail!("两次输入不一致");
            }
            let record = secret::pin_record_from(&new_pin).map_err(anyhow::Error::msg)?;
            save_pin(&mut config, Some(record));
            println!("PIN 已修改。");
        }
        PinCommand::Reset { force } => {
            if config.pin.is_none() {
                bail!("尚未设置 PIN");
            }
            if !force {
                println!("这将清除 PIN 并清空所有已存密码、口令与密钥文件，且不可恢复！");
                let confirm = read_hidden_line("确认请输入 yes: ")?;
                if !confirm.eq_ignore_ascii_case("yes") {
                    bail!("已取消");
                }
            }
            let mut data = Store::load();
            clear_all_saved_secrets(&mut data);
            save(&data)?;
            save_pin(&mut config, None);
            println!("PIN 与全部已存秘密已清除。SSH 项目仍可手动输密码/口令登录。");
        }
    }
    Ok(())
}

/// 清空全部连接秘密，并删除整个密钥目录。
/// 目录删除失败仅警告：字段已清，残留文件由下次启动的孤儿清理兜底。
pub(crate) fn clear_all_saved_secrets(data: &mut ProjectData) {
    data.clear_all_connection_secrets();
    if let Err(e) = std::fs::remove_dir_all(secret::data_root().join("keys"))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("警告：密钥目录删除失败（{e}），残留文件将在下次启动时清理。");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Connection, DeletedItem, Group, Project};

    #[test]
    fn clear_all_saved_secrets_clears_connections_only() {
        let conn = Connection {
            id: "c1".into(),
            host: "h".into(),
            ssh_key_file: "keys/c1.key".into(),
            ssh_password_enc: "PW".into(),
            ssh_key_pass_enc: "KP".into(),
            ..Connection::default()
        };

        let live = Project::new("app", "/opt/a", "").with_connection("c1");
        let gone = Project::new("gone", "/opt/b", "").with_connection("c1");

        let mut data = ProjectData {
            groups: vec![Group {
                name: "G".into(),
                alias: String::new(),
                projects: vec![live],
            }],
            trash: vec![DeletedItem::from_project(&gone, "G", 1)],
            connections: vec![conn],
            pending_key_deletes: vec!["keys/stale.key".into()],
        };

        data.clear_all_connection_secrets();

        assert!(data.connections[0].ssh_key_file.is_empty());
        assert!(data.connections[0].ssh_password_enc.is_empty());
        assert!(data.connections[0].ssh_key_pass_enc.is_empty());
        assert_eq!(data.groups[0].projects[0].connection_id, "c1");
        assert_eq!(data.trash[0].connection_id, "c1");
        assert!(data.pending_key_deletes.is_empty());
    }
}
