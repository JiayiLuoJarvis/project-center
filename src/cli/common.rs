use anyhow::Result;

use crate::domain as ops;
use crate::domain::models::{Project, ProjectData};
use crate::persist::Store;

use super::args::ProjectSelector;

pub(crate) fn find_selected(selector: &ProjectSelector) -> Result<(Project, String)> {
    let data = Store::load();
    let (group_index, project_index) =
        select_project(&data, &selector.name, selector.group.as_deref())?;
    Ok((
        data.groups[group_index].projects[project_index].clone(),
        data.groups[group_index].name.clone(),
    ))
}

/// 名字以 `@` 开头时按项目 id 查找，否则按名字查找。
pub(crate) fn select_project(
    data: &ProjectData,
    name: &str,
    group: Option<&str>,
) -> Result<(usize, usize)> {
    if let Some(id) = name.strip_prefix('@') {
        Ok(ops::find_project_by_id(data, id, group)?)
    } else {
        Ok(ops::find_project(data, name, group)?)
    }
}

/// 把 `@<id>` 解析为真实项目名及所在分组，供 edit / mv / rm 复用现有按名操作；
/// 返回所在分组可避免跨组同名导致的二次查找歧义。
pub(crate) fn resolve_project_name(
    data: &ProjectData,
    name: &str,
    group: Option<&str>,
) -> Result<(String, Option<String>)> {
    if name.starts_with('@') {
        let (group_index, project_index) = select_project(data, name, group)?;
        Ok((
            data.groups[group_index].projects[project_index]
                .name
                .clone(),
            Some(data.groups[group_index].name.clone()),
        ))
    } else {
        Ok((name.to_string(), None))
    }
}

/// 从 stdin 读取一行（`--password-stdin` / `--key-pass-stdin`）。
/// 控制台输入不回显（设计 §6：密码/口令录入不回显）；
/// 重定向/管道走普通读行，脚本喂入不受影响。
pub(crate) fn read_stdin_line(label: &str) -> Result<String> {
    read_hidden_line(&format!("请输入{label}: "))
}

/// 不回显读入一行：控制台走 Win32 逐字符读；重定向/管道回退普通 stdin。
/// 提示语走 stderr，保持 stdout 干净供脚本使用。
#[cfg(windows)]
pub(crate) fn read_hidden_line(prompt: &str) -> Result<String> {
    use windows_sys::Win32::System::Console::{
        ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, GetConsoleMode, GetStdHandle,
        ReadConsoleW, STD_INPUT_HANDLE, SetConsoleMode,
    };
    eprint!("{prompt}");
    use std::io::Write as _;
    std::io::stderr().flush()?;

    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut original = 0;
        if GetConsoleMode(handle, &mut original) != 0 {
            let raw = original & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT) | ENABLE_PROCESSED_INPUT;
            if SetConsoleMode(handle, raw) != 0 {
                let mut chars: Vec<u16> = Vec::new();
                let mut buf = [0u16; 1];
                let mut read;
                loop {
                    read = 0;
                    if ReadConsoleW(
                        handle,
                        buf.as_mut_ptr().cast(),
                        1,
                        &mut read,
                        std::ptr::null_mut(),
                    ) == 0
                        || read == 0
                    {
                        break;
                    }
                    let c = buf[0];
                    match c {
                        0x0D => break,                        // Enter
                        0x0A if chars.is_empty() => continue, // 吞掉上一次读取残留的换行
                        0x08 => {
                            chars.pop(); // Backspace
                        }
                        0x20..=0xFFFE => chars.push(c),
                        _ => {}
                    }
                }
                SetConsoleMode(handle, original);
                eprintln!();
                return Ok(String::from_utf16_lossy(&chars));
            }
        }
    }
    // 非 console stdin（重定向）：退化为普通读行。
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(not(windows))]
pub(crate) fn read_hidden_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    use std::io::Write as _;
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

pub(crate) fn save(data: &ProjectData) -> Result<()> {
    if Store::save(data) {
        Ok(())
    } else {
        Err(crate::Error::Persist(crate::persist::Error::SaveFailed).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Group, Project, ProjectData};

    fn dup_data() -> ProjectData {
        ProjectData {
            groups: vec![
                Group {
                    name: "Work".into(),
                    alias: String::new(),
                    projects: vec![Project {
                        id: "11111111-0000-0000-0000-000000000000".into(),
                        ..Project::new("app", r"E:\w\app", "")
                    }],
                },
                Group {
                    name: "Personal".into(),
                    alias: String::new(),
                    projects: vec![Project {
                        id: "22222222-0000-0000-0000-000000000000".into(),
                        ..Project::new("app", r"E:\p\app", "")
                    }],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn resolve_id_returns_name_and_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "@11111111", None).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group.as_deref(), Some("Work"));
    }

    #[test]
    fn resolve_name_keeps_no_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "app", Some("Work")).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group, None);
    }

    #[test]
    fn resolve_id_with_explicit_group() {
        let data = dup_data();
        let (name, group) = resolve_project_name(&data, "@22222222", Some("Personal")).unwrap();
        assert_eq!(name, "app");
        assert_eq!(group.as_deref(), Some("Personal"));
    }

    #[test]
    fn resolve_id_uses_selector_group() {
        let data = dup_data();
        let result = resolve_project_name(&data, "@11111111", Some("Personal"));
        assert!(result.is_err());
    }
}
