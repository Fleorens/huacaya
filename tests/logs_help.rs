// Cell-by-cell tests for the Logs screen and the help overlay (T17). The model has no visual access,
// so every screen is rendered into a `Buffer` and checked row by row; the golden files capture a frame.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::keys::handle;
use llamatop::app::{App, LevelFilter, Screen};
use llamatop::collect::logs::Level;
use llamatop::ui::draw;
use llamatop::ui::panels::Ctx;
use llamatop::ui::screens::logs;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::text;
use llamatop::ui::theme::{ColorMode, Theme};

/// One key press, as `keys::handle` expects.
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Render the Logs screen into a `Buffer` (not just its symbols), so a cell's style can be inspected.
fn app_buffer(app: &App, w: u16, h: u16) -> Buffer {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    logs::render(app, &ctx, Rect::new(0, 0, w, h), &mut buf);
    buf
}

/// Render `f` into a `w × h` buffer and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

/// The shared `Ctx`: true-colour theme, graphs on, the fixed clock.
fn make_ctx<'a>(theme: &'a Theme) -> Ctx<'a> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

/// The Logs screen alone, into a `w × h` buffer.
fn body(app: &App, w: u16, h: u16) -> Vec<String> {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    draw(|area, buf| logs::render(app, &ctx, area, buf), w, h)
}

/// The whole frame, through `draw::draw`.
fn frame(app: &App, w: u16, h: u16) -> Vec<String> {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    draw(|area, buf| draw::draw(app, &ctx, area, buf), w, h)
}

// --------------------------------------------------------------------------- level_name()

#[test]
fn level_names() {
    assert_eq!(logs::level_name(LevelFilter::All), "all");
    assert_eq!(logs::level_name(LevelFilter::WarnAndErrors), "warn+err");
    assert_eq!(logs::level_name(LevelFilter::Errors), "errors");
}

// --------------------------------------------------------------------------- The status row

#[test]
fn logs_status_row() {
    let app = demo_app(DemoState::Generating);
    // 80 cols would truncate the count and drop the hint, so keep the width and drop the height below
    // 12: the screen is still v1 (the status row renders as today), and every segment fits.
    let rows = body(&app, 140, 10);
    let last = rows.last().unwrap();
    assert!(last.contains("follow ●"), "{:?}", last);
    assert!(last.contains("level all"), "{:?}", last);
    assert!(last.contains("filter none"), "{:?}", last);
    assert!(last.contains("proxy hidden"), "{:?}", last);
    assert!(last.contains("lines compact"), "{:?}", last);
    assert!(last.contains("17 of 18 lines"), "{:?}", last);
    assert!(last.contains("r raw"), "{:?}", last);
    // The Activity panel title is on row 0.
    assert!(rows[0].contains("Activity"), "{:?}", rows[0]);
}

#[test]
fn logs_status_states() {
    let mut app = demo_app(DemoState::Generating);
    app.log_view.follow = false;
    app.log_view.scroll = 2;
    app.log_view.level = LevelFilter::Errors;
    app.log_view.filter = "slot".to_string();
    app.log_view.show_proxy = true;
    // v1 via height < 12 (see logs_status_row): the status row renders as today at this width.
    let rows = body(&app, 120, 10);
    let last = rows.last().unwrap();
    assert!(last.contains("scroll −2"), "{:?}", last);
    assert!(last.contains("level errors"), "{:?}", last);
    assert!(last.contains("filter \"slot\""), "{:?}", last);
    assert!(last.contains("proxy shown"), "{:?}", last);
}

#[test]
fn logs_editing_filter() {
    let mut app = demo_app(DemoState::Generating);
    app.log_view.editing_filter = true;
    app.log_view.filter = "n_g".to_string();
    let rows = body(&app, 80, 24);
    let last = rows.last().unwrap();
    assert!(last.starts_with("/n_g▏"), "{:?}", last);
    assert!(last.contains("Esc clear"), "{:?}", last);
    assert!(!last.contains("level"), "{:?}", last);
}

