use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::config::AppConfig;
use crate::models::ProjectData;
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
        Mode::Confirm { message, .. } => render_confirm(frame, chunks[2], message),
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
    options: &[crate::menu::LaunchOption],
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

fn render_confirm(frame: &mut Frame, area: Rect, message: &str) {
    let line = Line::from(vec![
        Span::styled(format!(" {message} "), theme::danger()),
        Span::styled(
            "[y/N]",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
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

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(theme::base()),
        inner,
    );
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
