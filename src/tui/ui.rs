use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::domain::models::ProjectData;
use crate::persist::AppConfig;
use crate::tui::app::{App, Focus, FormField, Mode, RightPane};
use crate::tui::theme;
use crate::tui::widgets;

pub fn render(frame: &mut Frame, app: &App, data: &ProjectData, config: &AppConfig) {
    let area = frame.area();
    frame.render_widget(Block::default().style(theme::base()), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    render_top(frame, chunks[0], app, data);
    render_body(frame, chunks[1], app, data, config);
    render_bottom(frame, chunks[2], app);

    match &app.mode {
        Mode::LaunchPicker {
            options,
            labels,
            selected,
            filter,
            ..
        } => render_launch_picker(frame, area, options, labels, *selected, filter),
        Mode::ActionMenu {
            items, selected, ..
        } => render_modal_list(frame, area, "操作", items, *selected),
        Mode::ListPicker {
            title,
            items,
            selected,
            ..
        } => render_modal_list(frame, area, title, items, *selected),
        Mode::Help => render_help(frame, area),
        Mode::Confirm { message, .. } => render_confirm(frame, area, message),
        Mode::SecretViewer {
            pin_input,
            revealed,
            error,
            ..
        } => render_secret_viewer(frame, area, pin_input, revealed.as_ref(), error.as_deref()),
        Mode::Form {
            title,
            fields,
            focus,
            error,
            ..
        } => render_form(frame, area, title, fields, *focus, error.as_deref()),
        Mode::Filter => render_input(frame, chunks[2], "/", &app.filter, true),
        Mode::Browse => {}
    }

    if app.quit_confirm {
        render_confirm(frame, area, "确认退出？");
    }
}

fn render_top(frame: &mut Frame, area: Rect, app: &App, data: &ProjectData) {
    let ctx = match &app.right_pane {
        RightPane::Projects => {
            if let Some(gi) = app.left_is_group(data, app.left_sel) {
                let g = &data.groups[gi];
                format!("{} · {} 项目", g.name, g.projects.len())
            } else {
                String::new()
            }
        }
        RightPane::Trash => format!("回收站 · {} 项", data.trash.len()),
        RightPane::ConfigEnvs | RightPane::ConfigTools { .. } => "配置".into(),
        RightPane::Commands { .. } => "自定义命令".into(),
    };
    let line = Line::from(vec![
        Span::styled(" pcs · Project Center ", theme::title()),
        Span::styled("─ ", theme::muted()),
        Span::styled(ctx, theme::muted()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App, data: &ProjectData, config: &AppConfig) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
        .split(area);

    render_left(frame, cols[0], app, data);
    render_right(frame, cols[1], app, data, config);
}

fn render_left(frame: &mut Frame, area: Rect, app: &App, data: &ProjectData) {
    let focused = app.focus == Focus::Groups && matches!(app.mode, Mode::Browse | Mode::Filter);
    let title = if app.filter.is_empty() || app.focus != Focus::Groups {
        " GROUPS ".to_string()
    } else {
        format!(" GROUPS /{} ", app.filter)
    };
    let block = Block::default()
        .title(Span::styled(title, theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(focused))
        .style(theme::panel());

    let mut items: Vec<ListItem> = Vec::new();
    for item in app.left_items(data) {
        match item {
            crate::tui::app::LeftItem::Group(gi) => {
                let g = &data.groups[gi];
                let label = if g.alias.trim().is_empty() {
                    format!("{}  {}", g.name, g.projects.len())
                } else {
                    format!("{} [{}]  {}", g.name, g.alias, g.projects.len())
                };
                items.push(widgets::simple_item(label));
            }
            crate::tui::app::LeftItem::Trash => {
                items.push(widgets::simple_item(format!(
                    "回收站  {}",
                    data.trash.len()
                )));
            }
            crate::tui::app::LeftItem::Config => {
                items.push(widgets::simple_item("配置"));
            }
        }
    }

    let mut state = ListState::default();
    state.select(Some(app.left_sel.min(items.len().saturating_sub(1))));

    let list = List::new(items)
        .block(block)
        .highlight_style(widgets::selected_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_right(frame: &mut Frame, area: Rect, app: &App, data: &ProjectData, config: &AppConfig) {
    let focused = app.focus == Focus::Projects && matches!(app.mode, Mode::Browse | Mode::Filter);
    let (title, items) = right_content(app, data, config);
    let block = Block::default()
        .title(Span::styled(format!(" {title} "), theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(focused))
        .style(theme::panel());

    if items.is_empty() {
        let empty = match app.right_pane {
            RightPane::Trash => "回收站为空",
            RightPane::Projects => "（暂无项目）",
            RightPane::ConfigTools { .. } => "（暂无工具）",
            RightPane::Commands { .. } => "（暂无命令）",
            _ => "（空）",
        };
        frame.render_widget(
            Paragraph::new(Span::styled(empty, theme::muted()))
                .block(block)
                .style(theme::panel()),
            area,
        );
        return;
    }

    let mut state = ListState::default();
    state.select(Some(app.right_sel.min(items.len().saturating_sub(1))));
    let list = List::new(items)
        .block(block)
        .highlight_style(widgets::selected_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn right_content<'a>(
    app: &App,
    data: &'a ProjectData,
    config: &'a AppConfig,
) -> (String, Vec<ListItem<'static>>) {
    match &app.right_pane {
        RightPane::Projects => {
            if let Some(gi) = app.left_is_group(data, app.left_sel) {
                let name = data.groups[gi].name.clone();
                let indices = app.filtered_project_indices(data, gi);
                let items = indices
                    .into_iter()
                    .map(|i| widgets::project_item(&data.groups[gi].projects[i], config))
                    .collect();
                (format!("PROJECTS · {name}"), items)
            } else {
                ("PROJECTS".into(), vec![])
            }
        }
        RightPane::Trash => {
            let indices = app.filtered_trash_indices(data);
            let items = indices
                .into_iter()
                .map(|i| widgets::trash_item(&data.trash[i]))
                .collect();
            ("TRASH".into(), items)
        }
        RightPane::ConfigEnvs => (
            "CONFIG".into(),
            vec![
                widgets::simple_item("WSL 工具"),
                widgets::simple_item("PowerShell 工具"),
                widgets::simple_item("IDE 工具"),
                widgets::simple_item("恢复默认配置"),
            ],
        ),
        RightPane::ConfigTools { env } => {
            let indices = app.filtered_tool_indices(config, *env);
            let tools = env.tools(config);
            let items = indices
                .into_iter()
                .map(|i| {
                    let t = &tools[i];
                    widgets::simple_item(format!("{} ({})", t.name, t.command))
                })
                .collect();
            (format!("{} 工具", env.label()), items)
        }
        RightPane::Commands { group, project_id } => {
            let Some(project) = crate::tui::actions::find_project_ref(data, group, project_id)
            else {
                return ("COMMANDS".into(), vec![]);
            };
            let indices = app.filtered_command_indices(project);
            let items = indices
                .into_iter()
                .map(|i| {
                    widgets::simple_item(crate::tui::actions::command_label(&project.commands[i]))
                })
                .collect();
            (format!("命令 · {}", project.name), items)
        }
    }
}

fn render_bottom(frame: &mut Frame, area: Rect, app: &App) {
    if !matches!(app.mode, Mode::Browse) {
        if let Some(flash) = &app.flash {
            frame.render_widget(
                Paragraph::new(Span::styled(flash.clone(), theme::warn())).style(theme::base()),
                area,
            );
        }
        return;
    }
    if let Some(flash) = &app.flash {
        frame.render_widget(
            Paragraph::new(Span::styled(flash.clone(), theme::success())).style(theme::base()),
            area,
        );
        return;
    }
    let help = match app.right_pane {
        RightPane::Trash => "enter/o 操作  r 恢复  d 彻底删除  D 清空  / 过滤  ? 帮助  q 退出",
        RightPane::ConfigEnvs | RightPane::ConfigTools { .. } => {
            "enter 进入  a 新增  e 编辑  d 删除  Esc 返回  ? 帮助  q 退出"
        }
        RightPane::Commands { .. } => "enter/o 操作  a 新增  e 编辑  d 删除  Esc 返回  q 退出",
        RightPane::Projects => {
            "enter 打开  o 操作  a 新增  e 编辑  d 删除  m 移动  / 过滤  ? 帮助  q 退出"
        }
    };
    let line = Line::from(vec![
        Span::styled(" ", theme::muted()),
        Span::styled(help, theme::muted()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
}

fn modal_area(area: Rect) -> Rect {
    let h = area.height.saturating_mul(60) / 100;
    let w = area.width.saturating_mul(50) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w.max(20), h.max(8))
}

fn render_modal_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
) {
    let area = modal_area(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(format!(" {title} "), theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(true))
        .style(theme::panel());
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|s| widgets::simple_item(s.clone()))
        .collect();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(selected.min(items.len() - 1)));
    }
    let list = List::new(list_items)
        .block(block)
        .highlight_style(widgets::selected_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_launch_picker(
    frame: &mut Frame,
    area: Rect,
    options: &[crate::launch::LaunchOption],
    labels: &[String],
    selected: usize,
    filter: &str,
) {
    use crate::tui::app::App;
    let area = modal_area(area);
    frame.render_widget(Clear, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);
    let filter_line = if filter.is_empty() {
        " 过滤: █  （直接输入）".to_string()
    } else {
        format!(" 过滤: {filter}█")
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(filter_line, theme::accent())))
            .block(
                Block::default()
                    .title(Span::styled(" 选择启动方式 ", theme::title()))
                    .borders(Borders::ALL)
                    .border_style(theme::border(true))
                    .style(theme::panel()),
            )
            .style(theme::base()),
        chunks[0],
    );
    let indices = App::launch_filter_indices(options, labels, filter);
    let list_items: Vec<ListItem> = if indices.is_empty() {
        vec![widgets::simple_item("（无匹配）")]
    } else {
        indices
            .iter()
            .map(|&i| labels.get(i).cloned().unwrap_or_else(|| options[i].label()))
            .map(widgets::simple_item)
            .collect()
    };
    let mut state = ListState::default();
    if !indices.is_empty() {
        let pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
        state.select(Some(pos));
    }
    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::border(true))
                .style(theme::panel()),
        )
        .highlight_style(widgets::selected_style())
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

/// 查看保存的秘密：PIN 输入态 / 明文展示态。
fn render_secret_viewer(
    frame: &mut Frame,
    area: Rect,
    pin_input: &str,
    revealed: Option<&(Option<String>, Option<String>)>,
    error: Option<&str>,
) {
    let area = modal_area(area);
    frame.render_widget(Clear, area);
    let title = if revealed.is_some() {
        " 保存的秘密 "
    } else {
        " 查看秘密 · PIN 验证 "
    };
    let block = Block::default()
        .title(Span::styled(title, theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(true))
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    match revealed {
        None => {
            lines.push(Line::from(Span::styled(
                "输入 PIN 查看该项目的保存密码与口令",
                theme::muted(),
            )));
            lines.push(Line::from(""));
            let mut spans = vec![
                Span::styled("PIN ", theme::muted()),
                Span::styled("*".repeat(pin_input.chars().count()), theme::selected()),
                Span::styled("█", theme::accent()),
            ];
            if pin_input.is_empty() {
                spans[1] = Span::styled("（4-12 位数字）", theme::muted());
            }
            lines.push(Line::from(spans));
        }
        Some((password, key_pass)) => {
            let row = |label: &str, value: Option<&String>| {
                let text = match value {
                    Some(v) => v.clone(),
                    None => "（未设置）".to_string(),
                };
                Line::from(vec![
                    Span::styled(format!("{label} "), theme::muted()),
                    Span::styled(text, theme::base()),
                ])
            };
            lines.push(row("登录密码:", password.as_ref()));
            lines.push(row("私钥口令:", key_pass.as_ref()));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "明文仅本次显示，Enter/Esc 返回",
                theme::muted(),
            )));
        }
    }
    if let Some(err) = error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(err.to_string(), theme::danger())));
    }
    lines.push(Line::from(""));
    let hint = if revealed.is_some() {
        "Enter / Esc  返回"
    } else {
        "Enter  验证    Esc  取消（共 3 次机会）"
    };
    lines.push(Line::from(Span::styled(hint, theme::muted())));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme::base()),
        inner,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    let area = modal_area(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(" 帮助 ", theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(true))
        .style(theme::panel());
    frame.render_widget(
        Paragraph::new(widgets::help_lines())
            .block(block)
            .wrap(Wrap { trim: false })
            .style(theme::base()),
        area,
    );
}

fn display_width(s: &str) -> u16 {
    s.chars()
        .map(|ch| if ch.is_ascii() { 1u16 } else { 2 })
        .sum()
}

fn confirm_area(area: Rect, message: &str) -> Rect {
    let content = display_width(message).max(display_width("[y/N]")).max(8);
    let w = (content.saturating_add(4)).max(24).min(area.width.max(24));
    let w = w.min(area.width);
    let h = 7u16.min(area.height.max(5)).min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

fn render_confirm(frame: &mut Frame, area: Rect, message: &str) {
    let area = confirm_area(area, message);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(" 确认 ", theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::danger())
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(message, theme::danger())))
            .alignment(Alignment::Center)
            .style(theme::panel()),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "[y/N]",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center)
        .style(theme::panel()),
        chunks[2],
    );
}

