//! The Agents activity map: braille spokes, pulses, context ring and station nodes (T43).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use crate::collect::activity::{Activity, PULSE_MS, Station};
use crate::ui::dots::Dots;
use crate::ui::text::tps;
use crate::ui::theme::{ColorMode, PCT_THRESHOLDS, Role, Theme, gauge_color, lerp_rgb, rgb};

/// The centre of the map: what llama.cpp says about the model serving the session.
pub struct Core {
    /// Short model name, e.g. `Coder-35B`.
    pub model: String,
    /// Generation speed, tokens per second.
    pub tps: Option<f64>,
    /// Context used, 0.0–1.0.
    pub ctx_ratio: Option<f64>,
}

/// Smallest width the map draws in.
pub const MIN_W: u16 = 60;
/// Smallest height the map draws in.
pub const MIN_H: u16 = 18;

/// The role colour of a station.
fn station_role(s: Station) -> Role {
    match s {
        Station::You => Role::Text,
        Station::Model => Role::Server,
        Station::Files => Role::Cpu,
        Station::Shell => Role::Memory,
        Station::Checks => Role::Good,
        Station::Web => Role::Gpu,
        Station::Agents => Role::Warning,
        Station::Tools => Role::Text,
    }
}

/// The drawing geometry in dots, derived from `area` (see the sheet's "Geometry" section).
struct Geo {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    ring: f64,
}

impl Geo {
    fn new(area: Rect) -> Self {
        let dw = area.width as f64 * 2.0;
        let dh = area.height as f64 * 4.0;
        let cx = (dw / 4.0).floor() * 2.0;
        let cy = (dh / 8.0).floor() * 4.0 + 1.0;
        let rx = (dh / 2.0 * 1.25).min(dw / 2.0 - 30.0);
        let ry = dh / 2.0 - 10.0;
        let ring = 19.0_f64.min(ry * 0.53_f64);
        Geo { cx, cy, rx, ry, ring }
    }

    fn station_point(&self, s: Station) -> (f64, f64) {
        let a = 2.0 * std::f64::consts::PI * (s.index() as f64) / 8.0;
        let x = self.cx + self.rx * a.sin();
        let y = self.cy - self.ry * a.cos();
        // Snap to the braille grid: even dot columns, `4k + 1` dot rows.
        ((x / 2.0).round() * 2.0, (y / 4.0).floor() * 4.0 + 1.0)
    }

    fn station_cell(&self, s: Station) -> (u16, u16) {
        let (x, y) = self.station_point(s);
        ((x / 2.0) as u16, (y / 4.0) as u16)
    }
}

/// One station's precomputed data, shared by the spokes, pulses, nodes and labels.
struct StationData {
    point: (f64, f64),
    angle: f64,
    cell: (u16, u16),
    role: Role,
    color: Option<Color>,
    calls: u32,
    running: u32,
    running_since: Option<i64>,
    errors: u32,
    last_ms: Option<i64>,
}

/// The cell of each station inside `area` (relative to its top-left), in `Station::ALL` order.
pub fn station_cells(area: Rect) -> [(u16, u16); 8] {
    let geo = Geo::new(area);
    Station::ALL.map(|s| geo.station_cell(s))
}

