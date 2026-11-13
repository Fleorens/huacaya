//! The three left-hand panels of the Logs screen v2: Levels, Filter and Tasks (T37).
//!
//! Below 90 columns or 12 rows the Logs screen is as before (see `logs::render`); at 90 × 12 and up
//! these draw the 32-column left band, from the top: Levels (a panel, 6 rows), Filter (7 rows) and the
//! Tasks list (a panel, the rest). Each function only draws inside the `area` it is given.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::app::App;
use crate::app::log_tasks::{TaskOutcome, log_tasks};
use crate::collect::logs::{Level, LogEvent};
use crate::ui::frame::panel;
use crate::ui::panels::Ctx;
use crate::ui::screens::logs::level_name;
use crate::ui::text::group_thousands;
use crate::ui::theme::{Role, Theme};
use crate::util::{count, duration};

/// The Levels panel: over **all** of `app.logs`, `● info`, `▲ warn`, `✖ error` with each count in
/// `group_thousands` right-aligned to 7 columns, then the hidden/shown proxy count.
fn render_levels(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let block = panel("Levels", Role::Server, false, theme);
    let inner = block.inner(area);
    block.render(area, buf);

    let mut counts = [0u64, 0u64, 0u64];
    let mut proxy = 0u64;
    for line in &app.logs {
        match line.level {
            Level::Info => counts[0] += 1,
            Level::Warn => counts[1] += 1,
            Level::Error => counts[2] += 1,
        }
        if matches!(line.event, Some(LogEvent::Proxy { .. })) {
            proxy += 1;
        }
    }

    let glyphs = ["●", "▲", "✖"];
    let labels = ["info", "warn", "error"];
    let roles = [Role::Text, Role::Warning, Role::Critical];
    for (i, y) in (inner.y..inner.bottom()).enumerate() {
        if i >= 4 {
            break;
        }
        if i < 3 {
            let row = format!("{} {:<7}{:>7}", glyphs[i], labels[i], group_thousands(counts[i]));
            buf.set_line(inner.x, y, &Line::from(Span::styled(row, theme.fg(roles[i]))), inner.width);
        } else {
            let state = if app.log_view.show_proxy { "shown" } else { "hidden" };
            let row = format!("  proxy  {:>7}  {}", group_thousands(proxy), state);
            buf.set_line(inner.x, y, &Line::from(Span::styled(row, theme.fg(Role::Dim))), inner.width);
        }
    }
}

/// One Filter row: the `Dim` label padded to 7, then the value span.
fn filter_row(buf: &mut Buffer, x: u16, y: u16, label: &str, value: Span, width: u16, theme: &Theme) {
    buf.set_span(x, y, &Span::styled(format!("{:<7}", label), theme.fg(Role::Dim)), 7);
    buf.set_span(x + 7, y, &value, width.saturating_sub(7));
}

/// The Filter section (7 rows, no border): the `Filter` title, then the `text`, `task`, `level`, `lines` and `shown` rows,
/// labels `Dim` padded to 7, starting one row below the section top.
fn render_filter(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let width = area.width;
    let lv = &app.log_view;

    let title = Span::styled("Filter", theme.fg(Role::Server).add_modifier(ratatui::style::Modifier::BOLD));
    buf.set_span(area.x + 1, area.y, &title, width.saturating_sub(1));

    filter_row(
        buf,
        area.x,
        area.y + 1,
        "text",
        if lv.editing_filter {
            Span::styled(format!("/{}▏", lv.filter), theme.fg(Role::Warning))
        } else if lv.filter.is_empty() {
            Span::styled("none", theme.fg(Role::Dim))
        } else {
            Span::styled(format!("/{}", lv.filter), theme.fg(Role::Warning))
        },
        width,
        theme,
    );

    let task_value = match lv.task {
        Some(task) => Span::styled(format!("#{task}"), theme.fg(Role::Warning)),
        None => Span::styled("all", theme.fg(Role::Text)),
    };
    filter_row(buf, area.x, area.y + 2, "task", task_value, width, theme);

    filter_row(
        buf,
        area.x,
        area.y + 3,
        "level",
        Span::styled(level_name(lv.level), theme.fg(Role::Text)),
        width,
        theme,
    );

    filter_row(
        buf,
        area.x,
        area.y + 4,
        "lines",
        Span::styled(if lv.raw { "raw" } else { "compact" }, theme.fg(Role::Text)),
        width,
        theme,
    );

    let visible = app.visible_logs().len();
    let total = app.logs.len();
    filter_row(
        buf,
        area.x,
        area.y + 5,
        "shown",
        Span::styled(
            format!("{} of {}", group_thousands(visible as u64), group_thousands(total as u64)),
            theme.fg(Role::Text),
        ),
        width,
        theme,
    );
}

