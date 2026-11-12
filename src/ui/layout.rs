//! The responsive layout: which tier a terminal size falls into and where each Overview panel goes.
//!
//! Built by hand with integer rules (`docs/02-ui.md` § 3), never with `Layout` constraints, whose
//! solver may round differently. Every height uses `saturating_sub` so a 0 × 0 size never underflows.

use ratatui::layout::Rect;

/// The layout family chosen from the terminal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Wide,
    Medium,
    Narrow,
    Compact,
    TooSmall,
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
    /// Passed to the panels as `Ctx::graphs`.
    pub graphs: bool,
}

/// The Overview layout of `area` for the `Wide`, `Medium` and `Narrow` tiers (rules below). Any other
/// tier returns the `Narrow` layout; callers check `tier` first.
pub fn overview(area: Rect) -> OverviewLayout {
    // `header` is always the top 3 rows; `body` is the rest. `saturating_sub` keeps it empty, never
    // negative, on the smallest sizes.
    let header = Rect::new(area.x, area.y, area.width, 3.min(area.height));
    let body = Rect::new(area.x, area.y.saturating_add(3), area.width, area.height.saturating_sub(3));
    match tier(area.width, area.height) {
        Tier::Wide => wide(area, header, body),
        Tier::Medium => medium(header, body),
        _ => narrow(header, body),
    }
}

/// Wide: two columns, graphs on. Left `model` / `request` / `log`; right `gpu` then `cpu` | `memory`.
fn wide(area: Rect, header: Rect, body: Rect) -> OverviewLayout {
    let left_w = body.width / 2;
    let right_x = area.x + left_w;
    let right_w = body.width - left_w;

    let model = Rect::new(area.x, body.y, left_w, 8);
    let request = Rect::new(area.x, body.y + 8, left_w, 5);

    let gpu_h = body.height * 55 / 100;
    let rest_h = body.height.saturating_sub(gpu_h);
    let cpu_w = right_w / 2;
    let gpu = Rect::new(right_x, body.y, right_w, gpu_h);
    let cpu = Rect::new(right_x, body.y + gpu_h, cpu_w, rest_h);
    let memory = Rect::new(right_x + cpu_w, body.y + gpu_h, right_w - cpu_w, rest_h);

    // `log` sits under `model` + `request`; drop `request` first, then `log` itself, to keep >= 3 rows.
    let keep_request = body.height.saturating_sub(13) >= 3;
    let log = if keep_request {
        Some(Rect::new(area.x, body.y + 13, left_w, body.height.saturating_sub(13)))
    } else if body.height.saturating_sub(8) >= 3 {
        Some(Rect::new(area.x, body.y + 8, left_w, body.height.saturating_sub(8)))
    } else {
        None
    };

    OverviewLayout {
        header,
        model: Some(model.intersection(body)),
        request: keep_request.then_some(request.intersection(body)),
        log,
        gpu: Some(gpu.intersection(body)),
        cpu: Some(cpu),
        memory: Some(memory),
        graphs: true,
    }
}

/// Medium: two bands (`model` | `gpu` on 8 rows, then `cpu` | `memory` on 6), `log` full width below.
fn medium(header: Rect, body: Rect) -> OverviewLayout {
    let half = body.width / 2;
    let model = Rect::new(body.x, body.y, half, 8);
    let gpu = Rect::new(body.x + half, body.y, body.width - half, 8);
    let cpu = Rect::new(body.x, body.y + 8, half, 6);
    let memory = Rect::new(body.x + half, body.y + 8, body.width - half, 6);

    let cpu_memory = body.height.saturating_sub(14) >= 3;
    let log = if body.height.saturating_sub(14) >= 3 {
        Some(Rect::new(body.x, body.y + 14, body.width, body.height.saturating_sub(14)))
    } else if body.height.saturating_sub(8) >= 3 {
        Some(Rect::new(body.x, body.y + 8, body.width, body.height.saturating_sub(8)))
    } else {
        None
    };

    OverviewLayout {
        header,
        model: Some(model.intersection(body)),
        request: None,
        log,
        gpu: Some(gpu.intersection(body)),
        cpu: if cpu_memory { Some(cpu) } else { None },
        memory: if cpu_memory { Some(memory) } else { None },
        graphs: true,
    }
}

/// Narrow: one column, graphs off. `model`, `gpu`, then `cpu` | `memory`, `log` the rest below.
fn narrow(header: Rect, body: Rect) -> OverviewLayout {
    let model = Rect::new(body.x, body.y, body.width, 8);
    let gpu = Rect::new(body.x, body.y + 8, body.width, 6);
    let cpu_w = body.width / 2;
    let cpu = Rect::new(body.x, body.y + 14, cpu_w, 5);
    let memory = Rect::new(body.x + cpu_w, body.y + 14, body.width - cpu_w, 5);

    let cpu_memory = body.height.saturating_sub(19) >= 3;
    let log = if body.height.saturating_sub(19) >= 3 {
        Some(Rect::new(body.x, body.y + 19, body.width, body.height.saturating_sub(19)))
    } else if body.height.saturating_sub(14) >= 3 {
        Some(Rect::new(body.x, body.y + 14, body.width, body.height.saturating_sub(14)))
    } else {
        None
    };

    OverviewLayout {
        header,
        model: Some(model.intersection(body)),
        request: None,
        log,
        gpu: Some(gpu.intersection(body)),
        cpu: if cpu_memory { Some(cpu) } else { None },
        memory: if cpu_memory { Some(memory) } else { None },
        graphs: false,
    }
}
