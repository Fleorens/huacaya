//! Integration tests for direct polling: a loaded instance is asked directly (`get_local`), not through
//! the router, when the poller is built with `with_local_instances` on a loopback URL.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use llamatop::collect::llama_poll::*;
use llamatop::http::is_loopback_url;

const TIEL: &str = "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M";
const INSTANCE_PORT: u16 = 35335;

/// A recorded `get` call: the path and its query pairs.
type CallLog = Vec<(String, Vec<(String, String)>)>;
/// A recorded `get_local` call: the instance port and the path.
type LocalLog = Vec<(u16, String)>;

/// Canned answers for `get` and `get_local`, plus a record of every call made.
#[derive(Clone)]
struct FakeFetch {
    answers: Arc<Mutex<HashMap<String, Result<String, String>>>>,
    calls: Arc<Mutex<CallLog>>,
    local_answers: Arc<Mutex<HashMap<String, Result<String, String>>>>,
    local_calls: Arc<Mutex<LocalLog>>,
}

impl FakeFetch {
    fn new() -> (Self, Arc<Mutex<CallLog>>, Arc<Mutex<LocalLog>>) {
        let answers = Arc::new(Mutex::new(HashMap::new()));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let local_answers = Arc::new(Mutex::new(HashMap::new()));
        let local_calls = Arc::new(Mutex::new(Vec::new()));
        let fetch =
            FakeFetch { answers, calls: calls.clone(), local_answers, local_calls: local_calls.clone() };
        (fetch, calls, local_calls)
    }

    /// A successful canned body for `path` (the router path).
    fn give(&self, path: &str, body: impl Into<String>) {
        self.answers.lock().unwrap().insert(path.to_string(), Ok(body.into()));
    }

    /// A successful canned body for `path` on the local instance.
    fn give_local(&self, path: &str, body: impl Into<String>) {
        self.local_answers.lock().unwrap().insert(path.to_string(), Ok(body.into()));
    }

