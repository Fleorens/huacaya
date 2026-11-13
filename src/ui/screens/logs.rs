//! The Logs screen: the server log full screen with its status row below (T17), and beside it the
//! Levels / Filter / Tasks bands at 90 × 12 and up (T37). See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::{App, LevelFilter};
use crate::collect::logs::{Level, LogLine};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::panels::log;
use crate::ui::screens::logs_side::message_line;
use crate::ui::theme::{Role, Theme};

/// The Logs screen in `area` (the body below the header). Below 90 columns or 12 rows: `panels::log::render`
/// on every row but the last, then the status row on the last one. Otherwise the v2 layout (see
/// `render_side`): the three side panels and the Log panel, then the hint row on the last one. Never
/// panics, whatever the size (0 × 0 included).
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = ctx.theme;
    if area.width < 90 || area.height < 12 {
        if area.height >= 2 {
            let panel_area = Rect::new(area.x, area.y, area.width, area.height - 1);
            log::render(app, ctx, panel_area, buf);
        }
        let status = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        render_status_row(app, status, theme, buf);
        return;
    }
    render_v2(app, ctx, area, buf);
}

/// The v2 Logs screen: the left band (Levels, Filter, Tasks) and the Log panel on the body, then the
/// hint row on the last row.
fn render_v2(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let hint_y = area.y + area.height - 1;
    let side_area = Rect::new(area.x, area.y, 32, hint_y - area.y);
    let log_area = Rect::new(area.x + 34, area.y, area.width - 34, hint_y - area.y);
    crate::ui::screens::logs_side::render_side(app, ctx, side_area, buf);
    render_log_panel(app, ctx, log_area, buf);
    render_hint_row(app, Rect::new(area.x, hint_y, area.width, 1), ctx.theme, buf);
}

/// The Log panel: `frame::panel("Log", Role::Server, !tasks_focus, theme)` with today's border
/// indicators, then a `time / task / lvl / message` header, then the end of `visible_logs()` (scroll as
/// today), each row `<time>  #<task>  <glyph>  <message>`. No line: one `Dim` row `no log line yet`;
/// lines all hidden: `<n> lines hidden — x proxy  e level  / filter`.
fn render_log_panel(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Log", Role::Server, !app.log_view.tasks_focus, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    log::render_indicators(app, area, theme, buf, "Log");

    let v = app.visible_logs();
    if v.is_empty() {
        let text = if app.logs.is_empty() {
            "no log line yet".to_string()
        } else {
            format!("{} lines hidden — x proxy  e level  / filter", app.logs.len())
        };
        buf.set_line(inner.x, inner.y, &Line::from(Span::styled(text, theme.fg(Role::Dim))), inner.width);
        return;
    }

    // The column header.
    let header = format!("{:<10}{:<10}{:<5}{}", "time", "task", "lvl", "message");
    buf.set_line(inner.x, inner.y, &Line::from(Span::styled(header, theme.fg(Role::Dim))), inner.width);

    // Show `v[end - h .. end]`, oldest at the top; `h` is the rows available below the header.
    let h = inner.height.saturating_sub(1) as usize;
    let scroll = app.log_view.scroll;
    let end = v.len().saturating_sub(scroll.min(v.len()));
    let start = end.saturating_sub(h);
    let filter_lower = app.log_view.filter.to_ascii_lowercase();
    for (i, line) in v[start..end].iter().enumerate() {
        let y = inner.y + 1 + i as u16;
        if y >= inner.bottom() {
            break;
        }
        render_log_row(
            line,
            app.log_view.raw,
            &filter_lower,
            theme,
            Rect::new(inner.x, y, inner.width, 1),
            buf,
        );
    }
}

/// One log row: `time` padded to 10 (`Dim`, blank without a time), `#<task>` padded to 10 (`Dim`, blank
/// without a non-negative task), the level glyph padded to 5 in its role, then the message (compact, or
/// `raw`) cut to the remaining width with the filter highlighted.
fn render_log_row(
    line: &LogLine,
    raw: bool,
    filter_lower: &str,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let role = match line.level {
        Level::Info => Role::Text,
        Level::Warn => Role::Warning,
        Level::Error => Role::Critical,
    };
    let time = match &line.time {
        Some(t) => format!("{:<10}", t),
        None => "          ".to_string(),
    };
    buf.set_span(area.x, area.y, &Span::styled(time, theme.fg(Role::Dim)), 10);

    let task = match line.task {
        Some(t) if t >= 0 => format!("#{t:<10}"),
        _ => "          ".to_string(),
    };
    buf.set_span(area.x + 10, area.y, &Span::styled(task, theme.fg(Role::Dim)), 10);

    let glyph = match line.level {
        Level::Info => "●",
        Level::Warn => "▲",
        Level::Error => "✖",
    };
    buf.set_span(area.x + 20, area.y, &Span::styled(format!("{:<5}", glyph), theme.fg(role)), 5);

    let text = if raw { line.text.clone() } else { crate::ui::logtext::compact(line) };
    let msg_w = area.width.saturating_sub(25);
    let line = message_line(&text, filter_lower, role, theme, msg_w as usize);
    buf.set_line(area.x + 25, area.y, &line, msg_w);
}

