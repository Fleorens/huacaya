//! The model band: name and quantization, state badge, generation speed, context fill and key
//! settings, in three responsive layouts. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::llama::{Metrics, ModelInfo, ModelState, SlotInfo};
use crate::collect::llama_poll::{ActiveModel, Health};
use crate::ui::big::big_digits;
use crate::ui::frame::{badge, panel};
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{fit, fit_groups, pct, tps};
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::{count, duration, short_model_name};

/// Height needed, borders included (4 rows inside).
pub const MIN_HEIGHT: u16 = 6;

/// The state badge's text and role (first match wins): no llama snapshot → `WAITING`; `Health::Down`
/// → `SERVER DOWN`; `Health::Unauthorized` → `UNAUTHORIZED`; a model `Loading` → `LOADING`; no active
/// model → `NO MODEL`; slot 0 of the first active model processing with `n_decoded == 0` → `PROMPT`;
/// processing → `GENERATING`; else `IDLE`.
pub fn badge_state(app: &App) -> (&'static str, Role) {
    let Some(llama) = &app.llama else {
        return ("WAITING", Role::Dim);
    };
    match llama.health {
        Health::Down(_) => return ("SERVER DOWN", Role::Critical),
        Health::Unauthorized => return ("UNAUTHORIZED", Role::Warning),
        Health::Ok => {}
    }
    if llama.models.iter().any(|m| m.state == ModelState::Loading) {
        return ("LOADING", Role::Warning);
    }
    if llama.active.is_empty() {
        return ("NO MODEL", Role::Dim);
    }
    let m = &llama.active[0];
    if let Some(s) = m.slots.first()
        && s.is_processing
        && s.n_decoded == 0
    {
        return ("PROMPT", Role::Server);
    }
    if m.slots.iter().any(|s| s.is_processing) {
        return ("GENERATING", Role::Good);
    }
    ("IDLE", Role::Dim)
}

/// The badge's pulse: lit every other `frame / 5` for the animated states, always lit otherwise.
fn badge_lit(text: &str, frame: u64) -> bool {
    match text {
        "PROMPT" | "GENERATING" | "LOADING" => (frame / 5).is_multiple_of(2),
        _ => true,
    }
}

/// Render the model band: one of three responsive layouts, or the without-active message.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = panel("Model", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    match app.llama.as_ref().and_then(|l| l.active.first()) {
        Some(m) => {
            let models = app.llama.as_ref().map(|l| l.models.as_slice()).unwrap_or(&[]);
            let info = models.iter().find(|mi| mi.id == m.id);
            match inner.width {
                w if w >= 110 => render_wide(app, m, info, ctx.graphs, theme, inner, buf),
                w if w >= 60 => render_medium(app, m, info, ctx.graphs, theme, inner, buf),
                _ => render_small(app, m, theme, inner, buf),
            }
        }
        None => render_without_active(app, theme, inner, buf),
    }
}

/// Slot 0 is decoding: the live speed is meaningful.
fn generating(m: &ActiveModel) -> bool {
    m.slots.first().is_some_and(|s| s.is_processing && s.n_decoded > 0)
}

/// The speed to show: live while generating, else the average.
fn speed(m: &ActiveModel) -> String {
    let value = if generating(m) { m.live_gen_tps } else { m.metrics.as_ref().and_then(|m| m.avg_gen_tps()) };
    tps(value)
}

/// The speed's label: `t/s now` while generating, else `t/s avg`.
fn label(m: &ActiveModel) -> &'static str {
    if generating(m) { "t/s now" } else { "t/s avg" }
}

/// The absolute row `k` of `inner`, when it fits inside the frame.
fn row(inner: Rect, k: u16) -> Option<u16> {
    (k < inner.height).then_some(inner.y + k)
}

