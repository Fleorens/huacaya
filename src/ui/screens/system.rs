//! The System screen: the CPU panel (per-core heatmap, usage graph, load) and the sensors on the left,
//! the disks, the network and the `llama-server` processes (plus the GPU summary) on the right.
//! Read-only. See `tasks/T38-system-v2.md`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::disk::DiskSample;
use crate::collect::gpu::GpuSample;
use crate::collect::net::NetSample;
use crate::collect::process::{ProcSample, ServerRole};
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::gpu::short_gpu_name;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::screens::system_left::{cpu_needed_height, render_cpu, render_sensors};
use crate::ui::text::{bytes_pair, fit, fit_groups};
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::History;
use crate::util::{bytes, short_model_name};

/// The System screen in `area` (the body below the header).
///
/// A left column of `area.width / 2` columns holds the CPU panel at its own height over the Sensors
/// panel; a right column stacks Disks, Network and Processes top to bottom. A right panel that does
/// not fit in the rows left is cut to them when it keeps at least 3 rows, else dropped with everything
/// under it. Without a system snapshot one `System` panel over the whole area shows a `Dim` `no data
/// yet` row. The last row of the area carries a `Dim` hint.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let Some(sys) = &app.system else {
        render_empty(theme, area, buf);
        return;
    };

    let left_w = area.width / 2;
    let right_x = area.x + left_w;
    let right_w = area.width - left_w;

    // Left column: the CPU panel at its own height, the Sensors panel on the rest.
    let left_h = area.height;
    let cpu_h = cpu_needed_height(sys, ctx).min(left_h);
    render_cpu(app, ctx, Rect::new(area.x, area.y, left_w, cpu_h), buf);
    render_sensors(app, ctx, Rect::new(area.x, area.y + cpu_h, left_w, left_h - cpu_h), buf);

    // The last row of the whole area: a `Dim` hint, fit at ` · `.
    if area.height >= 12 {
        let y = area.y + area.height - 1;
        let hint = "cores: brighter = busier · sensors grouped by chip · s: all sensors";
        let groups: Vec<Vec<Span>> =
            hint.split(" · ").map(|g| vec![Span::styled(g.to_string(), theme.fg(Role::Dim))]).collect();
        buf.set_line(area.x, y, &fit_groups(groups, area.width as usize), area.width);
    }

    // Right column: stack the panels, cutting or dropping the ones that do not fit.
    let mut y = area.y;
    let bottom = area.y + area.height;

    let r = slot(sys.disks.len() as u16 + 2, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_disks(buf, rect, &sys.disks, theme);
    y = rect.bottom();

    let r = slot(sys.nets.len() as u16 * 2 + 2, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_network(buf, app, rect, &sys.nets, ctx);
    y = rect.bottom();

    let r = slot(sys.procs.len() as u16 + 5, bottom - y, y, right_x, right_w);
    let Some(rect) = r else { return };
    draw_processes(buf, rect, &sys.procs, ctx, &sys.gpus);
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
    let h = if needed <= avail {
        needed
    } else if avail >= 3 {
        avail
    } else {
        return None;
    };
    Some(Rect::new(right_x, y, right_w, h))
}

/// The warning / critical thresholds of one sensor: `amdgpu` → `GPU_JUNCTION_THRESHOLDS`; `coretemp`
/// and `k10temp` → `CPU_TEMP_THRESHOLDS`; `nvme` → `(70.0, 80.0)`; any other chip → `(80.0, 95.0)`.
pub fn sensor_thresholds(chip: &str) -> (f32, f32) {
    match chip {
        "amdgpu" => crate::ui::theme::GPU_JUNCTION_THRESHOLDS,
        "coretemp" | "k10temp" => crate::ui::theme::CPU_TEMP_THRESHOLDS,
        "nvme" => (70.0, 80.0),
        _ => (80.0, 95.0),
    }
}

/// The rx/tx rate histories of one interface, from `app.net_rates`.
fn rates_for<'a>(app: &'a App, iface: &str) -> Option<&'a (String, History, History)> {
    app.net_rates.iter().find(|(i, _, _)| i == iface)
}

