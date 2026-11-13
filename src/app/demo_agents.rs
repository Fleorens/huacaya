//! A scripted, simulated OpenCode session for the Agents screen in `--demo` and `--snapshot` (T46).
//!
//! Pure and deterministic: the same `start_ms` and the same demo time `t` always yield the same
//! snapshot, so the golden of `demo_agents_golden` is stable. Only `run::demo_loop` reads the clock
//! and calls `round`; nothing here touches a clock or the randomness.

use crate::app::demo::LOADED_ID;
use crate::collect::activity::Activity;
use crate::collect::opencode::{OpenCodeSnapshot, SNAPSHOT_STEPS, Session, Step, StepKind};

/// Fixed demo wall-clock start for tests and `--snapshot` (2026-11-13 ~21:20 UTC).
pub const DEMO_START_MS: i64 = 1_794_600_000_000;
/// Length of one scripted cycle, seconds.
pub const AGENT_CYCLE_S: f64 = 60.0;
/// The demo time `--snapshot --screen 5` renders: the check has just started, its pulse is half way.
pub const SNAPSHOT_T: f64 = 24.3;

/// The followed session in every round: the one running session that replays [`SCRIPT`].
const RUNNING_ID: &str = "ses_demo_running";

/// One scripted step of one cycle. `kind` is carried by [`Kind`] (the `Other(name)` ones by name, so
/// the array stays `const`), turned into a [`StepKind`] in `Into`.
#[derive(Debug, Clone, Copy)]
struct Script {
    /// Seconds from the cycle start.
    start: f64,
    /// Duration, seconds.
    dur: f64,
    kind: Kind,
    summary: &'static str,
    detail: Option<&'static str>,
    /// The verdict once the step has ended (`None` for the rows with "—").
    ok: Option<bool>,
}

/// The step kinds the script uses, the `Other(name)` ones by name so the array stays `const`.
#[derive(Debug, Clone, Copy)]
enum Kind {
    User,
    Think,
    Say,
    Read,
    Edit,
    Write,
    Shell,
    Search,
    Other(&'static str),
}

impl From<Kind> for StepKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::User => StepKind::User,
            Kind::Think => StepKind::Think,
            Kind::Say => StepKind::Say,
            Kind::Read => StepKind::Read,
            Kind::Edit => StepKind::Edit,
            Kind::Write => StepKind::Write,
            Kind::Shell => StepKind::Shell,
            Kind::Search => StepKind::Search,
            Kind::Other(name) => StepKind::Other(name.to_string()),
        }
    }
}

/// The one cycle, in time order. `start` and `dur` are seconds from the cycle start; `ok` is the
/// verdict once the step has ended (`None` for the rows with "—").
const SCRIPT: [Script; 18] = [
    Script {
        start: 0.0,
        dur: 0.0,
        kind: Kind::User,
        summary: "Execute tasks/T12-demo-feature.md",
        detail: None,
        ok: None,
    },
    Script {
        start: 1.0,
        dur: 4.0,
        kind: Kind::Think,
        summary: "Read the sheet first, then the cited sections.",
        detail: None,
        ok: None,
    },
    Script {
        start: 5.0,
        dur: 0.5,
        kind: Kind::Read,
        summary: "tasks/T12-demo-feature.md",
        detail: None,
        ok: Some(true),
    },
    Script {
        start: 6.0,
        dur: 3.0,
        kind: Kind::Think,
        summary: "The renderer lives in src/ui; find its entry point.",
        detail: None,
        ok: None,
    },
    Script {
        start: 9.0,
        dur: 0.5,
        kind: Kind::Search,
        summary: "pub fn render in src/ui",
        detail: None,
        ok: Some(true),
    },
    Script {
        start: 10.0,
        dur: 0.5,
        kind: Kind::Read,
        summary: "src/missing.rs",
        detail: Some("File not found"),
        ok: Some(false),
    },
    Script {
        start: 11.0,
        dur: 2.0,
        kind: Kind::Other("webfetch"),
        summary: "webfetch",
        detail: Some("docs.rs/ratatui"),
        ok: Some(true),
    },
    Script {
        start: 14.0,
        dur: 4.0,
        kind: Kind::Think,
        summary: "Plan: one widget, one test, then the check.",
        detail: None,
        ok: None,
    },
    Script {
        start: 18.0,
        dur: 0.3,
        kind: Kind::Other("todowrite"),
        summary: "todowrite",
        detail: Some("3 todos"),
        ok: Some(true),
    },
    Script {
        start: 19.0,
        dur: 0.5,
        kind: Kind::Edit,
        summary: "src/ui/demo.rs",
        detail: Some("+12 −3"),
        ok: Some(true),
    },
    Script {
        start: 20.0,
        dur: 0.3,
        kind: Kind::Write,
        summary: "tests/demo.rs",
        detail: Some("24 lines"),
        ok: Some(true),
    },
    Script { start: 21.0, dur: 2.0, kind: Kind::Shell, summary: "cargo fmt", detail: None, ok: Some(true) },
    Script {
        start: 24.0,
        dur: 6.0,
        kind: Kind::Shell,
        summary: "cargo test --test demo",
        detail: Some("test result: FAILED. 7 passed; 1 failed"),
        ok: Some(false),
    },
    Script {
        start: 31.0,
        dur: 5.0,
        kind: Kind::Think,
        summary: "One assertion expects the old width; fix the widget.",
        detail: None,
        ok: None,
    },
    Script {
        start: 36.0,
        dur: 0.5,
        kind: Kind::Edit,
        summary: "src/ui/demo.rs",
        detail: Some("+2 −1"),
        ok: Some(true),
    },
    Script {
        start: 37.0,
        dur: 6.0,
        kind: Kind::Other("task"),
        summary: "task",
        detail: Some("review the change"),
        ok: Some(true),
    },
    Script {
        start: 44.0,
        dur: 8.0,
        kind: Kind::Shell,
        summary: "bash .devbox/check",
        detail: Some("test result: ok. 8 passed"),
        ok: Some(true),
    },
    Script {
        start: 53.0,
        dur: 0.0,
        kind: Kind::Say,
        summary: "Done: the check is green.",
        detail: None,
        ok: None,
    },
];