/// The wide layout: three columns (name, speed, context).
fn render_wide(
    app: &App,
    m: &ActiveModel,
    info: Option<&ModelInfo>,
    graphs: bool,
    theme: &Theme,
    inner: Rect,
    buf: &mut Buffer,
) {
    let left_w = 38u16;
    let gap = 2u16;
    let right_w = 34u16;
    let centre_w = inner.width.saturating_sub(left_w + gap + gap + right_w);
    let centre_x = inner.x + left_w + gap;
    let right_x = inner.x + inner.width - right_w;
    let y0 = inner.y;

    // Left column: name, badge + task, settings.
    name_row(m, theme, inner.x, y0, left_w, buf);
    if let Some(y) = row(inner, 1) {
        badge_task_row(app, m, theme, inner.x, y, left_w, buf);
    }
    if let Some(y) = row(inner, 2) {
        settings_row(info, theme, inner.x, y, left_w, buf);
    }

    // Centre: the big speed, the label + graph + average to its right, the prompt line below.
    let speed = speed(m);
    let bd = big_digits(&speed);
    let bd_w = bd[0].chars().count() as u16;
    for (i, row_text) in bd.iter().enumerate() {
        if let Some(y) = row(inner, i as u16) {
            let style = theme.fg(Role::Server).add_modifier(Modifier::BOLD);
            buf.set_line(centre_x, y, &Line::from(Span::styled(row_text, style)), centre_w);
        }
    }
    let side_x = centre_x + bd_w + 2;
    let side_w = centre_w.saturating_sub(bd_w + 2);
    set_text(buf, side_x, y0, side_w, label(m), Role::Text, theme);
    if graphs
        && side_w >= 8
        && let Some(y) = row(inner, 1)
    {
        let values = app.gen_tps.tail(usize::MAX);
        let max = app.gen_tps.max().unwrap_or(0.0).max(1.0);
        BrailleGraph { values: &values, max, accent: Role::Server, theme }
            .render(Rect::new(side_x, y, side_w, 1), buf);
    }
    let avg = tps(m.metrics.as_ref().and_then(|m| m.avg_gen_tps()));
    if let Some(y) = row(inner, 2) {
        set_text(buf, side_x, y, side_w, &format!("avg {avg} · 5 min"), Role::Dim, theme);
    }
    if let Some(y) = row(inner, 3) {
        prompt_row(m.metrics.as_ref(), theme, centre_x, y, centre_w, buf);
    }

    // Right column: the ctx gauge, its value and the request count.
    let slot = m.slots.first();
    gauge_row(buf, right_x, y0, right_w, "ctx  ", ctx_ratio(slot), Role::Server, PCT_THRESHOLDS, "", theme);
    if let Some(y) = row(inner, 1) {
        ctx_value_row(slot, theme, right_x, y, right_w, buf);
    }
    if let Some(y) = row(inner, 2) {
        req_row(m.metrics.as_ref(), theme, right_x, y, right_w, buf);
    }
}

/// The medium layout: the name on the left, the rest below.
fn render_medium(
    app: &App,
    m: &ActiveModel,
    info: Option<&ModelInfo>,
    graphs: bool,
    theme: &Theme,
    inner: Rect,
    buf: &mut Buffer,
) {
    let left_w = 38u16;
    let gap = 2u16;
    let rx = inner.x + left_w + gap;
    let rest_w = inner.width.saturating_sub(left_w + gap);
    let y0 = inner.y;

    name_row(m, theme, inner.x, y0, left_w, buf);
    if let Some(y) = row(inner, 1) {
        badge_task_row(app, m, theme, inner.x, y, left_w, buf);
    }
    if let Some(y) = row(inner, 2) {
        settings_row(info, theme, inner.x, y, left_w, buf);
    }

    // Row 0: the speed and label, then a one-row graph.
    let speed = speed(m);
    set_text(buf, rx, y0, rest_w, &format!("{} {}", speed, label(m)), Role::Text, theme);
    if graphs {
        let text_w = speed.chars().count() as u16 + 1 + label(m).chars().count() as u16;
        let graph_w = rest_w.saturating_sub(text_w + 2);
        if graph_w >= 8 {
            let values = app.gen_tps.tail(usize::MAX);
            let max = app.gen_tps.max().unwrap_or(0.0).max(1.0);
            BrailleGraph { values: &values, max, accent: Role::Server, theme }
                .render(Rect::new(rx + text_w + 2, y0, graph_w, 1), buf);
        }
    }
    if let Some(y) = row(inner, 1) {
        prompt_row(m.metrics.as_ref(), theme, rx, y, rest_w, buf);
    }
    let slot = m.slots.first();
    if let Some(y) = row(inner, 2) {
        let value = ctx_value_text(slot);
        gauge_row(buf, rx, y, rest_w, "ctx  ", ctx_ratio(slot), Role::Server, PCT_THRESHOLDS, &value, theme);
    }
    if let Some(y) = row(inner, 3) {
        req_row(m.metrics.as_ref(), theme, rx, y, rest_w, buf);
    }
}