/// Write a short `Dim` title right-aligned on the top border of `area`, before the right corner.
fn right_title(buf: &mut Buffer, area: Rect, title: &str, theme: &Theme) {
    let right = area.x + area.width.saturating_sub(1);
    let w = title.chars().count() as u16;
    let x = right.saturating_sub(w);
    for (i, ch) in title.chars().enumerate() {
        if let Some(cell) = buf.cell_mut((x + i as u16, area.y)) {
            cell.set_char(ch);
            if let Some(color) = theme.color(Role::Dim) {
                cell.fg = color;
            }
        }
    }
}

/// The Tasks panel: one row per `log_tasks(&app.logs, 50)` that fits, scrolled so `task_selected` stays
/// visible — a `▶` on the selected row, the task, its outcome glyph, its duration and its token count.
/// No task: one `Dim` row `no task yet`.
fn render_tasks(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let theme = ctx.theme;
    let block = panel("Tasks", Role::Server, app.log_view.tasks_focus, theme);
    let inner = block.inner(area);
    block.render(area, buf);
    right_title(buf, area, " Enter: filter ", theme);

    let tasks = log_tasks(app.logs.iter(), 50);
    if tasks.is_empty() {
        buf.set_line(
            inner.x,
            inner.y,
            &Line::from(Span::styled("no task yet", theme.fg(Role::Dim))),
            inner.width,
        );
        return;
    }

    // Show the list scrolled so `task_selected` stays visible (anchored to the bottom of the view).
    let visible_count = inner.height as usize;
    if visible_count == 0 {
        return;
    }
    let selected_idx = app.log_view.task_selected.min(tasks.len() - 1);
    let max_offset = tasks.len().saturating_sub(visible_count);
    let offset = if tasks.len() > visible_count {
        selected_idx.saturating_sub(visible_count - 1).min(max_offset)
    } else {
        0
    };

    for (j, summary) in tasks[offset..].iter().enumerate() {
        let y = inner.y + j as u16;
        if y >= inner.bottom() {
            break;
        }
        let selected = (offset + j) == selected_idx;
        let shown = app.log_view.task == Some(summary.task);
        let style = |role: Role| -> ratatui::style::Style {
            let mut s = theme.fg(role);
            if shown {
                s = s.add_modifier(ratatui::style::Modifier::BOLD);
            }
            s
        };
        let marker = if selected { "▶ " } else { "  " };
        let task_col = format!("{:<9}", format!("#{}", summary.task));
        let (glyph, glyph_role) = match summary.outcome {
            TaskOutcome::Running => ("◉", Role::Server),
            TaskOutcome::Done => ("✓", Role::Good),
            TaskOutcome::Failed => ("✖", Role::Critical),
        };
        let duration = match summary.seconds {
            Some(s) => format!("{:>7}", duration(s)),
            None => format!("{:>7}", "-"),
        };
        let tokens = match summary.tokens {
            Some(t) => format!("{:>10}", format!("{} {}", count(t), "tok")),
            None => String::new(),
        };
        let spans = [
            Span::styled(marker.to_string(), style(Role::Server)),
            Span::styled(task_col, style(Role::Dim)),
            Span::styled(glyph.to_string(), style(glyph_role)),
            Span::styled(duration, style(Role::Text)),
            Span::styled(tokens, style(Role::Text)),
        ];
        buf.set_line(inner.x, y, &Line::from(spans.to_vec()), inner.width);
    }
}

/// The left band at `area`: Levels, then Filter, then Tasks.
pub fn render_side(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer) {
    let left_w = 32u16;
    // A short body (12 to 16 rows) cannot hold all three: draw only the sections that fit whole.
    let levels_area = Rect::new(area.x, area.y, left_w, area.height.min(6));
    render_levels(app, ctx, levels_area, buf);
    if area.height >= 13 {
        render_filter(app, ctx, Rect::new(area.x, area.y + 6, left_w, 7), buf);
    }
    if area.height >= 16 {
        render_tasks(app, ctx, Rect::new(area.x, area.y + 13, left_w, area.height - 13), buf);
    }
}
