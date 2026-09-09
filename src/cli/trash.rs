use anyhow::{Result, bail};

use crate::domain as ops;
use crate::domain::models::{self, ProjectData};
use crate::persist::Store;

use super::args::TrashCommand;
use super::common::save;

/// 回收站列表行：`[项目] 组名/名称 (删除于 2026-08-12)  @id`；
/// `[分组] 分组名 (N 个项目)  @id`。尾部附 id 便于 `trash restore`。
pub(crate) fn trash_lines(data: &ProjectData) -> Vec<String> {
    data.trash
        .iter()
        .map(|item| {
            let id_suffix = format!("  @{}", item.id);
            if item.is_group() {
                format!(
                    "[分组] {} ({} 个项目){}",
                    item.name,
                    item.projects.len(),
                    id_suffix
                )
            } else {
                format!(
                    "[项目] {}/{} (删除于 {}){}",
                    item.group,
                    item.name,
                    models::format_date(item.deleted_at),
                    id_suffix
                )
            }
        })
        .collect()
}

/// 恢复回收站项；返回提示信息。
pub(crate) fn trash_restore(data: &mut ProjectData, id: &str) -> Result<String> {
    ops::restore_item(data, id).map_err(anyhow::Error::msg)
}

pub(crate) fn cmd_trash(command: TrashCommand) -> Result<()> {
    match command {
        TrashCommand::List => {
            let data = Store::load();
            let lines = trash_lines(&data);
            if lines.is_empty() {
                println!("回收站为空");
            } else {
                for line in lines {
                    println!("{line}");
                }
            }
        }
        TrashCommand::Restore { id } => {
            let mut data = Store::load();
            let message = trash_restore(&mut data, &id)?;
            save(&data)?;
            println!("{message}");
        }
        TrashCommand::Empty { force } => {
            if !force {
                bail!("清空回收站需指定 --force，例如: pcs trash empty --force");
            }
            let mut data = Store::load();
            ops::empty_trash(&mut data);
            save(&data)?;
            println!("回收站已清空。");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{self, Group, Project, ProjectData};

    fn trash_test_data() -> ProjectData {
        ProjectData {
            groups: vec![Group {
                name: "Work".into(),
                alias: String::new(),
                projects: vec![Project {
                    id: "11111111-0000-0000-0000-000000000000".into(),
                    ..Project::new("app", r"E:\w\app", "")
                }],
            }],
            trash: vec![
                models::DeletedItem {
                    id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
                    kind: "project".into(),
                    group: "Work".into(),
                    name: "old-app".into(),
                    alias: String::new(),
                    path: r"E:\old".into(),
                    wsl_path: "/mnt/e/old".into(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    ssh_target: String::new(),
                    ssh_key_file: String::new(),
                    ssh_key_path: String::new(),
                    ssh_password_enc: String::new(),
                    ssh_key_pass_enc: String::new(),
                    projects: Vec::new(),
                    deleted_at: 1700000000,
                },
                models::DeletedItem {
                    id: "bbbbbbbb-0000-0000-0000-000000000000".into(),
                    kind: "group".into(),
                    group: String::new(),
                    name: "Archive".into(),
                    alias: String::new(),
                    path: String::new(),
                    wsl_path: String::new(),
                    default_tool: String::new(),
                    commands: Vec::new(),
                    ssh_target: String::new(),
                    ssh_key_file: String::new(),
                    ssh_key_path: String::new(),
                    ssh_password_enc: String::new(),
                    ssh_key_pass_enc: String::new(),
                    projects: vec![Project::new("inner", r"E:\inner", "")],
                    deleted_at: 1700000000,
                },
            ],
            pending_key_deletes: Vec::new(),
        }
    }

    #[test]
    fn trash_lines_formats_project_and_group() {
        let data = trash_test_data();
        let lines = trash_lines(&data);
        assert_eq!(lines.len(), 2);
        let date = models::format_date(1700000000);
        assert_eq!(
            lines[0],
            format!("[项目] Work/old-app (删除于 {date})  @aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        );
        assert_eq!(
            lines[1],
            "[分组] Archive (1 个项目)  @bbbbbbbb-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn trash_lines_empty() {
        let data = ProjectData::default();
        assert!(trash_lines(&data).is_empty());
    }

    #[test]
    fn trash_restore_unknown_id_errors() {
        let mut data = ProjectData::default();
        assert!(trash_restore(&mut data, "zzzz").is_err());
    }

    #[test]
    fn trash_restore_restores_and_removes_from_trash() {
        let mut data = trash_test_data();
        let message = trash_restore(&mut data, "@aaaaaaaa").unwrap();
        assert_eq!(message, "已恢复项目: old-app");
        assert_eq!(data.groups[0].projects.len(), 2);
        assert_eq!(data.trash.len(), 1);
    }
}
