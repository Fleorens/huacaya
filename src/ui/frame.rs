//! A rounded panel frame and a state badge.

use ratatui::style::Modifier;
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders};

use crate::ui::theme::{Role, Theme};

/// A panel frame: all borders, `BorderType::Rounded`, border style `theme.fg(Role::BorderFocused)` when
/// `focused` else `theme.fg(Role::Border)`, title `" <title> "` (one space each side) as a `Span` styled
/// `theme.fg(accent)` plus `Modifier::BOLD`.
pub fn panel<'a>(title: &'a str, accent: Role, focused: bool, theme: &Theme) -> Block<'a> {
    let border_style = if focused { theme.fg(Role::BorderFocused) } else { theme.fg(Role::Border) };
    let title_span = Span::styled(format!(" {title} "), theme.fg(accent).add_modifier(Modifier::BOLD));
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title_span)
}

/// A state badge: `"◉ <text>"` when `lit`, `"○ <text>"` otherwise, styled `theme.fg(role)` plus
/// `Modifier::BOLD`. The UI toggles `lit` to make a badge pulse.
pub fn badge(text: &str, role: Role, theme: &Theme, lit: bool) -> Span<'static> {
    let mark = if lit { "◉ " } else { "○ " };
    Span::styled(format!("{mark}{text}"), theme.fg(role).add_modifier(Modifier::BOLD))
}
