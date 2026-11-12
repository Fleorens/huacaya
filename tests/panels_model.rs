// Cell-by-cell tests for the text helpers and the server-side Overview panels (T12). The model has no
// visual access, so every panel is rendered into a `Buffer` and checked row by row.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, Screen};
use llamatop::ui::panels::model::badge_state;
use llamatop::ui::panels::{Ctx, header, model, request};
use llamatop::ui::text::{bytes_pair, fit, pct, temp, tps};
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

// --------------------------------------------------------------------------- Text helpers

#[test]
fn text_helpers() {
    assert_eq!(tps(Some(62.54)), "62.5");
    assert_eq!(tps(Some(643.2)), "643");
    assert_eq!(tps(None), "n/a");
    assert_eq!(pct(Some(96.4)), "96 %");
    assert_eq!(temp(Some(70.4)), "70 °C");
    assert_eq!(fit("abcdef", 4), "abc…");
    assert_eq!(fit("abc", 4), "abc");
    assert_eq!(fit("héllo", 3), "hé…");
    assert_eq!(fit("abc", 0), "");
    assert_eq!(bytes_pair(15_894_315_008, 17_163_091_968), "14.8/16.0 G");
    assert_eq!(bytes_pair(1_310_720, 8_589_930_496), "0.0/8.0 G");
    assert_eq!(bytes_pair(500, 1000), "500/1000 B");
}

// --------------------------------------------------------------------------- Header

#[test]
fn header_generating() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| header::render(&app, &ctx, area, buf), 100, 3);
    assert!(rows[0].contains("llamatop ─ aibox-flo ─ up 4d 3h"), "{:?}", rows[0]);
    assert!(rows[0].contains("● server ok  b12151 ─ 10:21:03"), "{:?}", rows[0]);
    assert!(rows[0].ends_with('╮'), "{:?}", rows[0]);
    assert!(rows[1].contains("1 Overview"), "{:?}", rows[1]);
    assert!(rows[1].contains("4 System"), "{:?}", rows[1]);
    assert!(rows[1].contains("? help"), "{:?}", rows[1]);
}

#[test]
fn header_states() {
    let theme = true_color();

    let ctx = make_ctx(&theme);
    let app = App::new(1000);
    let rows = draw(|area, buf| header::render(&app, &ctx, area, buf), 60, 3);
    assert!(rows[0].contains("○ waiting"), "{:?}", rows[0]);

    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Down);
    let rows = draw(|area, buf| header::render(&app, &ctx, area, buf), 60, 3);
    assert!(rows[0].contains("● server down"), "{:?}", rows[0]);

    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);
    app.paused = true;
    let rows = draw(|area, buf| header::render(&app, &ctx, area, buf), 100, 3);
    assert!(rows[1].contains("PAUSED"), "{:?}", rows[1]);
}

#[test]
fn header_current_tab() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Idle);
    app.screen = Screen::Logs;
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 3));
    header::render(&app, &ctx, Rect::new(0, 0, 100, 3), &mut buf);
    let row1 = (0..100).map(|x| buf[(x, 1)].symbol().to_string()).collect::<String>();
    let i3 = row1.find("3 Logs").unwrap();
    let i1 = row1.find("1 Overview").unwrap();
    assert!(buf[(i3 as u16, 1)].modifier.contains(Modifier::REVERSED));
    assert!(!buf[(i1 as u16, 1)].modifier.contains(Modifier::REVERSED));
}

// --------------------------------------------------------------------------- Badge

#[test]
fn badge_states() {
    assert_eq!(badge_state(&demo_app(DemoState::Idle)).0, "IDLE");
    assert_eq!(badge_state(&demo_app(DemoState::Prompt)).0, "PROMPT");
    assert_eq!(badge_state(&demo_app(DemoState::Generating)).0, "GENERATING");
    assert_eq!(badge_state(&demo_app(DemoState::Loading)).0, "LOADING");
    assert_eq!(badge_state(&demo_app(DemoState::Down)).0, "SERVER DOWN");
    assert_eq!(badge_state(&App::new(1000)).0, "WAITING");
}

// --------------------------------------------------------------------------- Model

