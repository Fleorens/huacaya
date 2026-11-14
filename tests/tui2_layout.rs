// The TUI v2 Overview skeleton (T32): the one-line header, the key bar, the Requests table and the
// `group_thousands` helper. The model has no visual access, so every panel is rendered into a
// `Buffer` and checked row by row.

use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use huacaya::app::{App, Event};
use huacaya::ui::keybar;
use huacaya::ui::panels::{Ctx, request};
use huacaya::ui::text::group_thousands;
use huacaya::ui::theme::{ColorMode, Theme};

/// Render `f` into a `w × h` buffer and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

fn make_ctx<'a>(theme: &'a Theme) -> Ctx<'a> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

// --------------------------------------------------------------------------- group_thousands

#[test]
fn thousands() {
    assert_eq!(group_thousands(0), "0");
    assert_eq!(group_thousands(999), "999");
    assert_eq!(group_thousands(1043), "1 043");
    assert_eq!(group_thousands(1_234_567), "1 234 567");
}

// --------------------------------------------------------------------------- Key bar

#[test]
fn keybar_text() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = App::new(500);
    let rows = draw(|area, buf| keybar::render(&app, &ctx, area, buf), 100, 1);
    assert!(rows[0].contains("+/- refresh 0.5 s"), "{:?}", rows[0]);
    assert!(rows[0].contains("x proxy"), "{:?}", rows[0]);
}

// --------------------------------------------------------------------------- Requests table

/// The fixture journal, fed line by line into a fresh `App`.
fn journal_app() -> App {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journal/llama-server.txt");
    let content = std::fs::read_to_string(path).expect("fixture present");
    let mut app = App::new(1000);
    for line in content.split('\n') {
        if line.is_empty() {
            continue;
        }
        let log_line = huacaya::collect::logs::parse_line(line);
        app.apply(Event::Log(log_line));
    }
    app
}

#[test]
fn requests_newest_first() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = journal_app();
    let rows = draw(|area, buf| request::render(&app, &ctx, area, buf), 50, 6);
    let newest = rows.iter().position(|r| r.contains("10 229")).unwrap();
    let oldest = rows.iter().position(|r| r.contains("48")).unwrap();
    assert!(newest < oldest, "the newest request (10 229) is above the oldest (48): {:?}", rows);
}

#[test]
fn request_no_yet() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = App::new(1000);
    let rows = draw(|area, buf| request::render(&app, &ctx, area, buf), 50, 6);
    let all: String = rows.join("\n");
    assert!(all.contains("no request yet"), "{:?}", rows);
}
