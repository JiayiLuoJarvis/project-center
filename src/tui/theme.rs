use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
pub const PANEL: Color = Color::Rgb(0x16, 0x16, 0x1e);
pub const BORDER_FOCUS: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
pub const BORDER_DIM: Color = Color::Rgb(0x3b, 0x42, 0x61);
pub const SELECT_BG: Color = Color::Rgb(0x28, 0x34, 0x57);
pub const SELECT_FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
pub const TITLE: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
pub const SUCCESS: Color = Color::Rgb(0x9e, 0xce, 0x6a);
pub const WARN: Color = Color::Rgb(0xe0, 0xaf, 0x68);
pub const MUTED: Color = Color::Rgb(0x56, 0x5f, 0x89);
pub const DANGER: Color = Color::Rgb(0xf7, 0x76, 0x8e);
pub const ACCENT: Color = Color::Rgb(0x7d, 0xcf, 0xff);
pub const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);

pub fn base() -> Style {
    Style::default().fg(FG).bg(BG)
}

pub fn title() -> Style {
    Style::default().fg(TITLE).add_modifier(Modifier::BOLD)
}

pub fn border(focused: bool) -> Style {
    if focused {
        Style::default().fg(BORDER_FOCUS)
    } else {
        Style::default().fg(BORDER_DIM)
    }
}

pub fn selected() -> Style {
    Style::default().fg(SELECT_FG).bg(SELECT_BG)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn warn() -> Style {
    Style::default().fg(WARN)
}

pub fn danger() -> Style {
    Style::default().fg(DANGER)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn panel() -> Style {
    Style::default().bg(PANEL)
}
