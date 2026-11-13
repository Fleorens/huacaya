//! The dense Overview: one-row bands for the narrow terminal (Overview only). See `docs/02-ui.md` § 3.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::app::clock;
use crate::collect::gpu::GpuSample;
use crate::collect::llama_poll::ActiveModel;
use crate::collect::opencode::{Step, StepKind};
use crate::ui::gauge::Gauge;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::Ctx;
use crate::ui::panels::log::render_line;
use crate::ui::panels::model::{badge_state, ctx_ratio, ctx_used, no_model_message, speed};
use crate::ui::screens::agents::step_glyph;
use crate::ui::text::{bytes_pair, fit, fit_groups, pct, temp};
use crate::ui::theme::{GPU_JUNCTION_THRESHOLDS, PCT_THRESHOLDS, Role, Severity, Theme, severity};
use crate::util::{count, short_model_name};

/// The badge's short form (`◉ GEN`, `○ IDLE`, `◐ LOAD`, `✖ DOWN` …); any other text kept as-is.
pub fn short_badge(text: &str) -> String {
    match text {
        "GENERATING" => "◉ GEN".to_string(),
        "PROMPT" => "◉ PROMPT".to_string(),
        "IDLE" => "○ IDLE".to_string(),
        "LOADING" => "◐ LOAD".to_string(),
        "NO MODEL" => "○ NO MODEL".to_string(),
        "SERVER DOWN" => "✖ DOWN".to_string(),
        "UNAUTHORIZED" => "✖ AUTH".to_string(),
        "WAITING" => "○ WAIT".to_string(),
        other => other.to_string(),
    }
}

/// The Narrow tier's Overview in `area` (the body below the header): frameless rows, then the log.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let theme = ctx.theme;
    let x = area.x;
    let w = area.width;
    let h = area.height as usize;
    // When the body is short (below 12 rows) the inner rules drop first, then the context band.
    let drop = h < 12;

    let mut y = area.y;
    rule(buf, x, y, w, theme);
    y += 1;
    model_row(app, ctx, x, y, w, buf, theme);
    y += 1;
    if !drop {
        context_row(app, x, y, w, buf, theme);
        y += 1;
    }
    if !drop {
        rule(buf, x, y, w, theme);
        y += 1;
    }
    gpu_row(app, ctx, x, y, w, buf, theme);
    y += 1;
    cpu_row(app, ctx, x, y, w, buf, theme);
    y += 1;
    if !drop {
        rule(buf, x, y, w, theme);
        y += 1;
    }
    // The keys sit on the last body row; `y` is absolute, so it is `area.y + h - 1`, not `h - 1`.
    let last = area.y + h as u16 - 1;
    render_log(app, x, y, (last as usize).saturating_sub(y as usize), w, buf, theme);
    key_row(x, last, w, buf, theme);
}

fn rule(buf: &mut Buffer, x: u16, y: u16, w: u16, theme: &Theme) {
    buf.set_line(x, y, &Line::from(Span::styled("─".repeat(w as usize), theme.fg(Role::Border))), w);
}

