use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::domain::models::{Connection, DeletedItem, Project};
use crate::persist::AppConfig;
use crate::tui::actions;
use crate::tui::app::format_list_index;
use crate::tui::theme;

pub fn project_item(
    index: usize,
    project: &Project,
    config: &AppConfig,
    connection: Option<&Connection>,
) -> ListItem<'static> {
    if project.is_ssh_project() {
        return ssh_project_item(index, project, connection);
    }
    let path = if project.has_windows_path() {
        project.path.clone()
    } else {
        project.linux_path()
    };
    let mut name_spans = vec![
        Span::styled(format!("{} ", format_list_index(index)), theme::base()),
        Span::styled(project.name.clone(), theme::base()),
    ];
    if !project.alias.trim().is_empty() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled(
            format!("[{}]", project.alias.trim()),
            theme::muted(),
        ));
    }
    if project.has_default_tool() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled("★", theme::success()));
        name_spans.push(Span::styled(
            format!(" {}", project.default_tool),
            theme::success(),
        ));
    } else if let Some(first) = crate::launch::build_launch_options(project, config).first() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled(first.tool_name.clone(), theme::muted()));
    }
    if !project.has_windows_path() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled("WSL", theme::warn()));
    }
    let path_line = if path.is_empty() {
        Line::from(Span::styled("（无路径）", theme::dim()))
    } else {
        Line::from(Span::styled(truncate_width(&path, 60), theme::dim()))
    };
    ListItem::new(vec![Line::from(name_spans), path_line, Line::from("")])
}

/// SSH 远程项目行：黄色 `SSH` 标签 + 远程路径。
fn ssh_project_item(
    index: usize,
    project: &Project,
    connection: Option<&Connection>,
) -> ListItem<'static> {
    let mut name_spans = vec![
        Span::styled(format!("{} ", format_list_index(index)), theme::base()),
        Span::styled(project.name.clone(), theme::base()),
    ];
    if !project.alias.trim().is_empty() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled(
            format!("[{}]", project.alias.trim()),
            theme::muted(),
        ));
    }
    name_spans.push(Span::raw(" "));
    name_spans.push(Span::styled("SSH", theme::warn()));
    let host = match connection {
        Some(conn) => format!("{}  {}", conn.name, conn.label()),
        None => "（连接缺失）".to_string(),
    };
    let remote = if project.path.trim().is_empty() {
        host
    } else {
        format!("{}  → {}", host, project.path.trim())
    };
    let path_line = Line::from(Span::styled(truncate_width(&remote, 60), theme::dim()));
    ListItem::new(vec![Line::from(name_spans), path_line, Line::from("")])
}

pub fn connection_item(conn: &Connection, refs: usize) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        crate::tui::actions::connection_label(conn, refs),
        theme::base(),
    )))
}

pub fn trash_item(item: &DeletedItem) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(
        actions::trash_label(item),
        theme::base(),
    )))
}

pub fn simple_item(text: impl Into<String>) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(text.into(), theme::base())))
}

/// 分组行视图：优雅的层级结构与右对齐胶囊徽标。
/// 布局：序号(低亮) + 菱形图标(强调色) + 分组名(高亮/常规) + 可选别名(淡紫色) ... [ 项目数 ](暗胶囊)
pub fn group_item(
    index: usize,
    group: &crate::domain::models::Group,
    selected: bool,
    max_width: usize,
) -> ListItem<'static> {
    let mut left_spans = vec![
        Span::styled(format!("{} ", format_list_index(index)), theme::dim()),
        Span::styled(
            "◆ ",
            if selected {
                theme::accent()
            } else {
                theme::muted()
            },
        ),
        Span::styled(
            group.name.clone(),
            if selected {
                theme::base().add_modifier(Modifier::BOLD)
            } else {
                theme::base()
            },
        ),
    ];

    if !group.alias.trim().is_empty() {
        left_spans.push(Span::raw(" "));
        left_spans.push(Span::styled(
            format!("[{}]", group.alias.trim()),
            theme::muted(),
        ));
    }

    let left_line = Line::from(left_spans.clone());
    let left_w = left_line.width();

    // 胶囊徽章：[ 3 ] 或 [ 12 ]
    let count_text = format!(" {} ", group.projects.len());
    let badge_spans = vec![
        Span::styled("[", theme::dim()),
        Span::styled(
            count_text,
            if selected {
                theme::accent()
            } else {
                theme::muted()
            },
        ),
        Span::styled("]", theme::dim()),
    ];
    let badge_w = Line::from(badge_spans.clone()).width();

    let mut line_spans = left_spans;
    if max_width > left_w + badge_w + 1 {
        let padding = max_width.saturating_sub(left_w + badge_w);
        line_spans.push(Span::raw(" ".repeat(padding)));
        line_spans.extend(badge_spans);
    } else {
        // 区域较窄时保持紧凑并留一个空格
        line_spans.push(Span::raw(" "));
        line_spans.extend(badge_spans);
    }

    ListItem::new(Line::from(line_spans))
}

pub fn selected_style() -> Style {
    theme::selected().add_modifier(Modifier::BOLD)
}

pub fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled("快捷键", theme::title())),
        Line::from(""),
        Line::from("j/k ↑↓  移动    Tab/h/l  切换栏"),
        Line::from("Enter   打开/确认    o  操作菜单"),
        Line::from("a  新增   e  编辑   d  删除   m  移动"),
        Line::from("/  过滤   Esc  清过滤/返回"),
        Line::from("g/G  顶/底   ,  设置   ?  帮助   q/Ctrl+C  退出（确认）"),
        Line::from(""),
        Line::from("设置: 远程连接 / 回收站 / 启动工具（Esc 逐级返回）"),
        Line::from("连接: a 新增  e 编辑  d 删除  v 查看秘密"),
        Line::from("表单: Tab 切换字段  Enter 提交/选连接  Esc 取消"),
        Line::from("项目表单「远程连接」Enter 打开选择器，末项可新建"),
        Line::from("启动方式: 直接输入过滤  j/k 移动  Esc 清过滤/返回"),
        Line::from("别名: 分组/项目可设 alias，过滤与 CLI 可用"),
        Line::from(""),
        Line::from("回收站: r 恢复  D 清空"),
        Line::from("启动工具: Enter 进入环境工具列表"),
    ]
}

fn truncate_width(s: &str, max: usize) -> String {
    let mut width = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let w = unicode_width(ch);
        if width + w > max {
            out.push('…');
            break;
        }
        width += w;
        out.push(ch);
    }
    out
}

fn unicode_width(ch: char) -> usize {
    if ch.is_ascii() { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::Group;

    #[test]
    fn group_item_renders_aligned_badge() {
        let mut group = Group::new("backend");
        group.alias = "be".into();
        let item_normal = group_item(0, &group, false, 30);
        let item_selected = group_item(0, &group, true, 30);
        let item_narrow = group_item(0, &group, false, 10);
        let _ = (item_normal, item_selected, item_narrow);
    }
}