/// The small layout: one column, the essentials.
fn render_small(app: &App, m: &ActiveModel, theme: &Theme, inner: Rect, buf: &mut Buffer) {
    let w = inner.width;
    let y0 = inner.y;
    name_row(m, theme, inner.x, y0, w, buf);
    if let Some(y) = row(inner, 1) {
        badge_task_row(app, m, theme, inner.x, y, w, buf);
    }
    if let Some(y) = row(inner, 2) {
        set_text(buf, inner.x, y, w, &format!("{} {}", speed(m), label(m)), Role::Text, theme);
    }
    let slot = m.slots.first();
    if let Some(y) = row(inner, 3) {
        let value = ctx_value_text(slot);
        gauge_row(buf, inner.x, y, w, "ctx  ", ctx_ratio(slot), Role::Server, PCT_THRESHOLDS, &value, theme);
    }
}

/// Render the band with no active model: the Loading name (if any), the badge and a message.
fn render_without_active(app: &App, theme: &Theme, inner: Rect, buf: &mut Buffer) {
    let llama = &app.llama;

    // Row 0: the Loading model's name, when there is one.
    if let Some(llama) = llama.as_ref()
        && let Some(m) = llama.models.iter().find(|m| m.state == ModelState::Loading)
    {
        let (n, _) = short_model_name(&m.id);
        buf.set_line(inner.x, inner.y, &Line::from(Span::styled(n, theme.fg(Role::Text))), inner.width);
    }

    // Row 1: the badge.
    let (bt, br) = badge_state(app);
    let y = inner.y + 1;
    if y < inner.y + inner.height {
        let badge = badge(bt, br, theme, badge_lit(bt, app.frame));
        buf.set_line(inner.x, y, &Line::from(vec![badge]), inner.width);
    }

    // Row 2: the message.
    let msg: String = match llama.as_ref() {
        None => "waiting for the first poll…".to_string(),
        Some(llama) => match &llama.health {
            Health::Down(msg) => msg.clone(),
            Health::Unauthorized => "set LLAMA_API_KEY or --api-key-file".to_string(),
            Health::Ok => match llama.models.iter().find(|m| m.state == ModelState::Loading) {
                Some(m) => format!("loading {}…", short_model_name(&m.id).0),
                None => format!("no model loaded — {} available", llama.models.len()),
            },
        },
    };
    let y = inner.y + 2;
    if y < inner.y + inner.height {
        buf.set_line(inner.x, y, &Line::from(Span::styled(msg, theme.fg(Role::Dim))), inner.width);
    }
}

// --------------------------------------------------------------------------- Shared rows

/// Row 0: the short name (bold) and the quantization (`Dim`), two spaces apart.
fn name_row(m: &ActiveModel, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let (name, quant) = short_model_name(&m.id);
    let mut spans = vec![Span::styled(name, theme.fg(Role::Text).add_modifier(Modifier::BOLD))];
    if let Some(q) = quant {
        spans.push(Span::styled(format!("  {q}"), theme.fg(Role::Dim)));
    }
    buf.set_line(x, y, &Line::from(spans), width);
}

