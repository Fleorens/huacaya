//! The GPU panel: load and VRAM gauges, junction/edge temperatures with a hot mark, power and fan,
//! and the load graph. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::gpu::GpuSample;
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{bytes_pair, pct, temp};
use crate::ui::theme::{
    GPU_JUNCTION_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, VRAM_THRESHOLDS, hot_mark, severity,
};
use crate::util::bytes;

/// Height needed, borders included (4 rows inside).
pub const MIN_HEIGHT: u16 = 6;

/// The GPU panel of `system.gpus[0]`: `frame::panel` with a `"GPU  <name>"` title (leading `Radeon `,
/// `NVIDIA ` or `NVIDIA GeForce ` removed), the load and VRAM gauges, the temperature (junction marked
/// when hot, and edge; the edge alone when junction is absent, as NVIDIA reports it), the power and fan,
/// and the load graph. When the inner height is at least 10, three extra rows show the core
/// clock/voltage/fan, the DRAM temperature and GTT, and the PCIe link. No system snapshot or no GPU:
/// title `GPU`, one `Dim` row `no GPU`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let gpu = app.system.as_ref().and_then(|s| s.gpus.first());
    let title = match &gpu {
        Some(g) => {
            let name = g.name.as_str();
            let name = name
                .strip_prefix("NVIDIA GeForce ")
                .or_else(|| name.strip_prefix("NVIDIA "))
                .or_else(|| name.strip_prefix("Radeon "))
                .unwrap_or(name);
            format!("GPU  {name}")
        }
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
            buf.set_line(inner.x, y0, &Line::from(Span::styled("no GPU", theme.fg(Role::Dim))), inner.width);
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
            // Row 3: temperature. NVIDIA reports only the edge temperature, shown plainly as `temp`; AMD
            // reports junction (marked when hot), edge and DRAM temperature.
            let y = y0 + 2;
            if y < inner.bottom() {
                let line = Line::from(Span::styled(temp_line(g), theme.fg(Role::Text)));
                buf.set_line(inner.x, y, &line, inner.width);
            }
            // Row 4: power, fan and core clock.
            let y = y0 + 3;
            if y < inner.bottom() {
                let watts = |w: Option<f32>| match w {
                    Some(v) => format!("{:.0} W", v),
                    None => "n/a W".to_string(),
                };
                let fan = match (g.fan_pct, g.fan_rpm) {
                    (Some(p), _) => pct(Some(p as f64)),
                    (None, Some(r)) => format!("{r} rpm"),
                    (None, None) => "n/a rpm".to_string(),
                };
                let sclk = g.sclk_mhz
                    .map(|v| format!("{:.0} MHz", v))
                    .unwrap_or_else(|| "n/a MHz".to_string());
                let line = format!("power {} / {}   fan {}   clk {sclk}", watts(g.power_w), watts(g.power_cap_w), fan);
                buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
            }
            // Rows 5–6: extra detail, shown only when the panel is tall enough (inner height >= 10).
            if inner.height >= 10 {
                // Row 5: voltage and GTT usage.
                let y = y0 + 4;
                if y < inner.bottom() {
                    let mv = g.vddgfx_mv.map(|v| format!("{v} mV")).unwrap_or_else(|| "n/a mV".to_string());
                    let gtt = g.gtt_used.map(bytes).unwrap_or_else(|| "n/a".to_string());
                    let line = format!("volt  {mv}   GTT {gtt}");
                    buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
                }
                // Row 6: PCIe link.
                let y = y0 + 5;
                if y < inner.bottom() {
                    let link = g.link.as_deref().unwrap_or("n/a");
                    let line = format!("link  {link}");
                    buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
                }
            }
            // Load graph on all the rows left.
            if ctx.graphs && inner.height > 4 {
                let rows_before = if inner.height >= 10 { 6 } else { 4 };
                let grect = Rect::new(inner.x, y0 + rows_before, inner.width, inner.height - rows_before);
                BrailleGraph { values: &app.gpu_load.tail(usize::MAX), max: 100.0, accent: Role::Gpu, theme }
                    .render(grect, buf);
            }
        }
    }
}

/// The temperature line for `g`: NVIDIA reports only the edge, shown plainly as `temp  <edge>` marked
/// when hot; AMD reports junction (marked when hot), edge and DRAM temperature, each part only when
/// known, as `temp  <j> junction  <e> edge  <m> mem`.
fn temp_line(g: &GpuSample) -> String {
    if g.temp_junction_c.is_none() {
        let edge = g.temp_edge_c;
        let sev = edge
            .map(|e| severity(e, GPU_JUNCTION_THRESHOLDS.0, GPU_JUNCTION_THRESHOLDS.1))
            .unwrap_or(Severity::Normal);
        return format!("temp  {}{}", temp(edge), hot_mark(sev));
    }
    let junction = g.temp_junction_c;
    let sev = junction
        .map(|j| severity(j, GPU_JUNCTION_THRESHOLDS.0, GPU_JUNCTION_THRESHOLDS.1))
        .unwrap_or(Severity::Normal);
    let mut parts: Vec<String> = vec![format!("{}{} junction", temp(junction), hot_mark(sev))];
    if let Some(e) = g.temp_edge_c {
        parts.push(format!("{} edge", temp(Some(e))));
    }
    if let Some(m) = g.temp_mem_c {
        parts.push(format!("{} mem", temp(Some(m))));
    }
    format!("temp  {}", parts.join("  "))
}
