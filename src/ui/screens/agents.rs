//! The Agents screen: the OpenCode queue, the followed session, and its timeline of steps (T35).
//!
//! See `docs/02-ui.md` § 2. Read-only; the follow state lives in [`App`](crate::app::App).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::app::clock;
use crate::collect::opencode::{OpenCodeSnapshot, Session, Step, StepKind};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::text::fit;
use crate::ui::theme::{Role, Theme};
use crate::util::{count, duration, short_model_name};

/// The session panel height, borders included: five content rows.
const SESSION_H: u16 = 7;

/// Draws the Agents screen in `area` (the body under the header): the queue on the left, the followed
/// session on top right and its timeline below. Without OpenCode, a single `Agents` panel with the
/// setup message; with an error, the queue's last row shows it. Never panics.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let Some(snapshot) = app.opencode.as_ref() else {
        render_not_configured(theme, area, buf);
        return;
    };
    if area.width < 30 || area.height < SESSION_H {
        return;
    }

    // The queue on the left; the session (fixed height) then the timeline on the rest, to the right.
    let queue_w = (area.width * 2 / 5).min(52);
    let queue_area = Rect::new(area.x, area.y, queue_w, area.height);
    let right_x = area.x + queue_w;
    let right_w = area.width.saturating_sub(queue_w);
    let session_area = Rect::new(right_x, area.y, right_w, SESSION_H);
    let timeline_area =
        Rect::new(right_x, area.y + SESSION_H, right_w, area.height.saturating_sub(SESSION_H));

    render_queue(app, snapshot, theme, queue_area, buf);
    if let Some(session) = focused_session(app, snapshot) {
        render_session(snapshot, session, theme, session_area, buf);
    }
    render_timeline(app, snapshot, theme, timeline_area, buf);
}

/// The followed session: `agent_focus` when set, else the fetched `focus`, looked up in the queue.
fn focused_session<'a>(app: &App, snapshot: &'a OpenCodeSnapshot) -> Option<&'a Session> {
    let id = app.agent_focus.as_deref().or(snapshot.focus.as_deref())?;
    snapshot.sessions.iter().find(|s| s.id == id)
}

/// The short task name of a session title: `#136 Execute tasks/T30-nvidia.md` → `T30-nvidia`, other
/// titles kept. The leading `#<n> ` is removed first; a `Execute tasks/<file>.md` title keeps only the
/// final path segment without the trailing `.md`.
pub fn short_title(title: &str) -> String {
    // Strip a leading `#136 ` (the digits right after a leading `#`), then any following space.
    let without_job = match title.trim_start().strip_prefix('#') {
        Some(rest) => {
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[end..].trim_start()
        }
        None => title.trim_start(),
    };
    // A `Execute tasks/<file>.md` title reduces to the final segment without the `.md`.
    let file = without_job
        .strip_prefix("Execute tasks/")
        .or_else(|| without_job.strip_prefix("tasks/"))
        .unwrap_or(without_job);
    let name = file.strip_suffix(".md").unwrap_or(file);
    name.rsplit('/').next().unwrap_or(name).to_string()
}

/// The not-configured case: one `Agents` panel with a single `Dim` message.
fn render_not_configured(theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Agents", Role::Server, true, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    let msg = "OpenCode is not configured: llamatop setup, or --opencode <URL>";
    let w = msg.chars().count().min(inner.width as usize) as u16;
    buf.set_line(inner.x, inner.y, &Line::from(Span::styled(msg, theme.fg(Role::Dim))), w);
}

