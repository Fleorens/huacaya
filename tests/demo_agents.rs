// Tests for the scripted simulated OpenCode session of the Agents screen in `--demo` and `--snapshot`
// (T46): the pure `replay`, and the `--snapshot --screen 5` golden built the same way `main.rs` does.

use clap::Parser;
use huacaya::app::Screen;
use huacaya::app::demo::{DemoState, app as demo_app};
use huacaya::app::demo_agents::{DEMO_START_MS, SNAPSHOT_T, replay};
use huacaya::collect::activity::Station;
use huacaya::collect::opencode::StepKind;
use huacaya::config::Cli;
use huacaya::ui::snapshot::snapshot;
use huacaya::ui::theme::{ColorMode, Theme};

#[test]
fn steps_at_12() {
    let snap = replay(DEMO_START_MS, 12.0);
    assert_eq!(snap.steps.len(), 7);
    let kinds: Vec<StepKind> = snap.steps.iter().map(|s| s.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            StepKind::User,
            StepKind::Think,
            StepKind::Read,
            StepKind::Think,
            StepKind::Search,
            StepKind::Read,
            StepKind::Other("webfetch".into()),
        ]
    );
    assert_eq!(snap.steps[5].ok, Some(false));
    assert_eq!(snap.steps[5].detail.as_deref(), Some("File not found"));
    assert_eq!(snap.steps[6].end_ms, None);
    assert_eq!(snap.steps[1].end_ms, Some(DEMO_START_MS + 5_000));
    for (i, s) in snap.steps.iter().enumerate() {
        assert_eq!(s.id, format!("demo-0-{i}"));
    }
}

#[test]
fn all_stations_lit() {
    let snap = replay(DEMO_START_MS, 59.0);
    let st = &snap.activity.stations;
    assert_eq!(st[Station::You.index()].calls, 1);
    assert_eq!(st[Station::Model.index()].calls, 5);
    assert_eq!(st[Station::Files.index()].calls, 6);
    assert_eq!(st[Station::Shell.index()].calls, 1);
    assert_eq!(st[Station::Checks.index()].calls, 2);
    assert_eq!(st[Station::Web.index()].calls, 1);
    assert_eq!(st[Station::Agents.index()].calls, 1);
    assert_eq!(st[Station::Tools.index()].calls, 1);
    assert_eq!(st[Station::Files.index()].errors, 1);
    assert_eq!(st[Station::Checks.index()].errors, 1);
    assert_eq!(st[Station::You.index()].errors, 0);
    // Every step has ended by the end of the cycle: no station running.
    for s in st {
        assert_eq!(s.running, 0);
    }
}

#[test]
fn pulse_when_check_starts() {
    let snap = replay(DEMO_START_MS, 24.0);
    assert!(
        snap.activity
            .pulses
            .iter()
            .any(|p| p.station == Station::Checks && p.at_ms == DEMO_START_MS + 24_000 && p.outbound),
        "no outbound pulse at Checks when the check starts"
    );
}

#[test]
fn now_line() {
    let snap = replay(DEMO_START_MS, 26.0);
    assert_eq!(
        snap.activity.now,
        Some((Station::Checks, "cargo test --test demo".into(), DEMO_START_MS + 24_000))
    );
}

#[test]
fn second_cycle() {
    let snap = replay(DEMO_START_MS, 119.0);
    assert_eq!(snap.activity.stations[Station::Files.index()].calls, 12);
    assert_eq!(snap.activity.stations[Station::Checks.index()].errors, 2);
    assert_eq!(snap.steps.last().unwrap().id, "demo-1-17");
}

#[test]
fn queue() {
    let snap = replay(DEMO_START_MS, 30.0);
    assert_eq!(snap.sessions.len(), 3);
    assert_eq!(snap.sessions[0].id, "ses_demo_running");
    assert_eq!(snap.sessions[1].id, "ses_demo_done");
    assert_eq!(snap.sessions[2].id, "ses_demo_failed");
    assert_eq!(snap.focus.as_deref(), Some("ses_demo_running"));
    assert_eq!(snap.sessions[0].job, Some(42));
    assert_eq!(snap.sessions[0].tokens_in, 67_000);
}

#[test]
fn deterministic() {
    assert_eq!(replay(DEMO_START_MS, 37.5), replay(DEMO_START_MS, 37.5));
}

#[test]
fn screen_five() {
    assert!(
        Cli::try_parse_from(["huacaya", "--snapshot", "140x40", "--screen", "5"]).is_ok(),
        "--screen 5 must be accepted"
    );
    assert!(
        Cli::try_parse_from(["huacaya", "--snapshot", "140x40", "--screen", "6"]).is_err(),
        "--screen 6 must be rejected"
    );
}

#[test]
fn demo_agents_golden() {
    // The same app `main.rs` builds for `--snapshot 140x40 --screen 5`.
    let theme = Theme::new(ColorMode::TrueColor);
    let mut app = demo_app(DemoState::Generating);
    app.screen = Screen::Agents;
    app.opencode = Some(replay(DEMO_START_MS, SNAPSHOT_T));
    app.now_ms = DEMO_START_MS + 24_300;
    app.utc_offset_s = 0;

    let path = "tests/snapshots/demo-agents-140x40.txt";
    let fresh = snapshot(&app, &theme, 140, 40, false);
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(path, &fresh).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap();
    assert_eq!(expected, fresh, "golden file {path} is stale");
}

#[test]
fn long_run_keeps_newest_steps() {
    // Past SNAPSHOT_STEPS steps the snapshot keeps the newest ones, so the map keeps cycling.
    let snap = replay(DEMO_START_MS, 720.0);
    assert_eq!(snap.steps.len(), huacaya::collect::opencode::SNAPSHOT_STEPS);
    assert!(snap.steps.last().is_some_and(|s| s.id.starts_with("demo-12-")));
    assert!(snap.steps.windows(2).all(|w| w[0].time_ms <= w[1].time_ms));
}
