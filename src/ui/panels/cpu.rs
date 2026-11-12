//! The CPU panel: usage gauge, temperature with a hot mark and average frequency, load averages, and
//! the usage graph. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{pct, temp};
use crate::ui::theme::{CPU_TEMP_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, Theme, hot_mark, severity};

/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;

/// The CPU panel: `frame::panel("CPU", Role::Cpu, false, theme)` and three rows — the usage gauge, the
/// temperature (marked when hot) with the average frequency, and the load averages — then the usage
/// graph. Without a system snapshot the three rows show `n/a` values and no gauge.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = panel("CPU", Role::Cpu, false, theme);
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
    // Row 2: temperature (marked when hot) and average frequency.
    let y = y0 + 1;
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
            Span::styled(ghz, theme.fg(Role::Text)),
            Span::styled(" GHz avg", theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
    }
    // Row 3: load averages.
    let y = y0 + 2;
    if y < inner.bottom() {
        match load {
            Some(l) => {
                let line = format!("load  {:.2} {:.2} {:.2}", l.one, l.five, l.fifteen);
                buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
            }
            None => row(theme, buf, inner.x, y, inner.width, "load  ", "n/a n/a n/a"),
        }
    }
    // Row 4: usage graph on all the rows left.
    if ctx.graphs && inner.height > 3 {
        let grect = Rect::new(inner.x, y0 + 3, inner.width, inner.height - 3);
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
