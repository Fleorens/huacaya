//! The System screen: per-core CPU bars, every temperature sensor, the disks, the network and the
//! `llama-server` processes. Read-only. See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::disk::DiskSample;
use crate::collect::net::NetSample;
use crate::collect::process::{ProcSample, ServerRole};
use crate::collect::sensors::TempReading;
use crate::ui::frame::panel;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{bytes_pair, fit, pct, temp};
use crate::ui::theme::{CPU_TEMP_THRESHOLDS, GPU_JUNCTION_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, Theme, hot_mark, severity};
use crate::util::{bytes, duration, short_model_name};

/// The System screen in `area` (the body below the header).
///
/// A left column of `area.width / 2` columns holds the Cores panel over the whole height; a right
/// column stacks Sensors, Disks, Network and Processes top to bottom. A right panel that does not fit
/// in the rows left is cut to them when it keeps at least 3 rows, else dropped with everything under
/// it. Without a system snapshot one `System` panel over the whole area shows a `Dim` `no data yet`
/// row.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let Some(sys) = &app.system else {
        render_empty(theme, area, buf);
        return;
    };

    let left_w = area.width / 2;
    render_cores(buf, app, theme, Rect::new(area.x, area.y, left_w, area.height));

    let right_x = area.x + left_w;
    let right_w = area.width - left_w;
    let mut y = area.y;
    let bottom = area.y + area.height;

    // Right column: stack the panels, cutting or dropping the ones that do not fit.
    let r = slot(sys.temps.len() as u16 + 2, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_sensors(buf, rect, &sys.temps, theme);
    y = rect.bottom();

    let r = slot(sys.disks.len() as u16 + 2, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_disks(buf, rect, &sys.disks, theme);
    y = rect.bottom();

    let r = slot(sys.nets.len() as u16 + 2, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_network(buf, rect, &sys.nets, theme);
    y = rect.bottom();

    let r = slot(sys.procs.len() as u16 + 3, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_processes(buf, rect, &sys.procs, theme);
}

/// The "no snapshot" case: one `System` panel over the whole area with a single `Dim` row.
fn render_empty(theme: &Theme, area: Rect, buf: &mut Buffer) {
    let block = panel("System", Role::Cpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 {
        return;
    }
    let msg = "no data yet";
    let w = msg.chars().count().min(inner.width as usize) as u16;
    buf.set_line(inner.x, inner.y, &Line::from(Span::styled(msg, theme.fg(Role::Dim))), w);
}

/// The rectangle a right-column panel of `needed` rows gets in the `avail` rows left at `y`, or `None`
/// to drop it when fewer than 3 rows remain.
fn slot(needed: u16, avail: u16, y: u16, right_x: u16, right_w: u16) -> Option<Rect> {
    let h = if needed <= avail { needed } else { if avail >= 3 { avail } else { return None } };
    Some(Rect::new(right_x, y, right_w, h))
}

/// The warning / critical thresholds of one sensor: `amdgpu` → `GPU_JUNCTION_THRESHOLDS`; `coretemp`
/// and `k10temp` → `CPU_TEMP_THRESHOLDS`; `nvme` → `(70.0, 80.0)`; any other chip → `(80.0, 95.0)`.
pub fn sensor_thresholds(chip: &str) -> (f32, f32) {
    match chip {
        "amdgpu" => GPU_JUNCTION_THRESHOLDS,
        "coretemp" | "k10temp" => CPU_TEMP_THRESHOLDS,
        "nvme" => (70.0, 80.0),
        _ => (80.0, 95.0),
    }
}

/// The Cores panel: one core per row when they fit with the summary, otherwise two per row, then the
/// average frequency, load averages and uptime.
fn render_cores(buf: &mut Buffer, app: &App, theme: &Theme, area: Rect) {
    let block = panel("Cores", Role::Cpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 2 {
        return;
    }
    let cores = &app.system.as_ref().unwrap().cpu.per_core_pct;
    let n = cores.len();
    let mut y = inner.y;

    if inner.height >= n as u16 + 3 {
        // One core per row.
        for (i, pct_val) in cores.iter().enumerate() {
            if y >= inner.bottom() {
                break;
            }
            let label = format!("{:<6}", format!("cpu{i}"));
            gauge_row(buf, inner.x, y, inner.width, &label, *pct_val as f64 / 100.0, Role::Cpu, PCT_THRESHOLDS, &pct(Some(*pct_val as f64)), theme);
            y += 1;
        }
    } else {
        // Two per row: core `i` on row `i / 2`, even `i` on the left half.
        let half = inner.width / 2;
        for (i, pct_val) in cores.iter().enumerate() {
            let cy = inner.y + (i / 2) as u16;
            if cy >= inner.bottom() {
                break;
            }
            let x = if i % 2 == 0 { inner.x } else { inner.x + half };
            let label = format!("{:<6}", format!("cpu{i}"));
            gauge_row(buf, x, cy, half, &label, *pct_val as f64 / 100.0, Role::Cpu, PCT_THRESHOLDS, &pct(Some(*pct_val as f64)), theme);
        }
    }

    // One blank row, then the average frequency, load averages and uptime; rows that do not fit drop.
    if y < inner.bottom() {
        y += 1;
    }
    if let Some(freq) = app.system.as_ref().unwrap().cpu.freq.as_ref() {
        if y < inner.bottom() {
            let line = format!("freq   {:.1} GHz avg   {:.1} GHz max", freq.avg_mhz / 1000.0, freq.max_mhz / 1000.0);
            write_row(buf, inner.x, y, inner.width, &line, theme);
            y += 1;
        }
    }
    if let Some(load) = app.system.as_ref().unwrap().cpu.load.as_ref() {
        if y < inner.bottom() {
            let line = format!("load   {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen);
            write_row(buf, inner.x, y, inner.width, &line, theme);
            y += 1;
        }
    }
    if let Some(uptime) = app.system.as_ref().unwrap().uptime_s {
        if y < inner.bottom() {
            let line = format!("uptime {}", duration(uptime));
            write_row(buf, inner.x, y, inner.width, &line, theme);
        }
    }
}

/// The Sensors panel: one `<chip>/<label>  <temp><mark>` row per reading.
fn draw_sensors(buf: &mut Buffer, rect: Rect, temps: &[TempReading], theme: &Theme) {
    let block = panel("Sensors", Role::Gpu, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 1 || inner.width < 1 {
        return;
    }
    let mut y = inner.y;
    for t in temps {
        if y >= inner.bottom() {
            break;
        }
        let name = format!("{}/{}", t.chip, t.label);
        let label = format!("{} ", fit(&format!("{:<24}", name), 23));
        let (warning, critical) = sensor_thresholds(&t.chip);
        let sev = severity(t.celsius, warning, critical);
        let style = match sev {
            Severity::Normal => theme.fg(Role::Text),
            Severity::Warning => theme.fg(Role::Warning),
            Severity::Critical => theme.fg(Role::Critical),
        };
        let value = format!("{}{}", temp(Some(t.celsius)), hot_mark(sev));
        buf.set_line(inner.x, y, &Line::from(vec![Span::styled(label, theme.fg(Role::Dim)), Span::styled(value, style)]), inner.width);
        y += 1;
    }
}

/// The Disks panel: one `mount  <gauge>  used/total` row per disk.
fn draw_disks(buf: &mut Buffer, rect: Rect, disks: &[DiskSample], theme: &Theme) {
    let block = panel("Disks", Role::Memory, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 1 || inner.width < 1 {
        return;
    }
    let mut y = inner.y;
    for d in disks {
        if y >= inner.bottom() {
            break;
        }
        let label = format!("{} ", fit(&format!("{:<13}", d.mount), 12));
        let ratio = d.used as f64 / d.total as f64;
        gauge_row(buf, inner.x, y, inner.width, &label, ratio, Role::Memory, PCT_THRESHOLDS, &bytes_pair(d.used, d.total), theme);
        y += 1;
    }
}

/// The Network panel: one row per interface with its live rates and cumulative totals.
fn draw_network(buf: &mut Buffer, rect: Rect, nets: &[NetSample], theme: &Theme) {
    let block = panel("Network", Role::Memory, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 1 || inner.width < 1 {
        return;
    }
    let mut y = inner.y;
    for n in nets {
        if y >= inner.bottom() {
            break;
        }
        let rate = |x: Option<f64>| match x { Some(v) => format!("{}/s", bytes(v as u64)), None => "n/a".to_string() };
        let line = format!(
            "{:<10}↓ {}  ↑ {}   total ↓ {}  ↑ {}",
            n.iface, rate(n.rx_bps), rate(n.tx_bps), bytes(n.rx_total), bytes(n.tx_total)
        );
        write_row(buf, inner.x, y, inner.width, &line, theme);
        y += 1;
    }
}

/// The Processes panel: a header row, then one row per `llama-server` process.
fn draw_processes(buf: &mut Buffer, rect: Rect, procs: &[ProcSample], theme: &Theme) {
    let block = panel("Processes", Role::Server, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 2 || inner.width < 1 {
        return;
    }
    eprintln!("DBG processes inner.width={}", inner.width);
    let header = format!("{:<9}{:<10}{:<7}{:<10}{:<9}model", "pid", "role", "port", "rss", "threads");
    let fixed = 9 + 10 + 7 + 10 + 9;
    let model_w = inner.width.saturating_sub(fixed as u16);
    let mut y = inner.y;

    if y < inner.bottom() {
        buf.set_line(
            inner.x,
            y,
            &Line::from(Span::styled(header, theme.fg(Role::Dim).add_modifier(Modifier::BOLD))),
            inner.width,
        );
        y += 1;
    }

    for p in procs {
        if y >= inner.bottom() {
            break;
        }
        let role = match p.role { ServerRole::Router => "router", ServerRole::Instance => "instance" };
        let port = p.port.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string());
        let model = match &p.model { Some(m) => short_model_name(m).0, None => "-".to_string() };
        let line = format!(
            "{:<9}{:<10}{:<7}{:<10}{:<9}{}",
            p.pid, role, port, bytes(p.rss), p.threads, fit(&model, model_w as usize)
        );
        write_row(buf, inner.x, y, inner.width, &line, theme);
        y += 1;
    }
}

/// A plain `<value>` row in `Role::Text`, cut to `width`.
fn write_row(buf: &mut Buffer, x: u16, y: u16, width: u16, value: &str, theme: &Theme) {
    buf.set_line(x, y, &Line::from(Span::styled(value.to_string(), theme.fg(Role::Text))), width);
}
