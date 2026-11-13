//! Data threads that need no terminal: the OpenCode poller loop (`tasks/T34-opencode-client.md`, a round
//! every second since `tasks/T42`) and the agent loop (`tasks/T44`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::agent::client::AgentClient;
use crate::app::Event;
use crate::app::run::sleep;
use crate::collect::logs::{Level, LogLine};
use crate::collect::opencode::OpenCodePoller;
use crate::http::Client;

/// Polls the OpenCode server every second for the session in `focus` until `stop` is set or the UI is gone.
pub fn opencode_loop(
    client: Client,
    tx: Sender<Event>,
    stop: Arc<AtomicBool>,
    focus: Arc<Mutex<Option<String>>>,
) {
    let mut poller = OpenCodePoller::new(client);
    while !stop.load(Ordering::Relaxed) {
        let focused = focus.lock().unwrap().clone();
        if tx.send(Event::OpenCode(Box::new(poller.poll(focused.as_deref())))).is_err() {
            break;
        }
        sleep(&stop, 1000);
    }
}

/// The agent thread: every shared interval, `system()` → `Event::System`, then `log(since)` → one
/// `Event::Log` per line. Warns once on the transition to unreachable, says once on the return; never floods.
pub(super) fn agent_loop(
    tx: Sender<Event>,
    stop: Arc<AtomicBool>,
    interval: Arc<AtomicU64>,
    url: String,
    token: Option<String>,
) {
    let client = AgentClient::new(&url, token);
    let mut since = 0u64;
    let mut reachable: Option<bool> = None;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        // One reachability verdict per round (ok only if both calls succeed), so a half-working agent
        // cannot alternate `agent back` / `agent unreachable` every round.
        let mut events = Vec::new();
        let mut error = None;
        match client.system() {
            Ok(snapshot) => events.push(Event::System(snapshot)),
            Err(e) => error = Some(format!("{e:#}")),
        }
        match client.log(since) {
            Ok(page) if page.next < since => {
                // The agent restarted: its numbering began again at 1, below our cursor. Start over from its
                // whole ring next round rather than waiting forever for numbers it will not reach soon.
                since = 0;
            }
            Ok(page) => {
                events.extend(page.lines.into_iter().map(Event::Log));
                since = page.next;
                if page.dropped {
                    let line = agent_message("agent log: some lines were missed", Level::Warn);
                    events.push(Event::Log(line));
                }
            }
            Err(e) => {
                error.get_or_insert(format!("{e:#}"));
            }
        }
        if let Some(line) = reachability_line(&mut reachable, error) {
            events.insert(0, Event::Log(line));
        }
        for event in events {
            if tx.send(event).is_err() {
                return;
            }
        }

        sleep(&stop, interval.load(Ordering::Relaxed));
    }
}

/// The reachability transition line, if the state changed this round: `agent back` (Info) on return,
/// `agent unreachable: <error>` (Warn) on the way down. Never more than one per change.
fn reachability_line(reachable: &mut Option<bool>, error: Option<String>) -> Option<LogLine> {
    let prev = reachable.replace(error.is_none());
    match (prev, error) {
        (Some(false), None) => Some(agent_message("agent back", Level::Info)),
        (Some(true) | None, Some(e)) => Some(agent_message(&format!("agent unreachable: {e}"), Level::Warn)),
        _ => None,
    }
}

fn agent_message(text: &str, level: Level) -> LogLine {
    LogLine {
        time: None,
        source: None,
        instance: None,
        task: None,
        level,
        text: text.to_string(),
        event: None,
    }
}
