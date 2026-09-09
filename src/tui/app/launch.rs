use super::*;

impl App {
    pub fn open_launch_picker(
        &mut self,
        data: &ProjectData,
        config: &AppConfig,
        group: &str,
        project_id: &str,
    ) {
        let Some(project) = actions::find_project_ref(data, group, project_id) else {
            self.flash("未找到项目");
            return;
        };
        let (options, labels) = actions::launch_labels(project, config);
        if options.is_empty() {
            self.flash("无可用启动方式");
            return;
        }
        self.mode = Mode::LaunchPicker {
            group: group.to_string(),
            project_id: project_id.to_string(),
            options,
            labels,
            selected: 0,
            filter: String::new(),
        };
    }

    pub fn launch_filter_indices(
        options: &[LaunchOption],
        labels: &[String],
        filter: &str,
    ) -> Vec<usize> {
        if filter.is_empty() {
            return (0..options.len()).collect();
        }
        let q = filter.to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(i, opt)| {
                let label = labels.get(*i).map(|s| s.as_str()).unwrap_or("");
                label.to_lowercase().contains(&q)
                    || opt.tool_name.to_lowercase().contains(&q)
                    || opt.command.to_lowercase().contains(&q)
                    || opt.env.label().to_lowercase().contains(&q)
                    || opt.env.short_label().to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(crate) fn handle_launch_picker(&mut self, key: KeyEvent, data: &ProjectData) -> Outcome {
        let Mode::LaunchPicker {
            group,
            project_id,
            options,
            labels,
            selected,
            filter,
        } = self.mode.clone()
        else {
            return Outcome::Continue;
        };
        let indices = Self::launch_filter_indices(&options, &labels, &filter);
        let set = |filter: String, selected: usize| Mode::LaunchPicker {
            group: group.clone(),
            project_id: project_id.clone(),
            options: options.clone(),
            labels: labels.clone(),
            selected,
            filter,
        };
        match key.code {
            KeyCode::Esc => {
                if !filter.is_empty() {
                    self.mode = set(String::new(), 0);
                } else if self.short_session {
                    return Outcome::Quit;
                } else {
                    self.back_to_browse();
                }
            }
            KeyCode::Backspace => {
                let mut filter = filter;
                filter.pop();
                self.mode = set(filter, 0);
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
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let mut filter = filter;
                filter.push(c);
                let indices = Self::launch_filter_indices(&options, &labels, &filter);
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
                if let Some(option) = options.get(idx).cloned()
                    && let Some(project) = actions::find_project_ref(data, &group, &project_id)
                {
                    let exit_after = self.exit_after_launch || self.short_session;
                    if !self.short_session {
                        self.back_to_browse();
                    }
                    return Outcome::Launch {
                        project: Box::new(project.clone()),
                        group,
                        option,
                        exit_after,
                    };
                }
            }
            _ => {}
        }
        Outcome::Continue
    }
}
