// Cell-by-cell tests for the Agents screen (T35). The model has no visual access, so every screen is
// rendered into a `Buffer` and checked row by row; the golden file captures the whole frame.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::{App, Event, Screen};
use llamatop::collect::opencode::{OpenCodeSnapshot, parse_sessions, parse_steps};
use llamatop::ui::panels::{Ctx, header};
use llamatop::ui::screens::agents;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::theme::{ColorMode, Theme};

// --- Key + snapshot builders (the sheet forbids these in `src/`) -------------------------------
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}
fn code(k: KeyCode) -> KeyEvent {
    KeyEvent::new(k, KeyModifiers::NONE)
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

// --- The fixture-backed snapshot and app (T35) -------------------------------------------------
fn sessions_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/sessions.json").unwrap()
}
fn messages_json() -> String {
    std::fs::read_to_string("tests/fixtures/opencode/messages.json").unwrap()
}

/// The `sessions.json` snapshot: 3 sessions, the fetched time, and the followed session is the done
/// one (`#131`, `T24-setup-wizard`).
fn fixture_snapshot() -> OpenCodeSnapshot {
    let sessions = parse_sessions(&sessions_json()).unwrap();
    let steps = parse_steps(&messages_json(), Some("/work/.wt/demo-131")).unwrap();
    let focus = sessions.iter().find(|s| s.job == Some(131)).map(|s| s.id.clone());
    OpenCodeSnapshot { sessions, focus, steps, error: None, fetched_ms: 1794514000000, ..Default::default() }
}

/// An app on the Agents screen, following the done session, with `utc_offset_s = 0` (UTC).
fn fixture_app() -> App {
    let mut app = App::new(1000);
    let snapshot = fixture_snapshot();
    let focus = snapshot.focus.clone().unwrap();
    app.agent_selected = snapshot.sessions.iter().position(|s| s.id == focus).unwrap();
    app.opencode = Some(snapshot);
    app.screen = Screen::Agents;
    app.agent_focus = Some(focus);
    app.utc_offset_s = 0;
    app
}

// --------------------------------------------------------------------------- short_title()

#[test]
fn short_titles() {
    assert_eq!(agents::short_title("#136 Execute tasks/T30-nvidia.md"), "T30-nvidia");
    assert_eq!(agents::short_title("Execute tasks/T24-setup-wizard.md"), "T24-setup-wizard");
    assert_eq!(agents::short_title("Refactor the parser"), "Refactor the parser");
}

// --------------------------------------------------------------------------- The queue

#[test]
fn queue_rows() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = fixture_app();
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 30);
    let all: String = rows.join("\n");

    // The three sessions, in queue order: running, done, failed.
    assert!(all.contains("136"), "{:?}", rows);
    assert!(all.contains("◉ running"), "{:?}", rows);
    assert!(all.contains("T30-nvidia"), "{:?}", rows);
    assert!(all.contains("131"), "{:?}", rows);
    assert!(all.contains("✓ done"), "{:?}", rows);
    assert!(all.contains("✖ failed"), "{:?}", rows);
    assert!(all.contains("Refactor the parser"), "{:?}", rows);

    // The summary row: 3 sessions, 1 running.
    assert!(all.contains("3 sessions · 1 running"), "{:?}", rows);
}

// --------------------------------------------------------------------------- The session panel

#[test]
fn session_panel() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = fixture_app();
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 30);
    let all: String = rows.join("\n");

    // The followed session is #131: the panel title, its token counts, its done state.
    assert!(all.contains("#131 · T24-setup-wizard"), "{:?}", rows);
    assert!(all.contains("in 145.0K  out 112.2K"), "{:?}", rows);
    assert!(all.contains("✓ done"), "{:?}", rows);
}

// --------------------------------------------------------------------------- The timeline

#[test]
fn timeline_rows() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = fixture_app();
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 30);
    let all: String = rows.join("\n");

    // The edit step (time 16:30:01, offset 0), its path and its `+3 −1` delta.
    assert!(all.contains("16:30:01"), "{:?}", rows);
    assert!(all.contains("✎ edit"), "{:?}", rows);
    assert!(all.contains("src/setup.rs"), "{:?}", rows);
    assert!(all.contains("+3 −1"), "{:?}", rows);

    // The shell step: command and its FAILED output.
    assert!(all.contains("$ shell"), "{:?}", rows);
    assert!(all.contains("cargo test --test setup"), "{:?}", rows);
    assert!(all.contains("test result: FAILED. 8 passed; 1 failed"), "{:?}", rows);

    // A think step.
    assert!(all.contains("◉ think"), "{:?}", rows);

    // The FAILED detail cell is `Critical`.
    let buf = render_buf(&app, 140, 30);
    let failed_row = (0..30u16).find(|&y| row_symbols(&buf, 140, y).contains("test result: FAILED")).unwrap();
    let crit_fg = theme.fg(llamatop::ui::theme::Role::Critical).fg;
    let has_crit = (0..140u16).any(|x| buf[(x, failed_row)].style().fg == crit_fg);
    assert!(has_crit, "the FAILED detail row must be Critical, {:?}", rows);
}

/// The symbols of row `y` of `buf`.
fn row_symbols(buf: &Buffer, w: u16, y: u16) -> String {
    (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()
}

/// Render the Agents screen into a `Buffer` so its cell styles can be inspected.
fn render_buf(app: &App, w: u16, h: u16) -> Buffer {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    agents::render(app, &ctx, Rect::new(0, 0, w, h), &mut buf);
    buf
}

// --------------------------------------------------------------------------- Without OpenCode

#[test]
fn not_configured() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = App::new(1000);
    app.screen = Screen::Agents;
    let rows = draw(|area, buf| agents::render(&app, &ctx, area, buf), 140, 30);
    assert!(rows.join("\n").contains("OpenCode is not configured"), "{:?}", rows);
}

// --------------------------------------------------------------------------- The follow keys

#[test]
fn agents_keys() {
    // `5` selects the Agents screen.
    let mut app = App::new(1000);
    app.apply(Event::Key(key('5')));
    assert_eq!(app.screen, Screen::Agents);

    // `Down` moves the selection to the second session and follows it.
    app.opencode = Some(fixture_snapshot());
    app.apply(Event::Key(code(KeyCode::Down)));
    assert_eq!(app.agent_selected, 1);
    let sessions = app.opencode.as_ref().unwrap().sessions.clone();
    assert_eq!(app.agent_focus, Some(sessions[1].id.clone()));

    // `a` unfollows.
    app.apply(Event::Key(code(KeyCode::Char('a'))));
    assert_eq!(app.agent_focus, None);

    // `Tab` cycles back to Overview.
    app.apply(Event::Key(code(KeyCode::Tab)));
    assert_eq!(app.screen, Screen::Overview);
}

// --------------------------------------------------------------------------- The running dot

#[test]
fn running_dot() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = fixture_app();
    // The Agents tab, followed session running, shows the dot on its tab.
    let rows = draw(|area, buf| header::render(&app, &ctx, area, buf), 140, 1);
    assert!(rows[0].contains("5 Agents ●"), "{:?}", rows[0]);
}

// --------------------------------------------------------------------------- Golden file

#[test]
fn agents_golden() {
    let theme = true_color();
    let path = "tests/snapshots/agents-140x40.txt";
    let fresh = snapshot(&fixture_app(), &theme, 140, 40, false);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(path, &fresh).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap();
    assert_eq!(expected, fresh, "golden file {path} is stale");
}
