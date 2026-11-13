//! The Models screen: a table of every model the router knows, and the details of the selected one.
//! Read-only. See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::llama::{ModelInfo, ModelState};
use crate::collect::llama_poll::Health;
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::screens::models_details::render_details;
use crate::ui::text::{bytes_pair, fit};
use crate::ui::theme::{ColorMode, Role, Theme, rgb};
use crate::util::{count, short_model_name};

/// The Models screen in `area` (the body below the header): the table on top, the details below.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;

    let Some(llama) = app.llama.as_ref() else {
        render_no_model(theme, area, buf, "waiting for the first poll");
        return;
    };
    let reason = match llama.health {
        Health::Down(_) => Some("server down"),
        Health::Unauthorized => Some("no API key"),
        Health::Ok => None,
    };
    if let Some(reason) = reason {
        render_no_model(theme, area, buf, reason);
        return;
    }
    if llama.models.is_empty() {
        render_no_model(theme, area, buf, "the server lists no model");
        return;
    }

    // The table gets the models plus the header row and borders, capped at half the area, at least 4.
    let rows = u16::try_from(llama.models.len()).unwrap_or(u16::MAX).saturating_add(3);
    let table_h = rows.min(area.height / 2).max(4).min(area.height);
    let details_h = area.height - table_h;
    render_table(
        app,
        &llama.models,
        app.model_selected,
        theme,
        Rect::new(area.x, area.y, area.width, table_h),
        buf,
    );
    render_details(app, llama, ctx, Rect::new(area.x, area.y + table_h, area.width, details_h), buf);
}

/// The "no model" case: the table panel over the whole area, a single `Dim` row with the reason.
fn render_no_model(theme: &Theme, area: Rect, buf: &mut Buffer, reason: &str) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Models", Role::Server, true, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    let msg = format!("no model: {reason}");
    let w = msg.chars().count().min(inner.width as usize) as u16;
    buf.set_line(inner.x, inner.y, &Line::from(Span::styled(msg, theme.fg(Role::Dim))), w);
}

/// The table's side columns, in display order (identity, header label, width). The name column sits
/// between `state` and the first of these.
const SIDE: [(&str, &str, usize); 6] = [
    ("quant", "quant", 8),
    ("ctx", "ctx", 8),
    ("train", "train", 8),
    ("moe", "moe", 5),
    ("spec", "spec", 13),
    ("vision", "img", 4),
];

/// The order in which side columns are dropped when the panel is narrow (contract order).
const DROP_ORDER: [&str; 6] = ["train", "vision", "spec", "moe", "quant", "ctx"];

/// Widths of the marker and state columns.
const MARKER_W: usize = 2;
const STATE_W: usize = 11;

/// The state cell of the table: the one-column marker, the word, and its role.
fn state_cell(m: &ModelInfo) -> (String, Role) {
    match &m.state {
        ModelState::Loaded => ("● loaded".to_string(), Role::Good),
        ModelState::Loading => ("◐ loading".to_string(), Role::Warning),
        ModelState::Unloaded => ("○ unloaded".to_string(), Role::Dim),
        ModelState::Other(s) => (format!("○ {s}"), Role::Dim),
    }
}

/// Parameters in billions with one decimal above a billion (`35_505_251_456` → `35.5B`), in millions
/// under one billion (`8_190_000` → `8.2M`).
pub fn params(n: u64) -> String {
    if n >= 1_000_000_000 { format!("{:.1}B", n as f64 / 1e9) } else { format!("{:.1}M", n as f64 / 1e6) }
}

/// The name column width: the remainder after the marker, `state`, the kept side columns, and the
/// single-space separators between every shown column.
fn name_width(total: u16, side: &[(&str, &str, usize)]) -> usize {
    let fixed: usize = MARKER_W + STATE_W + side.iter().map(|(_, _, w)| *w).sum::<usize>();
    let sep = side.len() + 2; // marker, state, name and the sides: one space between each pair
    (total as usize).saturating_sub(fixed).saturating_sub(sep)
}

/// The side columns kept at `total` columns: dropped in `DROP_ORDER` until the name has at least 12.
fn kept_side(total: u16) -> Vec<(&'static str, &'static str, usize)> {
    let mut side: Vec<_> = SIDE.to_vec();
    for drop in DROP_ORDER {
        if name_width(total, &side) >= 12 {
            break;
        }
        side.retain(|(id, _, _)| *id != drop);
    }
    side
}

/// The widths of every column, in order: marker, `state`, name, then the kept side columns.
fn col_widths(side: &[(&str, &str, usize)], name_w: usize) -> Vec<usize> {
    let mut w = vec![MARKER_W, STATE_W, name_w];
    w.extend(side.iter().map(|(_, _, w)| *w));
    w
}