/// Row 1: the badge, then `task #<id_task>` (and the elapsed time when known).
fn badge_task_row(app: &App, m: &ActiveModel, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let (bt, br) = badge_state(app);
    let mut spans = vec![badge(bt, br, theme, badge_lit(bt, app.frame))];
    if let Some(s) = m.slots.first().filter(|s| s.is_processing) {
        spans.push(Span::styled(format!("  task #{}", s.id_task), theme.fg(Role::Dim)));
        if let Some(elapsed) = app.task_elapsed_s() {
            spans.push(Span::styled(format!(" · {}", duration(elapsed)), theme.fg(Role::Dim)));
        }
    }
    buf.set_line(x, y, &Line::from(spans), width);
}

/// Row 2: the model settings (`Dim`, fit to the column).
fn settings_row(info: Option<&ModelInfo>, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let mut parts = Vec::new();
    if let Some(info) = info {
        if let Some(v) = &info.spec_type {
            parts.push(v.clone());
        }
        if let Some(v) = info.n_cpu_moe {
            parts.push(format!("n-cpu-moe {v}"));
        }
        if let Some(v) = info.n_gpu_layers {
            parts.push(format!("ngl {v}"));
        }
    }
    set_text(buf, x, y, width, &fit(&parts.join(" · "), width as usize), Role::Dim, theme);
}

/// Row 3 (wide) or row 1 (medium/small): the prompt speed, cache reuse and draft acceptance.
fn prompt_row(metrics: Option<&Metrics>, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let dim = |t: &'static str| Span::styled(t, theme.fg(Role::Dim));
    let text = |t: String| Span::styled(t, theme.fg(Role::Text));
    // Draft, then cache, are dropped whole when the column is narrow.
    let groups = vec![
        vec![dim("prompt "), text(tps(metrics.and_then(|m| m.avg_prompt_tps()))), dim(" t/s")],
        vec![dim("   cache "), text(pct(metrics.and_then(|m| m.cache_reuse_pct())))],
        vec![dim("   draft "), text(pct(metrics.and_then(|m| m.draft_acceptance_pct())))],
    ];
    buf.set_line(x, y, &fit_groups(groups, width as usize), width);
}

/// The request count row: `req  <running> running · <queued> queued`.
fn req_row(metrics: Option<&Metrics>, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let processing = metrics.map(|m| m.requests_processing as u64).unwrap_or(0);
    let deferred = metrics.map(|m| m.requests_deferred as u64).unwrap_or(0);
    set_text(buf, x, y, width, &format!("req  {processing} running · {deferred} queued"), Role::Text, theme);
}

/// The context used by the active slot, `(used, n_ctx)`.
fn ctx_used(slot: Option<&SlotInfo>) -> (u64, u64) {
    match slot {
        Some(s) => (s.n_prompt_tokens.saturating_add(s.n_decoded), s.n_ctx),
        None => (0, 0),
    }
}

fn ctx_ratio(slot: Option<&SlotInfo>) -> f64 {
    let (used, n_ctx) = ctx_used(slot);
    if n_ctx == 0 { 0.0 } else { used as f64 / n_ctx as f64 }
}

/// `<used> / <n_ctx>`, or `n/a` without a slot or a context size.
fn ctx_value_text(slot: Option<&SlotInfo>) -> String {
    let (used, n_ctx) = ctx_used(slot);
    if n_ctx == 0 { "n/a".to_string() } else { format!("{} / {}", count(used), count(n_ctx)) }
}

/// The ctx value under the gauge: `     <used> / <n_ctx>  <pct>`.
fn ctx_value_row(slot: Option<&SlotInfo>, theme: &Theme, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let (used, n_ctx) = ctx_used(slot);
    let text = if n_ctx == 0 {
        "     n/a".to_string()
    } else {
        let pct_str = pct(Some(used as f64 / n_ctx as f64 * 100.0));
        format!("     {} / {}  {}", count(used), count(n_ctx), pct_str)
    };
    set_text(buf, x, y, width, &text, Role::Text, theme);
}

/// A single styled line, clipped to `width`.
fn set_text(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, role: Role, theme: &Theme) {
    buf.set_line(x, y, &Line::from(Span::styled(text, theme.fg(role))), width);
}
