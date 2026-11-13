//! The CPU panel: usage gauge, per-core bars, temperature with a hot mark and frequency, load
//! averages, and the usage graph. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::ui::big::level_bars;
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{pct, temp};
use crate::ui::theme::{CPU_TEMP_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, Theme, hot_mark, severity};

/// Height needed, borders included (4 content rows; 3 fit at the minimum).
pub const MIN_HEIGHT: u16 = 5;

/// The CPU panel: `frame::panel` with a `"CPU  <model>"` title (just `CPU` when unknown), the usage
/// gauge, the per-core bars (with the thread count), the temperature (marked when hot) with the
/// average frequency, and the load averages, then the usage graph. Without a system snapshot the rows
/// show `n/a` values and no gauge.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let title = match app.system.as_ref().map(|s| s.cpu_info.model.as_ref()) {
        Some(Some(model)) => format!("CPU  {model}"),
        _ => "CPU".to_string(),
    };
    let block = panel(&title, Role::Cpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let y0 = inner.y;
    if y0 >= inner.bottom() {
        return;
    }

    let usage = app.system.as_ref().and_then(|s| s.cpu.usage_pct);
    let cpu_temp = app.system.as_ref().and_then(|s| s.cpu_temp_c);
    let avg_mhz = app.system.as_ref().and_then(|s| s.cpu.freq.as_ref()).map(|f| f.avg_mhz);
    let load = app.system.as_ref().and_then(|s| s.cpu.load.as_ref());
    let per_core = app.system.as_ref().map(|s| s.cpu.per_core_pct.as_slice()).unwrap_or(&[]);
    let threads = app.system.as_ref().and_then(|s| s.cpu_info.threads);

    // Row 1: usage gauge (no gauge when the value is missing).
    let y = y0;
    if y < inner.bottom() {
        match usage {
            Some(u) => {
                let value = pct(Some(u as f64));
                gauge_row(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    "usage ",
                    u as f64 / 100.0,
                    Role::Cpu,
                    PCT_THRESHOLDS,
                    &value,
                    theme,
                );
            }
            None => row(theme, buf, inner.x, y, inner.width, "usage ", "n/a"),
        }
    }
    // Row 2: per-core bars, with the thread count when known.
    let y = y0 + 1;
    if y < inner.bottom() {
        if per_core.is_empty() {
            row(theme, buf, inner.x, y, inner.width, "cores ", "n/a");
        } else {
            let bars = level_bars(per_core);
            let line = match threads {
                Some(t) => format!("cores {bars}  {t} thr"),
                None => format!("cores {bars}"),
            };
            buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
        }
    }
    // Row 3: temperature (marked when hot) and average frequency.
    let y = y0 + 2;
    if y < inner.bottom() {
        let sev = cpu_temp
            .map(|t| severity(t, CPU_TEMP_THRESHOLDS.0, CPU_TEMP_THRESHOLDS.1))
            .unwrap_or(Severity::Normal);
        let style = match sev {
            Severity::Normal => theme.fg(Role::Text),
            Severity::Warning => theme.fg(Role::Warning),
            Severity::Critical => theme.fg(Role::Critical),
        };
        let ghz = avg_mhz.map(|m| format!("{:.1}", m / 1000.0)).unwrap_or_else(|| "n/a".to_string());
        let line = Line::from(vec![
            Span::styled("temp  ", theme.fg(Role::Dim)),
            Span::styled(format!("{}{}", temp(cpu_temp), hot_mark(sev)), style),
            Span::styled("   ", theme.fg(Role::Text)),
            Span::styled("freq ", theme.fg(Role::Dim)),
            Span::styled(ghz, theme.fg(Role::Text)),
            Span::styled(" GHz", theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
    }
    // Row 4: load averages.
    let y = y0 + 3;
    if y < inner.bottom() {
        match load {
            Some(l) => {
                let line = format!("load  {:.2} {:.2} {:.2}", l.one, l.five, l.fifteen);
                buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
            }
            None => row(theme, buf, inner.x, y, inner.width, "load  ", "n/a n/a n/a"),
        }
    }
    // Row 5: usage graph on all the rows left.
    if ctx.graphs && inner.height > 4 {
        let grect = Rect::new(inner.x, y0 + 4, inner.width, inner.height - 4);
        BrailleGraph { values: &app.cpu_usage.tail(usize::MAX), max: 100.0, accent: Role::Cpu, theme }
            .render(grect, buf);
    }
}

/// A plain `<label> <value>` row, `label` in `Role::Dim`, `value` in `Role::Text`.
fn row(theme: &Theme, buf: &mut Buffer, x: u16, y: u16, width: u16, label: &str, value: &str) {
    let line =
        Line::from(vec![Span::styled(label, theme.fg(Role::Dim)), Span::styled(value, theme.fg(Role::Text))]);
    buf.set_line(x, y, &line, width);
}
