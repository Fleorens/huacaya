//! The left-column panels of the System screen (T38): the CPU panel (core heatmap, usage graph,
//! load) and the Sensors panel (readings grouped by chip). See `tasks/T38-system-v2.md`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::sampler::SystemSnapshot;
use crate::collect::sensors::TempReading;
use crate::ui::frame::panel;
use crate::ui::gauge::Gauge;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::Ctx;
use crate::ui::screens::system::sensor_thresholds;
use crate::ui::text::{fit, temp};
use crate::ui::theme::{ColorMode, Role, Severity, Theme, hot_mark, lerp_rgb, rgb, severity};
use crate::util::duration;

/// The CPU panel height, borders included: borders + the heatmap rows + the graph (3, when shown) +
/// the load row.
pub(crate) fn cpu_needed_height(sys: &SystemSnapshot, ctx: &Ctx) -> u16 {
    2 + heatmap_rows(sys) + 1 + if ctx.graphs { 3 } else { 0 }
}

/// The heatmap rows: one `cores n/a` row on the first sample, otherwise 8 cores per row.
fn heatmap_rows(sys: &SystemSnapshot) -> u16 {
    if sys.cpu.per_core_pct.is_empty() { 1 } else { sys.cpu.per_core_pct.len().div_ceil(8) as u16 }
}

