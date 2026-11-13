// Cell-by-cell tests for the dense Overview of the Narrow tier (T39). The model has no visual access,
// so every frame is rendered into a `Buffer` and checked row by row; the golden file captures one frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, Screen};
use llamatop::collect::opencode::{OpenCodeSnapshot, parse_sessions, parse_steps};
use llamatop::ui::draw::draw;
use llamatop::ui::panels::Ctx;
use llamatop::ui::theme::{ColorMode, Theme};

/// Render `f` into a `w × h` buffer, return each row's symbols with trailing spaces removed.
fn rows<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(*buf.area(), &mut buf);
    (0..buf.area().height)
        .map(|y| (0..buf.area().width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
        .map(|s| s.trim_end().to_string())
        .collect()
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}
fn make_ctx(theme: &Theme) -> Ctx<'_> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

// --- The fixture-backed snapshot and app (the running session #136) ------------------------------
fn sessions_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/sessions.json").unwrap()
}
fn messages_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/messages.json").unwrap()
}

/// The `sessions.json` snapshot with the running session `#136` focused.
fn running_snapshot() -> OpenCodeSnapshot {
    let sessions = parse_sessions(&sessions_json()).unwrap();
    let steps = parse_steps(&messages_json(), Some("/work/.wt/demo-136")).unwrap();
    let focus = sessions.iter().find(|s| s.job == Some(136)).map(|s| s.id.clone());
    OpenCodeSnapshot { sessions, focus, steps, error: None, fetched_ms: 1794514000000 }
}

/// An app on the Overview screen, following the running session `#136`, UTC.
fn running_app() -> App {
    let mut app = App::new(1000);
    let snapshot = running_snapshot();
    app.agent_focus = snapshot.focus.clone();
    app.opencode = Some(snapshot);
    app.screen = Screen::Overview;
    app.utc_offset_s = 0;
    app
}

// --------------------------------------------------------------------------- short_badge()

#[test]
fn short_badges() {
    assert_eq!(llamatop::ui::dense::short_badge("GENERATING"), "◉ GEN");
    assert_eq!(llamatop::ui::dense::short_badge("PROMPT"), "◉ PROMPT");
    assert_eq!(llamatop::ui::dense::short_badge("IDLE"), "○ IDLE");
    assert_eq!(llamatop::ui::dense::short_badge("LOADING"), "◐ LOAD");
    assert_eq!(llamatop::ui::dense::short_badge("NO MODEL"), "○ NO MODEL");
    assert_eq!(llamatop::ui::dense::short_badge("SERVER DOWN"), "✖ DOWN");
    assert_eq!(llamatop::ui::dense::short_badge("UNAUTHORIZED"), "✖ AUTH");
    assert_eq!(llamatop::ui::dense::short_badge("WAITING"), "○ WAIT");
    assert_eq!(llamatop::ui::dense::short_badge("FOO"), "FOO");
}

// --------------------------------------------------------------------------- The dense rows

#[test]
fn dense_rows() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let rows = rows(|area, buf| draw(&demo_app(DemoState::Generating), &ctx, area, buf), 80, 24);

    // Row 2 (first under the header and rule): the badge, the model and its speed.
    assert!(rows[2].contains("◉ GEN"), "{:?}", rows);
    assert!(rows[2].contains("Cyber-Tiel-Coder-35B-A3B Q4_K_M"), "{:?}", rows);
    assert!(rows[2].contains("49.3 t/s"), "{:?}", rows);

    // The context band.
    assert!(
        rows.iter()
            .any(|r| r.contains("63.8K/196.6K") && r.contains("cache 98 %") && r.contains("draft 64 %")),
        "{:?}",
        rows
    );

    // The GPU band.
    assert!(
        rows.iter().any(|r| r.contains("GPU ")
            && r.contains("95 %")
            && r.contains("VRAM 15.9/16.0 G")
            && r.contains("73 °C")
            && r.contains("117 W")),
        "{:?}",
        rows
    );

    // The CPU band.
    assert!(
        rows.iter().any(|r| r.contains("CPU ")
            && r.contains("50 %")
            && r.contains("RAM  21.6/31.1 G")
            && r.contains("60 °C")),
        "{:?}",
        rows
    );
}

// --------------------------------------------------------------------------- The rules

#[test]
fn dense_rules() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let rows = rows(|area, buf| draw(&demo_app(DemoState::Generating), &ctx, area, buf), 80, 24);
    for &i in &[1usize, 4, 7] {
        assert!(rows[i].chars().all(|c| c == '─'), "row {i} is not a rule: {:?}", rows[i]);
    }
}

// --------------------------------------------------------------------------- The log and the keys

#[test]
fn dense_log_and_keys() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let rows = rows(|area, buf| draw(&demo_app(DemoState::Generating), &ctx, area, buf), 80, 24);
    let above_last = &rows[rows.len() - 2];
    let last = rows.last().unwrap();
    assert!(above_last.contains("all slots are idle"), "{:?}", rows);
    assert!(last.starts_with("q quit  ? help  1-5 screens"), "{:?}", rows);
}

// --------------------------------------------------------------------------- The followed step

#[test]
fn dense_agent_step() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = running_app();
    // The followed step's summary, read from the snapshot (never hard-coded). The row shows it, cut to
    // the row width, so we accept the full summary or its truncated leading part.
    let summary = app.opencode.as_ref().unwrap().steps.last().unwrap().summary.clone();
    let shown: String = summary.chars().take(40).collect();
    let rows = rows(|area, buf| draw(&app, &ctx, area, buf), 80, 24);
    let above_last = &rows[rows.len() - 2];
    assert!(above_last.contains("agent"), "{:?}", rows);
    assert!(above_last.contains(&summary) || above_last.contains(&shown), "summary not in row: {:?}", rows);
}

// --------------------------------------------------------------------------- Where it applies

#[test]
fn dense_only_on_narrow_overview() {
    let theme = true_color();
    let ctx = make_ctx(&theme);

    // 120 × 30: Medium tier, the Overview keeps its framed Model band.
    let wide = rows(|area, buf| draw(&demo_app(DemoState::Generating), &ctx, area, buf), 120, 30);
    assert!(wide[1].contains("Model"), "{:?}", wide[1]);
    assert!(!wide.iter().any(|r| r.chars().all(|c| c == '─')), "dense rules leaked into the Medium tier");

    // 80 × 24: Narrow tier, but the Models screen keeps its framed header.
    let mut models = demo_app(DemoState::Generating);
    models.screen = Screen::Models;
    let narrow = rows(|area, buf| draw(&models, &ctx, area, buf), 80, 24);
    assert!(narrow[1].contains("Models"), "{:?}", narrow[1]);
}

// --------------------------------------------------------------------------- The short body

#[test]
fn dense_short_height() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let rows = rows(|area, buf| draw(&demo_app(DemoState::Generating), &ctx, area, buf), 80, 20);
    // No row is cut mid-word at the right edge (a cut row ends with '…').
    assert!(!rows.iter().any(|r| r.ends_with('…')), "a row is cut mid-word: {:?}", rows);
    // The key row is the last one.
    assert!(rows.last().unwrap().starts_with("q quit"), "{:?}", rows);
}