fn model_row(app: &App, ctx: &Ctx, x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    let (badge_text, badge_role) = badge_state(app);
    let badge_text = short_badge(badge_text);
    let mut spans = vec![Span::styled(badge_text, theme.fg(badge_role).add_modifier(Modifier::BOLD))];
    match active_model(app) {
        Some(m) => {
            let (name, quant) = short_model_name(&m.id);
            spans.push(Span::styled(" ", theme.fg(Role::Text)));
            spans.push(Span::styled(name, theme.fg(Role::Text)));
            if let Some(q) = quant {
                spans.push(Span::styled(" ", theme.fg(Role::Text)));
                spans.push(Span::styled(q, theme.fg(Role::Dim)));
            }
            let speed = speed(m);
            spans.push(Span::styled("   ", theme.fg(Role::Text)));
            spans.push(Span::styled(speed, theme.fg(Role::Text)));
            spans.push(Span::styled(" t/s", theme.fg(Role::Server).add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(" ", theme.fg(Role::Text)));
            let used = spans.iter().map(|s| s.width()).sum::<usize>() as u16;
            if let Some(gw) = graph_width(ctx, w.saturating_sub(used)) {
                let values = app.gen_tps.tail(usize::MAX);
                let max = app.gen_tps.max().unwrap_or(0.0).max(1.0);
                BrailleGraph { values: &values, max, accent: Role::Server, theme }
                    .render(Rect::new(x + used, y, gw, 1), buf);
            }
        }
        None => {
            spans.push(Span::styled(" ", theme.fg(Role::Text)));
            spans.push(Span::styled(no_model_message(app), theme.fg(Role::Dim)));
        }
    }
    buf.set_line(x, y, &Line::from(spans), w);
}

fn context_row(app: &App, x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    let m = match active_model(app) {
        Some(m) => m,
        None => return,
    };
    let slot = m.slots.first();
    let (used, n_ctx) = ctx_used(slot);
    let count_text = if n_ctx == 0 { "n/a".to_string() } else { format!("{}/{}", count(used), count(n_ctx)) };
    let after = 4u16 + 20u16;
    buf.set_line(x, y, &Line::from(Span::styled("ctx ", theme.fg(Role::Dim))), 4);
    Gauge { ratio: ctx_ratio(slot), accent: Role::Server, thresholds: PCT_THRESHOLDS, theme }
        .render(Rect::new(x + 4, y, 20, 1), buf);
    let cache = m.metrics.as_ref().and_then(|m| m.cache_reuse_pct());
    let draft = m.metrics.as_ref().and_then(|m| m.draft_acceptance_pct());
    let groups = vec![
        vec![Span::styled(format!(" {count_text}"), theme.fg(Role::Text))],
        vec![Span::styled(format!("  cache {}", pct(cache)), theme.fg(Role::Dim))],
        vec![Span::styled(format!("  draft {}", pct(draft)), theme.fg(Role::Dim))],
    ];
    let width = w.saturating_sub(after);
    buf.set_line(x + after, y, &fit_groups(groups, width as usize), width);
}

fn gpu_row(app: &App, ctx: &Ctx, x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    match app.system.as_ref().and_then(|s| s.gpus.first()) {
        None => {
            let line = Line::from(vec![
                Span::styled("GPU ", theme.fg(Role::Gpu)),
                Span::styled("no GPU", theme.fg(Role::Dim)),
            ]);
            buf.set_line(x, y, &line, w);
        }
        Some(g) => {
            let values = app.gpu_load.tail(usize::MAX);
            let after = row_head(
                "GPU ",
                &values,
                ctx,
                g.busy_pct.map(|b| b as f64),
                5,
                x,
                y,
                w,
                Role::Gpu,
                buf,
                theme,
            );
            let groups = vec![vram_group(g, theme), temp_group(g, theme), power_group(g, theme)];
            let width = w.saturating_sub(after);
            buf.set_line(after, y, &fit_groups(groups, width as usize), width);
        }
    }
}

fn cpu_row(app: &App, ctx: &Ctx, x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    let Some(s) = app.system.as_ref() else {
        buf.set_line(x, y, &Line::from(Span::styled("CPU n/a", theme.fg(Role::Dim))), w);
        return;
    };
    let values = app.cpu_usage.tail(usize::MAX);
    let after =
        row_head("CPU ", &values, ctx, s.cpu.usage_pct.map(|b| b as f64), 5, x, y, w, Role::Cpu, buf, theme);
    let (ram_used, ram_total) = match s.mem.as_ref() {
        Some(m) => (m.used, m.total),
        None => (0, 0),
    };
    let ram_text = if ram_total == 0 { "n/a".to_string() } else { bytes_pair(ram_used, ram_total) };
    let groups = vec![
        vec![Span::styled("  RAM  ", theme.fg(Role::Dim)), Span::styled(ram_text, theme.fg(Role::Text))],
        vec![Span::styled(format!("  {}", temp(s.cpu_temp_c)), theme.fg(Role::Text))],
    ];
    let width = w.saturating_sub(after);
    buf.set_line(after, y, &fit_groups(groups, width as usize), width);
}

/// The shared band head: label, gauge, value and graph; returns the column after it.
#[allow(clippy::too_many_arguments)] // one row's worth of values
fn row_head(
    label: &str,
    values: &[f32],
    ctx: &Ctx,
    value: Option<f64>,
    graph_w: u16,
    x: u16,
    y: u16,
    w: u16,
    accent: Role,
    buf: &mut Buffer,
    theme: &Theme,
) -> u16 {
    let label_w = label.chars().count().max(1) as u16;
    buf.set_line(x, y, &Line::from(Span::styled(label, theme.fg(accent))), label_w);
    let mut after = label_w + 10;
    let ratio = value.map(|v| v / 100.0).unwrap_or(0.0);
    Gauge { ratio, accent, thresholds: PCT_THRESHOLDS, theme }.render(Rect::new(x + label_w, y, 10, 1), buf);
    let pct_text = pct(value);
    buf.set_line(
        after,
        y,
        &Line::from(Span::styled(format!(" {pct_text}"), theme.fg(Role::Text))),
        w.saturating_sub(after),
    );
    after += format!(" {pct_text}").chars().count() as u16;
    if ctx.graphs && graph_w > 0 {
        BrailleGraph { values, max: 100.0, accent, theme }.render(Rect::new(after, y, graph_w, 1), buf);
        after += graph_w + 1;
    }
    after
}

/// The log region: the last lines that fit, the newest replaced by the followed step when it runs.
fn render_log(app: &App, x: u16, y: u16, log_h: usize, w: u16, buf: &mut Buffer, theme: &Theme) {
    let Some((step, job)) = running_step(app) else {
        fill_log(app, x, y, log_h, w, buf, theme);
        return;
    };
    let v = app.visible_logs();
    let log_rows = log_h.saturating_sub(1);
    let n = v.len().min(log_rows);
    let base = y + log_h as u16 - 1 - n as u16;
    for (i, line) in v[v.len() - n..].iter().enumerate() {
        render_line(line, app.log_view.raw, theme, x, base + i as u16, w, buf);
    }
    agent_step_row(app, step, job, x, y + log_h as u16 - 1, w, buf, theme);
}

fn fill_log(app: &App, x: u16, y: u16, log_h: usize, w: u16, buf: &mut Buffer, theme: &Theme) {
    let v = app.visible_logs();
    if log_h == 0 || v.is_empty() {
        return;
    }
    let n = v.len().min(log_h);
    let start = v.len() - n;
    for (i, line) in v[start..start + n].iter().enumerate() {
        render_line(line, app.log_view.raw, theme, x, y + i as u16, w, buf);
    }
}

/// The followed session's newest step and its job, when the session is running and it has steps.
fn running_step(app: &App) -> Option<(&Step, Option<u32>)> {
    let snapshot = app.opencode.as_ref()?;
    let id = app.agent_focus.as_deref().or(snapshot.focus.as_deref())?;
    let session = snapshot.sessions.iter().find(|s| s.id == id)?;
    if !session.running {
        return None;
    }
    snapshot.steps.last().map(|step| (step, session.job))
}

/// The followed step as a log row, glyphs and colours as the Agents screen.
#[allow(clippy::too_many_arguments)] // the timeline row contract
fn agent_step_row(
    app: &App,
    step: &Step,
    job: Option<u32>,
    x: u16,
    y: u16,
    w: u16,
    buf: &mut Buffer,
    theme: &Theme,
) {
    let (glyph_text, glyph_role) = step_glyph(step);
    let time = clock::hms(step.time_ms / 1000, app.utc_offset_s);
    let time_span = Span::styled(format!("{time} "), theme.fg(Role::Dim));
    let agent_span = Span::styled(" agent ", theme.fg(Role::Gpu));
    let job_span = job.map(|j| Span::styled(format!("#{j} "), theme.fg(Role::Dim)));
    let glyph_span =
        Span::styled(format!("{:<8}", glyph_text), theme.fg(glyph_role).add_modifier(Modifier::BOLD));
    let prefix = time_span.width()
        + agent_span.width()
        + job_span.as_ref().map(|s| s.width()).unwrap_or(0)
        + 1
        + glyph_span.width()
        + 1;
    let value_w = w.saturating_sub(prefix as u16).max(1) as usize;
    let mut spans = vec![time_span, agent_span];
    if let Some(j) = job_span {
        spans.push(j);
    }
    spans.push(Span::raw(" "));
    spans.push(glyph_span);
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        fit(&step.summary, value_w),
        if step.kind == StepKind::Think { theme.fg(Role::Dim) } else { theme.fg(Role::Text) },
    ));
    let detail = match &step.detail {
        Some(d) => {
            let style = match (&step.kind, step.ok) {
                (StepKind::Shell, Some(true)) => theme.fg(Role::Good),
                (_, Some(false)) => theme.fg(Role::Critical),
                _ => theme.fg(Role::Dim),
            };
            Span::styled(fit(d, value_w), style)
        }
        None => Span::raw(""),
    };
    spans.push(Span::raw("  "));
    spans.push(detail);
    buf.set_line(x, y, &Line::from(spans), w);
}

