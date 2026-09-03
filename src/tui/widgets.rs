use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::AppConfig;
use crate::models::{DeletedItem, Project};
use crate::tui::actions;
use crate::tui::theme;

pub fn project_item(project: &Project, config: &AppConfig) -> ListItem<'static> {
    let path = if project.has_windows_path() {
        project.path.clone()
    } else {
        project.linux_path()
    };
    let mut name_spans = vec![Span::styled(project.name.clone(), theme::base())];
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
    } else if let Some(first) = crate::menu::build_launch_options(project, config).first() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled(first.tool_name.clone(), theme::muted()));
    }
    if !project.has_windows_path() {
        name_spans.push(Span::raw(" "));
        name_spans.push(Span::styled("WSL", theme::warn()));
    }
    let path_line = if path.is_empty() {
        Line::from(Span::styled("（无路径）", theme::muted()))
    } else {
        Line::from(Span::styled(truncate_width(&path, 60), theme::muted()))
    };
    ListItem::new(vec![Line::from(name_spans), path_line])
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
        Line::from("g/G  顶/底   ?  帮助   q/Ctrl+C  退出（确认）"),
        Line::from(""),
        Line::from("表单: Tab 切换字段  Enter 提交  Esc 取消"),
        Line::from("项目表单可选「浏览文件夹…」填 Windows 路径"),
        Line::from("启动方式: 直接输入过滤  j/k 移动  Esc 清过滤/返回"),
        Line::from("别名: 分组/项目可设 alias，过滤与 CLI 可用"),
        Line::from(""),
        Line::from("回收站: r 恢复  D 清空"),
        Line::from("配置: Enter 进入环境工具列表"),
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