    /// An error answer for `path` on the local instance.
    fn give_local_error(&self, path: &str, msg: impl Into<String>) {
        self.local_answers.lock().unwrap().insert(path.to_string(), Err(msg.into()));
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

    fn get_local(&self, port: u16, path: &str) -> Result<String> {
        self.local_calls.lock().unwrap().push((port, path.to_string()));
        match self.local_answers.lock().unwrap().get(path) {
            Some(Ok(body)) => Ok(body.clone()),
            Some(Err(msg)) => Err(anyhow::anyhow!("{msg}")),
            None => Err(anyhow::anyhow!("GET :{port}{path}: HTTP 404")),
        }
    }
}

fn llama(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/llama/{name}"));
    std::fs::read_to_string(path).expect("fixture present")
}

/// The fixture sysroot: the router (pid 1201) and the instance of `TIEL` (pid 48211, port 35335).
fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

/// A healthy router round: `/health`, `/props`, `/v1/models`, `/metrics`, `/slots`.
fn router_setup(fetch: &FakeFetch) {
    fetch.give("/health", r#"{"status":"ok"}"#);
    fetch.give("/props", llama("props-router.json"));
    fetch.give("/v1/models", llama("models.json"));
    fetch.give("/metrics", llama("metrics.prom"));
    fetch.give("/slots", llama("slots-idle.json"));
}

/// One call to `get` for `path` with a query carrying `model = TIEL`, if any.
fn calls_with_model(calls: &CallLog) -> bool {
    calls.iter().any(|(_p, q)| q.iter().any(|(k, v)| k == "model" && v == TIEL))
}

#[test]
fn loopback_urls() {
    assert!(is_loopback_url("http://127.0.0.1:8080"));
    assert!(is_loopback_url("http://localhost:8080/"));
    assert!(is_loopback_url("http://[::1]:8080"));
    assert!(!is_loopback_url("http://192.168.1.203:8080"));
    assert!(!is_loopback_url("http://127.0.0.1.example.com"));
}

#[test]
fn direct_round_skips_the_router() {
    let (fetch, calls, local_calls) = FakeFetch::new();
    router_setup(&fetch);
    fetch.give_local("/metrics", llama("metrics.prom"));
    fetch.give_local("/slots", llama("slots-idle.json"));

    let mut poller = Poller::new(fetch).with_local_instances(sysroot());
    let snap = poller.poll(Instant::now());

    let metrics = snap.active[0].metrics.as_ref().expect("metrics Some");
    assert_eq!(metrics.tokens_predicted_total, 24107.0);
    assert_eq!(snap.active[0].slots.len(), 1);

    // `get_local` was called with (35335, "/metrics") and (35335, "/slots").
    assert_eq!(
        &*local_calls.lock().unwrap(),
        &[(INSTANCE_PORT, "/metrics".to_string()), (INSTANCE_PORT, "/slots".to_string())]
    );

    // No `get` call carries a `model` query.
    assert!(!calls_with_model(&calls.lock().unwrap()));
}

#[test]
fn direct_failure_falls_back() {
    let (fetch, calls, local_calls) = FakeFetch::new();
    router_setup(&fetch);
    fetch.give_local("/metrics", llama("metrics.prom"));
    fetch.give_local_error("/slots", "connection reset");

    let mut poller = Poller::new(fetch).with_local_instances(sysroot());
    let snap = poller.poll(Instant::now());

    // Both filled from the router despite the `/slots` direct error.
    assert_eq!(snap.active.len(), 1);
    assert!(snap.active[0].metrics.is_some());
    assert_eq!(snap.active[0].slots.len(), 1);

    // Both direct calls were attempted before the fallback.
    assert_eq!(local_calls.lock().unwrap().len(), 2);

    // The fallback went through the router with `model = <the loaded id>`.
    let calls = calls.lock().unwrap();
    let queried: Vec<_> = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").collect();
    assert_eq!(queried.len(), 2);
    for (_p, q) in &queried {
        assert_eq!(q, &[("model".to_string(), TIEL.to_string())]);
    }
}

#[test]
fn no_instance_uses_the_router() {
    let (fetch, calls, local_calls) = FakeFetch::new();
    router_setup(&fetch);

    // An empty temp dir as sysroot: no `llama-server` processes.
    let dir = std::env::temp_dir().join(format!("llamatop-direct-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut poller = Poller::new(fetch).with_local_instances(dir.clone());
    let _ = poller.poll(Instant::now());

    // No direct call at all.
    assert!(local_calls.lock().unwrap().is_empty());

    // The router path with `model=`, as today.
    let calls = calls.lock().unwrap();
    let queried: Vec<_> = calls.iter().filter(|(p, _)| p == "/metrics" || p == "/slots").collect();
    assert_eq!(queried.len(), 2);
    for (_p, q) in &queried {
        assert_eq!(q, &[("model".to_string(), TIEL.to_string())]);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_direct_nothing_changes() {
    let (fetch, _calls, local_calls) = FakeFetch::new();
    router_setup(&fetch);

    let mut poller = Poller::new(fetch);
    let _ = poller.poll(Instant::now());

    assert!(local_calls.lock().unwrap().is_empty());
}

#[test]
fn direct_never_targets_unloaded_models() {
    let (fetch, calls, local_calls) = FakeFetch::new();
    router_setup(&fetch);
    // The instance's model is not `loaded` in this round (edit the JSON string, not the fixture).
    let mut models = llama("models.json");
    models = models.replace("\"value\":\"loaded\"", "\"value\":\"unloaded\"");
    fetch.give("/v1/models", models);

    let mut poller = Poller::new(fetch).with_local_instances(sysroot());
    let snap = poller.poll(Instant::now());

    assert!(snap.active.is_empty());
    assert!(local_calls.lock().unwrap().is_empty());

    // No `/metrics` or `/slots` call at all.
    let model_queries =
        calls.lock().unwrap().iter().filter(|(p, _)| p == "/metrics" || p == "/slots").count();
    assert_eq!(model_queries, 0);
}
