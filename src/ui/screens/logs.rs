//! The Logs screen: the server log full screen with its status row below (T17). See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app::{App, LevelFilter};
use crate::ui::panels::Ctx;
use crate::ui::panels::log;
use crate::ui::theme::{Role, Theme};

/// The Logs screen in `area` (the body below the header): `panels::log::render` on every row but the
/// last, then the status row on the last one. Never panics, whatever the size (0 × 0 included).
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = ctx.theme;
    if area.height >= 2 {
        let panel_area = Rect::new(area.x, area.y, area.width, area.height - 1);
        log::render(app, ctx, panel_area, buf);
    }
    let status = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
    render_status_row(app, status, theme, buf);
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
        let count = vec![
            (format!("{} of {}", app.visible_logs().len(), app.logs.len()), Role::Text),
            (" lines".to_string(), Role::Dim),
        ];
        vec![state, level, filter, proxy, count]
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
        let hint = "f follow  / filter  e level  x proxy  ↑↓ scroll";
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
