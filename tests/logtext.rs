// Compact log lines (`src/ui/logtext.rs`) and the `r` key that toggles raw (`T20`). The model has no
// visual access, so the panel and the Logs status row are rendered into a `Buffer` and checked.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use huacaya::app::demo::{self, DemoState};
use huacaya::app::{App, Event, Screen};
use huacaya::collect::logs::{LogEvent, LogLine, parse_line};
use huacaya::ui::logtext::{compact, strip_prefix};
use huacaya::ui::panels::{Ctx, log};
use huacaya::ui::screens::logs;
use huacaya::ui::theme::{ColorMode, Theme};

const FIXTURE: &str = include_str!("fixtures/journal/llama-server.txt");

/// 1-based line of the fixture, parsed.
fn fixture_line(n: usize) -> LogLine {
    parse_line(FIXTURE.lines().nth(n - 1).unwrap())
}

fn key(c: char) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char(c), crossterm::event::KeyModifiers::NONE)
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

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

fn make_ctx<'a>(theme: &'a Theme) -> Ctx<'a> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

// --------------------------------------------------------------------------- strip_prefix

#[test]
fn strip_prefix_examples() {
    assert_eq!(
        strip_prefix("slot get_availabl: id  0 | task -1 | selected slot by LCP similarity, f_keep = 1.000"),
        "selected slot by LCP similarity, f_keep = 1.000"
    );
    assert_eq!(
        strip_prefix(
            "slot print_timing: id  0 | task 9137 |       total time =   13987.46 ms /   675 tokens"
        ),
        "total time = 13987.46 ms / 675 tokens"
    );
    assert_eq!(strip_prefix("srv  update_slots: all slots are idle"), "all slots are idle");
    assert_eq!(
        strip_prefix(
            "srv    load_model: the slot context (196608) exceeds the training context of the model"
        ),
        "the slot context (196608) exceeds the training context of the model"
    );
    assert_eq!(
        strip_prefix("load_tensors: failed to allocate 2048.00 MiB on device 0: out of memory"),
        "load_tensors: failed to allocate 2048.00 MiB on device 0: out of memory"
    );
    assert_eq!(
        strip_prefix("this line has no journald prefix at all"),
        "this line has no journald prefix at all"
    );
    // Truncated lines must neither panic nor come out empty.
    assert_eq!(strip_prefix("slot release: id 0"), "id 0");
    assert_eq!(strip_prefix("slot release: id 0 | task"), "task");
    assert_eq!(strip_prefix("slot foo:"), "slot foo:");
    // `id … |` is only a slot prefix.
    assert_eq!(strip_prefix("srv  foo: id 1 | bar"), "id 1 | bar");
}

// --------------------------------------------------------------------------- compact events

#[test]
fn compact_events() {
    assert_eq!(compact(&fixture_line(4)), "n_gen 161  tg 53.0 t/s  3s 53.3 t/s");
    assert_eq!(compact(&fixture_line(8)), "prompt 48 tok  85.3 t/s");
    assert_eq!(compact(&fixture_line(9)), "gen 627 tok  46.6 t/s");
    assert_eq!(compact(&fixture_line(13)), "draft 369/771 accepted  48 %");
    assert_eq!(compact(&fixture_line(14)), "released  ctx 57.7K");
    assert_eq!(compact(&fixture_line(1)), "→ Cyber-Tiel-Coder-35B-A3B :35335");
    assert_eq!(compact(&fixture_line(34)), "POST /v1/chat/completions 401");

    // The demo's PromptProgress line at t in [9.0, 10.0).
    let demo_line = demo::log_lines(9.0, 10.0)
        .into_iter()
        .find(|l| matches!(l.event, Some(LogEvent::PromptProgress { .. })))
        .expect("a PromptProgress line in [9.0, 10.0)");
    assert_eq!(compact(&demo_line), "prompt 2048 tok  25 %  890 t/s");
}

// --------------------------------------------------------------------------- compact plain lines

#[test]
fn compact_plain_lines() {
    assert_eq!(compact(&fixture_line(3)), "processing task, is_child = 0");
    assert_eq!(compact(&fixture_line(10)), "total time = 13987.46 ms / 675 tokens");
    // Line 33 has no recognised event: compact is its text with whitespace collapsed.
    let line = fixture_line(33);
    assert_eq!(compact(&line), strip_prefix(&line.text));
}

// --------------------------------------------------------------------------- the r key

#[test]
fn raw_key() {
    for screen in [Screen::Overview, Screen::Models, Screen::Logs, Screen::System] {
        let mut app = App::new(1000);
        app.screen = screen;
        assert!(!app.log_view.raw, "raw starts false on {:?}", screen);
        app.apply(Event::Key(key('r')));
        assert!(app.log_view.raw, "r turns raw on on {:?}", screen);
        app.apply(Event::Key(key('r')));
        assert!(!app.log_view.raw, "r turns raw off on {:?}", screen);
    }
}

// --------------------------------------------------------------------------- the Log panel rows

#[test]
fn log_panel_rows() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo::app(DemoState::Generating);

    let rows = draw(|area, buf| log::render(&app, &ctx, area, buf), 70, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("10:00:32 #9000 released  ctx 9.2K"), "{:?}", rows);
    let last_inner = rows.len() - 2;
    assert!(rows[last_inner].contains("10:00:32 all slots are idle"), "{:?}", rows[last_inner]);

    // Raw: the full `srv  update_slots: …` line, and no `#<task>` column anywhere.
    let mut raw = demo::app(DemoState::Generating);
    raw.log_view.raw = true;
    let rows = draw(|area, buf| log::render(&raw, &ctx, area, buf), 70, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("srv  update_slots: all slots are idle"), "{:?}", rows);
    assert!(!all.contains("#9000"), "{:?}", rows);
    let last_inner = rows.len() - 2;
    assert!(rows[last_inner].contains("srv  update_slots: all slots are idle"), "{:?}", rows[last_inner]);
}

// --------------------------------------------------------------------------- the Logs status row

#[test]
fn logs_status_raw() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo::app(DemoState::Generating);
    // The status row + its right-aligned key hint need >= 139 columns to show the hint; 140 keeps
    // `r raw` visible (the sheet's 130 is too narrow for the now-longer row). Height < 12 forces the
    // v1 layout, where the status row is the last line (at 90+ cols the screen is v2 and the hint row
    // is last instead).
    let rows = draw(|area, buf| logs::render(&app, &ctx, area, buf), 140, 11);
    let last = rows.last().unwrap();
    assert!(last.contains("lines compact"), "{:?}", last);
    assert!(last.contains("r raw"), "{:?}", last);

    let mut raw = demo::app(DemoState::Generating);
    raw.log_view.raw = true;
    let rows = draw(|area, buf| logs::render(&raw, &ctx, area, buf), 140, 11);
    assert!(rows.last().unwrap().contains("lines raw"), "{:?}", rows.last());
}
