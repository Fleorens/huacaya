//! The help overlay, shown over any screen with `?` (T17). See `docs/02-ui.md` § 5.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Clear, Widget};

use crate::app::App;
use crate::ui::frame::panel;
use crate::ui::text::fit;
use crate::ui::theme::{Role, Theme};

/// The help overlay, centred in `area`, drawn over whatever is there.
///
/// A box `min(area.width, 56)` × `min(area.height, 14)`, centred; `Clear` first so it is not
/// see-through, then `frame::panel("Help", Role::BorderFocused, true, theme)` and the key list: each
/// key (`Role::Server`, padded to 15) then its action (`Role::Text`). Rows that do not fit are dropped
/// from the bottom. Never panics.
pub fn render(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let w = area.width.min(56);
    let h = area.height.min(14);
    let box_area = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h);
    Clear.render(box_area, buf);
    let panel = panel("Help", Role::BorderFocused, true, theme);
    let inner = panel.inner(box_area);
    panel.render(box_area, buf);

    for (i, (keys, action)) in help_rows(app).iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        // The key (Server, padded to 15) then the action (Text), cut to the remaining width.
        buf.set_span(inner.x, y, &Span::styled(format!("{:<15}", keys), theme.fg(Role::Server)), 15);
        let action_w = inner.width.saturating_sub(15);
        buf.set_span(
            inner.x + 15,
            y,
            &Span::styled(fit(action, action_w as usize), theme.fg(Role::Text)),
            action_w,
        );
    }
}

/// The 12 help rows: the keys then the action. The refresh row carries the current interval, in seconds
/// with one decimal (`interval_ms / 1000`).
fn help_rows(app: &App) -> Vec<(&'static str, String)> {
    let refresh = format!("faster / slower refresh (now {:.1} s)", app.interval_ms as f64 / 1000.0);
    vec![
        ("1-4 Tab S-Tab", "switch screen".into()),
        ("q Ctrl-C", "quit".into()),
        ("? Esc", "close this help".into()),
        ("p", "pause / resume the display".into()),
        ("+ -", refresh),
        ("↑ ↓ PgUp PgDn", "scroll (Logs, Models)".into()),
        ("Home End", "top / bottom".into()),
        ("f", "follow the log".into()),
        ("/", "filter the log, Esc clears".into()),
        ("e", "level filter: all, warnings, errors".into()),
        ("x", "show / hide the router's proxy lines".into()),
        ("r", "raw / compact log lines".into()),
    ]
}
