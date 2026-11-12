//! The responsive layout: which tier a terminal size falls into and where each Overview panel goes.
//!
//! Built by hand with integer rules (`docs/02-ui.md` § 3), never with `Layout` constraints, whose
//! solver may round differently. The header is row 0 (height 1), the key bar the last row, and the panels
//! fill the space between; every height uses `saturating_sub` so a 0 × 0 size never underflows.

use ratatui::layout::Rect;

/// The tiers, from the smallest terminal to the widest, as defined by `tier` (`docs/02-ui.md` § 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    TooSmall,
    Compact,
    Narrow,
    Medium,
    Wide,
}

/// First match: `w < 30 || h < 8` → `TooSmall`; `w < 60 || h < 20` → `Compact`; `w < 100` → `Narrow`;
/// `w >= 140 && h >= 40` → `Wide`; else `Medium`.
pub fn tier(width: u16, height: u16) -> Tier {
    if width < 30 || height < 8 {
        Tier::TooSmall
    } else if width < 60 || height < 20 {
        Tier::Compact
    } else if width < 100 {
        Tier::Narrow
    } else if width >= 140 && height >= 40 {
        Tier::Wide
    } else {
        Tier::Medium
    }
}

/// Where each Overview panel goes; `None` = not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewLayout {
    pub header: Rect,
    pub model: Option<Rect>,
    pub request: Option<Rect>,
    pub log: Option<Rect>,
    pub gpu: Option<Rect>,
    pub cpu: Option<Rect>,
    pub memory: Option<Rect>,
    /// The key bar, the last row; `None` when the body has fewer than 9 rows.
    pub keybar: Option<Rect>,
    /// Passed to the panels as `Ctx::graphs`.
    pub graphs: bool,
}

/// The Overview layout of `area` for the `Wide`, `Medium` and `Narrow` tiers (rules below). Any other
/// tier returns the `Narrow` layout; callers check `tier` first.
pub fn overview(area: Rect) -> OverviewLayout {
    // `header` is always the top row; `keybar` the last, when the body below the header has >= 9 rows;
    // `body` is the space between, where the panels go. `saturating_sub` keeps it empty, never negative.
    let header = Rect::new(area.x, area.y, area.width, 1.min(area.height));
    let below = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));
    let keybar = if below.height >= 9 {
        Some(Rect::new(area.x, area.height.saturating_sub(1).max(area.y), area.width, 1))
    } else {
        None
    };
    let body = Rect::new(
        area.x,
        area.y.saturating_add(1),
        area.width,
        below.height.saturating_sub(keybar.map(|k| k.height).unwrap_or(0)),
    );
    let mut layout = match tier(area.width, area.height) {
        Tier::Wide => wide(body, keybar),
        Tier::Medium => medium(body, keybar),
        _ => narrow(body, keybar),
    };
    layout.header = header;
    layout
}

/// Wide: `model` (6 rows, full width), then `gpu` | `cpu` | `memory` (9 rows), then `log` (`w * 63 / 100`)
/// and `request` (the rest) side by side on the remaining rows.
fn wide(body: Rect, keybar: Option<Rect>) -> OverviewLayout {
    let w = body.width;
    let model = Rect::new(body.x, body.y, w, 6);
    let machine_y = body.y + 6;
    let (a, b, c) = column_widths(w);
    let gpu = Rect::new(body.x, machine_y, a, 9);
    let cpu = Rect::new(body.x + a, machine_y, b, 9);
    let memory = Rect::new(body.x + a + b, machine_y, c, 9);

    // `log` and `request` share the remaining rows; both go when there are >= 3, else both drop.
    let region_h = body.height.saturating_sub(6).saturating_sub(9);
    let (log, request) = if region_h >= 3 {
        let log_w = w * 63 / 100;
        (
            Some(Rect::new(body.x, machine_y + 9, log_w, region_h)),
            Some(Rect::new(body.x + log_w, machine_y + 9, w - log_w, region_h)),
        )
    } else {
        (None, None)
    };

    OverviewLayout {
        header: Rect::default(),
        model: Some(model.intersection(body)),
        request: request.map(|r| r.intersection(body)),
        log: log.map(|r| r.intersection(body)),
        gpu: Some(gpu.intersection(body)),
        cpu: Some(cpu.intersection(body)),
        memory: Some(memory.intersection(body)),
        keybar,
        graphs: true,
    }
}

/// Medium: `model` (6 rows) then `gpu` | `cpu` | `memory` (8 rows), `log` full width on the rest.
fn medium(body: Rect, keybar: Option<Rect>) -> OverviewLayout {
    let w = body.width;
    let model = Rect::new(body.x, body.y, w, 6);
    let machine_y = body.y + 6;
    let (a, b, c) = column_widths(w);
    let gpu = Rect::new(body.x, machine_y, a, 8);
    let cpu = Rect::new(body.x + a, machine_y, b, 8);
    let memory = Rect::new(body.x + a + b, machine_y, c, 8);

    let region_h = body.height.saturating_sub(6).saturating_sub(8);
    let log = if region_h >= 3 {
        Some(Rect::new(body.x, machine_y + 8, w, region_h).intersection(body))
    } else {
        None
    };

    OverviewLayout {
        header: Rect::default(),
        model: Some(model.intersection(body)),
        request: None,
        log,
        gpu: Some(gpu.intersection(body)),
        cpu: Some(cpu.intersection(body)),
        memory: Some(memory.intersection(body)),
        keybar,
        graphs: true,
    }
}

/// Narrow: `model` (6 rows), `gpu` (6 rows, full width), then `cpu` | `memory` (5 rows) and `log` the rest.
/// When `log` would get fewer than 3 rows, drop the `cpu` | `memory` row, then `log`.
fn narrow(body: Rect, keybar: Option<Rect>) -> OverviewLayout {
    let w = body.width;
    let model = Rect::new(body.x, body.y, w, 6);
    let gpu_y = body.y + 6;
    let gpu = Rect::new(body.x, gpu_y, w, 6);
    let cpu_w = w / 2;

    let with_cpu = body.height.saturating_sub(6).saturating_sub(6).saturating_sub(5);
    let (cpu, memory, log) = if with_cpu >= 3 {
        let cpu_y = gpu_y + 6;
        (
            Some(Rect::new(body.x, cpu_y, cpu_w, 5)),
            Some(Rect::new(body.x + cpu_w, cpu_y, w - cpu_w, 5)),
            Some(Rect::new(body.x, cpu_y + 5, w, with_cpu)),
        )
    } else {
        let no_cpu = body.height.saturating_sub(6).saturating_sub(6);
        (None, None, if no_cpu >= 3 { Some(Rect::new(body.x, gpu_y + 6, w, no_cpu)) } else { None })
    };

    OverviewLayout {
        header: Rect::default(),
        model: Some(model.intersection(body)),
        request: None,
        log: log.map(|r| r.intersection(body)),
        gpu: Some(gpu.intersection(body)),
        cpu: cpu.map(|r| r.intersection(body)),
        memory: memory.map(|r| r.intersection(body)),
        keybar,
        graphs: false,
    }
}

/// The three machine-column widths: `a = w / 3`, `b = (w - a) / 2`, `c = w - a - b`.
fn column_widths(w: u16) -> (u16, u16, u16) {
    let a = w / 3;
    let b = (w - a) / 2;
    (a, b, w - a - b)
}
