//! The Models screen: a table of every model the router knows, and the details of the selected one.
//! Read-only. See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::llama::{ModelInfo, ModelState};
use crate::collect::llama_poll::{Health, LlamaSnapshot};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::text::{fit, pct, tps};
use crate::ui::theme::{Role, Theme};
use crate::util::{bytes, count, short_model_name};

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
        &llama.models,
        app.model_selected,
        theme,
        Rect::new(area.x, area.y, area.width, table_h),
        buf,
    );
    render_details(app, llama, theme, Rect::new(area.x, area.y + table_h, area.width, details_h), buf);
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
const SIDE: [(&str, &str, usize); 5] =
    [("quant", "quant", 8), ("ctx", "ctx", 8), ("moe", "moe", 5), ("spec", "spec", 13), ("vision", "img", 4)];

/// The order in which side columns are dropped when the panel is narrow (contract order).
const DROP_ORDER: [&str; 5] = ["vision", "spec", "moe", "quant", "ctx"];

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
fn render_table(models: &[ModelInfo], selected: usize, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Models", Role::Server, true, theme);
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
            "moe" => m.n_cpu_moe.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
            "spec" => m.spec_type.clone().unwrap_or_else(|| "-".to_string()),
            "vision" if m.vision => "img".to_string(),
            _ => String::new(),
        };
        cells.push((value, text_style));
    }
    place_row(buf, inner, y, &cells, widths, theme);
}

/// The details of the selected model: one `label value` row each, then the live metrics of the active
/// model. Rows that do not fit are dropped from the bottom.
fn render_details(app: &App, llama: &LlamaSnapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Details", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }

    let Some(model) = llama.models.get(app.model_selected) else {
        return;
    };

    let active = llama.active.iter().find(|a| a.id == model.id);
    let metrics = active.and_then(|a| a.metrics.as_ref());
    let label_w = 12usize.min(inner.width as usize);

    let path = model.model_path.clone().unwrap_or_else(|| "n/a".to_string());
    let context = match (model.ctx_size, model.ctx_train) {
        (Some(c), Some(t)) => format!("{} of {} trained", count(c), count(t)),
        (Some(c), None) => count(c),
        _ => "n/a".to_string(),
    };
    let ngl = model.n_gpu_layers.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
    let npu = model.n_cpu_moe.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
    let thr = model.threads.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
    let speculative = model.spec_type.clone().unwrap_or_else(|| "none".to_string());
    let size = match model.size_bytes {
        Some(sz) => {
            let p = model.n_params.map(params).unwrap_or_else(|| "n/a".to_string());
            let ft = model.ftype.as_deref().unwrap_or("n/a");
            format!("{}   {} params   {ft}", bytes(sz), p)
        }
        None => "loaded models only".to_string(),
    };

    let mut rows: Vec<(&str, String, Option<Role>)> = vec![
        ("id", model.id.clone(), None),
        ("state", state_text(model), None),
        ("path", fit(&path, inner.width.saturating_sub(label_w as u16) as usize), None),
        ("context", context, None),
        ("offload", format!("ngl {ngl}   n-cpu-moe {npu}   threads {thr}"), None),
        ("speculative", speculative, None),
        ("size", size, if model.size_bytes.is_some() { None } else { Some(Role::Dim) }),
    ];

    if let Some(m) = metrics {
        rows.push(("", String::new(), None)); // blank separator before the live metrics
        rows.push((
            "speed",
            format!("gen {} t/s   prompt {} t/s", tps(m.avg_gen_tps()), tps(m.avg_prompt_tps())),
            None,
        ));
        rows.push((
            "cache",
            format!("{} reused   draft {} accepted", pct(m.cache_reuse_pct()), pct(m.draft_acceptance_pct())),
            None,
        ));
        rows.push((
            "requests",
            format!(
                "{} processing   {} deferred",
                fmt_int(m.requests_processing),
                fmt_int(m.requests_deferred)
            ),
            None,
        ));
        rows.push((
            "slot",
            active
                .and_then(|a| a.slots.first())
                .map(|s| {
                    format!(
                        "{} / {} tokens",
                        count(s.n_prompt_tokens.saturating_add(s.n_decoded)),
                        count(s.n_ctx)
                    )
                })
                .unwrap_or_else(|| "n/a".to_string()),
            None,
        ));
    }

    for (y, (label, value, style)) in (inner.y..).zip(rows) {
        if y >= inner.y + inner.height {
            break;
        }
        let label = format!("{:<label_w$}", label);
        buf.set_line(
            inner.x,
            y,
            &Line::from(vec![
                Span::styled(label, theme.fg(Role::Dim)),
                Span::styled(value, theme.fg(style.unwrap_or(Role::Text))),
            ]),
            inner.width,
        );
    }
}

/// The state text of a model for the details (`● loaded`, `○ unloaded`, …), without its role.
fn state_text(m: &ModelInfo) -> String {
    state_cell(m).0
}

/// A float metric as an integer, rounded (`1.0` → `1`).
fn fmt_int(v: f64) -> String {
    format!("{:.0}", v)
}