/// The hint row on the last row: `fit_groups` of the key hints split at ` · `, or the status row while
/// the filter is being typed.
fn render_hint_row(app: &App, area: Rect, theme: &Theme, buf: &mut Buffer) {
    if app.log_view.editing_filter {
        render_status_row(app, area, theme, buf);
        return;
    }
    let hint = "f follow · / filter · e level · r raw · x proxy · t tasks · Enter filter on task · ↑↓ scroll";
    let groups: Vec<Vec<Span>> =
        hint.split(" · ").map(|g| vec![Span::styled(g.to_string(), theme.fg(Role::Dim))]).collect();
    let line = crate::ui::text::fit_groups(groups, area.width as usize);
    buf.set_line(area.x, area.y, &line, area.width);
}

/// `all`, `warn+err`, `errors`.
pub fn level_name(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::All => "all",
        LevelFilter::WarnAndErrors => "warn+err",
        LevelFilter::Errors => "errors",
    }
}

/// The status row segments, as styled runs: each segment is a list of runs (label `Dim`, value `Text`),
/// segments separated by three spaces on the row. While editing the filter, only the cursor and the
/// keep/clear hint.
fn status_runs(app: &App) -> Vec<Vec<(String, Role)>> {
    if app.log_view.editing_filter {
        vec![
            vec![(format!("/{}▏", app.log_view.filter), Role::Warning)],
            vec![("Enter keep".to_string(), Role::Dim)],
            vec![("Esc clear".to_string(), Role::Dim)],
        ]
    } else {
        let state = if app.log_view.follow {
            vec![("follow ●".to_string(), Role::Good)]
        } else {
            vec![(format!("scroll −{}", app.log_view.scroll), Role::Warning)]
        };
        let level = run_pair("level ", level_name(app.log_view.level));
        let filter = if app.log_view.filter.is_empty() {
            run_pair("filter ", "none")
        } else {
            run_pair("filter ", &format!("\"{}\"", app.log_view.filter))
        };
        let proxy = run_pair("proxy ", if app.log_view.show_proxy { "shown" } else { "hidden" });
        let lines = run_pair("lines ", if app.log_view.raw { "raw" } else { "compact" });
        let count = vec![
            (format!("{} of {}", app.visible_logs().len(), app.logs.len()), Role::Text),
            (" lines".to_string(), Role::Dim),
        ];
        vec![state, level, filter, proxy, lines, count]
    }
}

/// A `<label> <value>` segment: label `Dim`, value `Text`.
fn run_pair(label: &str, value: &str) -> Vec<(String, Role)> {
    vec![(label.to_string(), Role::Dim), (value.to_string(), Role::Text)]
}

/// The status row on the single-row `area`: the segments left to right (three spaces apart), then the
/// key hint right-aligned only when it fits after them.
fn render_status_row(app: &App, area: Rect, theme: &Theme, buf: &mut Buffer) {
    let y = area.y;
    let right = area.x + area.width;
    let mut x = area.x;

    for (i, seg) in status_runs(app).iter().enumerate() {
        if i > 0 {
            x = write_gap(buf, x, y, right, 3);
        }
        for (text, role) in seg {
            x = write_run(buf, x, y, right, text, *role, theme);
        }
    }

    // The key hint, right-aligned, only when it fits after the left segments (not while editing).
    if !app.log_view.editing_filter {
        let hint = "f follow  / filter  e level  x proxy  r raw  ↑↓ scroll";
        let hint_w = hint.chars().count() as u16;
        if x + 3 + hint_w <= right {
            let hx = right - hint_w;
            for (i, ch) in hint.chars().enumerate() {
                set_cell(buf, hx + i as u16, y, ch, Role::Dim, theme);
            }
        }
    }
}

/// Write `n` spaces from `x`, returning the cursor after them (clamped at `right`).
fn write_gap(buf: &mut Buffer, mut x: u16, y: u16, right: u16, n: u16) -> u16 {
    for _ in 0..n {
        if x >= right {
            break;
        }
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(' ');
        }
        x += 1;
    }
    x
}

/// Write `text` from `x` styled `role`, returning the cursor after it (clamped at `right`).
fn write_run(buf: &mut Buffer, mut x: u16, y: u16, right: u16, text: &str, role: Role, theme: &Theme) -> u16 {
    for ch in text.chars() {
        if x >= right {
            break;
        }
        set_cell(buf, x, y, ch, role, theme);
        x += 1;
    }
    x
}

/// Set one cell's char and foreground colour, when it is inside the buffer.
fn set_cell(buf: &mut Buffer, x: u16, y: u16, ch: char, role: Role, theme: &Theme) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        if let Some(color) = theme.color(role) {
            cell.fg = color;
        }
    }
}
