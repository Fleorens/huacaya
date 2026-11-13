//! The four detail panels of the Models screen (T36): Identity, Runtime, Performance, Slot, drawn
//! side by side when the area is at least 80 wide, else stacked. See `tasks/T36-models-v2.md`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::llama::ModelInfo;
use crate::collect::llama_poll::{ActiveModel, LlamaSnapshot};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::screens::models_details_panels::{
    render_identity, render_performance, render_runtime, render_slot,
};
use crate::ui::text::fit_groups;
use crate::ui::theme::{Role, Theme};

/// The details of the selected model: the four panels side by side when the area is at least 80 wide,
/// else stacked. A `Dim` hint line occupies its last row when the area is at least 12 rows high.
pub(crate) fn render_details(app: &App, llama: &LlamaSnapshot, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
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

    if area.width >= 80 {
        render_two_columns(app, model, active, ctx, inner, buf, theme);
    } else {
        render_one_column(app, model, active, ctx, inner, buf, theme);
    }

    if area.height >= 12 {
        render_hint(inner, buf, theme);
    }
}

/// Two equal columns (the left takes the odd one): Identity over Runtime on the left, Performance over
/// Slot on the right. Each lower panel takes the rest of its column.
fn render_two_columns(
    app: &App,
    model: &ModelInfo,
    active: Option<&ActiveModel>,
    ctx: &Ctx,
    inner: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    let left_w = inner.width.div_ceil(2);
    let right_w = inner.width - left_w;

    render_identity(model, Rect::new(inner.x, inner.y, left_w, 6), buf, theme);
    render_runtime(
        app,
        model,
        Rect::new(inner.x, inner.y + 6, left_w, inner.height.saturating_sub(6)),
        buf,
        theme,
    );
    render_performance(app, active, ctx, Rect::new(inner.x + left_w, inner.y, right_w, 7), buf, theme);
    render_slot(
        app,
        active,
        Rect::new(inner.x + left_w, inner.y + 7, right_w, inner.height.saturating_sub(7)),
        buf,
        theme,
    );
}

/// The four panels stacked, each taking its own height (6, 6, 7, 6); the ones that do not fit are not
/// drawn.
fn render_one_column(
    app: &App,
    model: &ModelInfo,
    active: Option<&ActiveModel>,
    ctx: &Ctx,
    inner: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    let (x, w) = (inner.x, inner.width);
    let mut y = inner.y;
    let bottom = inner.y.saturating_add(inner.height);
    if y + 6 <= bottom {
        render_identity(model, Rect::new(x, y, w, 6), buf, theme);
        y += 6;
    }
    if y + 6 <= bottom {
        render_runtime(app, model, Rect::new(x, y, w, 6), buf, theme);
        y += 6;
    }
    if y + 7 <= bottom {
        render_performance(app, active, ctx, Rect::new(x, y, w, 7), buf, theme);
        y += 7;
    }
    if y + 6 <= bottom {
        render_slot(app, active, Rect::new(x, y, w, 6), buf, theme);
    }
}

/// The hint line on the last row of `inner`: the help groups, dropped whole from the end.
fn render_hint(inner: Rect, buf: &mut Buffer, theme: &Theme) {
    let raw = "↑↓ select · Home/End · read-only: llamatop never loads or unloads a model";
    let parts: Vec<&str> = raw.split(" · ").collect();
    let groups: Vec<Vec<Span>> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let text = if i + 1 < parts.len() { format!("{part} · ") } else { part.to_string() };
            vec![Span::styled(text, theme.fg(Role::Dim))]
        })
        .collect();
    let line = fit_groups(groups, inner.width as usize);
    let y = inner.y.saturating_add(inner.height.saturating_sub(1));
    buf.set_line(inner.x, y, &line, inner.width);
}