#[test]
fn logs_narrow() {
    let app = demo_app(DemoState::Generating);
    let rows = body(&app, 40, 10);
    let last = rows.last().unwrap();
    assert!(last.contains("follow ●"), "{:?}", last);
    assert!(!last.contains("x proxy"), "{:?}", last);
    // No panic at the extremes.
    let _ = body(&app, 1, 1);
    let _ = body(&app, 0, 0);
}

// --------------------------------------------------------------------------- The help overlay

#[test]
fn help_overlay() {
    let mut app = demo_app(DemoState::Generating);
    app.help = true;
    let rows = frame(&app, 100, 30);
    // The box is centred: top-left corner `╭` at ((100 - 56) / 2, (30 - 15) / 2) = (22, 7).
    let corner: String = rows[7].chars().nth(22).map(|c| c.to_string()).unwrap_or_default();
    assert_eq!(corner, "╭", "{:?}", rows[7]);
    let all: String = rows.join("\n");
    assert!(all.contains("Help"), "{:?}", all);
    assert!(all.contains("switch screen"), "{:?}", all);
    assert!(all.contains("faster / slower refresh (now 1.0 s)"), "{:?}", all);
    assert!(all.contains("Logs: tasks list, Enter shows one task"), "{:?}", all);
    assert!(all.contains("show / hide the router's proxy lines"), "{:?}", all);

    let mut fast = demo_app(DemoState::Generating);
    fast.help = true;
    fast.interval_ms = 500;
    let rows = frame(&fast, 100, 30);
    let all: String = rows.join("\n");
    assert!(all.contains("faster / slower refresh (now 0.5 s)"), "{:?}", all);
}

#[test]
fn help_small() {
    let mut app = demo_app(DemoState::Generating);
    app.help = true;
    let rows = frame(&app, 40, 12);
    let all: String = rows.join("\n");
    assert!(all.contains("Help"), "{:?}", all);
    // No panic at 30 × 8.
    let _ = frame(&app, 30, 8);
}

// --------------------------------------------------------------------------- Golden files

#[test]
fn logs_and_help_golden() {
    let theme = Theme::new(ColorMode::TrueColor);

    let mut logs_app = demo_app(DemoState::Generating);
    logs_app.screen = Screen::Logs;
    let logs_path = "tests/snapshots/logs-120x30.txt";
    let logs_fresh = snapshot(&logs_app, &theme, 120, 30, false);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(logs_path, &logs_fresh).unwrap();
    } else {
        let expected = std::fs::read_to_string(logs_path).unwrap();
        assert_eq!(expected, logs_fresh, "golden file {logs_path} is stale");
    }

    let mut help_app = demo_app(DemoState::Generating);
    help_app.help = true;
    let help_path = "tests/snapshots/help-100x30.txt";
    let help_fresh = snapshot(&help_app, &theme, 100, 30, false);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(help_path, &help_fresh).unwrap();
    } else {
        let expected = std::fs::read_to_string(help_path).unwrap();
        assert_eq!(expected, help_fresh, "golden file {help_path} is stale");
    }
}

// --------------------------------------------------------------------------- The Logs screen v2

#[test]
fn logs_v2_layout() {
    let app = demo_app(DemoState::Generating);
    let rows = body(&app, 120, 30);
    let all: String = rows.join("\n");
    // The four sections are on screen: the three panel titles and the Filter section.
    assert!(all.contains("Levels"), "{:?}", all);
    assert!(all.contains("Tasks"), "{:?}", all);
    assert!(all.contains("Log"), "{:?}", all);
    assert!(all.contains("Filter"), "{:?}", all);
    // The log header is the `time  task  lvl  message` row.
    assert!(all.contains("time      task      lvl  message"), "{:?}", all);
    // A log row shows the task and a generation line.
    assert!(rows.iter().any(|r| r.contains("#9000") && r.contains("n_gen 156")), "{:?}", all);
}

