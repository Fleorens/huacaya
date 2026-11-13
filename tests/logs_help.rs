// Cell-by-cell tests for the Logs screen and the help overlay (T17). The model has no visual access,
// so every screen is rendered into a `Buffer` and checked row by row; the golden files capture a frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, LevelFilter, Screen};
use llamatop::ui::draw;
use llamatop::ui::panels::Ctx;
use llamatop::ui::screens::logs;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::theme::{ColorMode, Theme};

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
    let rows = body(&app, 140, 20);
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
    let rows = body(&app, 120, 20);
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
    let rows = body(&app, 120, 20);
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
