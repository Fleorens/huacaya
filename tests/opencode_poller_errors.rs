//! The OpenCode poller error paths (tasks/T42): a transient fetch/parse error keeps the last good
//! snapshot (steps, activity and last success time) with `error` set instead of flickering to empty; a
//! sessions or live error keeps the loaded session, a failed history load retries it. A `ScriptFetch`
//! double records every request and answers `/api/session` and every message request from a queue of
//! `Result<String, String>`, so a round can fail on fetch or parse; no network is needed.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use huacaya::collect::llama_poll::Fetch;
use huacaya::collect::opencode::*;

/// One recorded request: the path and the owned `(key, value)` query pairs.
type Record = (String, Vec<(String, String)>);

/// A `Fetch` that records every request and answers `/api/session` and every message request from a
/// queue of `Result<String, String>`, so a round can fail on fetch or parse. When the sessions queue is
/// empty it reuses the last served body.
struct ScriptFetch {
    records: Arc<Mutex<Vec<Record>>>,
    sessions: Arc<Mutex<VecDeque<Result<String, String>>>>,
    messages: Arc<Mutex<VecDeque<Result<String, String>>>>,
    last_sessions: Arc<Mutex<Option<String>>>,
}

impl Fetch for ScriptFetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String> {
        let mut records = self.records.lock().unwrap();
        records.push((path.to_string(), query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()));
        if path == "/api/session" {
            match self.sessions.lock().unwrap().pop_front() {
                Some(Ok(body)) => {
                    *self.last_sessions.lock().unwrap() = Some(body.clone());
                    Ok(body)
                }
                Some(Err(msg)) => Err(anyhow::anyhow!(msg)),
                None => Ok(self
                    .last_sessions
                    .lock()
                    .unwrap()
                    .clone()
                    .expect("the served sessions body is reused")),
            }
        } else {
            let body = self.messages.lock().unwrap().pop_front().expect("a queued message body");
            body.map_err(|msg| anyhow::anyhow!(msg))
        }
    }
}

/// A poller over `ScriptFetch`, plus a handle on the recorded requests.
fn setup(
    sessions: Vec<Result<String, String>>,
    messages: Vec<Result<String, String>>,
) -> (OpenCodePoller<ScriptFetch>, Arc<Mutex<Vec<Record>>>) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let fetch = ScriptFetch {
        records: records.clone(),
        sessions: Arc::new(Mutex::new(VecDeque::from(sessions))),
        messages: Arc::new(Mutex::new(VecDeque::from(messages))),
        last_sessions: Arc::new(Mutex::new(None)),
    };
    (OpenCodePoller::new(fetch), records)
}