#[test]
fn logs_v2_short_bodies() {
    // From 12 rows the v2 layout is used: the short heights must not panic, focused or not.
    let mut app = demo_app(DemoState::Generating);
    for focus in [false, true] {
        app.log_view.tasks_focus = focus;
        for w in [90, 100, 120] {
            for h in 12..=17 {
                let _ = body(&app, w, h);
            }
        }
    }
}

#[test]
fn logs_levels_counts() {
    let app = demo_app(DemoState::Generating);
    let count = |level: Level| app.logs.iter().filter(|l| l.level == level).count();
    let info = count(Level::Info);
    let warn = count(Level::Warn);
    let error = count(Level::Error);
    let rows = body(&app, 120, 30);
    // Each count (in group_thousands) is written beside its level on its row.
    let gth = |n: usize| text::group_thousands(n as u64);
    let info_row = rows.iter().find(|r| r.contains("info")).map_or("", |v| v.as_str());
    let warn_row = rows.iter().find(|r| r.contains("warn")).map_or("", |v| v.as_str());
    let error_row = rows.iter().find(|r| r.contains("error")).map_or("", |v| v.as_str());
    assert!(info_row.contains(&gth(info)), "{:?}", info_row);
    assert!(warn_row.contains(&gth(warn)), "{:?}", warn_row);
    assert!(error_row.contains(&gth(error)), "{:?}", error_row);
}

#[test]
fn logs_task_filter() {
    let mut app = demo_app(DemoState::Generating);
    app.screen = Screen::Logs;
    // Focus the tasks list, then pick the first task: it becomes the visible task.
    handle(&mut app, key(KeyCode::Char('t')));
    assert!(app.log_view.tasks_focus);
    handle(&mut app, key(KeyCode::Enter));
    assert_eq!(app.log_view.task, Some(9000));
    assert!(app.visible_logs().iter().all(|l| l.task == Some(9000)));
    let rows = body(&app, 120, 30);
    assert!(rows.join("\n").contains("task   #9000"), "{:?}", rows);
    // Enter again clears the task.
    handle(&mut app, key(KeyCode::Enter));
    assert_eq!(app.log_view.task, None);
    // Esc clears it too.
    app.log_view.task = Some(1234);
    handle(&mut app, key(KeyCode::Esc));
    assert_eq!(app.log_view.task, None);
}

#[test]
fn logs_filter_highlight() {
    let mut app = demo_app(DemoState::Generating);
    app.log_view.filter = "n_gen".to_string();
    let buf = app_buffer(&app, 120, 30);
    // Row 0 is the panel borders: the Log border shows the filter `/n_gen` as a right indicator, not
    // reversed, so skip it. Row 1 is the column header. The highlight lives in the log body, from row 2.
    // The first `n_gen` shown in the log body is drawn REVERSED.
    let needle = "n_gen";
    let n = needle.chars().count();
    let mut found = false;
    for y in 2..30u16 {
        // This row's symbols, one per cell (a glyph is one cell, though several bytes).
        let symbols: Vec<String> = (0..120u16).map(|x| buf[(x, y)].symbol().to_string()).collect();
        for start in 0..(symbols.len().saturating_sub(n)) {
            let run: String = symbols[start..start + n].iter().map(String::as_str).collect();
            if run == needle {
                let sx = start as u16;
                for i in 0..n as u16 {
                    assert!(
                        buf[(sx + i, y)].style().add_modifier.contains(Modifier::REVERSED),
                        "cell ({}, {}) of \"n_gen\" is not REVERSED: {:?}",
                        sx + i,
                        y,
                        buf[(sx + i, y)].symbol()
                    );
                }
                found = true;
                break;
            }
        }
        if found {
            break;
        }
    }
    assert!(found, "no \"n_gen\" row found");
}
