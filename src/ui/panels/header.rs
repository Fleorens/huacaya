//! The Overview header, one line: `llamatop`, host/uptime, server status and build on the left, the
//! shortcuts on the right, the screen tabs centred between. See `docs/02-ui.md` § 2.2 (T32).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::{App, Screen};
use crate::collect::llama_poll::Health;
use crate::ui::panels::Ctx;
use crate::ui::theme::{Role, Theme};
use crate::util::duration;

/// Height of the header, one row.
pub const HEIGHT: u16 = 1;

/// `<dot> <status>` of the server: `○ waiting` when there is no snapshot, else the health dot and
/// message, with its role.
pub(crate) fn status(app: &App) -> (&'static str, Role) {
    match &app.llama {
        None => ("○ waiting", Role::Dim),
        Some(llama) => match llama.health {
            Health::Down(_) => ("● server down", Role::Critical),
            Health::Unauthorized => ("● no API key", Role::Warning),
            Health::Ok => ("● server ok", Role::Good),
        },
    }
}

/// The header: on the first row of `area`, no frame. `llamatop` then host, uptime, status and build on the
/// left; `PAUSED` (when paused) then clock and `? help` on the right; the screen tabs centred in the space
/// between, dropping the right part first, then the tabs, when the space is too small.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 {
        return;
    }
    let theme = ctx.theme;
    let left = left_spans(app, theme);
    let right = right_spans(app, ctx, theme);
    let tabs = tabs_spans(app, theme);

    let right_edge = area.x + area.width;
    let left_w = spans_width(&left);
    let right_w = spans_width(&right);
    let tabs_w = spans_width(&tabs);
    let after_left = area.x + left_w;

    // Left part, left-aligned.
    set_row(buf, area.x, area.y, &left, left_w, right_edge);

    // Tabs centred between the left and the right part. When they do not fit there, drop the right part and
    // centre them between the left part and the right edge; when they still do not fit, draw none.
    let (show_right, tabs_x) = if tabs_w <= right_edge.saturating_sub(after_left + right_w) {
        (true, Some(after_left + (right_edge - after_left - right_w - tabs_w) / 2))
    } else if tabs_w <= right_edge.saturating_sub(after_left) {
        (false, Some(after_left + (right_edge - after_left - tabs_w) / 2))
    } else {
        (false, None)
    };
    if show_right {
        set_row(buf, right_edge - right_w, area.y, &right, right_w, right_edge);
    }
    if let Some(x) = tabs_x {
        set_row(buf, x, area.y, &tabs, tabs_w, right_edge);
    }
}

/// The left part: ` llamatop ` (reversed + bold) then host, `up …`, status and build.
fn left_spans(app: &App, theme: &Theme) -> Vec<Span<'static>> {
    let hostname = app.system.as_ref().and_then(|s| s.hostname.clone()).unwrap_or_else(|| "?".to_string());
    let uptime =
        app.system.as_ref().and_then(|s| s.uptime_s).map(duration).unwrap_or_else(|| "n/a".to_string());
    let (status, role) = status(app);
    let build = app
        .llama
        .as_ref()
        .and_then(|l| l.build_info.clone())
        .and_then(|b| b.split('-').next().map(str::to_string));

    let mut spans = vec![
        Span::styled(
            " llamatop ",
            theme.fg(Role::BorderFocused).add_modifier(Modifier::REVERSED | Modifier::BOLD),
        ),
        Span::styled(" ", theme.fg(Role::Text)),
        Span::styled(hostname, theme.fg(Role::Text)),
        Span::styled("  up ", theme.fg(Role::Dim)),
        Span::styled(uptime, theme.fg(Role::Dim)),
        Span::styled("  ", theme.fg(Role::Dim)),
        Span::styled(status.to_string(), theme.fg(role).add_modifier(Modifier::BOLD)),
    ];
    if let Some(b) = build {
        spans.push(Span::styled("  ", theme.fg(Role::Text)));
        spans.push(Span::styled(b, theme.fg(Role::Text)));
    }
    spans
}

/// The right part: `PAUSED` (when paused) then clock and `? help`.
fn right_spans(app: &App, ctx: &Ctx, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if app.paused {
        spans.push(Span::styled("PAUSED  ", theme.fg(Role::Warning).add_modifier(Modifier::BOLD)));
    }
    spans.push(Span::styled(ctx.clock.to_string(), theme.fg(Role::Text)));
    spans.push(Span::styled("  ? help", theme.fg(Role::Dim)));
    spans
}

/// The screen tabs, ` <n> <title> ` each, separated by one space; the current tab reversed + bold. The
/// Agents tab gains a trailing ` ●` (`Server`) when a session is running.
fn tabs_spans(app: &App, theme: &Theme) -> Vec<Span<'static>> {
    let mut tabs = Vec::new();
    for (i, sc) in Screen::ALL.iter().enumerate() {
        let style = if *sc == app.screen {
            theme.fg(Role::BorderFocused).add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            theme.fg(Role::Dim)
        };
        if *sc == Screen::Agents && agents_running(app) {
            // No trailing space: the running dot follows the tab on its own (`Server`) style.
            tabs.push(Span::styled(format!(" {} {}", i + 1, sc.title()), style));
            tabs.push(Span::styled(" ● ", theme.fg(Role::Server).add_modifier(Modifier::BOLD)));
        } else {
            tabs.push(Span::styled(format!(" {} {} ", i + 1, sc.title()), style));
        }
        tabs.push(Span::raw(" "));
    }
    tabs
}

/// One row of `spans` at `(x, y)`, cut to `width` and clamped to `right_edge` so it never overflows the
/// area.
fn set_row(buf: &mut Buffer, x: u16, y: u16, spans: &[Span], width: u16, right_edge: u16) {
    let width = width.min(right_edge.saturating_sub(x));
    buf.set_line(x, y, &Line::from(spans.to_vec()), width);
}

/// The cell width of `spans`.
fn spans_width(spans: &[Span]) -> u16 {
    spans.iter().map(|s| s.width() as u16).sum()
}

/// Whether the Agents screen has a running session, to show the running dot on its tab.
fn agents_running(app: &App) -> bool {
    app.opencode.as_ref().is_some_and(|o| o.sessions.iter().any(|s| s.running))
}
