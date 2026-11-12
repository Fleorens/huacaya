//! The key bar, the last row of the Overview: a single `Dim` line of the shortcuts, cut at the width.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::ui::panels::Ctx;
use crate::ui::theme::Role;

/// The key bar, last row of the Overview: `q quit · ? help · p pause · +/- refresh <s> s · / filter · e level
/// · r raw · x proxy`, `<s>` = `interval_ms / 1000` with one decimal, all `Dim`, cut at the width.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = ctx.theme;
    // `<s>` in seconds with one decimal (`500` ms → `0.5`, `1000` → `1.0`).
    let seconds = format!("{:.1}", app.interval_ms as f64 / 1000.0);
    let text =
        format!("q quit · ? help · p pause · +/- refresh {seconds} s · / filter · e level · r raw · x proxy");
    // One `Dim` line, left-aligned, cut to the width by `set_line`.
    buf.set_line(area.x, area.y, &Line::from(Span::styled(text, theme.fg(Role::Dim))), area.width);
}
