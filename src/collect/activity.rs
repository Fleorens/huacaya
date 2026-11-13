//! The pure data side of the Agents map (T41): which station a step belongs to, the classification of a
//! shell command line, and an [`Activity`] that folds the poller's rounds into per-station totals, running
//! steps and pulses. See `tasks/T41`.

use std::collections::HashMap;

use crate::collect::opencode::{Step, StepKind};

mod commands;
use commands::{is_check, is_file, is_neutral, is_web};

/// A place on the Agents map. `ALL` is the drawing order, clockwise from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Station {
    You,
    Model,
    Files,
    Shell,
    Checks,
    Web,
    Agents,
    Tools,
}

impl Station {
    /// The drawing order, clockwise from the top.
    pub const ALL: [Station; 8] = [
        Station::You,
        Station::Model,
        Station::Files,
        Station::Shell,
        Station::Checks,
        Station::Web,
        Station::Agents,
        Station::Tools,
    ];
    /// The station's name, indexed like `ALL`.
    const NAMES: [&'static str; 8] = ["You", "Model", "Files", "Shell", "Checks", "Web", "Agents", "Tools"];
    /// Position in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }
    /// `"You"`, `"Model"`, `"Files"`, `"Shell"`, `"Checks"`, `"Web"`, `"Agents"`, `"Tools"`.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
}

/// The station of a step.
pub fn station(step: &Step) -> Station {
    match &step.kind {
        StepKind::User => Station::You,
        StepKind::Think | StepKind::Say => Station::Model,
        StepKind::Read | StepKind::Search | StepKind::Edit | StepKind::Write => Station::Files,
        StepKind::Shell => command_station(&step.summary),
        StepKind::Other(name) => other_station(name),
    }
}

fn other_station(name: &str) -> Station {
    match name {
        "webfetch" | "websearch" | "codesearch" => Station::Web,
        "task" => Station::Agents,
        "glob" | "list" | "grep" | "patch" | "multiedit" => Station::Files,
        _ => Station::Tools,
    }
}

/// How long a pulse travels along its spoke, ms.
pub const PULSE_MS: i64 = 600;

/// One station's totals for the followed session.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StationStats {
    /// Steps seen at this station since the session was first folded in.
    pub calls: u32,
    /// Steps that ended with `ok == Some(false)`.
    pub errors: u32,
    /// Steps running now (`end_ms == None`) in the last round.
    pub running: u32,
    /// The earliest `time_ms` among the running ones.
    pub running_since: Option<i64>,
    /// `time_ms` of the newest step seen at this station.
    pub last_ms: Option<i64>,
}

/// A step seen starting (`outbound`) or ending by the poller.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulse {
    pub station: Station,
    /// When the pulse leaves, Unix ms (the round's time, see `update`).
    pub at_ms: i64,
    pub outbound: bool,
    /// An inbound pulse of a step that ended with `ok == Some(false)`.
    pub failed: bool,
}

/// The map's state for the followed session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Activity {
    /// The session folded in, `None` before the first round.
    pub session: Option<String>,
    /// Indexed by `Station::index`.
    pub stations: [StationStats; 8],
    /// Pulses not older than `2 * PULSE_MS`, oldest first.
    pub pulses: Vec<Pulse>,
    /// The newest running step: its station, summary and `time_ms`.
    pub now: Option<(Station, String, i64)>,
    /// The step id's `end_ms` as last seen, to tell new and ended steps apart.
    seen: HashMap<String, Option<i64>>,
}