/// A simulated OpenCode server: one running session that replays [`SCRIPT`] every cycle.
/// Deterministic: the same `start_ms` and the same rounds give the same snapshots.
pub struct DemoAgents {
    start_ms: i64,
    /// The running session folded in, so `round` returns the map's state for it.
    activity: Activity,
}

impl DemoAgents {
    /// Demo time 0 is wall-clock `start_ms`.
    pub fn new(start_ms: i64) -> Self {
        DemoAgents { start_ms, activity: Activity::default() }
    }

    /// One poll round at demo time `t` seconds (rounds come with increasing `t`).
    pub fn round(&mut self, t: f64) -> OpenCodeSnapshot {
        let now = (self.start_ms as f64 + t * 1000.0).round() as i64;
        let steps = steps_at(&SCRIPT, self.start_ms, t);
        self.activity.update(RUNNING_ID, &steps, now);
        OpenCodeSnapshot {
            sessions: sessions(self.start_ms, t),
            focus: Some(RUNNING_ID.into()),
            steps,
            activity: self.activity.clone(),
            error: None,
            fetched_ms: now,
        }
    }
}

/// The existing steps at demo time `t` (cycles 0, 1, …), in time order, newest `SNAPSHOT_STEPS` only.
fn steps_at(script: &[Script], start_ms: i64, t: f64) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut c = 0usize;
    loop {
        let c_s = c as f64 * AGENT_CYCLE_S;
        // The first step of this cycle already starts after `t`: later cycles do too.
        if script[0].start + c_s > t {
            break;
        }
        for (i, s) in script.iter().enumerate() {
            let local = c_s + s.start;
            if local > t {
                break;
            }
            let time_ms = start_ms + (local * 1000.0).round() as i64;
            let ended = local + s.dur <= t;
            steps.push(Step {
                time_ms,
                kind: s.kind.into(),
                id: format!("demo-{c}-{i}"),
                end_ms: if ended { Some(time_ms + (s.dur * 1000.0).round() as i64) } else { None },
                summary: s.summary.to_string(),
                ok: if ended { s.ok } else { None },
                detail: s.detail.map(str::to_string),
            });
        }
        c += 1;
    }
    if steps.len() > SNAPSHOT_STEPS {
        steps.truncate(steps.len() - SNAPSHOT_STEPS);
    }
    steps
}

/// The three demo sessions, newest first (running, done, failed), at demo time `t`.
fn sessions(start_ms: i64, t: f64) -> Vec<Session> {
    vec![
        Session {
            id: RUNNING_ID.into(),
            title: "#42 Execute tasks/T12-demo-feature.md".into(),
            job: Some(42),
            model: Some(LOADED_ID.into()),
            agent: Some("build".into()),
            outcome: None,
            running: true,
            created_ms: start_ms - 600_000,
            idle_ms: None,
            tokens_in: (40_000.0 + 900.0 * t) as u64,
            tokens_out: (12_000.0 + 250.0 * t) as u64,
            cache_read: (1_500_000.0 + 30_000.0 * t) as u64,
            directory: Some("/work/demo".into()),
        },
        Session {
            id: "ses_demo_done".into(),
            title: "#41 Execute tasks/T11-demo-data.md".into(),
            job: Some(41),
            model: None,
            agent: None,
            outcome: Some("succeeded".into()),
            running: false,
            created_ms: start_ms - 3_600_000,
            idle_ms: Some(start_ms - 1_800_000),
            tokens_in: 0,
            tokens_out: 0,
            cache_read: 0,
            directory: None,
        },
        Session {
            id: "ses_demo_failed".into(),
            title: "Refactor the parser".into(),
            job: None,
            model: None,
            agent: None,
            outcome: Some("failed".into()),
            running: false,
            created_ms: start_ms - 86_400_000,
            idle_ms: Some(start_ms - 84_000_000),
            tokens_in: 0,
            tokens_out: 0,
            cache_read: 0,
            directory: None,
        },
    ]
}

/// Rounds at `t` = 0, 1, …, `floor(t)`, then at `t` itself when it is not a whole number; the last
/// snapshot. Pure and deterministic.
pub fn replay(start_ms: i64, t: f64) -> OpenCodeSnapshot {
    let mut agents = DemoAgents::new(start_ms);
    let mut last = agents.round(0.0);
    let mut i = 1.0;
    while i <= t.floor() {
        last = agents.round(i);
        i += 1.0;
    }
    if t > t.floor() {
        last = agents.round(t);
    }
    last
}