/// Lay out one row: each cell cut and padded to its column width, one space between cells, the whole
/// row cut to the panel body so nothing is written past its right border.
fn place_row(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    cells: &[(String, Style)],
    widths: &[usize],
    theme: &Theme,
) {
    let mut spans: Vec<Span> = Vec::with_capacity(cells.len() * 2);
    for (i, ((text, style), w)) in cells.iter().zip(widths).enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ".to_string(), theme.fg(Role::Text)));
        }
        let w = *w;
        spans.push(Span::styled(format!("{:<w$}", fit(text, w)), *style));
    }
    buf.set_line(inner.x, y, &Line::from(spans), inner.width);
}

/// The table: a header row, then one row per model, the selected one bold, scrolled when it overflows.
fn render_table(
    app: &App,
    models: &[ModelInfo],
    selected: usize,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < 3 || area.height < 3 {
        return;
    }

    // The right title: how many models are known and loaded, plus VRAM when the first GPU reports it.
    let known = models.len();
    let loaded = models.iter().filter(|m| m.state == ModelState::Loaded).count();
    let mut right = format!("{known} known · {loaded} loaded");
    if let (Some(used), Some(total)) = (vram_used(app), vram_total(app)) {
        right.push_str(&format!(" · VRAM {}", bytes_pair(used, total)));
    }

    let block = panel("Models", Role::Server, true, theme)
        .title(Line::from(Span::styled(right, theme.fg(Role::Dim))).right_aligned());
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 2 || models.is_empty() {
        return;
    }

    let side = kept_side(inner.width);
    let widths = col_widths(&side, name_width(inner.width, &side));

    // Header row: the empty marker column, then `state`, `model`, then the kept side labels (`Dim`, bold).
    let style = theme.fg(Role::Dim).add_modifier(Modifier::BOLD);
    let mut cells: Vec<(String, Style)> = vec![(String::new(), theme.fg(Role::Text))];
    cells.push(("state".to_string(), style));
    cells.push(("model".to_string(), style));
    for (_, label, _) in &side {
        cells.push((label.to_string(), style));
    }
    place_row(buf, inner, inner.y, &cells, &widths, theme);

    let selected = selected.min(models.len() - 1);
    let visible = inner.height.saturating_sub(1) as usize; // data rows that fit
    let top = if models.len() > visible {
        (selected.saturating_sub(visible - 1)).min(models.len().saturating_sub(visible))
    } else {
        0
    };

    for (i, m) in models.iter().enumerate().skip(top) {
        let y = inner.y + 1 + (i - top) as u16;
        if y >= inner.y + inner.height {
            break;
        }
        render_row(theme, m, i == selected, &side, &widths, inner, y, buf);
    }
}

/// One model row: marker, state, name, then the side columns in order. The selected row is bold.
#[allow(clippy::too_many_arguments)]
fn render_row(
    theme: &Theme,
    m: &ModelInfo,
    selected: bool,
    side: &[(&str, &str, usize)],
    widths: &[usize],
    inner: Rect,
    y: u16,
    buf: &mut Buffer,
) {
    let (state_text, state_role) = state_cell(m);
    let (name, quant) = short_model_name(&m.id);

    let bold = |s: Style| if selected { s.add_modifier(Modifier::BOLD) } else { s };
    let mark_style = if selected { bold(theme.fg(Role::Server)) } else { theme.fg(Role::Text) };
    let text_style = bold(theme.fg(Role::Text));

    let marker = if selected { "▶ " } else { "  " };
    let mut cells: Vec<(String, Style)> =
        vec![(marker.to_string(), mark_style), (state_text, bold(theme.fg(state_role))), (name, text_style)];
    for (id, _, _) in side {
        let value = match *id {
            "quant" => quant.clone().unwrap_or_default(),
            "ctx" => m.ctx_size.map(count).unwrap_or_else(|| "n/a".to_string()),
            "train" => m.ctx_train.map(count).unwrap_or_else(|| "-".to_string()),
            "moe" => m.n_cpu_moe.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
            "spec" => m.spec_type.clone().unwrap_or_else(|| "-".to_string()),
            "vision" if m.vision => "img".to_string(),
            _ => String::new(),
        };
        cells.push((value, text_style));
    }
    place_row(buf, inner, y, &cells, widths, theme);
    if selected {
        let (r, g, b) = rgb(Role::Selection);
        let rect = Rect::new(inner.x, y, inner.width, 1);
        let style = if theme.mode == ColorMode::TrueColor {
            Style::default().bg(Color::Rgb(r, g, b))
        } else {
            Style::default().add_modifier(Modifier::REVERSED)
        };
        buf.set_style(rect, style);
    }
}

/// VRAM used of the first GPU, for the table's right title (`None` on a machine without one).
fn vram_used(app: &App) -> Option<u64> {
    app.system.as_ref().and_then(|s| s.gpus.first()).and_then(|g| g.vram_used)
}

/// VRAM total of the first GPU, for the table's right title (`None` on a machine without one).
fn vram_total(app: &App) -> Option<u64> {
    app.system.as_ref().and_then(|s| s.gpus.first()).and_then(|g| g.vram_total)
}
