//! The compact tier: one line per subsystem, then the log. See `docs/02-ui.md` § 3 (< 60 columns).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::App;
use crate::ui::panels::{self, Ctx, log};
use crate::ui::theme::{Role, Theme};

/// The compact tier: no frames, one line per subsystem, then the log.
///
/// Row 0 `huacaya · <hostname> · <dot> <status>` (the header's status text and role); row 1 the Model
/// panel's badge and speed (`<badge> <short model> <tps> t/s`, badge alone without a model); row 2
/// `GPU <busy>% <temp>°C <vram> │ CPU <usage>% │ RAM <used>` (`temp` = junction, else edge; `n/a` for a
/// missing value, no space before `%` or `°C`); then the end of `visible_logs()` as `<time> <text>`
/// coloured by level. Every row is cut at the width.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = ctx.theme;

    render_row_0(app, theme, area, buf);
    render_row_1(app, theme, area, buf);
    render_row_2(app, theme, area, buf);
    render_logs(app, theme, area, buf);
}

/// Row 0: `huacaya · <hostname> · <dot> <status>`.
fn render_row_0(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let hostname = app.system.as_ref().and_then(|s| s.hostname.clone()).unwrap_or_else(|| "?".to_string());
    let (status, role) = panels::header::status(app);
    let line = Line::from(vec![
        Span::styled("huacaya", theme.fg(Role::BorderFocused).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" · {hostname} · "), theme.fg(Role::Text)),
        Span::styled(status.to_string(), theme.fg(role).add_modifier(Modifier::BOLD)),
    ]);
    buf.set_line(area.x, area.y, &line, area.width);
}

/// Row 1: the badge, then the short model name and the live/average speed (`badge` alone without a model).
fn render_row_1(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let (text, role) = panels::model::badge_state(app);
    let badge = Span::styled(format!("◉ {text}"), theme.fg(role).add_modifier(Modifier::BOLD));

    let speed = live_or_avg_speed(app);
    let name =
        app.llama.as_ref().and_then(|l| l.active.first()).map(|m| crate::util::short_model_name(&m.id).0);

    let mut spans = vec![badge];
    if let Some(name) = name {
        spans.push(Span::styled(format!(" {name} "), theme.fg(Role::Text)));
        if let Some(speed) = speed {
            spans.push(Span::styled(
                format!("{} t/s", crate::ui::text::tps(Some(speed))),
                theme.fg(Role::Text),
            ));
        }
    }
    let y = area.y + 1;
    if y < area.y + area.height {
        buf.set_line(area.x, y, &Line::from(spans), area.width);
    }
}

/// Row 2: `GPU <busy>% <temp>°C <vram> │ CPU <usage>% │ RAM <used>` (`temp` = junction, else edge;
/// `n/a` for a missing value, no space before `%` or `°C`).
fn render_row_2(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let gpu = app.system.as_ref().and_then(|s| s.gpus.first());
    let busy = gpu.and_then(|g| g.busy_pct).map(|b| format!("{b:.0}%")).unwrap_or_else(|| "n/a".to_string());
    let junction = gpu
        .and_then(|g| g.temp_junction_c.or(g.temp_edge_c))
        .map(|j| format!("{j:.0}°C"))
        .unwrap_or_else(|| "n/a".to_string());
    let vram = gpu.and_then(|g| g.vram_used).map(crate::util::bytes).unwrap_or_else(|| "n/a".to_string());
    let usage = app
        .system
        .as_ref()
        .and_then(|s| s.cpu.usage_pct)
        .map(|u| format!("{u:.0}%"))
        .unwrap_or_else(|| "n/a".to_string());
    let ram = app
        .system
        .as_ref()
        .and_then(|s| s.mem.as_ref())
        .map(|m| crate::util::bytes(m.used))
        .unwrap_or_else(|| "n/a".to_string());

    let line = Line::from(Span::styled(
        format!("GPU {busy} {junction} {vram} │ CPU {usage} │ RAM {ram}"),
        theme.fg(Role::Text),
    ));
    let y = area.y + 2;
    if y < area.y + area.height {
        buf.set_line(area.x, y, &line, area.width);
    }
}

/// The end of `visible_logs()` on the remaining rows, `<time> <text>` coloured by level.
fn render_logs(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let v = app.visible_logs();
    let rows = area.height.saturating_sub(3) as usize;
    if rows == 0 || v.is_empty() {
        return;
    }
    // The last `rows` visible lines, oldest first.
    let start = v.len().saturating_sub(rows);
    for (i, line) in v[start..].iter().enumerate() {
        let y = area.y + 3 + i as u16;
        if y >= area.y + area.height {
            break;
        }
        log::render_line(line, app.log_view.raw, theme, area.x, y, area.width, buf);
    }
}

/// The Model panel's speed: the live generation speed while generating, else the average.
fn live_or_avg_speed(app: &App) -> Option<f64> {
    let m = app.llama.as_ref()?.active.first()?;
    let generating = m.slots.first().map(|s| s.is_processing && s.n_decoded > 0).unwrap_or(false);
    if generating { m.live_gen_tps } else { m.metrics.as_ref().and_then(|m| m.avg_gen_tps()) }
}