fn key_row(x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    let groups = vec![
        vec![Span::styled("q quit", theme.fg(Role::Dim))],
        vec![Span::styled("  ? help", theme.fg(Role::Dim))],
        vec![Span::styled("  1-5 screens", theme.fg(Role::Dim))],
        vec![Span::styled("  / filter", theme.fg(Role::Dim))],
        vec![Span::styled("  p pause", theme.fg(Role::Dim))],
    ];
    buf.set_line(x, y, &fit_groups(groups, w as usize), w);
}

fn vram_group(g: &GpuSample, theme: &Theme) -> Vec<Span<'static>> {
    match (g.vram_used, g.vram_total) {
        (Some(u), Some(t)) if t > 0 => vec![
            Span::styled("  VRAM ", theme.fg(Role::Dim)),
            Span::styled(bytes_pair(u, t), theme.fg(Role::Text)),
        ],
        _ => vec![Span::styled("  VRAM n/a", theme.fg(Role::Dim))],
    }
}

fn temp_group(g: &GpuSample, theme: &Theme) -> Vec<Span<'static>> {
    match g.temp_junction_c.or(g.temp_edge_c) {
        Some(t) => {
            let sev = severity(t, GPU_JUNCTION_THRESHOLDS.0, GPU_JUNCTION_THRESHOLDS.1);
            vec![
                Span::styled("  ", theme.fg(Role::Dim)),
                Span::styled(format!("{:.0}", t), sev_style(theme, sev)),
                Span::styled(" °C", theme.fg(Role::Dim)),
            ]
        }
        None => vec![Span::styled("  n/a °C", theme.fg(Role::Dim))],
    }
}

fn power_group(g: &GpuSample, theme: &Theme) -> Vec<Span<'static>> {
    match g.power_w {
        Some(p) => vec![
            Span::styled("  ", theme.fg(Role::Dim)),
            Span::styled(format!("{:.0}", p), theme.fg(Role::Text)),
            Span::styled(" W", theme.fg(Role::Dim)),
        ],
        None => vec![Span::styled("  n/a W", theme.fg(Role::Dim))],
    }
}

fn sev_style(theme: &Theme, sev: Severity) -> Style {
    match sev {
        Severity::Normal => theme.fg(Role::Text),
        Severity::Warning => theme.fg(Role::Warning),
        Severity::Critical => theme.fg(Role::Critical),
    }
}

fn active_model(app: &App) -> Option<&ActiveModel> {
    app.llama.as_ref().and_then(|l| l.active.first())
}

fn graph_width(ctx: &Ctx, remaining: u16) -> Option<u16> {
    if ctx.graphs { Some(remaining.clamp(8, 12)) } else { None }
}
