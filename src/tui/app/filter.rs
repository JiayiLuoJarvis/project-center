use super::*;

impl App {
    pub(crate) fn matches_filter(&self, text: &str) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        text.to_lowercase().contains(&self.filter.to_lowercase())
    }

    #[cfg(test)]
    pub fn filtered_group_indices(&self, data: &ProjectData) -> Vec<usize> {
        self.left_items(data)
            .into_iter()
            .filter_map(|item| match item {
                LeftItem::Group(i) => Some(i),
                _ => None,
            })
            .collect()
    }

    pub fn filtered_project_indices(&self, data: &ProjectData, gi: usize) -> Vec<usize> {
        data.groups
            .get(gi)
            .map(|g| {
                g.projects
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        if self.focus != Focus::Projects || self.filter.is_empty() {
                            return true;
                        }
                        self.matches_filter(&p.name)
                            || self.matches_filter(&p.alias)
                            || self.matches_filter(&p.path)
                            || self.matches_filter(&p.linux_path())
                            || (!p.id.is_empty()
                                && p.id
                                    .get(..self.filter.len())
                                    .is_some_and(|prefix| self.matches_filter(prefix)))
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn filtered_trash_indices(&self, data: &ProjectData) -> Vec<usize> {
        data.trash
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&actions::trash_label(item))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_tool_indices(&self, config: &AppConfig, env: ConfigEnv) -> Vec<usize> {
        env.tools(config)
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&t.name) || self.matches_filter(&t.command)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_command_indices(&self, project: &Project) -> Vec<usize> {
        project
            .commands
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                if self.focus != Focus::Projects || self.filter.is_empty() {
                    return true;
                }
                self.matches_filter(&c.name) || self.matches_filter(&c.command)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(crate) fn handle_filter(
        &mut self,
        key: KeyEvent,
        data: &mut ProjectData,
        config: &mut AppConfig,
    ) -> Outcome {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.mode = Mode::Browse;
                self.clamp_selection(data, config);
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                // 清空过滤串前，把过滤视图中的选中位置换算成完整列表下标，
                // 避免清空后高亮错位。
                match self.focus {
                    Focus::Groups => {
                        let pos = self.map_left_selection(data);
                        self.filter.clear();
                        self.left_sel = pos;
                    }
                    Focus::Projects => {
                        let pi = self.map_right_selection(data, config);
                        self.filter.clear();
                        self.right_sel = pi.unwrap_or(0);
                    }
                }
                self.clamp_selection(data, config);
                return self.on_enter(data, config);
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.reselect_after_filter(data, config);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_sel(1, data, config);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_sel(-1, data, config);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.reselect_after_filter(data, config);
            }
            _ => {}
        }
        Outcome::Continue
    }

    /// 过滤视图中左栏选中项在完整列表中的下标；
    /// 回收站与配置固定在末尾，无匹配时回退到回收站。
    pub(crate) fn map_left_selection(&self, data: &ProjectData) -> usize {
        match self.left_items(data).get(self.left_sel) {
            Some(LeftItem::Group(gi)) => *gi,
            Some(LeftItem::Config) => data.groups.len() + 1,
            _ => data.groups.len(),
        }
    }

    /// 过滤视图中右栏选中项在完整列表中的下标；
    /// 环境列表不参与过滤，保持原选中。
    pub(crate) fn map_right_selection(
        &self,
        data: &ProjectData,
        config: &AppConfig,
    ) -> Option<usize> {
        match self.right_pane.clone() {
            RightPane::Projects => self.left_is_group(data, self.left_sel).and_then(|gi| {
                self.filtered_project_indices(data, gi)
                    .get(self.right_sel)
                    .copied()
            }),
            RightPane::Trash => self
                .filtered_trash_indices(data)
                .get(self.right_sel)
                .copied(),
            RightPane::ConfigTools { env } => self
                .filtered_tool_indices(config, env)
                .get(self.right_sel)
                .copied(),
            RightPane::Commands { group, project_id } => {
                actions::find_project_ref(data, &group, &project_id).and_then(|p| {
                    self.filtered_command_indices(p)
                        .get(self.right_sel)
                        .copied()
                })
            }
            RightPane::ConfigEnvs => Some(self.right_sel),
        }
    }

    /// 过滤串变化后，把选中项重定位到首个匹配结果。
    pub(crate) fn reselect_after_filter(&mut self, data: &ProjectData, config: &AppConfig) {
        if self.focus == Focus::Groups {
            self.left_sel = 0;
            self.right_sel = 0;
            self.sync_right_pane(data);
        } else {
            self.right_sel = 0;
        }
        self.clamp_selection(data, config);
    }
}
