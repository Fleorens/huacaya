//! The Model panel: name, quantization, state badge, live speed with graph, prompt speed, cache
//! reuse, context gauge and draft acceptance with key settings. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::llama::{Metrics, ModelInfo, ModelState};
use crate::collect::llama_poll::{ActiveModel, Health};
use crate::ui::frame::{badge, panel};
use crate::ui::gauge::Gauge;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::Ctx;
use crate::ui::text::{fit, pct, tps};
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::{count, short_model_name};

/// Height needed, borders included (6 rows inside).
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

/// The Model panel: `frame::panel("Model", Role::Server, false, theme)`, then the rows below.
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
            render_with_active(app, m, models, ctx.graphs, theme, inner, buf)
        }
        None => render_without_active(app, theme, inner, buf),
    }
}

/// The badge's pulse: lit every other `frame / 5` for the animated states, always lit otherwise.
fn badge_lit(text: &str, frame: u64) -> bool {
    match text {
        "PROMPT" | "GENERATING" | "LOADING" => (frame / 5).is_multiple_of(2),
        _ => true,
    }
}

/// The Model panel with an active model.
fn render_with_active(
    app: &App,
    m: &ActiveModel,
    models: &[ModelInfo],
    graphs: bool,
    theme: &Theme,
    inner: Rect,
    buf: &mut Buffer,
) {
    let info = models.iter().find(|mi| mi.id == m.id);
    let s = m.slots.first();
    let metrics: Option<&Metrics> = m.metrics.as_ref();

    let (name, quant) = short_model_name(&m.id);

    // Row 1: `<short name>   <quant>`.
    let y = inner.y;
    if y < inner.y + inner.height {
        let mut spans = vec![Span::styled(name, theme.fg(Role::Text).add_modifier(Modifier::BOLD))];
        if let Some(q) = quant {
            spans.push(Span::styled(format!("   {q}"), theme.fg(Role::Dim)));
        }
        buf.set_line(inner.x, y, &Line::from(spans), inner.width);
    }

    // Row 2: the badge, then `   task <id_task>` when the slot is processing.
    let (bt, br) = badge_state(app);
    let y = inner.y + 1;
    if y < inner.y + inner.height {
        let mut spans = vec![badge(bt, br, theme, badge_lit(bt, app.frame))];
        if let Some(s) = s
            && s.is_processing
        {
            spans.push(Span::styled(format!("   task {}", s.id_task), theme.fg(Role::Dim)));
        }
        buf.set_line(inner.x, y, &Line::from(spans), inner.width);
    }

    // Row 3: `gen    <tps> t/s` (+ graph), or the average when not generating.
    let y = inner.y + 2;
    if y < inner.y + inner.height {
        // PROMPT (processing, nothing decoded yet) shows the average, like IDLE.
        let generating = s.map(|s| s.is_processing && s.n_decoded > 0).unwrap_or(false);
        let gen_speed = if generating { m.live_gen_tps } else { metrics.and_then(|m| m.avg_gen_tps()) };
        let value = format!("{} t/s{}", tps(gen_speed), if generating { "" } else { " avg" });
        let text_w = 7 + value.chars().count() as u16;
        let line = Line::from(vec![
            Span::styled("gen    ", theme.fg(Role::Dim)),
            Span::styled(value, theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
        let remaining = inner.width.saturating_sub(text_w);
        if graphs && remaining.saturating_sub(2) >= 8 {
            let vals = app.gen_tps.tail(usize::MAX);
            let max = app.gen_tps.max().unwrap_or(0.0).max(1.0);
            let grect = Rect::new(inner.x + text_w + 2, y, remaining - 2, 1);
            BrailleGraph { values: &vals, max, accent: Role::Server, theme }.render(grect, buf);
        }
    }

    // Row 4: `prompt <tps> t/s   cache <pct>`.
    let y = inner.y + 3;
    if y < inner.y + inner.height {
        let line = Line::from(vec![
            Span::styled("prompt ", theme.fg(Role::Dim)),
            Span::styled(tps(metrics.and_then(|m| m.avg_prompt_tps())), theme.fg(Role::Text)),
            Span::styled(" t/s   cache ", theme.fg(Role::Dim)),
            Span::styled(pct(metrics.and_then(|m| m.cache_reuse_pct())), theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
    }

    // Row 5: `ctx    <gauge>  <used> / <n_ctx>`, or `ctx    n/a` without a slot.
    let y = inner.y + 4;
    if y < inner.y + inner.height {
        let label_w: u16 = 7.min(inner.width);
        buf.set_line(inner.x, y, &Line::from(Span::styled("ctx    ", theme.fg(Role::Dim))), label_w);
        let rest = inner.width - label_w;
        match s {
            None => {
                let line = Line::from(Span::styled("n/a", theme.fg(Role::Text)));
                buf.set_line(inner.x + label_w, y, &line, rest);
            }
            Some(s) => {
                let (used, n_ctx) = (s.n_prompt_tokens.saturating_add(s.n_decoded), s.n_ctx);
                let value = format!("{} / {}", count(used), count(n_ctx));
                let value_w = value.chars().count() as u16;
                let gap: u16 = 2;
                let gauge_w = rest.saturating_sub(gap).saturating_sub(value_w);
                let mut x = inner.x + label_w;
                if gauge_w >= 4 {
                    let ratio = if n_ctx == 0 { 0.0 } else { used as f64 / n_ctx as f64 };
                    let grect = Rect::new(x, y, gauge_w, 1);
                    Gauge { ratio, accent: Role::Server, thresholds: PCT_THRESHOLDS, theme }
                        .render(grect, buf);
                    x += gauge_w + gap;
                }
                // Clip the value to the inner area so a narrow panel never draws over its border.
                if x < inner.right() {
                    let line = Line::from(Span::styled(value, theme.fg(Role::Text)));
                    buf.set_line(x, y, &line, inner.right() - x);
                }
            }
        }
    }

    // Row 6: `draft  <pct>` then the key settings, the whole row truncated to the panel width.
    let y = inner.y + 5;
    if y < inner.y + inner.height {
        let draft = metrics.and_then(|m| m.draft_acceptance_pct());
        let mut parts = vec![pct(draft)];
        if let Some(info) = info {
            if let Some(v) = info.ctx_size {
                parts.push(format!("   ctx-size {}", count(v)));
            }
            if let Some(v) = info.n_cpu_moe {
                parts.push(format!("   n-cpu-moe {v}"));
            }
            if let Some(v) = info.threads {
                parts.push(format!("   threads {v}"));
            }
        }
        let fitted = fit(&parts.concat(), (inner.width as usize).saturating_sub(7));
        let line = Line::from(vec![
            Span::styled("draft  ", theme.fg(Role::Dim)),
            Span::styled(fitted, theme.fg(Role::Text)),
        ]);
        buf.set_line(inner.x, y, &line, inner.width);
    }
}

/// The Model panel without an active model: the Loading name (if any), the badge, and a message.
fn render_without_active(app: &App, theme: &Theme, inner: Rect, buf: &mut Buffer) {
    let llama = &app.llama;

    // Row 1: the Loading model's name, when there is one.
    if let Some(llama) = llama.as_ref()
        && let Some(m) = llama.models.iter().find(|m| m.state == ModelState::Loading)
    {
        let (n, _) = short_model_name(&m.id);
        buf.set_line(inner.x, inner.y, &Line::from(Span::styled(n, theme.fg(Role::Text))), inner.width);
    }

    // Row 2: the badge.
    let (bt, br) = badge_state(app);
    let y = inner.y + 1;
    if y < inner.y + inner.height {
        let badge = badge(bt, br, theme, badge_lit(bt, app.frame));
        buf.set_line(inner.x, y, &Line::from(vec![badge]), inner.width);
    }

    // Row 3: the message.
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
