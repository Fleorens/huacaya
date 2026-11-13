//! Integration tests for `collect::activity`: station classification and the `Activity` fold (T41).

use llamatop::collect::activity::{Activity, PULSE_MS, Station, command_station, station};
use llamatop::collect::opencode::{Step, StepKind, parse_steps};

/// The `tests/fixtures/opencode` directory.
fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/opencode/{name}")).expect("fixture present")
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

#[test]
fn station_order() {
    assert_eq!(
        Station::ALL.map(Station::name),
        ["You", "Model", "Files", "Shell", "Checks", "Web", "Agents", "Tools"]
    );
    assert_eq!(Station::Checks.index(), 4);
}

#[test]
fn command_stations() {
    let st = |c: &str| command_station(c);
    assert_eq!(st("cargo test --test setup"), Station::Checks);
    assert_eq!(st("cargo fmt"), Station::Shell);
    assert_eq!(st("cargo fmt --check"), Station::Checks);
    assert_eq!(
        st("cd /w && cargo fmt && cargo clippy --all-targets -- -D warnings; echo EXIT: $?"),
        Station::Checks
    );
    assert_eq!(st("bash .devbox/check > /tmp/check4.out 2>&1; echo EXIT: $?"), Station::Checks);
    assert_eq!(st("FOO=1 timeout 30 cargo test"), Station::Checks);
    assert_eq!(st("/usr/bin/cargo test"), Station::Checks);
    assert_eq!(st("cd /w && tail -8 /tmp/check4.out"), Station::Files);
    assert_eq!(st("sed -n 1,40p src/a.rs | grep fn"), Station::Files);
    assert_eq!(st("git push origin main"), Station::Web);
    assert_eq!(st("curl -s http://example.test"), Station::Web);
    assert_eq!(st("git status --short"), Station::Shell);
    assert_eq!(st("python3 script.py"), Station::Shell);
    assert_eq!(st("ls src && cargo build"), Station::Shell);
    assert_eq!(st("cd /w"), Station::Shell);
    assert_eq!(st(""), Station::Shell);
}

#[test]
fn step_stations() {
    assert_eq!(station(&step("u", StepKind::User, "u", 1, None, None)), Station::You);
    assert_eq!(station(&step("t", StepKind::Think, "t", 1, None, None)), Station::Model);
    assert_eq!(station(&step("e", StepKind::Edit, "e", 1, None, None)), Station::Files);
    assert_eq!(station(&step("s", StepKind::Shell, "cargo test", 1, None, None)), Station::Checks);
    assert_eq!(station(&step("w", StepKind::Other("webfetch".into()), "w", 1, None, None)), Station::Web);
    assert_eq!(station(&step("tk", StepKind::Other("task".into()), "tk", 1, None, None)), Station::Agents);
    assert_eq!(station(&step("cp", StepKind::Other("compress".into()), "cp", 1, None, None)), Station::Tools);
    assert_eq!(station(&step("g", StepKind::Other("glob".into()), "g", 1, None, None)), Station::Files);
}

