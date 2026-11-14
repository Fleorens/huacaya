// Cell-by-cell tests for the Agents map view (T44). The model has no visual access, so the screen is
// rendered into a `Buffer` and checked row by row; the golden file captures the whole frame.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use huacaya::app::{App, Event, Screen};
use huacaya::collect::activity::Activity;
use huacaya::collect::llama_poll::{Health, LlamaSnapshot};
use huacaya::collect::opencode::{OpenCodeSnapshot, Step, StepKind, parse_sessions, parse_steps};
use huacaya::ui::panels::Ctx;
use huacaya::ui::screens::agents;
use huacaya::ui::snapshot::snapshot;
use huacaya::ui::theme::{ColorMode, Role, Theme};

// --- Key + snapshot builders (the sheet forbids these in `src/`) -------------------------------
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}
fn make_ctx(theme: &Theme) -> Ctx<'_> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

/// Render `f` over a `w × h` area and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

/// Render the Agents screen into a `Buffer` so its cell styles can be inspected.
fn render_buf(app: &App, w: u16, h: u16) -> Buffer {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    agents::render(app, &ctx, Rect::new(0, 0, w, h), &mut buf);
    buf
}

// --- The fixture-backed snapshot, real steps and the T43 activity (T44) -------------------------
fn sessions_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/sessions.json").unwrap()
}
fn messages_real_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/messages-real.json").unwrap()
}

/// A `Step` built for the tests.
fn step(
    id: &str,
    kind: StepKind,
    summary: &str,
    time_ms: i64,
    end_ms: Option<i64>,
    ok: Option<bool>,
) -> Step {
    Step { time_ms, kind, id: id.to_string(), end_ms, summary: summary.to_string(), ok, detail: None }
}

/// The `sessions.json` snapshot with the real steps: the followed session is the done one (`#131`).
fn fixture_snapshot() -> OpenCodeSnapshot {
    let sessions = parse_sessions(&sessions_json()).unwrap();
    let steps = parse_steps(&messages_real_json(), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    let focus = sessions.iter().find(|s| s.job == Some(131)).map(|s| s.id.clone());
    OpenCodeSnapshot { sessions, focus, steps, error: None, fetched_ms: 1794514000000, ..Default::default() }
}

/// The T43 test activity: a history round at 1794586720000, then a live round with the running `cargo
/// test` step `x1` at 1794586722000.
fn build_activity() -> Activity {
    let steps = parse_steps(&messages_real_json(), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    let mut a = Activity::default();
    a.update("s1", &steps, 1794586720000);
    a.update("s1", &[step("x1", StepKind::Shell, "cargo test", 1794586721000, None, None)], 1794586722000);
    a
}

/// A snapshot with the real steps and the T43 activity; an app on the Agents screen, following `#131`,
/// the map on, wall-clock `now_ms` set, no llama snapshot.
fn map_app() -> App {
    let mut snapshot = fixture_snapshot();
    snapshot.activity = build_activity();
    let mut app = App::new(1000);
    let focus = snapshot.focus.clone().unwrap();
    app.agent_selected = snapshot.sessions.iter().position(|s| s.id == focus).unwrap();
    app.opencode = Some(snapshot);
    app.screen = Screen::Agents;
    app.agent_focus = Some(focus);
    app.utc_offset_s = 0;
    app.now_ms = 1794586722300;
    app.agent_map = true;
    app
}

// --------------------------------------------------------------------------- The `m` key

#[test]
fn m_toggles() {
    // `m` on the Agents screen flips `agent_map` (default true).
    let mut app = App::new(1000);
    app.screen = Screen::Agents;
    assert!(app.agent_map);
    app.apply(Event::Key(key('m')));
    assert!(!app.agent_map);
    app.apply(Event::Key(key('m')));
    assert!(app.agent_map);

    // `m` on the Overview leaves `agent_map` as it is.
    let mut app = App::new(1000);
    app.agent_map = false;
    app.screen = Screen::Overview;
    app.apply(Event::Key(key('m')));
    assert!(!app.agent_map);
}

// --------------------------------------------------------------------------- The map view

#[test]
fn map_view() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = map_app();
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 40);
    let all: String = rows.join("\n");

    // The panel title and the toggle hint on the right.
    assert!(all.contains("#131 · T24-setup-wizard"), "{:?}", all);
    assert!(all.contains("m list"), "{:?}", all);
    // The station nodes.
    assert!(all.contains("Model 14"), "{:?}", all);
    assert!(all.contains("Files 5"), "{:?}", all);
    assert!(all.contains("Checks 2 ◷ 0:01"), "{:?}", all);
    // A feed row and the now line.
    assert!(all.contains("src/app/state.rs"), "{:?}", all);
    assert!(all.contains("◉ Checks  cargo test  0:01"), "{:?}", all);
}

