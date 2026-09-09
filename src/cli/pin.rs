use anyhow::{Result, bail};

use crate::domain::models::{Project, ProjectData};
use crate::persist as secret;
use crate::persist::{AppConfig, Config, Store};

use super::args::PinCommand;
use super::common::{read_hidden_line, save};

/// 保存 PIN 记录；失败仅警告。
pub(crate) fn save_pin(config: &mut AppConfig, record: Option<secret::PinRecord>) {
    config.pin = record;
    if !Config::save(config) {
        eprintln!("警告：无法保存 config.json，PIN 状态可能未持久化。");
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

/// 清空 groups 与 trash 快照中的全部秘密字段，并删除整个密钥目录。
/// 目录删除失败仅警告：字段已清，残留文件由下次启动的孤儿清理兜底。
pub(crate) fn clear_all_saved_secrets(data: &mut ProjectData) {
    let clear = |p: &mut Project| {
        p.ssh_key_file.clear();
        p.ssh_key_path.clear();
        p.ssh_password_enc.clear();
        p.ssh_key_pass_enc.clear();
    };
    for project in data.groups.iter_mut().flat_map(|g| g.projects.iter_mut()) {
        clear(project);
    }
    for item in &mut data.trash {
        item.ssh_key_file.clear();
        item.ssh_key_path.clear();
        item.ssh_password_enc.clear();
        item.ssh_key_pass_enc.clear();
        for project in &mut item.projects {
            clear(project);
        }
    }
    data.pending_key_deletes.clear();
    if let Err(e) = std::fs::remove_dir_all(secret::data_root().join("keys"))
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("警告：密钥目录删除失败（{e}），残留文件将在下次启动时清理。");
    }
}
