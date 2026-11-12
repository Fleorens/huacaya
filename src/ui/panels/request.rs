//! The Requests table: the newest requests as a row per request, with a header and a summary. See
//! `docs/02-ui.md` § 2.2 (T32).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::{App, LastRequest};
use crate::ui::frame;
use crate::ui::panels::Ctx;
use crate::ui::text::{fit, group_thousands, pct, tps};
use crate::ui::theme::{Role, Theme};
use crate::util::count;

/// Height needed, borders included (header row and one request).
pub const MIN_HEIGHT: u16 = 4;

/// The columns, one space between: `time` (8, left), `prompt` (7), `gen` (7), `t/s` (6), `draft` (6).
const COLS: [(u16, bool); 5] = [(8, false), (7, true), (7, true), (6, true), (6, true)];

/// The Requests panel: `frame::panel("Requests", Role::Server, false, theme)` with a right title `last <n>`
/// (`Dim`), a header row and the newest `app.requests` first, as many as fit, keeping the last inner row for
/// the summary when there are at least 3 inner rows. See `docs/02-ui.md` § 2.2 (T32).
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = frame::panel("Requests", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    // The `last <n>` count, right-aligned on the top border.
    let right = area.x + area.width;
    let title = format!("last {} ", app.requests.len());
    let w = title.chars().count() as u16;
    let x = right.saturating_sub(w).max(area.x);
    buf.set_line(
        x,
        area.y,
        &Line::from(Span::styled(title, theme.fg(Role::Dim))),
        area.width.saturating_sub(x),
    );

    if app.requests.is_empty() {
        set(
            buf,
            inner.x,
            inner.y,
            &Line::from(Span::styled("no request yet", theme.fg(Role::Dim))),
            inner.width,
        );
        return;
    }

    // The header row, then the newest requests, the summary on the last inner row when there is room for it.
    let has_summary = inner.height >= 3;
    let summary_y = inner.y + inner.height - 1;
    let data_rows = if has_summary { inner.height - 2 } else { inner.height - 1 };
    set(buf, inner.x, inner.y, &header_line(theme), inner.width);

    let newest: Vec<&LastRequest> = app.requests.iter().rev().take(data_rows as usize).collect();
    for (i, req) in newest.iter().enumerate() {
        set(buf, inner.x, inner.y + 1 + i as u16, &request_line(req, theme), inner.width);
    }
    if has_summary {
        set(buf, inner.x, summary_y, &summary_line(&newest, theme), inner.width);
    }
}

/// `set_line` at `(x, y)`, cut to `width`.
fn set(buf: &mut Buffer, x: u16, y: u16, line: &Line, width: u16) {
    buf.set_line(x, y, line, width);
}

/// One table row: the five cells, one space between columns, all styled `style`.
fn row(parts: &[&str], style: Style) -> Line<'static> {
    let mut spans = Vec::with_capacity(COLS.len() * 2);
    for (i, (width, right)) in COLS.iter().enumerate() {
        spans.push(Span::styled(cell(parts[i], *width, *right), style));
        if i + 1 < COLS.len() {
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}

/// `s` cut to `width` columns, padded to it (right when `right`).
fn cell(s: &str, width: u16, right: bool) -> String {
    let s = fit(s, width as usize);
    let width = width as usize;
    if right { format!("{:>width$}", s) } else { format!("{:<width$}", s) }
}

/// The header row: the column names in `Dim`.
fn header_line(theme: &Theme) -> Line<'static> {
    row(&["time", "prompt", "gen", "t/s", "draft"], theme.fg(Role::Dim))
}

/// One request row: the values in `Text`, a missing value being `-`.
fn request_line(req: &LastRequest, theme: &Theme) -> Line<'static> {
    let time = req.time.as_deref().unwrap_or("-");
    let prompt = req.prompt_tokens.map(group_thousands).unwrap_or_else(|| "-".to_string());
    let gen_t = req.gen_tokens.map(group_thousands).unwrap_or_else(|| "-".to_string());
    let tps_v = match req.gen_tps {
        Some(v) => tps(Some(v)),
        None => "-".to_string(),
    };
    let draft = match req.draft_pct {
        Some(v) => pct(Some(v)),
        None => "-".to_string(),
    };
    row(&[time, prompt.as_str(), gen_t.as_str(), tps_v.as_str(), draft.as_str()], theme.fg(Role::Text))
}

/// The summary row (`Dim`): `avg <mean gen_tps> t/s · <n> req · <tokens> tok` over the shown requests.
fn summary_line(reqs: &[&LastRequest], theme: &Theme) -> Line<'static> {
    let known: Vec<f64> = reqs.iter().filter_map(|r| r.gen_tps).collect();
    let mean = if known.is_empty() { None } else { Some(known.iter().sum::<f64>() / known.len() as f64) };
    let tokens: u64 = reqs.iter().map(|r| r.prompt_tokens.unwrap_or(0) + r.gen_tokens.unwrap_or(0)).sum();
    let text = format!("avg {} t/s · {} req · {} tok", tps(mean), reqs.len(), count(tokens));
    Line::from(vec![Span::styled(text, theme.fg(Role::Dim))])
}
