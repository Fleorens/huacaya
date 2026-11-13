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
/// between, shrunk then dropped (with the right part) when the space is too small.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 {
        return;
    }
    let theme = ctx.theme;
    let left = left_spans(app, theme);
    let right = right_spans(app, ctx, theme);

    let right_edge = area.x + area.width;
    let left_w = spans_width(&left);
    let right_w = spans_width(&right);
    let after_left = area.x + left_w;

    // Left part, left-aligned.
    set_row(buf, area.x, area.y, &left, left_w, right_edge);

    // Tabs centred between the left and the right part. Five full tabs do not fit at 100 columns, so the
    // tabs shrink before anything is dropped: full, then tight (unpadded), with the right part; then the
    // same without it; then compact (numbers, the current tab titled), with then without the right part.
    let free_with_right = right_edge.saturating_sub(after_left + right_w);
    let free_alone = right_edge.saturating_sub(after_left);
    let candidates = [
        (TabStyle::Full, true),
        (TabStyle::Tight, true),
        (TabStyle::Full, false),
        (TabStyle::Tight, false),
        (TabStyle::Compact, true),
        (TabStyle::Compact, false),
    ];
    let chosen = candidates.iter().find_map(|&(style, with_right)| {
        let tabs = tabs_spans(app, theme, style);
        let w = spans_width(&tabs);
        // One column of margin on each side, so the tabs never touch the build or the clock.
        let free = if with_right { free_with_right } else { free_alone }.saturating_sub(2);
        (w <= free).then_some((tabs, w, free, with_right))
    });
    if let Some((tabs, tabs_w, free, with_right)) = chosen {
        if with_right {
            set_row(buf, right_edge - right_w, area.y, &right, right_w, right_edge);
        }
        set_row(buf, after_left + 1 + (free - tabs_w) / 2, area.y, &tabs, tabs_w, right_edge);
    }
}

/// How much room the tabs take: ` 1 Overview ` padded, `1 Overview` unpadded, or the number alone for
/// all but the current tab.
#[derive(Clone, Copy, PartialEq)]
enum TabStyle {
    Full,
    Tight,
    Compact,
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

/// The screen tabs, ` <n> <title> ` each (see [`TabStyle`]), separated by one space; the current tab
/// reversed + bold and always titled. The Agents tab gains a trailing ` ●` (`Server`) when a session is
/// running.
fn tabs_spans(app: &App, theme: &Theme, tab_style: TabStyle) -> Vec<Span<'static>> {
    let mut tabs = Vec::new();
    let count = Screen::ALL.len();
    for (i, sc) in Screen::ALL.iter().enumerate() {
        let current = *sc == app.screen;
        let style = if current {
            theme.fg(Role::BorderFocused).add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            theme.fg(Role::Dim)
        };
        let label = match tab_style {
            TabStyle::Compact if !current => format!("{}", i + 1),
            _ => format!("{} {}", i + 1, sc.title()),
        };
        // In compact style the current tab keeps its padding so the reversed block still reads as a tab.
        let pad = tab_style == TabStyle::Full || (tab_style == TabStyle::Compact && current);
        let running = *sc == Screen::Agents && agents_running(app);
        let text = match (pad, running) {
            (true, true) => format!(" {label}"),
            (true, false) => format!(" {label} "),
            (false, _) => label,
        };
        tabs.push(Span::styled(text, style));
        if running {
            // The running dot follows the tab on its own (`Server`) style.
            let dot = if pad { " ● " } else { " ●" };
            tabs.push(Span::styled(dot, theme.fg(Role::Server).add_modifier(Modifier::BOLD)));
        }
        // Full tabs keep the trailing separator they always had; the shrunk ones drop it.
        if tab_style == TabStyle::Full || i + 1 < count {
            tabs.push(Span::raw(" "));
        }
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
