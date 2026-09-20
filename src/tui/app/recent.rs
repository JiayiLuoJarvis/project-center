use super::*;
use crate::launch::LaunchEnv;
use crate::persist::RecentRecord;

impl App {
    pub fn open_recent_picker(&mut self, data: &ProjectData) {
        let records = crate::persist::load_recent();
        let visible = Self::recent_visible_indices(&records, data, "");
        let selected = visible.first().copied().unwrap_or(0);
        self.mode = Mode::RecentPicker {
            records,
            selected,
            filter: String::new(),
        };
    }

    /// 仍存在的项目下标。已删或不在任何分组的 id 跳过，不挡后面的行。
    pub fn recent_visible_indices(
        records: &[RecentRecord],
        data: &ProjectData,
        filter: &str,
    ) -> Vec<usize> {
        let q = filter.to_lowercase();
        records
            .iter()
            .enumerate()
            .filter(|(_, record)| {
                let Some(label) = Self::recent_row_label(record, data) else {
                    return false;
                };
                if q.is_empty() {
                    return true;
                }
                let Some((_, project)) = find_live(data, &record.id) else {
                    return false;
                };
                let method = method_label(&label);
                project.name.to_lowercase().contains(&q)
                    || project.alias.to_lowercase().contains(&q)
                    || method.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// `分组 / 项目名 · 方式标签`。项目不存在则 `None`。
    /// `env` 能解析时用 `LaunchOption::label()`。否则用原始 `env` 与 `tool_name`。
    pub fn recent_row_label(record: &RecentRecord, data: &ProjectData) -> Option<String> {
        let (group, project) = find_live(data, &record.id)?;
        let method = match option_from_record(record, project) {
            Some(option) => option.label(),
            None => format!("{} {}", record.env, record.tool_name)
                .trim()
                .to_string(),
        };
        Some(format!("{} / {} · {}", group, project.name, method))
    }

    pub(crate) fn handle_recent_picker(
        &mut self,
        key: KeyEvent,
        data: &ProjectData,
        config: &AppConfig,
    ) -> Outcome {
        let Mode::RecentPicker {
            records,
            selected,
            filter,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        let indices = Self::recent_visible_indices(&records, data, &filter);
        let set = |filter: String, selected: usize| Mode::RecentPicker {
            records: records.clone(),
            selected,
            filter,
        };
        match key.code {
            KeyCode::Esc => {
                if !filter.is_empty() {
                    let visible = Self::recent_visible_indices(&records, data, "");
                    self.mode = set(String::new(), visible.first().copied().unwrap_or(0));
                } else {
                    self.back_to_browse();
                }
            }
            KeyCode::Backspace => {
                let mut filter = filter;
                filter.pop();
                let indices = Self::recent_visible_indices(&records, data, &filter);
                let selected = indices.first().copied().unwrap_or(0);
                self.mode = set(filter, selected);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let n = indices.len();
                if n > 0 {
                    let pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
                    let next = indices[(pos + 1) % n];
                    self.mode = set(filter, next);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let n = indices.len();
                if n > 0 {
                    let pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
                    let next = indices[(pos + n - 1) % n];
                    self.mode = set(filter, next);
                }
            }
            KeyCode::Char(' ') => {
                if let Some((group, project_id)) = selected_live(&records, &indices, selected, data)
                {
                    self.open_launch_picker(data, config, group, project_id);
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut filter = filter;
                filter.push(c);
                let indices = Self::recent_visible_indices(&records, data, &filter);
                let selected = indices.first().copied().unwrap_or(0);
                self.mode = set(filter, selected);
            }
            KeyCode::Enter => {
                if indices.is_empty() {
                    return Outcome::Continue;
                }
                let idx = if indices.contains(&selected) {
                    selected
                } else {
                    indices[0]
                };
                let record = &records[idx];
                let Some((group, project)) = find_live(data, &record.id) else {
                    return Outcome::Continue;
                };
                let Some(option) = option_from_record(record, project) else {
                    self.flash("无法解析上次启动方式");
                    return Outcome::Continue;
                };
                let exit_after = self.exit_after_launch || self.short_session;
                if !self.short_session {
                    self.back_to_browse();
                }
                return Outcome::Launch {
                    project: Box::new(project.clone()),
                    group: group.to_string(),
                    option,
                    exit_after,
                };
            }
            _ => {}
        }
        Outcome::Continue
    }
}

fn selected_live<'a>(
    records: &'a [RecentRecord],
    indices: &[usize],
    selected: usize,
    data: &'a ProjectData,
) -> Option<(&'a str, &'a str)> {
    if indices.is_empty() {
        return None;
    }
    let idx = if indices.contains(&selected) {
        selected
    } else {
        indices[0]
    };
    let (group, project) = find_live(data, &records[idx].id)?;
    Some((group, project.id.as_str()))
}

/// 精确 id 匹配（大小写不敏感）。
fn find_live<'a>(data: &'a ProjectData, id: &str) -> Option<(&'a str, &'a Project)> {
    for group in &data.groups {
        if let Some(project) = group
            .projects
            .iter()
            .find(|project| project.id.eq_ignore_ascii_case(id))
        {
            return Some((group.name.as_str(), project));
        }
    }
    None
}

fn option_from_record(record: &RecentRecord, project: &Project) -> Option<LaunchOption> {
    let env = LaunchEnv::from_recent_str(&record.env)?;
    let is_custom = project
        .commands
        .iter()
        .any(|c| c.name == record.tool_name && c.command == record.command);
    Some(LaunchOption {
        env,
        tool_name: record.tool_name.clone(),
        command: record.command.clone(),
        is_custom,
    })
}

fn method_label(row: &str) -> &str {
    match row.split_once(" · ") {
        Some((_, method)) => method,
        None => row,
    }
}
