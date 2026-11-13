//! The Agents map view (T44): the activity map on top, a rule, the step feed with durations, and a
//! one-line "now" status, drawn in the right side of the Agents screen. See `docs/02-ui.md` § 2.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::collect::opencode::{OpenCodeSnapshot, Session, Step};
use crate::ui::frame::panel;
use crate::ui::map;
use crate::ui::panels::model;
use crate::ui::screens::agents::{render_timeline_row, session_time, session_title};
use crate::ui::theme::{ColorMode, Role, Theme, lerp_rgb, rgb};
use crate::util::{count, duration, short_model_name};

/// The map view of the followed session in `area` (the right side of the Agents screen).
pub fn render(
    app: &App,
    snapshot: &OpenCodeSnapshot,
    session: &Session,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let title = session_title(session);
    let block = panel(&title, Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 3 || inner.width < 3 {
        return;
    }

    // The right titles: ` m list ` always, `· follow ● ` in `Good` when nothing is followed.
    let mut titles: Vec<(String, Style)> = vec![(" m list ".to_string(), theme.fg(Role::Dim))];
    if app.agent_focus.is_none() {
        titles.insert(0, ("· follow ● ".to_string(), theme.fg(Role::Good)));
    }
    set_right_titles(buf, area, &title, &titles);

    // The map on top, then a rule, then the feed, then the now line on the last row.
    let map_rows = (inner.height * 3 / 5).clamp(map::MIN_H, 23);
    let rule_y = inner.y + map_rows;
    let now_y = inner.y + inner.height - 1;
    let feed_rows = inner.height.saturating_sub(map_rows).saturating_sub(2);

    let core = map_core(app, session);
    map::render(
        &snapshot.activity,
        &core,
        app.now_ms,
        theme,
        Rect::new(inner.x, inner.y, inner.width, map_rows),
        buf,
    );

    // The rule: a `─` across the inner width, a true-colour blend of the background and the border.
    let (r, g, b) = lerp_rgb((0x1a, 0x1b, 0x26), rgb(Role::Border), 0.6);
    let rule_style = match theme.mode {
        ColorMode::TrueColor => Style::new().fg(Color::Rgb(r, g, b)),
        _ => theme.fg(Role::Border),
    };
    let rule = Span::styled("─".repeat(inner.width as usize), rule_style);
    buf.set_line(inner.x, rule_y, &Line::from(rule), inner.width);

    // The feed: the newest steps that fit, oldest on top, each with its duration in the last four columns.
    let shown: Vec<&Step> =
        snapshot.steps.iter().skip(snapshot.steps.len().saturating_sub(feed_rows as usize)).collect();
    for (i, step) in shown.iter().enumerate() {
        let y = rule_y + 1 + i as u16;
        if y >= now_y {
            break;
        }
        render_timeline_row(
            buf,
            Rect::new(inner.x, y, inner.width.saturating_sub(5), 1),
            y,
            step,
            app.utc_offset_s,
            theme,
        );
        set_duration(buf, inner, y, step, app.now_ms, theme);
    }

    // The now line: the running station, then the session's tokens, time and step count.
    set_now_line(buf, inner, now_y, app, snapshot, session, theme);
}

/// The map's centre: the live model's name, speed and context fill, or the session's model when no
/// llama.cpp model is active (both values `None`).
fn map_core(app: &App, session: &Session) -> map::Core {
    match app.llama.as_ref().and_then(|l| l.active.first()) {
        Some(m) => map::Core {
            model: short_model_name(&m.id).0,
            tps: model::speed_value(m),
            ctx_ratio: model::ctx_ratio_of(m),
        },
        None => map::Core {
            model: session.model.as_deref().map(short_model_name).map(|(n, _)| n).unwrap_or_default(),
            tps: None,
            ctx_ratio: None,
        },
    }
}

/// The duration of a step, right-aligned in four columns: `<s>s` below 60 s, `<m>m` below an hour,
/// `<h>h` otherwise, `0s` when negative. `Good` for a running step, else `Dim`.
fn set_duration(buf: &mut Buffer, inner: Rect, y: u16, step: &Step, now_ms: i64, theme: &Theme) {
    let end = step.end_ms.unwrap_or(now_ms);
    let dur_s = (end - step.time_ms).max(0) / 1000;
    let raw = if dur_s < 60 {
        format!("{}s", dur_s)
    } else if dur_s < 3600 {
        format!("{}m", dur_s / 60)
    } else {
        format!("{}h", dur_s / 3600)
    };
    let dur = Span::styled(
        format!("{:>4}", raw),
        theme.fg(if step.end_ms.is_none() { Role::Good } else { Role::Dim }),
    );
    let x = inner.x + inner.width.saturating_sub(4);
    buf.set_line(x, y, &Line::from(dur), 4);
}

/// The now line: `◉ <station>  <summary>  <m:ss>`, or `○ idle`; then the session's tokens, time and the
/// step count, all `Dim`. Cut to the inner width.
fn set_now_line(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    app: &App,
    snapshot: &OpenCodeSnapshot,
    session: &Session,
    theme: &Theme,
) {
    if y >= inner.y + inner.height {
        return;
    }
    let mut spans: Vec<Span> = Vec::new();
    match &snapshot.activity.now {
        Some((station, summary, start)) => {
            let style = theme.fg(map::station_role(*station)).add_modifier(Modifier::BOLD);
            spans.push(Span::styled("◉ ", style));
            spans.push(Span::styled(station.name().to_string(), style));
            spans.push(Span::styled("  ", theme.fg(Role::Text)));
            spans.push(Span::styled(summary.clone(), theme.fg(Role::Text)));
            spans.push(Span::styled("  ", theme.fg(Role::Text)));
            let secs = (app.now_ms - start).max(0) / 1000;
            spans.push(Span::styled(format!("{}:{:02}", secs / 60, secs % 60), theme.fg(Role::Dim)));
        }
        None => spans.push(Span::styled("○ idle", theme.fg(Role::Dim))),
    }
    let stats = format!(
        "   in {}  out {} · {}",
        count(session.tokens_in),
        count(session.tokens_out),
        duration(session_time(snapshot, session))
    );
    let total_calls: u64 = snapshot.activity.stations.iter().map(|s| s.calls as u64).sum();
    spans.push(Span::styled(stats, theme.fg(Role::Dim)));
    spans.push(Span::styled(format!("   step {}", total_calls), theme.fg(Role::Dim)));
    buf.set_line(inner.x, y, &Line::from(spans), inner.width);
}

/// Right-title segments on the top border, packed from the right corner leftward: the first segment
/// is the rightmost;
/// omitted when they collide with the left title or the corner.
fn set_right_titles(buf: &mut Buffer, area: Rect, title: &str, parts: &[(String, Style)]) {
    let right = (area.x + area.width.saturating_sub(1)) as usize;
    let left = area.x as usize + title.chars().count() + 3;
    let total: usize = parts.iter().map(|(t, _)| t.chars().count()).sum();
    if total > right.saturating_sub(left) {
        return;
    }
    let mut x = right;
    for (text, style) in parts.iter() {
        let w = text.chars().count() as u16;
        x = x.saturating_sub(w as usize);
        if x < area.x as usize {
            break;
        }
        buf.set_line(x as u16, area.y, &Line::from(Span::styled(text.clone(), *style)), w);
    }
}