/// Draws the map of `activity` at wall-clock `now_ms` in `area`. Smaller than `MIN_W × MIN_H`: draws
/// nothing. Never panics.
pub fn render(activity: &Activity, core: &Core, now_ms: i64, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < MIN_W || area.height < MIN_H {
        return;
    }
    let geo = Geo::new(area);

    // The faint grid colour: a true-colour blend of the background and the border role.
    let faint_rgb = lerp_rgb((0x1a, 0x1b, 0x26), rgb(Role::Border), 0.7);
    let faint = match theme.mode {
        ColorMode::TrueColor => Some(Color::Rgb(faint_rgb.0, faint_rgb.1, faint_rgb.2)),
        ColorMode::Ansi16 => theme.color(Role::Dim),
        ColorMode::NoColor => None,
    };

    // Per-station data, in `ALL` order.
    let stations: [StationData; 8] = std::array::from_fn(|i| {
        let s = Station::ALL[i];
        StationData {
            point: geo.station_point(s),
            angle: 2.0 * std::f64::consts::PI * (i as f64) / 8.0,
            cell: geo.station_cell(s),
            role: station_role(s),
            color: theme.color(station_role(s)),
            calls: activity.stations[i].calls,
            running: activity.stations[i].running,
            running_since: activity.stations[i].running_since,
            errors: activity.stations[i].errors,
            last_ms: activity.stations[i].last_ms,
        }
    });

    // A station's recency: 1 just seen, 0 for more than two minutes.
    let recency = |s: &StationData| match s.last_ms {
        None => 0.0,
        Some(last) => (1.0 - (now_ms - last) as f64 / 120_000.0).clamp(0.0, 1.0),
    };

    let mut dots = Dots::new(area.width, area.height);

    // 1. Spokes: dotted and faint when idle, coloured and solid otherwise.
    for sd in stations.iter() {
        let (x, y) = sd.point;
        let (ux, uy) = unit(geo.cx, geo.cy, x, y);
        let start = (geo.cx + ux * (geo.ring + 4.0), geo.cy + uy * (geo.ring + 4.0));
        let end = (x - ux * 3.0, y - uy * 4.0);
        if sd.calls == 0 {
            dots.line(start, end, faint, 1, 3);
        } else {
            let r = recency(sd);
            let color = match theme.mode {
                ColorMode::TrueColor => {
                    let (fr, fg, fb) = lerp_rgb(faint_rgb, rgb(sd.role), 0.35 + 0.65 * r as f32);
                    Some(Color::Rgb(fr, fg, fb))
                }
                ColorMode::Ansi16 => {
                    if r > 0.75 {
                        sd.color
                    } else {
                        theme.color(Role::Dim)
                    }
                }
                ColorMode::NoColor => None,
            };
            dots.line(start, end, color, 6, 1);
        }
    }

    // 2. Pulses: a square head with a tail on the spoke, travelling from where the step came to where
    // it goes (outbound) or back (inbound).
    for p in &activity.pulses {
        let dt = now_ms - p.at_ms;
        if !(0..PULSE_MS).contains(&dt) {
            continue;
        }
        let sd = &stations[p.station.index()];
        let (x, y) = sd.point;
        let (ux, uy) = unit(geo.cx, geo.cy, x, y);
        let start = (geo.cx + ux * (geo.ring + 4.0), geo.cy + uy * (geo.ring + 4.0));
        let end = (x - ux * 3.0, y - uy * 4.0);
        let t = dt as f64 / PULSE_MS as f64;
        let param = if p.outbound { t } else { 1.0 - t };
        let hx = (start.0 + (end.0 - start.0) * param).round();
        let hy = (start.1 + (end.1 - start.1) * param).round();
        // Behind the head is one spoke dot towards where the pulse came from.
        let travel = if p.outbound { 1.0 } else { -1.0 };
        let head = if !p.outbound && p.failed {
            theme.color(Role::Critical)
        } else {
            match theme.mode {
                ColorMode::TrueColor => Some(Color::Rgb(255, 255, 255)),
                ColorMode::Ansi16 => Some(Color::White),
                ColorMode::NoColor => None,
            }
        };
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            dots.plot(hx as i32 + dx, hy as i32 + dy, head, 20);
        }
        let station = rgb(sd.role);
        for k in 1..=5 {
            let decay = 0.5 - 0.08 * k as f32;
            let tail = match theme.mode {
                ColorMode::TrueColor => {
                    let (fr, fg, fb) = lerp_rgb(station, (255, 255, 255), decay);
                    Some(Color::Rgb(fr, fg, fb))
                }
                ColorMode::Ansi16 => sd.color,
                ColorMode::NoColor => None,
            };
            let tx = hx - travel * ux * k as f64;
            let ty = hy - travel * uy * k as f64;
            dots.plot(tx.round() as i32, ty.round() as i32, tail, 19);
        }
    }

    // 3. Context ring: one faint circle, then the lit part from the top over `ctx_ratio` turns.
    dots.arc((geo.cx, geo.cy), geo.ring, 0.0, 1.0, faint, 3);
    if let Some(ratio) = core.ctx_ratio {
        let lit = gauge_color(theme, Role::Good, ratio as f32 * 100.0, PCT_THRESHOLDS);
        // Clamped: a bogus ratio must not wind the ring many times (or loop for ages).
        dots.arc((geo.cx, geo.cy), geo.ring, 0.0, ratio.clamp(0.0, 1.0), lit, 9);
    }

    // 4. The braille canvas.
    dots.render(area, buf);

    // 5. Nodes: a glyph on each station's cell.
    for sd in stations.iter() {
        let (col, row) = sd.cell;
        let (glyph, color) = if sd.calls == 0 {
            ('○', theme.color(Role::Dim))
        } else if sd.running > 0 {
            ('◉', sd.color)
        } else {
            ('●', sd.color)
        };
        put_glyph(buf, area, col as i32, row as i32, glyph, color, true);
    }

    // 6. Core text, centred on the model column.
    let core_col = (geo.cx / 2.0).round() as i32;
    let model_row = (geo.cy / 4.0).floor() as i32 - 1;
    put_text(buf, area, core_col, model_row, &core.model, theme.color(Role::Server), true);
    put_text(
        buf,
        area,
        core_col,
        model_row + 1,
        &format!("{} t/s", tps(core.tps)),
        theme.color(Role::Text),
        false,
    );
    let (ctx_line, ctx_color) = match core.ctx_ratio {
        Some(r) => {
            let n = (r * 100.0).round() as i32;
            (format!("ctx {}%", n), gauge_color(theme, Role::Good, r as f32 * 100.0, PCT_THRESHOLDS))
        }
        None => ("ctx n/a".to_string(), theme.color(Role::Dim)),
    };
    put_text(buf, area, core_col, model_row + 2, &ctx_line, ctx_color, false);

    // 7. Station labels.
    for (i, sd) in stations.iter().enumerate() {
        label(buf, area, sd, Station::ALL[i].name(), now_ms, theme);
    }
}