fn render_form(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    fields: &[FormField],
    focus: usize,
    error: Option<&str>,
) {
    let area = modal_area(area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(format!(" {title} "), theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border(true))
        .style(theme::panel());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let focused = i == focus;
        match field {
            FormField::Text { label, value } => {
                let label_style = if focused {
                    theme::accent().add_modifier(Modifier::BOLD)
                } else {
                    theme::muted()
                };
                lines.push(Line::from(Span::styled(label.clone(), label_style)));
                let mut spans = vec![
                    Span::styled(
                        if focused { "▶ " } else { "  " },
                        if focused {
                            theme::accent()
                        } else {
                            theme::muted()
                        },
                    ),
                    Span::styled(
                        value.clone(),
                        if focused {
                            theme::selected()
                        } else {
                            theme::base()
                        },
                    ),
                ];
                if focused {
                    spans.push(Span::styled("█", theme::accent()));
                }
                lines.push(Line::from(spans));
            }
            FormField::Password {
                label,
                value,
                empty_hint,
            } => {
                let label_style = if focused {
                    theme::accent().add_modifier(Modifier::BOLD)
                } else {
                    theme::muted()
                };
                lines.push(Line::from(Span::styled(label.clone(), label_style)));
                let len = value.chars().count();
                let masked = if len == 0 {
                    empty_hint.clone()
                } else {
                    format!(
                        "{}{}",
                        "*".repeat(len.min(16)),
                        if len > 16 { "…" } else { "" }
                    )
                };
                let mut spans = vec![
                    Span::styled(
                        if focused { "▶ " } else { "  " },
                        if focused {
                            theme::accent()
                        } else {
                            theme::muted()
                        },
                    ),
                    Span::styled(
                        masked,
                        if focused {
                            theme::selected()
                        } else {
                            theme::base()
                        },
                    ),
                ];
                if focused {
                    spans.push(Span::styled("█", theme::accent()));
                }
                lines.push(Line::from(spans));
            }
            FormField::Button { label } => {
                let marker = if focused { "▶ " } else { "  " };
                let style = if focused {
                    theme::selected().add_modifier(Modifier::BOLD)
                } else {
                    theme::accent()
                };
                lines.push(Line::from(vec![
                    Span::styled(marker, theme::accent()),
                    Span::styled(format!("[ {label} ]"), style),
                ]));
            }
        }
        lines.push(Line::from(""));
    }
    if let Some(err) = error {
        lines.push(Line::from(Span::styled(err.to_string(), theme::danger())));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Tab 切换  Enter 提交/确认  Esc 取消",
        theme::muted(),
    )));

    // 表单弹窗只有 60% 高且无滚动容器：字段多了（SSH 新增表单 12 字段）
    // 在矮终端会裁掉尾部；按焦点滚动，保证当前字段首行始终可见。
    let scroll = form_scroll_offset(fields, focus, inner.height as usize, error.is_some());

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .style(theme::base()),
        inner,
    );
}