impl Activity {
    /// Folds one round in. `steps` is any window of the session's timeline (a full history or only the
    /// newest steps); `now_ms` is the round's wall-clock time.
    pub fn update(&mut self, session: &str, steps: &[Step], now_ms: i64) {
        let same_session = self.session.as_deref() == Some(session);
        if !same_session {
            *self = Activity::default();
            self.session = Some(session.to_string());
        }
        // A fresh round is "history": it fills the totals but makes no pulses.
        let history = !same_session;

        for step in steps {
            let s = station(step);
            let idx = s.index();
            let stats = &mut self.stations[idx];
            let ended = step.end_ms.is_some();
            let failed = step.ok == Some(false);
            match self.seen.get(&step.id) {
                // A step never seen before: new, with its starting pulse (and an ending one if already done).
                None => {
                    stats.calls += 1;
                    stats.last_ms = Some(stats.last_ms.map_or(step.time_ms, |c| c.max(step.time_ms)));
                    if ended && failed {
                        stats.errors += 1;
                    }
                    if !history {
                        self.pulses.push(Pulse { station: s, at_ms: now_ms, outbound: true, failed: false });
                        if ended {
                            self.pulses.push(Pulse {
                                station: s,
                                at_ms: now_ms + PULSE_MS,
                                outbound: false,
                                failed,
                            });
                        }
                    }
                }
                // Seen running, now ended: its ending pulse.
                Some(None) => {
                    if ended {
                        if failed {
                            stats.errors += 1;
                        }
                        if !history {
                            self.pulses.push(Pulse { station: s, at_ms: now_ms, outbound: false, failed });
                        }
                    }
                }
                // Seen ended before (or ended again, or back to running): a plain update, no pulse.
                Some(Some(_)) => {}
            }
            self.seen.insert(step.id.clone(), step.end_ms);
        }

        // Running steps and the "now" line come from this round's steps only.
        for stats in self.stations.iter_mut() {
            stats.running = 0;
            stats.running_since = None;
        }
        let mut now: Option<(Station, String, i64)> = None;
        for step in steps {
            if step.end_ms.is_some() {
                continue;
            }
            let s = station(step);
            let idx = s.index();
            self.stations[idx].running += 1;
            self.stations[idx].running_since =
                Some(self.stations[idx].running_since.map_or(step.time_ms, |c| c.min(step.time_ms)));
            if now.as_ref().is_none_or(|n| step.time_ms > n.2) {
                now = Some((s, step.summary.clone(), step.time_ms));
            }
        }
        self.now = now;

        // Drop pulses older than two spans, keep the rest oldest first.
        let cutoff = now_ms - 2 * PULSE_MS;
        self.pulses.retain(|p| p.at_ms >= cutoff);
        self.pulses.sort_by_key(|p| p.at_ms);
    }
}

/// The station of a shell command line, per `tasks/T41`.
pub fn command_station(command: &str) -> Station {
    let mut any_check = false;
    let mut any_web = false;
    let mut non_neutral = 0usize;
    let mut all_file = true;
    // Turn `&&`/`||` into `;` first so a lone `&` (as in `2>&1`) never splits a segment.
    let line = command.replace("&&", ";").replace("||", ";");
    for seg in line.split([';', '|']) {
        match classify_segment(seg) {
            Seg::Neutral => {}
            Seg::Check => {
                any_check = true;
                non_neutral += 1;
            }
            Seg::Web => {
                any_web = true;
                non_neutral += 1;
                all_file = false;
            }
            Seg::File => non_neutral += 1,
            Seg::Other => {
                non_neutral += 1;
                all_file = false;
            }
        }
    }
    if any_check {
        Station::Checks
    } else if any_web {
        Station::Web
    } else if non_neutral > 0 && all_file {
        Station::Files
    } else {
        Station::Shell
    }
}

/// The verdict on one `&&`/`;`/`|`-separated segment.
enum Seg {
    Neutral,
    Check,
    Web,
    File,
    Other,
}

/// Splits a segment on redirections, `NAME=value` and wrappers, then classifies its program and arguments.
fn classify_segment(seg: &str) -> Seg {
    let words = drop_redirections(seg);
    if words.is_empty() {
        return Seg::Neutral;
    }
    // Skip leading `NAME=value`, then the wrappers `sudo`, `env`, `nohup`, `time`, `timeout` (+ its duration).
    let mut i = 0;
    loop {
        match words.get(i).copied() {
            Some(w) if w.contains('=') => i += 1,
            Some("sudo") | Some("env") | Some("nohup") | Some("time") => i += 1,
            Some("timeout") => i += 2,
            _ => break,
        }
    }
    let program = match words.get(i) {
        Some(w) => program_name(w),
        None => return Seg::Neutral,
    };
    let args = &words[i + 1..];
    if is_neutral(program) {
        return Seg::Neutral;
    }
    if is_check(program, args) {
        Seg::Check
    } else if is_web(program, args) {
        Seg::Web
    } else if is_file(program) {
        Seg::File
    } else {
        Seg::Other
    }
}

/// The words of `seg` with redirections (and the word after a bare operator) dropped.
fn drop_redirections(seg: &str) -> Vec<&str> {
    let words: Vec<&str> = seg.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if is_redirect(w) {
            i += usize::from(is_exact_operator(w)) + 1;
        } else {
            kept.push(w);
            i += 1;
        }
    }
    kept
}

/// A redirection word: starts with `>`, `<`, `2>`, `&>` or `1>`.
fn is_redirect(w: &str) -> bool {
    w.starts_with(">")
        || w.starts_with("<")
        || w.starts_with("2>")
        || w.starts_with("&>")
        || w.starts_with("1>")
}

/// A redirection word that is exactly an operator (`>`, `>>`, `<`, `2>`), so the next word is its target.
fn is_exact_operator(w: &str) -> bool {
    matches!(w, ">" | ">>" | "<" | "2>")
}

/// The program's file name: `/usr/bin/cargo` → `cargo`.
fn program_name(w: &str) -> &str {
    match std::path::Path::new(w).file_name().and_then(|s| s.to_str()) {
        Some(name) => name,
        None => w,
    }
}
