//! Integration tests for `collect::llama_poll`: one safe polling round and the spawn loop.
//!
//! A `FakeFetch` answers canned bodies keyed by path (loaded from `tests/fixtures/llama/`) and
//! records every call, so the tests run without a network and can assert the safety rule.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use llamatop::collect::llama_poll::*;

const TIEL: &str = "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M";

/// A recorded call: the path and its query pairs.
type CallLog = Vec<(String, Vec<(String, String)>)>;

/// Canned answers keyed by path, and a record of every `(path, query)` call.
#[derive(Clone)]
struct FakeFetch {
    answers: Arc<Mutex<HashMap<String, Result<String, String>>>>,
    calls: Arc<Mutex<CallLog>>,
}

impl FakeFetch {
    fn new() -> (Self, Arc<Mutex<CallLog>>) {
        let answers = Arc::new(Mutex::new(HashMap::new()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fetch = FakeFetch { answers, calls: calls.clone() };
        (fetch, calls)
    }

    /// A successful canned body for `path`.
    fn give(&self, path: &str, body: impl Into<String>) {
        self.answers.lock().unwrap().insert(path.to_string(), Ok(body.into()));
    }

    /// An error answer for `path` (the message is surfaced verbatim).
    fn give_error(&self, path: &str, msg: impl Into<String>) {
        self.answers.lock().unwrap().insert(path.to_string(), Err(msg.into()));
    }
}

impl Fetch for FakeFetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<String> {
        self.calls
            .lock()
            .unwrap()
            .push((path.to_string(), query.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()));
        match self.answers.lock().unwrap().get(path) {
            Some(Ok(body)) => Ok(body.clone()),
            Some(Err(msg)) => Err(anyhow::anyhow!("{msg}")),
            None => Err(anyhow::anyhow!("GET {path}: HTTP 404")),
        }
    }
}

fn llama(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/llama/{name}"));
    std::fs::read_to_string(path).expect("fixture present")
}

/// A healthy router round, with the idle loaded model.
fn router_setup(fetch: &FakeFetch) {
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-idle.json"));
}

#[test]
fn healthy_router_round() {
    let (fetch, _calls) = FakeFetch::new();
    router_setup(&fetch);

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert_eq!(snap.health, Health::Ok);
    assert!(snap.router);
    assert_eq!(snap.build_info.as_deref(), Some("b12151-10d4846a5"));
    assert_eq!(snap.models.len(), 5);
    assert_eq!(snap.active.len(), 1);
    assert_eq!(snap.active[0].id, TIEL);
    let metrics = snap.active[0].metrics.as_ref().expect("metrics Some");
    assert_eq!(metrics.tokens_predicted_total, 24107.0);
    assert_eq!(snap.active[0].slots.len(), 1);
    assert_eq!(snap.active[0].live_gen_tps, None);
}

#[test]
fn only_loaded_models_are_queried() {
    let (fetch, calls) = FakeFetch::new();
    router_setup(&fetch);

    let mut poller = Poller::new(fetch);
    let _ = poller.poll(Instant::now());

    let calls = calls.lock().unwrap().clone();
    let queried: Vec<_> = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").collect();
    assert_eq!(queried.len(), 2);
    for (_p, q) in &queried {
        assert_eq!(q, &[("model".to_string(), TIEL.to_string())]);
    }
    // No recorded call anywhere carries another model id.
    for (_p, q) in &calls {
        for (k, v) in q {
            assert_eq!(k, "model");
            assert_eq!(v, TIEL);
        }
    }
}

#[test]
fn nothing_loaded_means_no_model_query() {
    let (fetch, calls) = FakeFetch::new();
    router_setup(&fetch);
    let mut models = llama("models.json");
    models = models.replace("\"value\":\"loaded\"", "\"value\":\"unloaded\"");
    fetch.give("/v1/models", models);

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert!(snap.active.is_empty());
    let calls = calls.lock().unwrap();
    let model_queries = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").count();
    assert_eq!(model_queries, 0);
}

#[test]
fn loading_model_is_not_queried() {
    let (fetch, calls) = FakeFetch::new();
    router_setup(&fetch);
    let mut models = llama("models.json");
    models = models.replace("\"value\":\"loaded\"", "\"value\":\"loading\"");
    fetch.give("/v1/models", models);

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert!(snap.active.is_empty());
    let calls = calls.lock().unwrap();
    let model_queries = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").count();
    assert_eq!(model_queries, 0);
}

#[test]
fn server_down() {
    let (fetch, calls) = FakeFetch::new();
    fetch.give_error("/health", "GET /health: connection refused");

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert!(matches!(snap.health, Health::Down(_)));
    assert!(snap.models.is_empty());
    assert!(snap.active.is_empty());

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "/health");
}

