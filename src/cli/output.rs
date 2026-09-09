use anyhow::Result;

use crate::domain as ops;
use crate::domain::models::{self, ProjectData};
use crate::persist::Store;

pub(crate) fn cmd_ls(group: Option<&str>, json: bool) -> Result<()> {
    let data = Store::load();
    if let Some(group_name) = group {
        let index = ops::find_group(&data, group_name)?;
        if json {
            let selected = ProjectData {
                groups: vec![data.groups[index].clone()],
                ..Default::default()
            };
            println!("{}", serde_json::to_string_pretty(&selected)?);
        } else {
            print_group(&data.groups[index]);
        }
        return Ok(());
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else if data.groups.is_empty() {
        println!("暂无项目。");
    } else {
        for group in &data.groups {
            print_group(group);
        }
    }
    Ok(())
}

pub(crate) fn print_group(group: &models::Group) {
    println!("[{}]", group.name);
    if group.projects.is_empty() {
        println!("  （暂无项目）");
    } else {
        for project in &group.projects {
            let path = if project.has_windows_path() {
                project.path.clone()
            } else {
                project.linux_path()
            };
            if path.is_empty() {
                println!("  {}", project.name);
            } else {
                println!("  {}  ({})", project.name, path);
            }
        }
    }
}
