//! The OpenCode poller (tasks/T42): load a followed session's whole history once (pages of 200), then
//! every round fetch only its newest messages and merge them by step id, so the map sees steps within a
//! second. Rounds run through any `llama_poll::Fetch` so tests need no network.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::collect::activity::Activity;
use crate::collect::llama_poll::Fetch;
use crate::collect::opencode::{
    HISTORY_PAGE, HISTORY_PAGES, LIVE_LIMIT, OpenCodeSnapshot, SNAPSHOT_STEPS, Step, next_cursor,
    parse_sessions, parse_steps,
};

/// Polls OpenCode through any `llama_poll::Fetch`.
pub struct OpenCodePoller<F: Fetch> {
    fetch: F,
    /// The id of the session whose history is loaded, `None` at first or after an error.
    loaded_session: Option<String>,
    /// The followed session's steps, oldest first, kept between rounds.
    steps: Vec<Step>,
    /// The map's state for the followed session.
    activity: Activity,
}

impl<F: Fetch> OpenCodePoller<F> {
    /// A poller over `fetch`.
    pub fn new(fetch: F) -> Self {
        Self { fetch, loaded_session: None, steps: Vec::new(), activity: Activity::default() }
    }

    /// The followed session's kept steps, oldest first: the whole history merged with every live round.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// One round at wall-clock `now_ms`; `poll` calls it with the current time.
    pub fn poll_at(&mut self, focus: Option<&str>, now_ms: i64) -> OpenCodeSnapshot {
        let empty = |error: String| OpenCodeSnapshot { error: Some(error), ..Default::default() };

        // 1. The sessions, then the choice of the focus, exactly as before.
        let sessions = match self.fetch.get("/api/session", &[]) {
            Ok(body) => match parse_sessions(&body) {
                Ok(sessions) => sessions,
                Err(e) => {
                    self.loaded_session = None;
                    return empty(format!("{e:#}"));
                }
            },
            Err(e) => {
                self.loaded_session = None;
                return empty(fetch_error(&e));
            }
        };
        let focus = match focus {
            Some(id) if sessions.iter().any(|s| s.id == id) => Some(id.to_string()),
            _ => sessions.iter().find(|s| s.running).or_else(|| sessions.first()).map(|s| s.id.clone()),
        };

        // No focus (no sessions): keep nothing, steps empty, activity default.
        if focus.is_none() {
            self.steps.clear();
            self.activity = Activity::default();
            return OpenCodeSnapshot {
                sessions,
                focus: None,
                steps: Vec::new(),
                activity: Activity::default(),
                error: None,
                fetched_ms: now_ms,
            };
        }
        let focus_id = focus.as_ref().unwrap();
        let directory = sessions.iter().find(|s| s.id == *focus_id).and_then(|s| s.directory.clone());

        // A different session than the loaded one: load its whole history first.
        let history = self.loaded_session.as_deref() != Some(focus_id.as_str());
        if history {
            match self.load_history(focus_id, directory.as_deref()) {
                Ok(steps) => {
                    self.steps = steps;
                    self.loaded_session = Some(focus_id.clone());
                    // A new session: a history round, no pulses.
                    self.activity.update(focus_id, &self.steps, now_ms);
                }
                Err(e) => {
                    self.loaded_session = None;
                    return empty(e);
                }
            }
        } else {
            // Live: fetch only the newest messages and merge them by step id.
            let limit = LIVE_LIMIT.to_string();
            let query: Vec<(&str, &str)> = vec![("limit", limit.as_str())];
            let body = match self.fetch.get(&format!("/api/session/{focus_id}/message"), &query) {
                Ok(body) => body,
                Err(e) => {
                    self.loaded_session = None;
                    return empty(fetch_error(&e));
                }
            };
            let round_steps = match parse_steps(&body, directory.as_deref()) {
                Ok(steps) => steps,
                Err(e) => {
                    self.loaded_session = None;
                    return empty(format!("{e:#}"));
                }
            };
            self.activity.update(focus_id, &round_steps, now_ms);
            merge_kept(&mut self.steps, round_steps);
        }

        // The snapshot: the newest SNAPSHOT_STEPS kept steps (oldest first), a clone of the activity.
        let steps =
            self.steps.iter().skip(self.steps.len().saturating_sub(SNAPSHOT_STEPS)).cloned().collect();
        OpenCodeSnapshot {
            sessions,
            focus,
            steps,
            activity: self.activity.clone(),
            error: None,
            fetched_ms: now_ms,
        }
    }

    /// The whole history of `focus_id`, oldest first: pages of 200 newest first, then reversed. Stops when a
    /// page holds fewer than `HISTORY_PAGE` messages, when no `cursor.next` token remains, or after
    /// `HISTORY_PAGES` pages.
    fn load_history(&mut self, focus_id: &str, directory: Option<&str>) -> Result<Vec<Step>, String> {
        let mut pages: Vec<Vec<Step>> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let limit = HISTORY_PAGE.to_string();
            let mut query: Vec<(&str, &str)> = vec![("limit", limit.as_str())];
            if let Some(token) = &cursor {
                query.push(("cursor", token.as_str()));
            }
            let body = self
                .fetch
                .get(&format!("/api/session/{focus_id}/message"), &query)
                .map_err(|e| fetch_error(&e))?;
            let page = parse_steps(&body, directory).map_err(|e| format!("{e:#}"))?;
            let full = page.len() == HISTORY_PAGE;
            pages.push(page);

            let more = full && next_cursor(&body).is_some() && pages.len() < HISTORY_PAGES;
            if !more {
                break;
            }
            cursor = next_cursor(&body);
        }
        pages.reverse();
        Ok(pages.into_iter().flatten().collect())
    }

    /// One round: the sessions, then the steps of `focus` when it is one of them, else of the newest
    /// running session, else of the newest session. An error gives a snapshot with `error` set and empty lists.
    pub fn poll(&mut self, focus: Option<&str>) -> OpenCodeSnapshot {
        self.poll_at(focus, now_ms())
    }
}

/// Merge `round` into the kept steps: a step whose id is already kept replaces it in place, a new one is
/// appended. Keeps the kept order (oldest first).
fn merge_kept(steps: &mut Vec<Step>, round: Vec<Step>) {
    for step in round {
        match steps.iter().position(|s| s.id == step.id) {
            Some(pos) => steps[pos] = step,
            None => steps.push(step),
        }
    }
}

/// Wall-clock round time in Unix ms, `0` when the clock cannot be read.
fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A fetch error as shown: the server's answer as is (`GET …: HTTP 401`), else `unreachable: …`.
fn fetch_error(e: &anyhow::Error) -> String {
    let message = format!("{e:#}");
    if message.contains(": HTTP ") { message } else { format!("unreachable: {message}") }
}
