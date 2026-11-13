//! The four detail panels of the Models screen (T36): Identity, Runtime, Performance, Slot. Drawn by
//! `render_details` side by side or stacked; each renderer takes its own area and writes `Dim` labels
//! padded to 8 then `Text` values, every row cut at the panel width. See `tasks/T36-models-v2.md`.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use crate::app::App;
use crate::collect::llama::{Metrics, ModelInfo};
use crate::collect::llama_poll::ActiveModel;
use crate::collect::process::{ProcSample, ServerRole};
use crate::ui::frame::panel;
use crate::ui::graph::BrailleGraph;
use crate::ui::panels::{Ctx, gauge_row};
use crate::ui::text::{fit, group_thousands, pct, tps};
use crate::ui::theme::{PCT_THRESHOLDS, Role, Theme};
use crate::util::{bytes, count, duration};

/// Identity: what the model is (`id`, `file`, `size`/`params`/`type`, `vision`).
pub(crate) fn render_identity(model: &ModelInfo, area: Rect, buf: &mut Buffer, theme: &Theme) {
    let block = panel("Identity", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }
    let (x, w) = (inner.x, inner.width);
    let mut y = inner.y;
    if y < inner.y + inner.height {
        value_row(buf, x, y, w, "id", &model.id, Role::Text, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let path = model.model_path.clone().unwrap_or_else(|| "n/a".to_string());
        value_row(buf, x, y, w, "file", &fit(&path, (w.saturating_sub(8)) as usize), Role::Text, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let (value, style) = match model.size_bytes {
            Some(sz) => {
                let p = model
                    .n_params
                    .map(crate::ui::screens::models::params)
                    .unwrap_or_else(|| "n/a".to_string());
                let ft = model.ftype.as_deref().unwrap_or("n/a");
                (format!("{}   params {}   type {ft}", bytes(sz), p), Role::Text)
            }
            None => ("loaded models only".to_string(), Role::Dim),
        };
        value_row(buf, x, y, w, "size", &value, style, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let vision = if model.vision { "yes" } else { "no" };
        value_row(buf, x, y, w, "vision", vision, Role::Text, theme);
    }
}

/// Runtime: how the model runs (context gauge, offload, spec, and the instance process when known).
pub(crate) fn render_runtime(app: &App, model: &ModelInfo, area: Rect, buf: &mut Buffer, theme: &Theme) {
    let block = panel("Runtime", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }
    let (x, w) = (inner.x, inner.width);
    let mut y = inner.y;
    if y < inner.y + inner.height {
        match (model.ctx_size, model.ctx_train) {
            (Some(c), Some(t)) => {
                let ratio = (c as f64) / (t as f64);
                let value = format!("{} of {} trained", count(c), count(t));
                gauge_row(buf, x, y, w, "context ", ratio, Role::Server, PCT_THRESHOLDS, &value, theme);
            }
            (Some(c), None) => value_row(buf, x, y, w, "context", &count(c), Role::Text, theme),
            _ => value_row(buf, x, y, w, "context", "n/a", Role::Text, theme),
        }
        y += 1;
    }
    if y < inner.y + inner.height {
        let ngl = model.n_gpu_layers.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
        let npu = model.n_cpu_moe.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
        let thr = model.threads.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string());
        value_row(
            buf,
            x,
            y,
            w,
            "offload",
            &format!("ngl {ngl}   n-cpu-moe {npu}   threads {thr}"),
            Role::Text,
            theme,
        );
        y += 1;
    }
    if y < inner.y + inner.height {
        let spec = model.spec_type.clone().unwrap_or_else(|| "none".to_string());
        value_row(buf, x, y, w, "spec", &spec, Role::Text, theme);
        y += 1;
    }
    if y < inner.y + inner.height
        && let Some(proc) = instance_proc(app, model)
    {
        let port = proc.port.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
        value_row(
            buf,
            x,
            y,
            w,
            "port",
            &format!("{port}   pid {}   rss {}", proc.pid, bytes(proc.rss)),
            Role::Text,
            theme,
        );
    }
}

/// Performance: how fast the active model is (live graph, averages, cache and draft gauges, totals).
pub(crate) fn render_performance(
    app: &App,
    active: Option<&ActiveModel>,
    ctx: &Ctx,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    let block = styled_block(
        Span::styled(" Performance ".to_string(), theme.fg(Role::Server).add_modifier(Modifier::BOLD)),
        Some(Span::styled("since load", theme.fg(Role::Dim))),
        theme,
    );
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }
    let (x, w) = (inner.x, inner.width);
    let mut y = inner.y;

    if active.is_none() {
        if y < inner.y + inner.height {
            buf.set_line(x, y, &Line::from(Span::styled("not loaded", theme.fg(Role::Dim))), w);
        }
        return;
    }
    let Some(m) = active.and_then(|a| a.metrics.as_ref()) else {
        return;
    };

    if y < inner.y + inner.height {
        draw_gen_row(app, m, ctx, x, y, w, buf, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        tps_row(buf, x, y, w, "prompt", m.avg_prompt_tps(), theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let reuse = m.cache_reuse_pct();
        gauge_row(
            buf,
            x,
            y,
            w,
            "cache   ",
            reuse.unwrap_or(0.0) / 100.0,
            Role::Good,
            (101.0, 101.0),
            &pct(reuse),
            theme,
        );
        y += 1;
    }
    if y < inner.y + inner.height
        && let Some(value) = m.draft_acceptance_pct()
    {
        gauge_row(
            buf,
            x,
            y,
            w,
            "draft   ",
            value / 100.0,
            Role::Server,
            (101.0, 101.0),
            &pct(Some(value)),
            theme,
        );
        y += 1;
    }
    if y < inner.y + inner.height {
        let value = format!(
            "{} generated   {} prompted",
            count(m.tokens_predicted_total as u64),
            count(m.prompt_tokens_total as u64)
        );
        value_row(buf, x, y, w, "tokens", &value, Role::Text, theme);
    }
}

/// Slot: what the active model's first slot is doing (task, context gauge, decoded/remain, requests).
pub(crate) fn render_slot(
    app: &App,
    active: Option<&ActiveModel>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    let slot = active.and_then(|a| a.slots.first());
    let metrics = active.and_then(|a| a.metrics.as_ref());

    let title = match slot {
        Some(s) => format!("Slot {}", s.id),
        None => "Slot".to_string(),
    };
    let right = slot.map(|s| {
        let (text, role) = if s.is_processing && s.n_decoded > 0 {
            ("◉ generating".to_string(), Role::Server)
        } else if s.is_processing {
            ("◉ prompt".to_string(), Role::Server)
        } else {
            ("○ idle".to_string(), Role::Dim)
        };
        Span::styled(text, theme.fg(role))
    });

    let block = styled_block(
        Span::styled(format!(" {title} "), theme.fg(Role::Server).add_modifier(Modifier::BOLD)),
        right,
        theme,
    );
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }
    let (x, w) = (inner.x, inner.width);
    let mut y = inner.y;

    if slot.is_none() {
        if y < inner.y + inner.height {
            buf.set_line(x, y, &Line::from(Span::styled("no slot", theme.fg(Role::Dim))), w);
        }
        return;
    }
    let s = slot.unwrap();

    if y < inner.y + inner.height {
        let value = if s.id_task < 0 {
            "-".to_string()
        } else {
            let mut v = format!("#{}", s.id_task);
            if s.is_processing
                && let Some(elapsed) = app.task_elapsed_s()
            {
                v.push_str(&format!("   running {}", duration(elapsed)));
            }
            v
        };
        value_row(buf, x, y, w, "task", &value, Role::Text, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let used = s.n_prompt_tokens.saturating_add(s.n_decoded);
        let ratio = if s.n_ctx > 0 { used as f64 / s.n_ctx as f64 } else { 0.0 };
        let value = format!("{} / {}", count(used), count(s.n_ctx));
        gauge_row(buf, x, y, w, "ctx     ", ratio, Role::Server, PCT_THRESHOLDS, &value, theme);
        y += 1;
    }
    if y < inner.y + inner.height {
        let mut v = format!("{} tok", group_thousands(s.n_decoded));
        if s.n_remain >= 0 {
            v.push_str(&format!(" remain {}", group_thousands(s.n_remain as u64)));
        }
        value_row(buf, x, y, w, "decoded", &v, Role::Text, theme);
        y += 1;
    }
    if y < inner.y + inner.height
        && let Some(m) = metrics
    {
        let value = format!(
            "{} processing   {} deferred",
            fmt_int(m.requests_processing),
            fmt_int(m.requests_deferred)
        );
        value_row(buf, x, y, w, "reqs", &value, Role::Text, theme);
    }
}

/// Draw the `gen` row: the live average (`tps`, bold) and `t/s avg`, then a one-row Braille graph of
/// `app.gen_tps` on the columns left. The graph is omitted without graphs or fewer than 8 columns.
#[allow(clippy::too_many_arguments)]
fn draw_gen_row(app: &App, m: &Metrics, ctx: &Ctx, x: u16, y: u16, w: u16, buf: &mut Buffer, theme: &Theme) {
    let num = tps(m.avg_gen_tps());
    let num_w = num.chars().count() as u16;
    let rest = " t/s avg";
    let rest_w = rest.chars().count() as u16;
    let graph_w = w.saturating_sub(8).saturating_sub(num_w).saturating_sub(rest_w);
    let draw_graph = ctx.graphs && graph_w >= 8;
    let spans = vec![
        Span::styled(format!("{:<8}", "gen"), theme.fg(Role::Dim)),
        Span::styled(num, theme.fg(Role::Text).add_modifier(Modifier::BOLD)),
        Span::styled(fit(rest, rest_w as usize), theme.fg(Role::Text)),
    ];
    buf.set_line(x, y, &Line::from(spans), w);
    if draw_graph {
        let gx = x + 8 + num_w + rest_w;
        let max = app.gen_tps.max().map_or(1.0, |v| v.max(1.0));
        let values = app.gen_tps.tail(app.gen_tps.len());
        BrailleGraph { values: &values, max, accent: Role::Server, theme: ctx.theme }
            .render(Rect::new(gx, y, graph_w, 1), buf);
    }
}

/// Draw `<label> <tps> t/s avg` with the number bold (`prompt` row).
fn tps_row(buf: &mut Buffer, x: u16, y: u16, w: u16, label: &str, tps_opt: Option<f64>, theme: &Theme) {
    let num = tps(tps_opt);
    let num_w = num.chars().count() as u16;
    let rest = " t/s avg";
    let spans = vec![
        Span::styled(format!("{:<8}", label), theme.fg(Role::Dim)),
        Span::styled(num, theme.fg(Role::Text).add_modifier(Modifier::BOLD)),
        Span::styled(fit(rest, (w.saturating_sub(8).saturating_sub(num_w)) as usize), theme.fg(Role::Text)),
    ];
    buf.set_line(x, y, &Line::from(spans), w);
}

/// A bordered panel with a bold left title and an optional right title (both on the top border).
fn styled_block<'a>(left: Span<'a>, right: Option<Span<'a>>, theme: &Theme) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.fg(Role::Border))
        .title(left);
    if let Some(right) = right {
        block = block.title(Line::from(right).right_aligned());
    }
    block.title_alignment(Alignment::Left)
}

/// One `label value` row: `label` (`Dim`, padded to 8) then `value` (`style`), cut at `width`.
#[allow(clippy::too_many_arguments)]
fn value_row(buf: &mut Buffer, x: u16, y: u16, w: u16, label: &str, value: &str, style: Role, theme: &Theme) {
    let label_w = 8u16.min(w);
    let label = Span::styled(format!("{:<8}", label), theme.fg(Role::Dim));
    let value = Span::styled(fit(value, (w.saturating_sub(label_w)) as usize), theme.fg(style));
    buf.set_line(x, y, &Line::from(vec![label, value]), w);
}

/// The `llama-server` instance process serving `model`: role `Instance` whose `model` equals the id.
fn instance_proc<'a>(app: &'a App, model: &'a ModelInfo) -> Option<&'a ProcSample> {
    app.system.as_ref().and_then(|s| {
        s.procs
            .iter()
            .find(|p| p.role == ServerRole::Instance && p.model.as_deref() == Some(model.id.as_str()))
    })
}

/// A float metric as an integer, rounded (`1.0` → `1`).
fn fmt_int(v: f64) -> String {
    format!("{:.0}", v)
}