/// The queue: a header, one row per session, and a summary (or the error) on the last row.
fn render_queue(app: &App, snapshot: &OpenCodeSnapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 2 {
        return;
    }
    let block = panel("Queue", Role::Server, true, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 2 || inner.width < 3 {
        return;
    }
    // The `opencode` right title on the top border.
    set_right(buf, area, "Queue", "opencode", theme.fg(Role::Dim));
    let header = Line::from(Span::styled(QUEUE_HEADER, theme.fg(Role::Dim)));
    buf.set_line(inner.x, inner.y, &header, inner.width);

    // The rows between the header and the summary; scrolled so the selected session stays visible.
    let last_y = inner.y + inner.height - 1;
    let rows = last_y.saturating_sub(inner.y + 1) as usize;
    let first = (app.agent_selected + 1).saturating_sub(rows);
    for (i, session) in snapshot.sessions.iter().enumerate().skip(first).take(rows) {
        let y = inner.y + 1 + (i - first) as u16;
        render_queue_row(buf, inner, y, session, i == app.agent_selected, snapshot.fetched_ms, theme);
    }

    // The last row: the error in `Warning`, else the session / running counts in `Dim`.
    let line = match &snapshot.error {
        Some(err) => Span::styled(fit(err, inner.width as usize), theme.fg(Role::Warning)),
        None => {
            let running = snapshot.sessions.iter().filter(|s| s.running).count();
            Span::styled(
                format!("{} sessions · {} running", snapshot.sessions.len(), running),
                theme.fg(Role::Dim),
            )
        }
    };
    buf.set_line(inner.x, last_y, &Line::from(line), inner.width);
}

/// The queue header of the sheet; each data cell below is followed by one space so columns never touch.
const QUEUE_HEADER: &str = "  #     state        age    task";

/// One queue row: the marker (reversed only on the `▶` of the selected row), the job, state, age and
/// title, each cut and padded to its column, the whole row cut to the panel body.
fn render_queue_row(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    session: &Session,
    selected: bool,
    fetched_ms: i64,
    theme: &Theme,
) {
    let mut spans: Vec<Span> = Vec::new();
    if selected {
        spans.push(Span::styled("▶", theme.fg(Role::Text).add_modifier(Modifier::REVERSED)));
        spans.push(Span::styled(" ", theme.fg(Role::Text)));
    } else {
        spans.push(Span::styled("  ", theme.fg(Role::Text)));
    }
    let job = session.job.map(|j| j.to_string()).unwrap_or_else(|| "-".to_string());
    spans.push(Span::styled(format!("{:<5} ", job), theme.fg(Role::Text)));

    let (state_text, state_role) = queue_state(session);
    let bold = state_role != Role::Dim;
    spans.push(Span::styled(
        format!("{:<12} ", state_text),
        theme.fg(state_role).add_modifier(if bold { Modifier::BOLD } else { Modifier::empty() }),
    ));

    let age_s = fetched_ms.saturating_sub(session.created_ms) / 1000;
    spans.push(Span::styled(format!("{:<6} ", duration(age_s.max(0) as u64)), theme.fg(Role::Text)));

    // The title takes what is left after the marker and the padded cells (a longer age eats into it).
    let used: usize = spans.iter().map(|s| s.width()).sum();
    let title = short_title(&session.title);
    spans.push(Span::styled(fit(&title, (inner.width as usize).saturating_sub(used)), theme.fg(Role::Text)));
    buf.set_line(inner.x, y, &Line::from(spans), inner.width);
}

/// The queue state cell: `◉ running`, `✓ done`, `✖ failed`, else `○ <outcome>`; the text and its role.
fn queue_state(session: &Session) -> (String, Role) {
    match session.outcome.as_deref() {
        _ if session.running => ("◉ running".to_string(), Role::Server),
        Some("succeeded") => ("✓ done".to_string(), Role::Good),
        Some("failed") => ("✖ failed".to_string(), Role::Critical),
        Some(outcome) => (format!("○ {outcome}"), Role::Dim),
        None => ("○ n/a".to_string(), Role::Dim),
    }
}

/// The followed session's details: model, tokens, state and age, one `label value` row each.
fn render_session(
    snapshot: &OpenCodeSnapshot,
    session: &Session,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let title = format!(
        "#{} · {}",
        session.job.map(|j| j.to_string()).unwrap_or_else(|| "?".to_string()),
        short_title(&session.title)
    );
    let block = panel(&title, Role::Server, true, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }

    let model = session
        .model
        .as_deref()
        .map(short_model_name)
        .map(|(name, _)| name)
        .unwrap_or_else(|| "n/a".to_string());
    let agent = session.agent.as_deref().unwrap_or("n/a");
    // `◉ working` while running (same role as the queue's `◉ running`), else the queue's outcome cell.
    let (outcome, state_role) = queue_state(session);
    let state = if session.running { "◉ working".to_string() } else { outcome };

    let rows: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("model   ".to_string(), theme.fg(Role::Dim)),
            Span::styled(format!("{model} · {agent}"), theme.fg(Role::Text)),
        ]),
        Line::from(vec![
            Span::styled("tokens  ".to_string(), theme.fg(Role::Dim)),
            Span::styled(
                format!(
                    "in {}  out {}  cache {}",
                    count(session.tokens_in),
                    count(session.tokens_out),
                    count(session.cache_read)
                ),
                theme.fg(Role::Text),
            ),
        ]),
        Line::from(vec![
            Span::styled("state   ".to_string(), theme.fg(Role::Dim)),
            Span::styled(state, theme.fg(state_role).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("time    ".to_string(), theme.fg(Role::Dim)),
            Span::styled(duration(session_time(snapshot, session)), theme.fg(Role::Text)),
        ]),
    ];

    for (i, row) in rows.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        buf.set_line(inner.x, y, row, inner.width);
    }
}

