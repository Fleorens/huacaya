// Tests for `log_tasks` (T37): summarising the tasks (requests) seen in the log. Pure — no terminal.

use huacaya::app::log_tasks::{TaskOutcome, log_tasks};
use huacaya::collect::logs::{Level, LogEvent, LogLine};

/// One line, built for the fields `log_tasks` reads: `task`, `time`, `level` and `event`.
fn line(task: i64, time: Option<&str>, level: Level, event: Option<LogEvent>) -> LogLine {
    LogLine {
        time: time.map(str::to_string),
        source: None,
        instance: None,
        task: Some(task),
        level,
        text: String::new(),
        event,
    }
}

/// The tasks, in order, of the returned summaries.
fn tasks(lines: Vec<LogLine>) -> Vec<i64> {
    log_tasks(lines.iter(), 10).into_iter().map(|t| t.task).collect()
}

#[test]
fn tasks_newest_first() {
    // Tasks 7, 8, 7: task 7 is seen last (its last line is later), so it comes first.
    let lines = vec![
        line(7, Some("10:00:00"), Level::Info, None),
        line(8, Some("10:00:00"), Level::Info, None),
        line(7, Some("10:00:01"), Level::Info, None),
    ];
    assert_eq!(tasks(lines), vec![7, 8]);
}

#[test]
fn task_outcomes() {
    // A task with an Error line is Failed; one with a Released event (no error) is Done; neither is
    // Running.
    let failed = [line(1, Some("10:00:00"), Level::Error, None)];
    let done = [line(2, Some("10:00:00"), Level::Info, Some(LogEvent::Released { n_tokens: 100 }))];
    let running = [line(3, Some("10:00:00"), Level::Info, None)];
    assert_eq!(log_tasks(failed.iter(), 10)[0].outcome, TaskOutcome::Failed);
    assert_eq!(log_tasks(done.iter(), 10)[0].outcome, TaskOutcome::Done);
    assert_eq!(log_tasks(running.iter(), 10)[0].outcome, TaskOutcome::Running);
}

#[test]
fn task_seconds_and_midnight() {
    // 10:00:08 then 10:00:32 → 24 s.
    let a = [line(1, Some("10:00:08"), Level::Info, None), line(1, Some("10:00:32"), Level::Info, None)];
    assert_eq!(log_tasks(a.iter(), 10)[0].seconds, Some(24));

    // 23:59:50 then 00:00:10 (midnight passed) → 20 s.
    let b = [line(2, Some("23:59:50"), Level::Info, None), line(2, Some("00:00:10"), Level::Info, None)];
    assert_eq!(log_tasks(b.iter(), 10)[0].seconds, Some(20));

    // A task with a single timed line (the others have no time) → 0 s.
    let c = [line(3, Some("10:00:05"), Level::Info, None), line(3, None, Level::Info, None)];
    assert_eq!(log_tasks(c.iter(), 10)[0].seconds, Some(0));

    // A task whose lines all lack a time → None.
    let d = [line(4, None, Level::Info, None), line(4, None, Level::Info, None)];
    assert_eq!(log_tasks(d.iter(), 10)[0].seconds, None);
}

#[test]
fn task_tokens() {
    // Released wins over GenProgress.
    let released = [line(1, Some("10:00:00"), Level::Info, Some(LogEvent::Released { n_tokens: 9232 }))];
    assert_eq!(log_tasks(released.iter(), 10)[0].tokens, Some(9232));

    // Only GenProgress seen: its last n_gen.
    let gen_progress = [line(
        2,
        Some("10:00:00"),
        Level::Info,
        Some(LogEvent::GenProgress { n_gen: 780, tps: 0.0, tps_3s: 0.0 }),
    )];
    assert_eq!(log_tasks(gen_progress.iter(), 10)[0].tokens, Some(780));
}

#[test]
fn negative_tasks_skipped() {
    // A negative task number is ignored; only task 5 remains.
    let lines =
        vec![line(-1, Some("10:00:00"), Level::Info, None), line(5, Some("10:00:00"), Level::Info, None)];
    assert_eq!(tasks(lines), vec![5]);
}

#[test]
fn max_respected() {
    let lines = [
        line(1, Some("10:00:00"), Level::Info, None),
        line(2, Some("10:00:00"), Level::Info, None),
        line(3, Some("10:00:00"), Level::Info, None),
    ];
    assert_eq!(log_tasks(lines.iter(), 1).len(), 1);
}
