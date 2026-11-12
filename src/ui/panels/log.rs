//! The Log panel: the end of `app.visible_logs()`, coloured by level, with the filter and follow/scroll
//! indicators on the top border. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::logs::{Level, LogLine};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::text::fit;
use crate::ui::theme::{Role, Theme};

/// Height needed, borders included (1 row inside).
pub const MIN_HEIGHT: u16 = 3;

/// The log panel: `frame::panel("Activity", Role::Server, false, theme)` with the filter and follow/scroll
/// indicators right-aligned on the top border, then the last `h` visible lines (oldest at the top),
/// each `<time> ` in `Role::Dim` then the text coloured by level and cut to the panel width. No line:
/// one `Dim` row `no log line yet`; lines that are all hidden: `<n> lines hidden — x proxy  e level  / filter`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = panel("Activity", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    // The filter and follow/scroll indicators, right-aligned on the top border.
    render_indicators(app, area, theme, buf, "Activity");

    let v = app.visible_logs();
    if v.is_empty() {
        // Lines that exist but are all filtered out (typically llamatop's own proxy lines) must not look
        // like an empty log.
        let text = if app.logs.is_empty() {
            "no log line yet".to_string()
        } else {
            format!("{} lines hidden — x proxy  e level  / filter", app.logs.len())
        };
        buf.set_line(inner.x, inner.y, &Line::from(Span::styled(text, theme.fg(Role::Dim))), inner.width);
        return;
    }

    // Show `v[end - h .. end]`, oldest at the top; `h` is the number of rows available.
    let h = inner.height as usize;
    let scroll = app.log_view.scroll;
    let end = v.len().saturating_sub(scroll.min(v.len()));
    let start = end.saturating_sub(h);

    for (i, line) in v[start..end].iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        render_line(line, app.log_view.raw, theme, inner.x, y, inner.width, buf);
    }
}

/// The top-border indicators: ` /<filter> ` (Warning) when the filter is not empty, then ` follow ● `
/// (Good) when following, else ` scroll −<scroll> ` (Warning). Placed right-aligned, leaving the title
/// readable (`╭ <title> ` = `title.len() + 3` columns).
fn render_indicators(app: &App, area: Rect, theme: &Theme, buf: &mut Buffer, title: &str) {
    let state = if app.log_view.follow {
        (" follow ● ".to_string(), Role::Good)
    } else {
        (format!(" scroll −{} ", app.log_view.scroll), Role::Warning)
    };
    // Between the two corners, leaving the title readable (`╭ Log ` = 6 columns, `╭ Activity ` = 11).
    let left = area.x.saturating_add(title.len() as u16 + 3).min(area.x + area.width.saturating_sub(1));
    let right = area.x + area.width.saturating_sub(1);
    let avail = right.saturating_sub(left) as usize;
    let state_w = state.0.chars().count();

    let mut parts: Vec<(String, Role)> = Vec::new();
    let filter = &app.log_view.filter;
    // ` /` + at least one character + ` ` must fit beside the state, else the filter is omitted.
    if !filter.is_empty() && avail >= state_w + 4 {
        let filter_w = avail - state_w - 3;
        parts.push((format!(" /{} ", fit(filter, filter_w)), Role::Warning));
    }
    parts.push(state);

    // Place the parts right-aligned on the top border, the last one just before the right corner.
    let y = area.y;
    let mut x = right;
    for (text, role) in parts.into_iter().rev() {
        let w = text.chars().count() as u16;
        x = x.saturating_sub(w).max(left);
        let color = theme.color(role);
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i as u16;
            if cx >= right {
                break;
            }
            let Some(cell) = buf.cell_mut((cx, y)) else { continue };
            cell.set_char(ch);
            if let Some(color) = color {
                cell.fg = color;
            }
        }
    }
}

/// One log row: `<time> ` in `Role::Dim` (nothing without a time), then, when not `raw` and the task
/// is a non-negative number, `#<task> ` in `Role::Dim`, then the text — `logtext::compact(line)` when
/// not `raw`, `line.text` when `raw` — coloured by level and cut to `width` columns.
pub(crate) fn render_line(
    line: &LogLine,
    raw: bool,
    theme: &Theme,
    x: u16,
    y: u16,
    width: u16,
    buf: &mut Buffer,
) {
    let role = match line.level {
        Level::Info => Role::Text,
        Level::Warn => Role::Warning,
        Level::Error => Role::Critical,
    };
    let time_w = line.time.as_ref().map(|t| t.chars().count() + 1).unwrap_or(0) as u16;
    let task = line.task.filter(|t| !raw && *t >= 0).map(|t| format!("#{t} "));
    let task_w = task.as_ref().map(|t| t.chars().count()).unwrap_or(0) as u16;
    let text_w = width.saturating_sub(time_w).saturating_sub(task_w);
    let mut spans = Vec::new();
    if let Some(t) = &line.time {
        spans.push(Span::styled(format!("{t} "), theme.fg(Role::Dim)));
    }
    if let Some(t) = task {
        spans.push(Span::styled(t, theme.fg(Role::Dim)));
    }
    let text = if raw { line.text.clone() } else { crate::ui::logtext::compact(line) };
    spans.push(Span::styled(fit(&text, text_w as usize), theme.fg(role)));
    buf.set_line(x, y, &Line::from(spans), width);
}
