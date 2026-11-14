//! The OpenCode poller (tasks/T42): `next_cursor`, whole history once then light live rounds, the page
//! cap, focus changes and error reloads, and the snapshot window. A `ScriptFetch` double records every
//! `(path, query)` and serves bodies from a queue, so no network is needed.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use huacaya::collect::activity::Station;
use huacaya::collect::llama_poll::Fetch;
use huacaya::collect::opencode::*;

/// One recorded request: the path and the owned `(key, value)` query pairs.
type Record = (String, Vec<(String, String)>);

/// A `Fetch` that records every request and answers `/api/session` with a fixed body, every message
/// request with the next body from a queue.
struct ScriptFetch {
    records: Arc<Mutex<Vec<Record>>>,
    sessions: String,
    messages: Arc<Mutex<VecDeque<String>>>,
}

impl Fetch for ScriptFetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String> {
        let mut records = self.records.lock().unwrap();
        records.push((path.to_string(), query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()));
        if path == "/api/session" {
            Ok(self.sessions.clone())
        } else {
            Ok(self.messages.lock().unwrap().pop_front().expect("a queued message body"))
        }
    }
}

/// A poller over `ScriptFetch`, plus a handle on the recorded requests.
fn setup(sessions: String, bodies: Vec<String>) -> (OpenCodePoller<ScriptFetch>, Arc<Mutex<Vec<Record>>>) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let fetch = ScriptFetch {
        records: records.clone(),
        sessions,
        messages: Arc::new(Mutex::new(VecDeque::from(bodies))),
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

/// A page of `n` `read` steps, newest first in `data`, created from `oldest_created` down.
fn page_of(n: usize, oldest_created: i64, next: Option<&str>) -> String {
    let ids: Vec<String> = (0..n).map(|i| format!("m{i}")).collect();
    let names: Vec<String> = (0..n).map(|i| format!("t{i}")).collect();
    let mut rows: Vec<(&str, &str, &str, i64, Option<i64>)> = Vec::with_capacity(n);
    for i in 0..n {
        let created = oldest_created + (n - 1 - i) as i64; // data[0] is the newest
        rows.push((ids[i].as_str(), names[i].as_str(), "read", created, Some(created + 1)));
    }
    page(&rows, next)
}

#[test]
fn next_cursor_values() {
    assert_eq!(
        next_cursor(r#"{"data": [], "cursor": {"next": "abc", "previous": null}}"#),
        Some("abc".to_string())
    );
    assert_eq!(next_cursor(r#"{"data": [], "cursor": {"next": null}}"#), None);
    assert_eq!(next_cursor(r#"{"data": []}"#), None);
    assert_eq!(next_cursor("not json"), None);
}

#[test]
fn history_then_live() {
    let round1 = page(
        &[
            ("m1", "t1", "read", 100, Some(110)),
            ("m2", "t2", "write", 105, Some(120)),
            ("m3", "t3", "read", 110, Some(115)),
        ],
        None,
    );
    let round2 = page(
        &[
            ("m1", "t1", "read", 100, Some(110)),
            ("m2", "t2", "write", 105, Some(120)),
            ("m3", "t3", "read", 110, Some(115)),
            ("m4", "t4", "shell", 120, None),
        ],
        None,
    );
    let round3 = page(&[("m4", "t4", "shell", 120, Some(130))], None);
    let (mut poller, records) = setup(sessions_with(&["sess_done"]), vec![round1, round2, round3]);

    // Round 1: history, no pulses.
    let snap = poller.poll_at(Some("sess_done"), 1_000);
    assert_eq!(snap.steps.len(), 3);
    assert!(snap.activity.pulses.is_empty());

    // Round 2: live, one new running shell → one outbound pulse.
    let snap = poller.poll_at(Some("sess_done"), 2_000);
    assert_eq!(snap.steps.len(), 4);
    let pulses = &snap.activity.pulses;
    assert_eq!(pulses.len(), 1);
    assert_eq!(pulses[0].station, Station::Shell);
    assert_eq!(pulses[0].at_ms, 2_000);
    assert!(pulses[0].outbound);

    // Round 3: the shell completes → a new inbound pulse at 3_000 (round 2's outbound is still retained).
    let snap = poller.poll_at(Some("sess_done"), 3_000);
    assert_eq!(snap.steps.len(), 4);
    let inbound: Vec<_> = snap.activity.pulses.iter().filter(|p| !p.outbound && p.at_ms == 3_000).collect();
    assert_eq!(inbound.len(), 1);

    // Requests: /api/session, history (limit 200), then two live rounds (limit 12).
    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 6);
    assert_eq!(rec[0].0, "/api/session");
    assert_eq!(rec[0].1, Vec::<(String, String)>::new());
    assert_eq!(rec[1].0, "/api/session/sess_done/message");
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(rec[3].1, vec![("limit".to_string(), "12".to_string())]);
    assert_eq!(rec[5].1, vec![("limit".to_string(), "12".to_string())]);
}

#[test]
fn history_pages() {
    let p1 = page_of(200, 1_000, Some("c1"));
    let p2: &[(&str, &str, &str, i64, Option<i64>)] = &[
        ("msg_a", "z4", "read", 5_004, Some(5_005)),
        ("msg_b", "z3", "read", 5_003, Some(5_004)),
        ("msg_c", "z2", "read", 5_002, Some(5_003)),
        ("msg_d", "z1", "read", 5_001, Some(5_002)),
        ("msg_e", "z0", "read", 5_000, Some(5_001)),
    ];
    let p2 = page(p2, None);
    let (mut poller, records) = setup(sessions_with(&["sess_done"]), vec![p1, p2]);

    let snap = poller.poll_at(Some("sess_done"), 1_000);
    // The kept history is the whole 205 steps (oldest first); the snapshot caps them.
    assert_eq!(poller.steps().len(), 205);
    // The first kept step is the oldest of the second (older) page.
    assert_eq!(poller.steps()[0].id, "z0");
    assert_eq!(poller.steps()[0].time_ms, 5_000);
    assert_eq!(snap.steps.len(), SNAPSHOT_STEPS);

    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 3);
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(
        rec[2].1,
        vec![("limit".to_string(), "200".to_string()), ("cursor".to_string(), "c1".to_string())]
    );
}

#[test]
fn history_page_cap() {
    let bodies = (0..HISTORY_PAGES).map(|_| page_of(200, 0, Some("tok"))).collect();
    let (mut poller, records) = setup(sessions_with(&["sess_done"]), bodies);

    let snap = poller.poll_at(Some("sess_done"), 1_000);
    assert!(snap.error.is_none());
    assert_eq!(snap.steps.len(), SNAPSHOT_STEPS);

    let rec = records.lock().unwrap();
    assert_eq!(rec.len(), 1 + HISTORY_PAGES);
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(
        rec[2].1,
        vec![("limit".to_string(), "200".to_string()), ("cursor".to_string(), "tok".to_string())]
    );
}

#[test]
fn focus_change_reloads() {
    let a_hist = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let a_live = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let b_hist = page(&[("b1", "tb", "shell", 100, None)], None);
    let (mut poller, records) = setup(sessions_with(&["sessA", "sessB"]), vec![a_hist, a_live, b_hist]);

    let _ = poller.poll_at(Some("sessA"), 1_000);
    let _ = poller.poll_at(Some("sessA"), 2_000);
    let snap_b = poller.poll_at(Some("sessB"), 3_000);

    // B's activity holds only B's steps.
    assert_eq!(snap_b.activity.session.as_deref(), Some("sessB"));
    assert_eq!(snap_b.steps.len(), 1);
    assert_eq!(snap_b.activity.stations.iter().map(|s| s.calls).sum::<u32>(), 1);

    // The focus change makes a limit-200 request again.
    let rec = records.lock().unwrap();
    assert_eq!(rec[5].0, "/api/session/sessB/message");
    assert_eq!(rec[5].1, vec![("limit".to_string(), "200".to_string())]);
}

#[test]
fn error_reloads() {
    let a_hist = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let bad = "not json".to_string();
    let a_hist2 = page(&[("a1", "ta", "read", 100, Some(110))], None);
    let (mut poller, records) = setup(sessions_with(&["sessA"]), vec![a_hist, bad, a_hist2]);

    assert!(poller.poll_at(Some("sessA"), 1_000).error.is_none());
    assert!(poller.poll_at(Some("sessA"), 2_000).error.is_some());
    let snap3 = poller.poll_at(Some("sessA"), 3_000);
    assert!(snap3.error.is_none());

    let rec = records.lock().unwrap();
    assert_eq!(rec[1].1, vec![("limit".to_string(), "200".to_string())]);
    assert_eq!(rec[3].1, vec![("limit".to_string(), "12".to_string())]);
    assert_eq!(rec[5].1, vec![("limit".to_string(), "200".to_string())]);
}

#[test]
fn snapshot_window() {
    // A history of 250 steps (200 + 50): the snapshot keeps the newest SNAPSHOT_STEPS, ending newest.
    let p1 = page_of(200, 6_000, Some("c1"));
    let p2 = page_of(50, 5_000, None);
    let (mut poller, _records) = setup(sessions_with(&["sessA"]), vec![p1, p2]);

    let snap = poller.poll_at(Some("sessA"), 1_000);
    assert_eq!(snap.steps.len(), SNAPSHOT_STEPS);
    assert_eq!(snap.steps.last().unwrap().time_ms, 6_199);
}
