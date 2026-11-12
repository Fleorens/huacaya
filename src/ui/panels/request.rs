//! The Last request panel: prompt and generated tokens and speeds, and draft acceptance with the
//! context size and time of the last request. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use crate::app::App;
use crate::ui::panels::Ctx;
use crate::ui::text::{pct, tps};
use crate::ui::theme::Role;
use crate::util::count;

/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;

/// The Last request panel: `frame::panel("Last request", Role::Server, false, theme)` and three rows:
/// `prompt <tokens> tok   <tps> t/s`, `gen    <tokens> tok   <tps> t/s`,
/// `draft  <pct>   ctx <count(ctx_tokens)>   at <time>` (`n/a` for each missing value; labels `Dim`,
/// padded to 7). Without a last request: one `Dim` row `no request yet`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.fg(Role::Border))
        .title(Span::styled(
            " Last request ".to_string(),
            theme.fg(Role::Server).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, buf);

    match &app.last_request {
        None => {
            buf.set_line(
                inner.x,
                inner.y,
                &Line::from(Span::styled("no request yet", theme.fg(Role::Dim))),
                inner.width,
            );
        }
        Some(req) => {
            let prompt_tokens = req.prompt_tokens.map(|n| n.to_string());
            let gen_tokens = req.gen_tokens.map(|n| n.to_string());
            let ctx_tokens = req.ctx_tokens.map(count);
            let time = req.time.clone().unwrap_or_else(|| "n/a".to_string());

            let rows: Vec<Line> = vec![
                Line::from(vec![
                    Span::styled("prompt ", theme.fg(Role::Dim)),
                    Span::styled(prompt_tokens.unwrap_or_else(|| "n/a".to_string()), theme.fg(Role::Text)),
                    Span::styled(format!(" tok   {} t/s", tps(req.prompt_tps)), theme.fg(Role::Text)),
                ]),
                Line::from(vec![
                    Span::styled("gen    ", theme.fg(Role::Dim)),
                    Span::styled(gen_tokens.unwrap_or_else(|| "n/a".to_string()), theme.fg(Role::Text)),
                    Span::styled(format!(" tok   {} t/s", tps(req.gen_tps)), theme.fg(Role::Text)),
                ]),
                Line::from(vec![
                    Span::styled("draft  ", theme.fg(Role::Dim)),
                    Span::styled(pct(req.draft_pct), theme.fg(Role::Text)),
                    Span::styled(
                        format!("   ctx {}   at {}", ctx_tokens.unwrap_or_else(|| "n/a".to_string()), time),
                        theme.fg(Role::Text),
                    ),
                ]),
            ];
            for (i, line) in rows.iter().enumerate() {
                let y = inner.y + i as u16;
                if y < inner.y + inner.height {
                    buf.set_line(inner.x, y, line, inner.width);
                }
            }
        }
    }
}
