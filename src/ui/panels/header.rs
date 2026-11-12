//! The Overview header: a bordered title bar with host/uptime, server status, build and clock, then
//! the screen tabs. See `docs/02-ui.md` § 2.2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use crate::app::{App, Screen};
use crate::collect::llama_poll::Health;
use crate::ui::panels::Ctx;
use crate::ui::theme::Role;
use crate::util::duration;

/// Height of the header, borders included.
pub const HEIGHT: u16 = 3;

/// `<dot> <status>` of the server: `○ waiting` when there is no snapshot, else the health dot and
/// message, with its role.
fn status(app: &App) -> (&'static str, Role) {
    match &app.llama {
        None => ("○ waiting", Role::Dim),
        Some(llama) => match llama.health {
            Health::Down(_) => ("● server down", Role::Critical),
            Health::Unauthorized => ("● no API key", Role::Warning),
            Health::Ok => ("● server ok", Role::Good),
        },
    }
}

/// The header: a `frame::panel` with an empty title (border `Role::Border`) and two titles on the top
/// border, then the tabs on the inner row.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let theme = ctx.theme;

    // Left title: ` llamatop ─ <hostname> ─ up <uptime> `.
    let hostname = app.system.as_ref().and_then(|s| s.hostname.clone()).unwrap_or_else(|| "?".to_string());
    let uptime =
        app.system.as_ref().and_then(|s| s.uptime_s).map(duration).unwrap_or_else(|| "n/a".to_string());
    let left_spans = Line::from(vec![
        Span::styled(" llamatop", theme.fg(Role::BorderFocused).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ─ {hostname} ─ up {uptime} "), theme.fg(Role::Text)),
    ]);

    // Right title: ` <dot> <status>  <build> ─ <clock> `; only `<dot> <status>` takes the status role.
    let (status, role) = status(app);
    let build = app
        .llama
        .as_ref()
        .and_then(|l| l.build_info.clone())
        .and_then(|b| b.split('-').next().map(str::to_string));
    let tail = match build {
        Some(b) => format!("  {b} ─ {} ", ctx.clock),
        None => format!(" ─ {} ", ctx.clock),
    };
    let right_spans = vec![
        Span::styled(format!(" {status}"), theme.fg(role).add_modifier(Modifier::BOLD)),
        Span::styled(tail, theme.fg(Role::Text)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.fg(Role::Border))
        .title_top(left_spans)
        .title_top(Line::from(right_spans).right_aligned());
    let inner = block.inner(area);
    block.render(area, buf);
    let y = inner.y + inner.height - 1;
    let mut tabs = Vec::new();
    for (i, sc) in Screen::ALL.iter().enumerate() {
        let style = if *sc == app.screen {
            theme.fg(Role::BorderFocused).add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            theme.fg(Role::Dim)
        };
        tabs.push(Span::styled(format!(" {} {} ", i + 1, sc.title()), style));
        tabs.push(Span::raw(" "));
    }
    let tabs_line = Line::from(tabs);
    let mut right = Vec::new();
    if app.paused {
        right.push(Span::styled("PAUSED  ", theme.fg(Role::Warning).add_modifier(Modifier::BOLD)));
    }
    right.push(Span::styled("? help  q ", theme.fg(Role::Dim)));
    let right_line = Line::from(right);

    let avail = inner.width;
    let right_w = right_line.width();
    buf.set_line(inner.x, y, &tabs_line, avail);
    if tabs_line.width() + right_w <= avail as usize {
        let x = inner.right() - right_w as u16;
        buf.set_line(x, y, &right_line, right_w as u16);
    }
}