/// 表单滚动偏移：保证焦点字段首行始终可见（绝不把焦点滚出屏）。
/// Text/Password 占 3 行（标签/值/空行），Button 占 2 行；尾部为
/// 错误行（有错 2 行）+ 底部提示行（1 行）。
pub(crate) fn form_scroll_offset(
    fields: &[FormField],
    focus: usize,
    visible_height: usize,
    has_error: bool,
) -> u16 {
    let mut focused_start = 0usize;
    let mut focused_lines = 3usize;
    let mut line = 0usize;
    for (i, field) in fields.iter().enumerate() {
        let height = match field {
            FormField::Button { .. } => 2,
            _ => 3,
        };
        if i == focus {
            focused_start = line;
            focused_lines = height;
        }
        line += height;
    }
    let total = line + if has_error { 2 } else { 0 } + 1;
    let mut scroll = (focused_start + focused_lines).saturating_sub(visible_height);
    scroll = scroll.min(focused_start);
    scroll = scroll.min(total.saturating_sub(visible_height));
    scroll.min(u16::MAX as usize) as u16
}

fn render_input(frame: &mut Frame, area: Rect, prompt: &str, buffer: &str, filter: bool) {
    let style = if filter {
        theme::accent()
    } else {
        theme::base()
    };
    let line = Line::from(vec![
        Span::styled(format!(" {prompt}"), theme::accent()),
        Span::styled(format!("{buffer}█"), style),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_field() -> FormField {
        FormField::Text {
            label: String::new(),
            value: String::new(),
        }
    }

    fn button_field() -> FormField {
        FormField::Button {
            label: String::new(),
        }
    }

    #[test]
    fn form_scroll_keeps_focus_visible() {
        // 12 文本字段：36 行 + 提示行 = 37 行内容
        let fields: Vec<FormField> = (0..12).map(|_| text_field()).collect();
        // 焦点在头部：不滚
        assert_eq!(form_scroll_offset(&fields, 0, 16, false), 0);
        // 焦点在尾部（首行 33）：滚到刚好容下该字段，且首行仍可见
        assert_eq!(form_scroll_offset(&fields, 11, 16, false), 20);
        // 可见高度足够：不滚
        assert_eq!(form_scroll_offset(&fields, 11, 40, false), 0);
        // 有错误行时尾部多 2 行
        assert_eq!(form_scroll_offset(&fields, 11, 16, true), 20);
        // 按钮占 2 行：焦点在按钮上按 2 行算
        let mixed = vec![text_field(), button_field(), text_field()];
        assert_eq!(form_scroll_offset(&mixed, 1, 4, false), 1);
        assert_eq!(form_scroll_offset(&mixed, 2, 4, false), 4);
    }
}