#[test]
fn real_ids_and_ends() {
    let steps = parse_steps(&fixture("messages-real.json"), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    // The running think: no `id` on its part, so `<message id>#<part index>`; no `time.completed`.
    let last = steps.last().unwrap();
    assert_eq!(last.id, "msg_1d5b4257d001SGHohrO5bXFuks#0");
    assert_eq!(last.end_ms, None);
    // The edit and compress steps: the tool part's own `id` and `time.completed`.
    assert_eq!(steps[8].id, "XpttFmriPBEmPIK9GrDtFqFUFJGA2O7X");
    assert_eq!(steps[8].end_ms, Some(1794586230621));
    assert_eq!(steps[2].id, "G8VMZWonRg17YrfMtuJiimpVoT3ZfIY1");
    assert_eq!(steps[2].end_ms, Some(1794585602850));
}

#[test]
fn cd_prefix() {
    let steps = parse_steps(&fixture("messages-real.json"), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    // The model prefixes every command with `cd <worktree> && `; it is dropped here.
    assert!(steps[13].summary.starts_with("cargo fmt && cargo clippy"));
}

#[test]
fn user_step() {
    let steps = parse_steps(&fixture("messages.json"), Some("/work/.wt/demo-131")).unwrap();
    assert_eq!(steps[0].kind, StepKind::User);
    assert_eq!(steps[0].summary, "Execute tasks/T24-setup-wizard.md");
}

#[test]
fn history_round() {
    let steps = parse_steps(&fixture("messages-real.json"), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    assert_eq!(steps.len(), 21);

    let mut a = Activity::default();
    a.update("s1", &steps, 1794586720000);

    // A first round is history: totals filled, no pulses.
    assert!(a.pulses.is_empty());
    assert_eq!(a.now, Some((Station::Model, String::new(), 1794586716911)));
    assert_eq!(a.stations[Station::Model.index()].calls, 14);
    assert_eq!(a.stations[Station::Files.index()].calls, 5);
    assert_eq!(a.stations[Station::Checks.index()].calls, 1);
    assert_eq!(a.stations[Station::Tools.index()].calls, 1);
    assert_eq!(a.stations[Station::You.index()].calls, 0);
    assert_eq!(a.stations[Station::Shell.index()].calls, 0);
    assert_eq!(a.stations[Station::Web.index()].calls, 0);
    assert_eq!(a.stations[Station::Agents.index()].calls, 0);
    assert_eq!(a.stations[Station::Model.index()].running, 1);
    assert_eq!(a.stations[Station::Model.index()].running_since, Some(1794586716911));
    for s in Station::ALL {
        if s != Station::Model {
            assert_eq!(a.stations[s.index()].running, 0);
        }
    }
    for s in &a.stations {
        assert_eq!(s.errors, 0);
    }
}

#[test]
fn live_round() {
    let steps = parse_steps(&fixture("messages-real.json"), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    let mut a = Activity::default();
    a.update("s1", &steps, 1794586720000);

    // A new step starts.
    a.update("s1", &[step("x1", StepKind::Shell, "cargo test", 1794586721000, None, None)], 1794586722000);
    assert_eq!(a.pulses.len(), 1);
    let p = &a.pulses[0];
    assert_eq!((p.station, p.at_ms, p.outbound, p.failed), (Station::Checks, 1794586722000, true, false));
    assert_eq!(a.stations[Station::Checks.index()].running, 1);
    assert_eq!(a.stations[Station::Checks.index()].running_since, Some(1794586721000));

    // The same step ends, failed.
    a.update(
        "s1",
        &[step("x1", StepKind::Shell, "cargo test", 1794586721000, Some(1794586723000), Some(false))],
        1794586724000,
    );
    assert_eq!(a.pulses.len(), 1);
    let p = &a.pulses[0];
    assert_eq!((p.station, p.at_ms, p.outbound, p.failed), (Station::Checks, 1794586724000, false, true));
    assert_eq!(a.stations[Station::Checks.index()].errors, 1);
    assert_eq!(a.stations[Station::Checks.index()].running, 0);
}

#[test]
fn fast_step() {
    let mut a = Activity::default();
    // The poller's whole-history round first (no pulses), then the step appears.
    a.update("s", &[], 1000);
    // A new step already ended at its live round makes two pulses.
    a.update("s", &[step("f", StepKind::Read, "f.rs", 1100, Some(1100), Some(true))], 2000);
    assert_eq!(a.pulses.len(), 2);
    assert_eq!((a.pulses[0].at_ms, a.pulses[0].outbound, a.pulses[0].failed), (2000, true, false));
    assert_eq!(
        (a.pulses[1].at_ms, a.pulses[1].outbound, a.pulses[1].failed),
        (2000 + PULSE_MS, false, false)
    );
}

#[test]
fn session_change_resets() {
    let mut a = Activity::default();
    a.update("s1", &[step("a", StepKind::Read, "a.rs", 1000, Some(1100), Some(true))], 2000);
    assert_eq!(a.stations[Station::Files.index()].calls, 1);

    // A new session: only its counts, no leftover pulses.
    a.update("s2", &[step("b", StepKind::Read, "b.rs", 3000, Some(3100), Some(true))], 4000);
    assert_eq!(a.session.as_deref(), Some("s2"));
    assert_eq!(a.stations[Station::Files.index()].calls, 1);
    assert_eq!(a.stations[Station::Files.index()].last_ms, Some(3000));
    assert!(a.pulses.is_empty());
}
