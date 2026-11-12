//! The Memory panel: RAM and swap gauges, and the resident memory of the model process (or the router).
//! See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::process::ServerRole;
use crate::ui::frame::panel;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::bytes_pair;
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::bytes;

/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;

/// The Memory panel: `frame::panel("Memory", Role::Memory, false, theme)` and three rows — the RAM and
/// swap gauges, then `model <rss>` for the first model instance, `server <rss>` for the router without
/// one, or `model n/a` without any. A swap total of 0 shows `swap  none`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = panel("Memory", Role::Memory, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let y0 = inner.y;
    if y0 >= inner.bottom() {
        return;
    }

    let mem = app.system.as_ref().and_then(|s| s.mem.as_ref());
    let procs = app.system.as_ref().map(|s| s.procs.as_slice()).unwrap_or(&[]);

    // Row 1: RAM.
    let y = y0;
    if y < inner.bottom() {
        match mem {
            Some(m) => {
                let ratio = if m.total > 0 { m.used as f64 / m.total as f64 } else { 0.0 };
                let value = bytes_pair(m.used, m.total);
                gauge_row(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    "RAM   ",
                    ratio,
                    Role::Memory,
                    PCT_THRESHOLDS,
                    &value,
                    theme,
                );
            }
            None => row(theme, buf, inner.x, y, inner.width, "RAM   ", "n/a"),
        }
    }
    // Row 2: swap.
    let y = y0 + 1;
    if y < inner.bottom() {
        match mem {
            Some(m) if m.swap_total > 0 => {
                let value = bytes_pair(m.swap_used, m.swap_total);
                gauge_row(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    "swap  ",
                    m.swap_used as f64 / m.swap_total as f64,
                    Role::Memory,
                    PCT_THRESHOLDS,
                    &value,
                    theme,
                );
            }
            Some(_) => row(theme, buf, inner.x, y, inner.width, "swap  ", "none"),
            None => row(theme, buf, inner.x, y, inner.width, "swap  ", "n/a"),
        }
    }
    // Row 3: resident memory of the model instance, then the router, then `n/a`.
    let y = y0 + 2;
    if y < inner.bottom() {
        let (label, value) = match procs.iter().find(|p| p.role == ServerRole::Instance) {
            Some(p) => ("model", bytes(p.rss)),
            None => match procs.iter().find(|p| p.role == ServerRole::Router) {
                Some(p) => ("server", bytes(p.rss)),
                None => ("model", "n/a".to_string()),
            },
        };
        let line = format!("{label} {value}");
        buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
    }
}

/// A plain `<label> <value>` row, `label` in `Role::Dim`, `value` in `Role::Text`.
fn row(theme: &Theme, buf: &mut Buffer, x: u16, y: u16, width: u16, label: &str, value: &str) {
    let line =
        Line::from(vec![Span::styled(label, theme.fg(Role::Dim)), Span::styled(value, theme.fg(Role::Text))]);
    buf.set_line(x, y, &line, width);
}