/// The CPU panel: `frame::panel` with a `"CPU  <model> · <cores> cores / <threads> threads"` title
/// (just `CPU` when unknown) and a `<temp> · <freq> GHz` right title, the core heatmap (brighter =
/// busier), the usage graph, and the load averages plus uptime.
pub(crate) fn render_cpu(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let sys = match &app.system {
        Some(s) => s,
        None => return,
    };

    let title = cpu_title(sys);
    let right = cpu_right_title(sys);
    let block = panel(&title, Role::Cpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    place_right_title(area, &right, theme.fg(Role::Dim), buf, title.chars().count() as u16);

    let y0 = inner.y;
    if y0 >= inner.bottom() {
        return;
    }

    // The heatmap, then the usage graph, then the load row.
    draw_heatmap(buf, sys, theme, inner.x, y0, inner.width, inner.bottom());
    let after_heatmap = y0 + heatmap_rows(sys);
    if ctx.graphs {
        let gtop = after_heatmap;
        if gtop + 3 <= inner.bottom() {
            BrailleGraph { values: &app.cpu_usage.tail(usize::MAX), max: 100.0, accent: Role::Cpu, theme }
                .render(Rect::new(inner.x, gtop, inner.width, 3), buf);
        }
    }
    let ly = after_heatmap + if ctx.graphs { 3 } else { 0 };
    if ly < inner.bottom() {
        draw_load(buf, sys, theme, inner.x, ly, inner.width);
    }
}

/// The CPU title: `CPU`, then ` <model>`, ` · <cores> cores`, ` / <threads> threads` for the parts
/// that exist.
fn cpu_title(sys: &SystemSnapshot) -> String {
    let mut s = "CPU".to_string();
    if let Some(m) = &sys.cpu_info.model {
        s = format!("CPU  {m}");
    }
    if let Some(c) = sys.cpu_info.cores {
        s = format!("{s} · {c} cores");
    }
    if let Some(t) = sys.cpu_info.threads {
        s = format!("{s} / {t} threads");
    }
    s
}

/// The CPU right title: `<temp> · <freq> GHz` for the parts that exist (`temp` of `cpu_temp_c`, the
/// average frequency with one decimal).
fn cpu_right_title(sys: &SystemSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = sys.cpu_temp_c {
        parts.push(temp(Some(t)));
    }
    if let Some(f) = &sys.cpu.freq {
        parts.push(format!("{:.1} GHz", f.avg_mhz / 1000.0));
    }
    parts.join(" · ")
}

/// The core heatmap: 8 cells per row, one column gap, each cell the core's rounded percentage centred
/// and coloured by load. One `cores n/a` row when there is no per-core value yet.
fn draw_heatmap(
    buf: &mut Buffer,
    sys: &SystemSnapshot,
    theme: &Theme,
    x: u16,
    y: u16,
    width: u16,
    bottom: u16,
) {
    let cores = &sys.cpu.per_core_pct;
    if cores.is_empty() {
        write_dim(buf, x, y, width, "cores n/a", theme);
        return;
    }
    // `w = (inner.width − 7) / 8` (8 cells, 7 one-column gaps), at least 3. Cells are clipped to the
    // panel width, so a narrow panel shows fewer than eight per row without spilling past it.
    let w = (width.saturating_sub(7) / 8).max(3);
    let right = x + width;
    for (i, pct) in cores.iter().enumerate() {
        let row = i / 8;
        let cy = y + row as u16;
        if cy >= bottom {
            break;
        }
        let cx = x + (i % 8) as u16 * (w + 1);
        if cx >= right {
            break; // the rest of the row is past the panel
        }
        draw_cell(buf, cx, cy, w, *pct, theme, right);
    }
}

/// The styled core cell: the rounded number centred in `w` columns, with the load colour. Clipped to
/// `right` so a cell never writes past the panel.
fn draw_cell(buf: &mut Buffer, cx: u16, cy: u16, w: u16, pct: f32, theme: &Theme, right: u16) {
    let style = match theme.mode {
        // True colour: the background lerp from Selection to Cpu (brighter = busier); the number goes
        // dark from 80 % so it stays readable on a hot cell.
        ColorMode::TrueColor => {
            let bg = lerp_rgb(rgb(Role::Selection), rgb(Role::Cpu), pct / 100.0);
            let fg = if pct >= 80.0 { (0x1a, 0x1b, 0x26) } else { rgb(Role::Text) };
            Style::default().bg(Color::Rgb(bg.0, bg.1, bg.2)).fg(Color::Rgb(fg.0, fg.1, fg.2))
        }
        // 16 colours: no background, the number by load band.
        ColorMode::Ansi16 => match severity(pct, 70.0, 90.0) {
            Severity::Normal => theme.color(Role::Text),
            Severity::Warning => theme.color(Role::Warning),
            Severity::Critical => theme.color(Role::Critical),
        }
        .map_or(Style::default(), Style::from),
        // No colour: the number alone, bold from 90 %.
        ColorMode::NoColor => {
            if pct >= 90.0 {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        }
    };
    // The number centred in `w` columns, so the cell is a single styled span.
    let n = format!("{:.0}", pct);
    let nw = n.chars().count() as i64;
    let left = ((w as i64 - nw) / 2).max(0);
    let s: Vec<char> = std::iter::repeat_n(' ', left as usize)
        .chain(n.chars())
        .chain(std::iter::repeat_n(' ', (w as i64 - left - nw) as usize))
        .collect();
    let span_w = w.min(right.saturating_sub(cx));
    buf.set_span(cx, cy, &Span::styled(s.into_iter().collect::<String>(), style), span_w);
}

/// The load row: `load <one> <five> <fifteen>   uptime <duration>`, labels in `Dim`.
fn draw_load(buf: &mut Buffer, sys: &SystemSnapshot, theme: &Theme, x: u16, y: u16, width: u16) {
    let load = sys
        .cpu
        .load
        .as_ref()
        .map(|l| format!("{} {} {}", l.one, l.five, l.fifteen))
        .unwrap_or_else(|| "n/a n/a n/a".to_string());
    let uptime = sys.uptime_s.map(duration).unwrap_or_else(|| "n/a".to_string());
    let line = Line::from(vec![
        Span::styled("load ", theme.fg(Role::Dim)),
        Span::styled(load, theme.fg(Role::Text)),
        Span::styled("   uptime ", theme.fg(Role::Dim)),
        Span::styled(uptime, theme.fg(Role::Text)),
    ]);
    buf.set_line(x, y, &line, width);
}

/// The Sensors panel: `frame::panel` with a `"Sensors"` title and a `<n> sensors · s: all/summary`
/// right title, the readings grouped by chip (GPU, CPU, Storage, Board), each group under a bold
/// `Dim` header, rows that do not fit dropped from the bottom.
pub(crate) fn render_sensors(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let sys = match &app.system {
        Some(s) => s,
        None => return,
    };

    let right = if app.all_sensors {
        format!("{} sensors · s: all", sys.temps.len())
    } else {
        format!("{} sensors · s: summary", sys.temps.len())
    };
    let block = panel("Sensors", Role::Gpu, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    place_right_title(area, &right, theme.fg(Role::Dim), buf, "Sensors".chars().count() as u16);

    let y0 = inner.y;
    if y0 >= inner.bottom() {
        return;
    }
    let all = app.all_sensors;

    let mut y = y0;
    for group in sensor_groups(sys) {
        y = draw_group_header(buf, theme, inner.x, inner.width, &group, y);
        if y >= inner.bottom() {
            return;
        }
        for (label, celsius) in sensor_rows(&group.readings, all) {
            if y >= inner.bottom() {
                break;
            }
            draw_sensor_row(buf, theme, inner.x, y, inner.width, group.accent, &group.chip, &label, celsius);
            y += 1;
        }
    }
}

/// One sensor group: its header text, chip (for thresholds), accent and the readings, in reading order.
struct SensorGroup {
    header: String,
    chip: String,
    accent: Role,
    readings: Vec<TempReading>,
}

/// The groups present, in order: GPU (`amdgpu`), CPU (`coretemp` / `k10temp`), Storage (`nvme`), Board
/// (the other chips). An empty group is skipped.
fn sensor_groups(sys: &SystemSnapshot) -> Vec<SensorGroup> {
    let mut groups = Vec::new();
    let amd: Vec<TempReading> = sys.temps.iter().filter(|t| t.chip == "amdgpu").cloned().collect();
    if !amd.is_empty() {
        groups.push(SensorGroup {
            header: "GPU · amdgpu".to_string(),
            chip: "amdgpu".to_string(),
            accent: Role::Gpu,
            readings: amd,
        });
    }
    let cpu: Vec<TempReading> =
        sys.temps.iter().filter(|t| t.chip == "coretemp" || t.chip == "k10temp").cloned().collect();
    if !cpu.is_empty() {
        let chip = cpu[0].chip.clone();
        groups.push(SensorGroup { header: format!("CPU · {chip}"), chip, accent: Role::Cpu, readings: cpu });
    }
    let nvme: Vec<TempReading> = sys.temps.iter().filter(|t| t.chip == "nvme").cloned().collect();
    if !nvme.is_empty() {
        groups.push(SensorGroup {
            header: "Storage · nvme".to_string(),
            chip: "nvme".to_string(),
            accent: Role::Memory,
            readings: nvme,
        });
    }
    let board: Vec<TempReading> = sys
        .temps
        .iter()
        .filter(|t| !matches!(t.chip.as_str(), "amdgpu" | "coretemp" | "k10temp" | "nvme"))
        .cloned()
        .collect();
    if !board.is_empty() {
        let mut chips: Vec<String> = Vec::new();
        for t in &board {
            if !chips.iter().any(|c| c == &t.chip) {
                chips.push(t.chip.clone());
            }
        }
        let header = format!("Board · {}", chips.join(", "));
        groups.push(SensorGroup { header, chip: "board".to_string(), accent: Role::Good, readings: board });
    }
    groups
}

/// The rows of one group: `<label>, <celsius>`. In `all` mode every reading, label lowercased; in
/// summary mode the synthetic `package`, `hottest core`, `composite` and `hottest` rows.
fn sensor_rows(readings: &[TempReading], all: bool) -> Vec<(String, f32)> {
    if all {
        return readings.iter().map(|t| (t.label.to_ascii_lowercase(), t.celsius)).collect();
    }
    let mut rows = Vec::new();
    match readings[0].chip.as_str() {
        "amdgpu" => {
            rows.extend(readings.iter().map(|t| (t.label.to_ascii_lowercase(), t.celsius)));
        }
        "coretemp" | "k10temp" => {
            if let Some(pkg) = readings
                .iter()
                .find(|t| t.label.starts_with("Package"))
                .or_else(|| readings.iter().find(|t| t.label.starts_with("Tctl")))
                .or_else(|| readings.first())
            {
                rows.push(("package".to_string(), pkg.celsius));
            }
            if let Some(core) = readings
                .iter()
                .filter(|t| t.label.starts_with("Core"))
                .max_by(|a, b| a.celsius.total_cmp(&b.celsius))
            {
                rows.push(("hottest core".to_string(), core.celsius));
            }
        }
        "nvme" => {
            let mut chips: Vec<String> = Vec::new();
            for t in readings {
                if !chips.iter().any(|c| c == &t.chip) {
                    chips.push(t.chip.clone());
                    let c = readings
                        .iter()
                        .filter(|x| x.chip == t.chip)
                        .find(|x| x.label == "Composite")
                        .map(|x| x.celsius)
                        .or_else(|| readings.iter().find(|x| x.chip == t.chip).map(|x| x.celsius));
                    if let Some(c) = c {
                        rows.push(("composite".to_string(), c));
                    }
                }
            }
        }
        _ => {
            if let Some(hot) = readings.iter().max_by(|a, b| a.celsius.total_cmp(&b.celsius)) {
                rows.push(("hottest".to_string(), hot.celsius));
            }
        }
    }
    rows
}

/// The bold `Dim` header row of a group, and the row just below it.
fn draw_group_header(
    buf: &mut Buffer,
    theme: &Theme,
    x: u16,
    width: u16,
    group: &SensorGroup,
    y: u16,
) -> u16 {
    let line =
        Line::from(Span::styled(group.header.clone(), theme.fg(Role::Dim).add_modifier(Modifier::BOLD)));
    buf.set_line(x, y, &line, width);
    y + 1
}

/// One sensor row: `  <label padded to 15><Gauge, 16 cols>  <temp right-aligned to 6><hot mark>`.
#[allow(clippy::too_many_arguments)] // the panel contract; callers pass one row's worth
fn draw_sensor_row(
    buf: &mut Buffer,
    theme: &Theme,
    x: u16,
    y: u16,
    width: u16,
    accent: Role,
    chip: &str,
    label: &str,
    celsius: f32,
) {
    let label = format!("{:<15}", fit(label, 15));
    buf.set_line(x, y, &Line::from(Span::styled(label, theme.fg(Role::Dim))), 17);
    Gauge { ratio: celsius as f64 / 100.0, accent, thresholds: sensor_thresholds(chip), theme }
        .render(Rect::new(x + 17, y, 16, 1), buf);
    let sev = severity(celsius, sensor_thresholds(chip).0, sensor_thresholds(chip).1);
    let style = match sev {
        Severity::Normal => theme.fg(Role::Text),
        Severity::Warning => theme.fg(Role::Warning),
        Severity::Critical => theme.fg(Role::Critical),
    };
    let value = format!("  {:>6}{}", temp(Some(celsius)), hot_mark(sev));
    buf.set_line(x + 33, y, &Line::from(Span::styled(value, style)), width.saturating_sub(33));
}

/// A plain `<value>` row in `Role::Dim`, cut to `width`.
fn write_dim(buf: &mut Buffer, x: u16, y: u16, width: u16, value: &str, theme: &Theme) {
    buf.set_line(x, y, &Line::from(Span::styled(value.to_string(), theme.fg(Role::Dim))), width);
}

/// A right-aligned title on the top border of `area`, leaving `left_cols` columns for the left title.
fn place_right_title(area: Rect, text: &str, style: Style, buf: &mut Buffer, left_cols: u16) {
    let right = area.x + area.width.saturating_sub(1);
    let left = area.x + left_cols + 3;
    let x = right.saturating_sub(text.chars().count() as u16).max(left);
    let avail = right.saturating_sub(x);
    if avail == 0 {
        return;
    }
    buf.set_span(x, area.y, &Span::styled(text.to_string(), style), avail);
}