/// The Disks panel: one `mount  <gauge>  used/total` row per disk.
fn draw_disks(buf: &mut Buffer, rect: Rect, disks: &[DiskSample], theme: &Theme) {
    let block = panel("Disks", Role::Memory, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 1 || inner.width < 1 {
        return;
    }
    for (d, y) in disks.iter().zip(inner.y..inner.bottom()) {
        let label = format!("{:<12} ", fit(&d.mount, 12));
        let ratio = if d.total > 0 { d.used as f64 / d.total as f64 } else { 0.0 };
        gauge_row(
            buf,
            inner.x,
            y,
            inner.width,
            &label,
            ratio,
            Role::Memory,
            PCT_THRESHOLDS,
            &bytes_pair(d.used, d.total),
            theme,
        );
    }
}

/// The Network panel: two rows per interface, one for the down rates and one for the up rates.
fn draw_network(buf: &mut Buffer, app: &App, rect: Rect, nets: &[NetSample], ctx: &Ctx) {
    let theme = ctx.theme;
    let block = panel("Network", Role::Memory, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 1 || inner.width < 1 {
        return;
    }
    for (n, y) in nets.iter().zip(inner.y..inner.bottom()) {
        draw_net_row(buf, app, inner.x, y, inner.width, n, ctx, true);
        draw_net_row(buf, app, inner.x, y + 1, inner.width, n, ctx, false);
    }
}

/// One network row: `<iface pad8>↓ <rate>  <graph>   total <bytes>`, the down row (`down = true`) or
/// the up row (`down = false`, blank interface column). The 22-col graph is omitted when graphs are off
/// or fewer than 22 columns remain after the rest of the row.
#[allow(clippy::too_many_arguments)] // the panel contract; callers pass one row's worth
fn draw_net_row(
    buf: &mut Buffer,
    app: &App,
    x: u16,
    y: u16,
    width: u16,
    n: &NetSample,
    ctx: &Ctx,
    down: bool,
) {
    let theme = ctx.theme;
    let rate = match (down, n.rx_bps, n.tx_bps) {
        (true, Some(v), _) => format!("{}/s", bytes(v as u64)),
        (false, _, Some(v)) => format!("{}/s", bytes(v as u64)),
        _ => "n/a".to_string(),
    };
    let rate = format!("{:>10}", rate);
    let arrow = if down { "↓" } else { "↑" };
    let iface = if down { format!("{:<8}", fit(&n.iface, 8)) } else { "        ".to_string() };
    // `iface(8) ↓ 1 rate(10) 2`, then the 22-col graph, then `   total <bytes>`.
    let prefix = format!("{iface}{arrow} {rate}  ");
    let total = if down { n.rx_total } else { n.tx_total };
    let suffix = format!("   total {}", bytes(total));
    let rest = prefix.chars().count() as u16 + suffix.chars().count() as u16;
    let graph_present = ctx.graphs && width.saturating_sub(rest) >= 22;
    let graph_w = if graph_present { 22 } else { 0 };

    let prefix_w = prefix.chars().count().min(width as usize) as u16;
    buf.set_span(x, y, &Span::styled(prefix, theme.fg(Role::Text)), prefix_w);
    if graph_present {
        let accent = if down { Role::Memory } else { Role::Gpu };
        let values = match rates_for(app, &n.iface) {
            Some((_, r, _)) if down => r.tail(usize::MAX),
            Some((_, _, t)) => t.tail(usize::MAX),
            None => Vec::new(),
        };
        let max = graph_max(&values);
        BrailleGraph { values: &values, max, accent, theme }.render(Rect::new(x + prefix_w, y, 22, 1), buf);
    }
    let suffix_x = prefix_w + graph_w;
    let suffix_w = width.saturating_sub(suffix_x);
    if suffix_w > 0 {
        buf.set_span(x + suffix_x, y, &Span::styled(suffix, theme.fg(Role::Text)), suffix_w);
    }
}

/// The graph max: the larger of the rx/tx histories' maxima, at least 1 (NaN becomes 1).
fn graph_max(values: &[f32]) -> f32 {
    let max = values.iter().copied().fold(0.0, f32::max);
    if max.is_nan() || max <= 0.0 { 1.0 } else { max }
}

/// The Processes panel: a header row, then one row per `llama-server` process, and when a GPU is known
/// and a row is left, a blank row followed by the GPU summary line.
fn draw_processes(buf: &mut Buffer, rect: Rect, procs: &[ProcSample], ctx: &Ctx, gpus: &[GpuSample]) {
    let theme = ctx.theme;
    let block = panel("Processes", Role::Server, false, theme);
    let inner = block.inner(rect);
    block.render(rect, buf);
    if inner.height < 2 || inner.width < 1 {
        return;
    }
    // rss is 9 wide, not the sheet's 10: at 140 columns the model column then holds the full
    // 24-character demo name (`Cyber-Tiel-Coder-35B-A3B`) that the sheet's test expects.
    let header = format!("{:<9}{:<10}{:<7}{:<9}{:<9}model", "pid", "role", "port", "rss", "threads");
    let fixed: u16 = 9 + 10 + 7 + 9 + 9;
    let model_w = inner.width.saturating_sub(fixed);
    buf.set_line(
        inner.x,
        inner.y,
        &Line::from(Span::styled(header, theme.fg(Role::Dim).add_modifier(Modifier::BOLD))),
        inner.width,
    );

    let mut y = inner.y + 1;
    for p in procs.iter() {
        if y >= inner.bottom() {
            break;
        }
        let role = match p.role {
            ServerRole::Router => "router",
            ServerRole::Instance => "instance",
        };
        let port = p.port.map(|x| x.to_string()).unwrap_or_else(|| "-".to_string());
        let model = match &p.model {
            Some(m) => short_model_name(m).0,
            None => "-".to_string(),
        };
        let line = format!(
            "{:<9}{:<10}{:<7}{:<9}{:<9}{}",
            p.pid,
            role,
            port,
            bytes(p.rss),
            p.threads,
            fit(&model, model_w as usize)
        );
        buf.set_line(inner.x, y, &Line::from(Span::styled(line, theme.fg(Role::Text))), inner.width);
        y += 1;
    }

    // The GPU summary line: a blank row, then `GPU  <name>  <link> · <sclk> MHz · <vddgfx> mV · fan <fan_pct> · GTT <bytes(gtt)>`.
    if let Some(g) = gpus.first()
        && y + 2 <= inner.bottom()
    {
        y += 1;
        buf.set_line(inner.x, y, &Line::raw(""), inner.width);
        y += 1;
        buf.set_line(inner.x, y, &fit_groups(gpu_summary_line(g, theme), inner.width as usize), inner.width);
    }
}

/// The GPU summary line, as groups for `fit_groups` split at ` · `: `GPU  <name>` in `Gpu`, the rest in
/// `Dim`, only the parts that exist.
fn gpu_summary_line(g: &GpuSample, theme: &Theme) -> Vec<Vec<Span<'static>>> {
    let mut groups: Vec<Vec<Span>> = vec![vec![
        Span::styled("GPU  ", theme.fg(Role::Gpu)),
        Span::styled(short_gpu_name(&g.name).to_string(), theme.fg(Role::Gpu)),
    ]];
    if let Some(link) = &g.link {
        groups.last_mut().unwrap().push(Span::styled(format!("  {link}"), theme.fg(Role::Dim)));
    }
    if let Some(sclk) = g.sclk_mhz {
        groups.push(vec![Span::styled(format!(" · {sclk:.0} MHz"), theme.fg(Role::Dim))]);
    }
    if let Some(vdd) = g.vddgfx_mv {
        groups.push(vec![Span::styled(format!(" · {vdd} mV"), theme.fg(Role::Dim))]);
    }
    if let Some(fan) = g.fan_pct {
        groups.push(vec![Span::styled(format!(" · fan {fan:.0}"), theme.fg(Role::Dim))]);
    }
    if let Some(gtt) = g.gtt_used {
        groups.push(vec![Span::styled(format!(" · GTT {}", bytes(gtt)), theme.fg(Role::Dim))]);
    }
    groups
}