/// One session entry in a `/api/session` answer.
fn sessions_with(ids: &[&str]) -> String {
    let parts: Vec<String> =
        ids.iter().map(|id| format!(r#"{{"id":"{}","title":"session {}"}}"#, id, id)).collect();
    format!(r#"{{"data":[{}]}}"#, parts.join(","))
}

/// One messages page: assistant messages, each with one tool part. `completed` set → `completed` status,
/// else `running`.
fn page(msgs: &[(&str, &str, &str, i64, Option<i64>)], next: Option<&str>) -> String {
    let mut items = Vec::new();
    for (mid, tid, tname, created, completed) in msgs {
        let status = if completed.is_some() { "completed" } else { "running" };
        let completed_field = match completed {
            Some(c) => format!(",\"completed\": {c}"),
            None => String::new(),
        };
        let part = format!(
            "{{\"type\":\"tool\",\"id\":\"{tid}\",\"name\":\"{tname}\",\"time\":{{\"created\":{created}{completed_field}}},\"state\":{{\"status\":\"{status}\"}}}}"
        );
        let msg = format!(
            "{{\"type\":\"assistant\",\"id\":\"{mid}\",\"time\":{{\"created\":{created}}},\"content\":[{part}]}}"
        );
        items.push(msg);
    }
    let cursor = match next {
        Some(n) => format!("\"cursor\":{{\"next\":\"{n}\"}}"),
        None => "\"cursor\":null".to_string(),
    };
    format!("{{\"data\":[{}],{cursor}}}", items.join(","))
}

#[test]
fn transient_error_keeps_the_last_snapshot() {
    let hist = page(
        &[
            ("m1", "t1", "read", 100, Some(110)),
            ("m2", "t2", "write", 105, Some(120)),
            ("m3", "t3", "read", 110, Some(115)),
        ],
        None,
    );
    let live = page(&[("m3", "t3", "read", 110, Some(115)), ("m4", "t4", "shell", 120, None)], None);
    let sessions = sessions_with(&["sessA"]);
    let (mut poller, records) =
        setup(vec![Ok(sessions.clone()), Err("boom".to_string()), Ok(sessions)], vec![Ok(hist), Ok(live)]);

    // Round 1: history, no error.
    let snap1 = poller.poll_at(Some("sessA"), 1_000);
    assert!(snap1.error.is_none());
    assert_eq!(snap1.steps.len(), 3);
    assert_eq!(snap1.focus.as_deref(), Some("sessA"));

    // Round 2: the sessions fetch fails → keeps round 1's data, error set, round 1 time.
    let snap2 = poller.poll_at(Some("sessA"), 2_000);
    assert!(snap2.error.is_some());
    assert_eq!(snap2.sessions, snap1.sessions);
    assert_eq!(snap2.steps, snap1.steps);
    assert_eq!(snap2.focus.as_deref(), Some("sessA"));
    assert_eq!(snap2.fetched_ms, 1_000);

    // Round 3: succeeds, live only (no history reload).
    let snap3 = poller.poll_at(Some("sessA"), 3_000);
    assert!(snap3.error.is_none());

    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 5);
    assert_eq!(rec[0].0, "/api/session");
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(rec[3].0, "/api/session");
    assert_eq!(rec[4].1, vec![("limit".to_string(), "12".to_string())]);
}

#[test]
fn live_error_keeps_the_history() {
    let hist = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let live = page(&[("a1", "ta", "read", 100, Some(110)), ("b1", "tb", "shell", 110, None)], None);
    let sessions = sessions_with(&["sessA"]);
    let (mut poller, records) = setup(
        vec![Ok(sessions.clone()), Ok(sessions.clone()), Ok(sessions)],
        vec![Ok(hist), Err("boom".to_string()), Ok(live)],
    );

    let snap1 = poller.poll_at(Some("sessA"), 1_000);
    assert!(snap1.error.is_none());
    assert_eq!(snap1.steps.len(), 1);

    // Round 2: the live messages fetch fails → keeps round 1's history, error set.
    let snap2 = poller.poll_at(Some("sessA"), 2_000);
    assert!(snap2.error.is_some());
    assert_eq!(snap2.steps, snap1.steps);
    assert_eq!(snap2.fetched_ms, 1_000);

    // Round 3: live only (no history reload).
    let snap3 = poller.poll_at(Some("sessA"), 3_000);
    assert!(snap3.error.is_none());

    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 6);
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(rec[3].0, "/api/session/sessA/message");
    assert_eq!(rec[5].1, vec![("limit".to_string(), "12".to_string())]);
}

#[test]
fn first_error_is_empty() {
    let (mut poller, _records) = setup(vec![Err("boom".to_string())], vec![]);

    // The very first round fails → empty sessions/steps, error set, no prior success to keep.
    let snap = poller.poll_at(Some("sessA"), 1_000);
    assert!(snap.error.is_some());
    assert!(snap.sessions.is_empty());
    assert!(snap.steps.is_empty());
    assert_eq!(snap.focus, None);
    assert_eq!(snap.fetched_ms, 0);
}

#[test]
fn history_error_retries_history() {
    let hist = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let sessions = sessions_with(&["sessA"]);
    let (mut poller, records) =
        setup(vec![Ok(sessions.clone()), Ok(sessions)], vec![Err("boom".to_string()), Ok(hist)]);

    // Round 1: history fetch fails, no prior success → empty.
    let snap1 = poller.poll_at(Some("sessA"), 1_000);
    assert!(snap1.error.is_some());
    assert!(snap1.sessions.is_empty());
    assert!(snap1.steps.is_empty());

    // Round 2: history retried (limit 200), succeeds.
    let snap2 = poller.poll_at(Some("sessA"), 2_000);
    assert!(snap2.error.is_none());
    assert_eq!(snap2.steps.len(), 1);
    assert_eq!(snap2.focus.as_deref(), Some("sessA"));

    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 4);
    assert_eq!(rec[1].0, "/api/session/sessA/message");
    assert_eq!(rec[3].1, vec![("limit".to_string(), "200".to_string())]);
}