/// The session age: `fetched - created` while running, else `idle - created` (or `fetched - created`).
fn session_time(snapshot: &OpenCodeSnapshot, session: &Session) -> u64 {
    let secs = if session.running {
        snapshot.fetched_ms.saturating_sub(session.created_ms) / 1000
    } else if let Some(idle) = session.idle_ms {
        idle.saturating_sub(session.created_ms) / 1000
    } else {
        snapshot.fetched_ms.saturating_sub(session.created_ms) / 1000
    };
    secs.max(0) as u64
}

/// The timeline: the last steps that fit, oldest on top, each `<time> <glyph> <summary>  <detail>`.
fn render_timeline(app: &App, snapshot: &OpenCodeSnapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let block = panel("Timeline", Role::Server, true, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height < 1 || inner.width < 3 {
        return;
    }
    set_right(buf, area, "Timeline", "follow ●", theme.fg(Role::Good));

    // The last `rows` steps, oldest first (the data is already oldest first).
    let rows = inner.height as usize;
    let shown: Vec<&Step> = snapshot.steps.iter().skip(snapshot.steps.len().saturating_sub(rows)).collect();
    for (i, step) in shown.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        render_timeline_row(buf, inner, y, step, app.utc_offset_s, theme);
    }
}

/// One timeline row: the dim time, the coloured glyph, the summary (`Dim` for `think`) and the detail
/// (`Critical` on failure, `Good` for a successful shell, else `Dim`), cut to the panel body.
fn render_timeline_row(buf: &mut Buffer, inner: Rect, y: u16, step: &Step, utc_offset_s: i32, theme: &Theme) {
    let (glyph, glyph_role) = step_glyph(step);
    let time = Span::styled(clock::hms(step.time_ms / 1000, utc_offset_s), theme.fg(Role::Dim));
    let glyph = Span::styled(format!("{:<8}", glyph), theme.fg(glyph_role).add_modifier(Modifier::BOLD));
    // 18 cols for time+glyph, 2 for the separator; the rest splits between summary and detail.
    let value_w = inner.width.saturating_sub(20).max(1) as usize;
    let summary = Span::styled(
        fit(&step.summary, value_w),
        if step.kind == StepKind::Think { theme.fg(Role::Dim) } else { theme.fg(Role::Text) },
    );
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
    buf.set_line(
        inner.x,
        y,
        &Line::from(vec![time, Span::raw(" "), glyph, Span::raw(" "), summary, Span::raw("  "), detail]),
        inner.width,
    );
}

/// The glyph + label of a step and its colour: Read / Search in `Cpu`, Edit / Write in `Gpu`, Shell in
/// `Memory`, Think / Say in `Server`, other tools in `Text`.
pub(crate) fn step_glyph(step: &Step) -> (String, Role) {
    match &step.kind {
        StepKind::Read => ("▸ read".to_string(), Role::Cpu),
        StepKind::Search => ("⌕ grep".to_string(), Role::Cpu),
        StepKind::Edit => ("✎ edit".to_string(), Role::Gpu),
        StepKind::Write => ("✚ write".to_string(), Role::Gpu),
        StepKind::Shell => ("$ shell".to_string(), Role::Memory),
        StepKind::Think => ("◉ think".to_string(), Role::Server),
        StepKind::Say => ("» say".to_string(), Role::Server),
        StepKind::Other(name) => (format!("• {name}"), Role::Text),
    }
}

/// A right-aligned ` <text> ` on `area`'s top border, ending just before the right corner and never over
/// the `╭ <title> ` on the left (as the Log panel's indicators); omitted when it does not fit.
fn set_right(buf: &mut Buffer, area: Rect, title: &str, text: &str, style: ratatui::style::Style) {
    let padded = format!(" {text} ");
    let w = padded.chars().count() as u16;
    let left = area.x.saturating_add(title.chars().count() as u16 + 3);
    let right = area.x + area.width.saturating_sub(1);
    if right < left + w {
        return;
    }
    buf.set_line(right - w, area.y, &Line::from(Span::styled(padded, style)), w);
}
