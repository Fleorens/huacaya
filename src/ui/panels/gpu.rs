//! The GPU panel: load and VRAM gauges, junction/edge temperatures with a hot mark, power and fan,
//! and the load graph. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{bytes_pair, pct, temp};
use crate::ui::theme::{
    GPU_JUNCTION_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, VRAM_THRESHOLDS, hot_mark, severity,
};

/// Height needed, borders included (4 rows inside).
pub const MIN_HEIGHT: u16 = 6;

/// The GPU panel of `system.gpus[0]`: `frame::panel` with a `"GPU  <name>"` title (leading `Radeon `
/// removed), the load and VRAM gauges, the junction/edge temperatures (junction marked when hot), the
/// power and fan, and the load graph. No system snapshot or no GPU: title `GPU`, one `Dim` row
/// `no AMD GPU`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let gpu = app.system.as_ref().and_then(|s| s.gpus.first());
    let title = match &gpu {
        Some(g) => format!("GPU  {}", g.name.strip_prefix("Radeon ").unwrap_or(&g.name)),
        None => "GPU".to_string(),
    };
    let block = panel(&title, Role::Gpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let y0 = inner.y;
    if y0 >= inner.bottom() {
        return;
    }

    match gpu {
        None => {
            buf.set_line(
                inner.x,
                y0,
                &Line::from(Span::styled("no AMD GPU", theme.fg(Role::Dim))),
                inner.width,
            );
        }
        Some(g) => {
            // Row 1: load.
            let y = y0;
            if y < inner.bottom() {
                let ratio = g.busy_pct.map(|b| b as f64 / 100.0).unwrap_or(0.0);
                let value = pct(g.busy_pct.map(|b| b as f64));
                gauge_row(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    "load  ",
                    ratio,
                    Role::Gpu,
                    PCT_THRESHOLDS,
                    &value,
                    theme,
                );
            }
            // Row 2: VRAM.
            let y = y0 + 1;
            if y < inner.bottom() {
                match (g.vram_used, g.vram_total) {
                    (Some(u), Some(t)) if t > 0 => gauge_row(
                        buf,
                        inner.x,
                        y,
                        inner.width,
                        "VRAM  ",
                        u as f64 / t as f64,
                        Role::Gpu,
                        VRAM_THRESHOLDS,
                        &bytes_pair(u, t),
                        theme,
                    ),
                    _ => {
                        let line = Line::from(vec![
                            Span::styled("VRAM  ", theme.fg(Role::Dim)),
                            Span::styled("n/a", theme.fg(Role::Text)),
                        ]);
                        buf.set_line(inner.x, y, &line, inner.width);
                    }
                }
            }
            // Row 3: junction/edge temperatures, junction marked when hot.
            let y = y0 + 2;
            if y < inner.bottom() {
                let junction = g.temp_junction_c;
                let edge = g.temp_edge_c;
                let sev = junction
                    .map(|j| severity(j, GPU_JUNCTION_THRESHOLDS.0, GPU_JUNCTION_THRESHOLDS.1))
                    .unwrap_or(Severity::Normal);
                let style = match sev {
                    Severity::Normal => theme.fg(Role::Text),
                    Severity::Warning => theme.fg(Role::Warning),
                    Severity::Critical => theme.fg(Role::Critical),
                };
                let line = Line::from(vec![
                    Span::styled("temp  ", theme.fg(Role::Dim)),
                    Span::styled(format!("{}{}", temp(junction), hot_mark(sev)), style),
                    Span::styled(" junction   ", theme.fg(Role::Text)),
                    Span::styled(temp(edge), theme.fg(Role::Text)),
                    Span::styled(" edge", theme.fg(Role::Text)),
                ]);
                buf.set_line(inner.x, y, &line, inner.width);
            }
            // Row 4: power and fan.
            let y = y0 + 3;
            if y < inner.bottom() {
                let watts = |w: Option<f32>| match w {
                    Some(v) => format!("{:.0} W", v),
                    None => "n/a W".to_string(),
                };
                let fan = match g.fan_rpm {
                    Some(r) => r.to_string(),
                    None => "n/a".to_string(),
                };
                let line = format!("power {} / {}   fan {} rpm", watts(g.power_w), watts(g.power_cap_w), fan);
                buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
            }
            // Row 5: load graph on all the rows left.
            if ctx.graphs && inner.height > 4 {
                let grect = Rect::new(inner.x, y0 + 4, inner.width, inner.height - 4);
                BrailleGraph { values: &app.gpu_load.tail(usize::MAX), max: 100.0, accent: Role::Gpu, theme }
                    .render(grect, buf);
            }
        }
    }
}
