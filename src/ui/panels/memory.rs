//! The Memory panel: RAM and swap gauges, the model instance and router resident memory, available
//! and cached RAM, and the disk usage. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::process::ServerRole;
use crate::ui::frame::panel;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{bytes_pair, fit, pct};
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::bytes;

/// Height needed, borders included (4 content rows; 3 fit at the minimum).
pub const MIN_HEIGHT: u16 = 5;

/// The Memory panel: `frame::panel("Memory", Role::Memory, false, theme)` and the rows below — the RAM
/// and swap gauges, the model instance resident memory (with the router when it is a separate process,
/// else `server <rss>` for the router alone, or `model n/a`), the available and cached RAM, then the
/// disks as `disk  <mount> <pct>` joined by three spaces. Rows that do not fit are dropped from the
/// bottom. A swap total of 0 shows `swap  none`.
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
    // Row 3: the model instance resident memory, with the router when it is a separate process.
    let y = y0 + 2;
    if y < inner.bottom() {
        let line = match procs.iter().find(|p| p.role == ServerRole::Instance) {
            Some(inst) => match procs.iter().find(|p| p.role == ServerRole::Router) {
                Some(r) => format!("model {} rss   router {}", bytes(inst.rss), bytes(r.rss)),
                None => format!("model {} rss", bytes(inst.rss)),
            },
            None => match procs.iter().find(|p| p.role == ServerRole::Router) {
                Some(r) => format!("server {}", bytes(r.rss)),
                None => "model n/a".to_string(),
            },
        };
        buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
    }
    // Row 4: available and cached RAM.
    let y = y0 + 3;
    if y < inner.bottom() {
        let (avail, cached) = match mem {
            Some(m) => (bytes(m.available), bytes(m.cached)),
            None => ("n/a".to_string(), "n/a".to_string()),
        };
        let line = Line::from(vec![
            Span::styled("avail ", theme.fg(Role::Dim)),
            Span::styled(avail, theme.fg(Role::Text)),
            Span::styled("   ", theme.fg(Role::Text)),
            Span::styled("cached ", theme.fg(Role::Dim)),
            Span::styled(cached, theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
    }
    // Row 5: disk usage, one `disk  <mount> <pct>` per disk joined by three spaces.
    let y = y0 + 4;
    if y < inner.bottom() {
        let disks = app.system.as_ref().map(|s| s.disks.as_slice()).unwrap_or(&[]);
        if !disks.is_empty() {
            let parts: Vec<String> = disks
                .iter()
                .map(|d| {
                    let ratio = if d.total > 0 { d.used as f64 / d.total as f64 * 100.0 } else { 0.0 };
                    format!("{} {}", d.mount, pct(Some(ratio)))
                })
                .collect();
            let line = fit(&format!("disk  {}", parts.join("   ")), inner.width as usize);
            buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
        }
    }
}

/// A plain `<label> <value>` row, `label` in `Role::Dim`, `value` in `Role::Text`.
fn row(theme: &Theme, buf: &mut Buffer, x: u16, y: u16, width: u16, label: &str, value: &str) {
    let line =
        Line::from(vec![Span::styled(label, theme.fg(Role::Dim)), Span::styled(value, theme.fg(Role::Text))]);
    buf.set_line(x, y, &line, width);
}