// --------------------------------------------------------------------------- Feed durations

#[test]
fn feed_durations() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = map_app();
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 40);
    let buf = render_buf(&app, 140, 40);

    // The edit step ends after 2 s (1794586230621 − 1794586228054): the duration is the last four
    // inner columns, immediately before the panel's right border.
    let edit_row = rows.iter().position(|r| r.contains("src/app/state.rs")).unwrap();
    assert!(rows[edit_row].ends_with("2s│"), "edit row: {:?}", rows[edit_row]);

    // The running think (the last step) ends after 5 s (1794586722300 − 1794586716911), in `Good`.
    let think_row = rows.iter().rposition(|r| r.contains("◉ think")).unwrap();
    assert!(rows[think_row].ends_with("5s│"), "think row: {:?}", rows[think_row]);
    let good_fg: Option<ratatui::style::Color> = theme.fg(Role::Good).fg;
    let has_good = (0..140u16).any(|x| {
        buf[(x, think_row as u16)].symbol() == "s" && buf[(x, think_row as u16)].style().fg == good_fg
    });
    assert!(has_good, "the running think duration must be `Good`, {:?}", rows[think_row]);
}

// --------------------------------------------------------------------------- The list view

#[test]
fn list_when_small() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = map_app();
    // At 100 × 30 the right side is too small for the map.
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 100, 30);
    let all: String = rows.join("\n");
    assert!(all.contains("Timeline"), "{:?}", all);
    assert!(!all.contains("m list"), "{:?}", all);
}

#[test]
fn list_when_off() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = map_app();
    app.agent_map = false;
    // The map off: the list view at 140 × 40.
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 40);
    assert!(rows.join("\n").contains("Timeline"), "{:?}", rows);
}

// --------------------------------------------------------------------------- The animation tick

#[test]
fn animating_map() {
    let mut app = map_app();
    assert_eq!(app.screen, Screen::Agents);
    // The activity has a pulse and a running station: the map animates.
    assert!(app.animating(), "the map on should animate");
    app.agent_map = false;
    assert!(!app.animating(), "the map off should not animate");
}

#[test]
fn animating_map_with_idle_llama() {
    let mut app = map_app();
    // A running server with nothing in flight must not hide the map's own animation.
    app.llama = Some(LlamaSnapshot {
        health: Health::Ok,
        router: true,
        build_info: None,
        models: vec![],
        active: vec![],
    });
    assert!(app.animating(), "the map should animate next to an idle llama snapshot");
    app.agent_map = false;
    assert!(!app.animating(), "an idle llama snapshot with the map off should not animate");
}

// --------------------------------------------------------------------------- Golden file

#[test]
fn agents_map_golden() {
    let theme = true_color();
    let path = "tests/snapshots/agents-map-140x40.txt";
    let fresh = snapshot(&map_app(), &theme, 140, 40, false);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(path, &fresh).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap();
    assert_eq!(expected, fresh, "golden file {path} is stale");
}