/// The unit vector from `(cx, cy)` to `(x, y)`, or `(0, 0)` when they coincide.
fn unit(cx: f64, cy: f64, x: f64, y: f64) -> (f64, f64) {
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist == 0.0 { (0.0, 0.0) } else { (dx / dist, dy / dist) }
}

/// Writes a string at `(col, row)` relative to `area`, clipping off-screen cells.
fn put_text(buf: &mut Buffer, area: Rect, col: i32, row: i32, text: &str, color: Option<Color>, bold: bool) {
    let start = col - text.chars().count() as i32 / 2;
    for (i, ch) in text.chars().enumerate() {
        put_glyph(buf, area, start + i as i32, row, ch, color, bold);
    }
}

/// Writes one glyph at `(col, row)` relative to `area`, clipping off-screen cells.
fn put_glyph(buf: &mut Buffer, area: Rect, col: i32, row: i32, ch: char, color: Option<Color>, bold: bool) {
    if col < 0 || row < 0 || col as u16 >= area.width || row as u16 >= area.height {
        return;
    }
    let x = area.x.saturating_add(col as u16);
    let y = area.y.saturating_add(row as u16);
    // A cell outside the buffer would panic on indexing: skip it instead.
    if !buf.area.contains((x, y).into()) {
        return;
    }
    let cell = &mut buf[(x, y)];
    cell.set_char(ch);
    if let Some(color) = color {
        cell.fg = color;
    }
    if bold {
        cell.modifier |= Modifier::BOLD;
    }
}

/// One station's label: `<Name> <calls>`, the running clock and the error count, placed around the
/// node according to the station's angle.
fn label(buf: &mut Buffer, area: Rect, sd: &StationData, name: &str, now_ms: i64, theme: &Theme) {
    let mut parts: Vec<(String, Option<Color>, bool)> = Vec::new();
    let name_color = if sd.calls == 0 { theme.color(Role::Dim) } else { sd.color };
    parts.push((name.to_string(), name_color, sd.calls != 0));
    parts.push((format!(" {}", sd.calls), theme.color(Role::Dim), false));
    if sd.running > 0 {
        let since = sd.running_since.unwrap_or(now_ms);
        // Clock skew can put `since` after `now_ms`: show 0:00 rather than a negative clock.
        let secs = (now_ms - since).max(0) / 1000;
        parts.push((format!(" ◷ {}:{:02}", secs / 60, secs % 60), sd.color, false));
    }
    if sd.errors > 0 {
        parts.push((format!(" ✕{}", sd.errors), theme.color(Role::Critical), true));
    }
    let chars: Vec<(char, Option<Color>, bool)> =
        parts.iter().flat_map(|(text, color, bold)| text.chars().map(|c| (c, *color, *bold))).collect();
    let len = chars.len();

    let (label_row, start) = {
        let sin_a = sd.angle.sin();
        let node_col = sd.cell.0 as i32;
        let node_row = sd.cell.1 as i32;
        if sin_a.abs() < 0.2 {
            // Top station above the node, bottom station below it, centred on the node's column.
            let label_row = if sd.angle.cos() > 0.0 { node_row - 1 } else { node_row + 1 };
            (label_row, node_col - len as i32 / 2)
        } else if sin_a > 0.0 {
            (node_row, node_col + 2)
        } else {
            // Mirrors the right side: the last char sits 2 columns left of the node.
            (node_row, node_col - 1 - len as i32)
        }
    };

    for (i, (ch, color, bold)) in chars.iter().enumerate() {
        put_glyph(buf, area, start + i as i32, label_row, *ch, *color, *bold);
    }
}