#[test]
fn unauthorized() {
    let (fetch, _calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give_error("/v1/models", "GET /v1/models: HTTP 401");

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert_eq!(snap.health, Health::Unauthorized);
    assert!(snap.models.is_empty());
    assert!(snap.active.is_empty());
}

#[test]
fn props_are_fetched_once() {
    let (fetch, calls) = FakeFetch::new();
    router_setup(&fetch);

    let mut poller = Poller::new(fetch);
    poller.poll(Instant::now());
    poller.poll(Instant::now());

    let calls = calls.lock().unwrap();
    let props = calls.iter().filter(|(p, _)| p == "/props").count();
    assert_eq!(props, 1);
}

#[test]
fn metrics_failure_is_not_fatal() {
    let (fetch, _calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    // No /metrics answer → 404 for that call only.
    fetch.give("/slots", llama("slots-idle.json"));

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert_eq!(snap.health, Health::Ok);
    assert_eq!(snap.active.len(), 1);
    assert_eq!(snap.active[0].metrics, None);
    assert_eq!(snap.active[0].slots.len(), 1);
}

#[test]
fn single_model_server() {
    let (fetch, calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", r#"{}"#);
    fetch.give("/v1/models", r#"{"data":[{"id":"solo"}]}"#);
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-idle.json"));

    let mut poller = Poller::new(fetch);
    let snap = poller.poll(Instant::now());

    assert!(!snap.router);
    assert_eq!(snap.active.len(), 1);
    assert_eq!(snap.active[0].id, "solo");

    let calls = calls.lock().unwrap();
    let queried: Vec<_> = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").collect();
    assert_eq!(queried.len(), 2);
    for (_p, q) in &queried {
        assert!(q.is_empty());
    }
}

#[test]
fn live_speed() {
    let (fetch, _calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-busy.json")); // n_decoded 427

    let mut poller = Poller::new(fetch.clone());
    let t0 = Instant::now();
    let snap1 = poller.poll(t0);
    assert_eq!(snap1.active[0].live_gen_tps, None);

    // Next sample: n_decoded grew to 552, 2 s later.
    let mut slots = llama("slots-busy.json");
    slots = slots.replace("\"n_decoded\":427", "\"n_decoded\":552");
    fetch.give("/slots", slots);

    let t1 = t0 + Duration::from_secs(2);
    let snap2 = poller.poll(t1);
    let tps = snap2.active[0].live_gen_tps.expect("live Some");
    assert!((tps - 62.5).abs() < 0.01);
}

#[test]
fn live_speed_resets_on_new_task() {
    let (fetch, _calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-busy.json"));

    let mut poller = Poller::new(fetch.clone());
    let t0 = Instant::now();
    let _ = poller.poll(t0);

    // Same generation, but on a new id_task.
    let mut slots = llama("slots-busy.json");
    slots = slots
        .replace("\"id_task\":8040", "\"id_task\":8041")
        .replace("\"n_decoded\":427", "\"n_decoded\":552");
    fetch.give("/slots", slots);

    let t1 = t0 + Duration::from_secs(2);
    let snap2 = poller.poll(t1);
    assert_eq!(snap2.active[0].live_gen_tps, None);
}

#[test]
fn live_speed_reference_dropped_when_idle() {
    let (fetch, _calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-busy.json"));

    let mut poller = Poller::new(fetch.clone());
    let t0 = Instant::now();
    let _ = poller.poll(t0);

    // No processing slot: `None`, and the reference must be forgotten.
    fetch.give("/slots", llama("slots-idle.json"));
    let snap2 = poller.poll(t0 + Duration::from_secs(1));
    assert_eq!(snap2.active[0].live_gen_tps, None);

    // Same task again with growth: no stale reference, so still `None`.
    fetch.give("/slots", llama("slots-busy.json").replace("\"n_decoded\":427", "\"n_decoded\":552"));
    let snap3 = poller.poll(t0 + Duration::from_secs(2));
    assert_eq!(snap3.active[0].live_gen_tps, None);
}

#[test]
fn spawn_stops_on_flag() {
    let (fetch, _calls) = FakeFetch::new();
    router_setup(&fetch);

    let poller = Poller::new(fetch);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let count = Arc::new(Mutex::new(0u64));
    let count_for_thread = count.clone();
    let handle = spawn(poller, Duration::from_millis(50), stop_for_thread, move |_snap| {
        *count_for_thread.lock().unwrap() += 1;
        true
    });

    std::thread::sleep(Duration::from_millis(300));
    stop.store(true, Ordering::Relaxed);
    let asked = Instant::now();
    handle.join().expect("the thread joins");
    assert!(asked.elapsed() < Duration::from_secs(1));

    assert!(*count.lock().unwrap() >= 2);
}

#[test]
fn spawn_stops_when_sink_refuses() {
    let (fetch, _calls) = FakeFetch::new();
    router_setup(&fetch);

    let poller = Poller::new(fetch);
    let stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let handle = spawn(poller, Duration::from_millis(50), stop, |_snap| false);

    handle.join().expect("the thread joins as soon as the sink refuses");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn refused_key_backs_off() {
    // Each refused request prints a warning in the server's log: after a 401, stay silent for 30 s.
    let (fetch, calls) = FakeFetch::new();
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give_error("/v1/models", "GET /v1/models: HTTP 401");
    let mut poller = Poller::new(fetch);
    let t0 = Instant::now();
    assert_eq!(poller.poll(t0).health, Health::Unauthorized);
    let after_first = calls.lock().unwrap().len();
    for s in 1..30 {
        assert_eq!(poller.poll(t0 + Duration::from_secs(s)).health, Health::Unauthorized);
    }
    assert_eq!(calls.lock().unwrap().len(), after_first, "no request during the back-off");
    poller.poll(t0 + UNAUTHORIZED_RETRY);
    assert!(calls.lock().unwrap().len() > after_first, "asks again after the back-off");
}
