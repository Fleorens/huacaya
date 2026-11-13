//! The key bar, the last row of the Overview: a single `Dim` line of the shortcuts that fit.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;

use crate::app::App;
use crate::ui::panels::Ctx;
use crate::ui::text::fit_groups;
use crate::ui::theme::Role;

/// The key bar, last row of the Overview: `q quit · ? help · p pause · +/- refresh <s> s · / filter · e level
/// · r raw · x proxy`, `<s>` = `interval_ms / 1000` with one decimal, all `Dim`; the shortcuts that do not
/// fit are dropped whole from the end.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = ctx.theme;
    // `<s>` in seconds with one decimal (`500` ms → `0.5`, `1000` → `1.0`).
    let seconds = format!("{:.1}", app.interval_ms as f64 / 1000.0);
    let keys = [
        "q quit".to_string(),
        "? help".to_string(),
        "p pause".to_string(),
        format!("+/- refresh {seconds} s"),
        "/ filter".to_string(),
        "e level".to_string(),
        "r raw".to_string(),
        "x proxy".to_string(),
    ];
    let groups = keys
        .into_iter()
        .enumerate()
        .map(|(i, k)| {
            let text = if i == 0 { k } else { format!(" · {k}") };
            vec![Span::styled(text, theme.fg(Role::Dim))]
        })
        .collect();
    buf.set_line(area.x, area.y, &fit_groups(groups, area.width as usize), area.width);
}