#[test]
fn model_generating() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| model::render(&app, &ctx, area, buf), 60, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("Cyber-Tiel-Coder-35B-A3B   Q4_K_M"), "{:?}", rows);
    assert!(all.contains("◉ GENERATING   task 9000"), "{:?}", rows);
    assert!(all.contains("prompt 591 t/s   cache 98 %"), "{:?}", rows);
    assert!(all.contains("63.8K / 196.6K"), "{:?}", rows);
    assert!(all.contains("draft  64 %"), "{:?}", rows);
    assert!(all.contains("n-cpu-moe 21"), "{:?}", rows);

    // The gen row has ` t/s` and no ` avg`.
    let gen_row = rows.iter().find(|r| r.contains("gen    ")).unwrap();
    assert!(gen_row.contains(" t/s") && !gen_row.contains(" avg"), "{:?}", gen_row);
}

#[test]
fn model_idle() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Idle);
    let rows = draw(|area, buf| model::render(&app, &ctx, area, buf), 60, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("◉ IDLE"), "{:?}", rows);
    assert!(all.contains("gen    56.7 t/s avg"), "{:?}", rows);
    assert!(all.contains("9.2K / 196.6K"), "{:?}", rows);
}

#[test]
fn model_without_active() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Down);
    let rows = draw(|area, buf| model::render(&app, &ctx, area, buf), 60, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("SERVER DOWN"), "{:?}", rows);
    assert!(all.contains("Connection refused"), "{:?}", rows);

    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Loading);
    let rows = draw(|area, buf| model::render(&app, &ctx, area, buf), 60, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("LOADING"), "{:?}", rows);
    assert!(all.contains("loading Cyber-Tiel-Coder-35B-A3B…"), "{:?}", rows);
}

#[test]
fn model_graph_toggle() {
    let theme = true_color();
    let app = demo_app(DemoState::Generating);

    let ctx = make_ctx(&theme);
    let rows = draw(|area, buf| model::render(&app, &ctx, area, buf), 60, 8);
    let gen_row = rows.iter().find(|r| r.contains("gen    ")).unwrap();
    assert!(gen_row.chars().any(|c| (0x2800..=0x28ff).contains(&(c as u32))), "{:?}", gen_row);

    let graphs_false = Ctx { theme: &theme, graphs: false, clock: "10:21:03" };
    let rows = draw(|area, buf| model::render(&app, &graphs_false, area, buf), 60, 8);
    let gen_row = rows.iter().find(|r| r.contains("gen    ")).unwrap();
    assert!(!gen_row.chars().any(|c| (0x2800..=0x28ff).contains(&(c as u32))), "{:?}", gen_row);
}

// --------------------------------------------------------------------------- Request

#[test]
fn request_panel() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| request::render(&app, &ctx, area, buf), 50, 5);
    let all: String = rows.join("\n");
    assert!(all.contains("prompt 8192 tok   891 t/s"), "{:?}", rows);
    assert!(all.contains("gen    1040 tok   52.0 t/s"), "{:?}", rows);
    assert!(all.contains("draft  70 %   ctx 9.2K   at 10:00:12"), "{:?}", rows);

    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = App::new(1000);
    let rows = draw(|area, buf| request::render(&app, &ctx, area, buf), 50, 5);
    let all: String = rows.join("\n");
    assert!(all.contains("no request yet"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Bounds

#[test]
fn panels_stay_inside() {
    let theme = true_color();
    let states =
        [DemoState::Idle, DemoState::Prompt, DemoState::Generating, DemoState::Loading, DemoState::Down];
    for state in states {
        let app = demo_app(state);
        let area = Rect::new(5, 2, 30, 6);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        header::render(&app, &ctx, area, &mut buf);
        check_inside(&buf);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        model::render(&app, &ctx, area, &mut buf);
        check_inside(&buf);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        request::render(&app, &ctx, area, &mut buf);
        check_inside(&buf);
    }
}

/// Every cell outside the 30 × 6 area at (5, 2) is still a space.
fn check_inside(buf: &Buffer) {
    for y in 0..10 {
        for x in 0..40 {
            let inside = (5..35).contains(&x) && (2..8).contains(&y);
            if !inside {
                assert_eq!(buf[(x, y)].symbol(), " ", "cell ({x},{y})");
            }
        }
    }
}
