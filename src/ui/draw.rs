//! Draws one whole frame of `app` into `area`, choosing the layout from the terminal size.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::ui::compact;
use crate::ui::layout::{self, Tier};
use crate::ui::panels::Ctx;
use crate::ui::panels::{cpu, gpu, header, log, memory, model, request};
use crate::ui::theme::Role;

/// Draws one whole frame of `app` into `area`.
///
/// By `tier(area.width, area.height)`: `TooSmall` → `terminal too small` centred (`Dim`), and on the
/// next row, when it fits, `<w>×<h>, need 30×8`; `Compact` → `compact::render`; otherwise the header on
/// `layout.header`, then each present panel on its rectangle, with `Ctx { graphs: layout.graphs, ..*ctx }`.
/// The Models, Logs and System screens show the Overview body too (the header says which tab is selected).
/// Never panics, whatever the size (0 × 0 included).
pub fn draw(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    match layout::tier(area.width, area.height) {
        Tier::TooSmall => draw_too_small(app, ctx, area, buf),
        Tier::Compact => compact::render(app, ctx, area, buf),
        _ => {
            let layout = layout::overview(area);
            let ctx = Ctx { theme: ctx.theme, graphs: layout.graphs, clock: ctx.clock };
            header::render(app, &ctx, layout.header, buf);
            draw_panel(app, &ctx, layout.model, buf, model::render);
            draw_panel(app, &ctx, layout.request, buf, request::render);
            draw_panel(app, &ctx, layout.gpu, buf, gpu::render);
            draw_panel(app, &ctx, layout.cpu, buf, cpu::render);
            draw_panel(app, &ctx, layout.memory, buf, memory::render);
            draw_panel(app, &ctx, layout.log, buf, log::render);
        }
    }
}

/// Render one optional panel on its rectangle, skipping it when `None`.
fn draw_panel<F>(app: &App, ctx: &Ctx, area: Option<Rect>, buf: &mut Buffer, render: F)
where
    F: Fn(&App, &Ctx, Rect, &mut Buffer),
{
    if let Some(area) = area {
        render(app, ctx, area, buf);
    }
}

/// The "terminal too small" message, centred, and the size / need on the row below when it fits.
fn draw_too_small(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    // A 0 × 0 buffer has no cells to write; bail before the first `set_line`.
    if area.height == 0 || area.width == 0 {
        return;
    }
    let _ = (app, ctx);
    let theme = ctx.theme;
    let msg = "terminal too small";
    let msg_w = msg.chars().count() as u16;
    let y = area.y + area.height.saturating_sub(1) / 2;
    let x = area.x + (area.width.saturating_sub(msg_w)) / 2;
    buf.set_line(x, y, &Line::from(Span::styled(msg, theme.fg(Role::Dim))), area.width);

    // The size / need line, on the row below, only when there is room for it.
    if area.height >= 2 {
        let need = format!("{}×{}, need 30×8", area.width, area.height);
        let need_w = need.chars().count() as u16;
        let y = y + 1;
        if y < area.y + area.height {
            let x = area.x + (area.width.saturating_sub(need_w)) / 2;
            buf.set_line(x, y, &Line::from(Span::styled(need, theme.fg(Role::Dim))), area.width);
        }
    }
}
