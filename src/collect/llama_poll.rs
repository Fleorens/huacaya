//! One polling round of the llama.cpp server into an immutable `LlamaSnapshot`, plus the thread
//! that runs rounds on an interval.
//!
//! The round is **safe** (see `docs/01-architecture.md` § 2.2): `/metrics` and `/slots` are queried
//! only for models whose `status.value` is `Loaded` in the same round's `/v1/models` answer, so a
//! request never loads the model in use.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::collect::llama::{Metrics, ModelInfo, ModelState, SlotInfo, parse_metrics, parse_slots};
use crate::collect::process::{ProcSample, ServerRole, read_server_procs};
use crate::http::Client;

/// What the poller needs from an HTTP client; lets the tests run without a network.
pub trait Fetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String>;
    /// GET `path` on the local instance listening on `port`. The default refuses, so fetchers that cannot
    /// reach instances (and the existing test doubles) keep working unchanged.
    fn get_local(&self, port: u16, path: &str) -> anyhow::Result<String> {
        anyhow::bail!("GET :{port}{path}: no direct access")
    }
}

impl Fetch for Client {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String> {
        Client::get(self, path, query)
    }

    fn get_local(&self, port: u16, path: &str) -> anyhow::Result<String> {
        Client::get_local(self, port, path)
    }
}

/// Health of the server at the end of a round.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Health {
    Ok,
    /// `/health` failed; the message is the error text (it never contains the key).
    Down(String),
    /// `/health` is fine but `/v1/models` answered 401: missing or wrong API key.
    Unauthorized,
}

/// A model that is loaded, and therefore polled this round.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ActiveModel {
    pub id: String,
    pub metrics: Option<Metrics>,
    pub slots: Vec<SlotInfo>,
    /// Tokens per second generated right now, see "Live speed".
    pub live_gen_tps: Option<f64>,
}

/// One immutable snapshot of the server, assembled by a single polling round.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LlamaSnapshot {
    pub health: Health,
    pub router: bool,
    pub build_info: Option<String>,
    pub models: Vec<ModelInfo>,
    /// One entry per model in state `Loaded`, same order as `models`.
    pub active: Vec<ActiveModel>,
}

/// The last reading of a generating slot, used to derive `live_gen_tps` on the next round.
#[derive(Debug, Clone, Copy)]
struct LiveRef {
    id_task: i64,
    n_decoded: u64,
    instant: Instant,
}

/// A polling round over any `Fetch`. Owns the cached props and the live-speed references between
/// rounds; never panics and never returns an error.
pub struct Poller<F: Fetch> {
    fetch: F,
    router: bool,
    build_info: Option<String>,
    props_cached: bool,
    live: HashMap<String, LiveRef>,
    /// When set, each round reads the loaded instances under `sysroot` and asks them directly.
    local_instances: bool,
    sysroot: PathBuf,
}

impl<F: Fetch> Poller<F> {
    /// Creates a poller over `fetch`, with no props cached yet.
    pub fn new(fetch: F) -> Self {
        Self {
            fetch,
            router: false,
            build_info: None,
            props_cached: false,
            live: HashMap::new(),
            local_instances: false,
            sysroot: PathBuf::new(),
        }
    }

    /// Enables direct polling: each round reads the `llama-server` processes under `sysroot` and, for a
    /// `Loaded` model whose instance has a `port`, asks `/metrics` and `/slots` on that port directly.
    /// See `docs/01-architecture.md` § 2.2.
    pub fn with_local_instances(self, sysroot: PathBuf) -> Self {
        Self { local_instances: true, sysroot, ..self }
    }

    /// One round. Never panics, never returns an error: failures end up in `health` or as `None`s.
    pub fn poll(&mut self, now: Instant) -> LlamaSnapshot {
        // 1. `/health`. Any failure stops the round, keeping last-known props in the snapshot.
        if let Err(e) = self.fetch.get("/health", &[]) {
            // `{:#}` keeps the cause chain (connection refused, timeout), not just the outer context.
            let msg = format!("{e:#}");
            let snapshot = LlamaSnapshot {
                health: Health::Down(msg),
                router: self.router,
                build_info: self.build_info.clone(),
                models: Vec::new(),
                active: Vec::new(),
            };
            // Forget the cached props and the live-speed references: re-fetch next round.
            self.router = false;
            self.build_info = None;
            self.props_cached = false;
            self.live.clear();
            return snapshot;
        }

        // 2. `/props` only when not yet cached. On failure nothing changes: while uncached, `router` is
        // already `false` and `build_info` `None`, and the fetch is retried next round.
        if !self.props_cached {
            let props =
                self.fetch.get("/props", &[]).ok().and_then(|b| crate::collect::llama::parse_props(&b).ok());
            if let Some(props) = props {
                self.router = props.router;
                self.build_info = props.build_info;
                self.props_cached = true;
            }
        }

        // 3. `/v1/models`.
        let models = match self.fetch.get("/v1/models", &[]) {
            Ok(body) => match crate::collect::llama::parse_models(&body) {
                Ok(models) => models,
                Err(e) => return self.empty_snapshot(Health::Down(format!("{e:#}"))),
            },
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("HTTP 401") {
                    return self.empty_snapshot(Health::Unauthorized);
                }
                return self.empty_snapshot(Health::Down(msg));
            }
        };

        // 4. Read the `llama-server` processes once, so a loaded model may be reached directly.
        let instances = if self.local_instances { read_server_procs(&self.sysroot) } else { Vec::new() };

        // 5. Poll only the loaded models, and derive live speed per model.
        let mut active = Vec::new();
        for model in &models {
            if model.state != ModelState::Loaded {
                continue; // safety rule: never query a model that is not loaded.
            }
            let (metrics, slots) = match Self::instance_for(&instances, &model.id) {
                Some(port) => {
                    match (self.fetch.get_local(port, "/metrics"), self.fetch.get_local(port, "/slots")) {
                        (Ok(m_body), Ok(s_body)) => {
                            (Some(parse_metrics(&m_body)), parse_slots(&s_body).ok().unwrap_or_default())
                        }
                        // Either direct call failed: fall back to the router path for both, this round.
                        _ => self.router_metrics_slots(&model.id),
                    }
                }
                // No instance for this model: the router path, exactly as today.
                None => self.router_metrics_slots(&model.id),
            };

            let live = self.live_gen_tps(model.id.as_str(), &slots, now);
            active.push(ActiveModel { id: model.id.clone(), metrics, slots, live_gen_tps: live });
        }
        // A model that left `Loaded` must not keep a stale reference for when it comes back.
        self.live.retain(|id, _| active.iter().any(|a| &a.id == id));

        LlamaSnapshot {
            health: Health::Ok,
            router: self.router,
            build_info: self.build_info.clone(),
            models,
            active,
        }
    }

    /// A snapshot with empty lists and the last-known props, for the health/models error paths.
    fn empty_snapshot(&self, health: Health) -> LlamaSnapshot {
        LlamaSnapshot {
            health,
            router: self.router,
            build_info: self.build_info.clone(),
            models: Vec::new(),
            active: Vec::new(),
        }
    }

    /// The port of the `llama-server` instance serving `model_id` (its `--alias` equals the model id and it
    /// has a `--port`), or `None` when no instance does.
    fn instance_for(instances: &[ProcSample], model_id: &str) -> Option<u16> {
        instances
            .iter()
            .find(|p| {
                p.role == ServerRole::Instance && p.port.is_some() && p.model.as_deref() == Some(model_id)
            })
            .and_then(|p| p.port)
    }

    /// `/metrics` then `/slots` through the router: with `model=` when this is a router, without otherwise.
    fn router_metrics_slots(&self, model_id: &str) -> (Option<Metrics>, Vec<SlotInfo>) {
        let query: &[(&str, &str)] = if self.router { &[("model", model_id)] } else { &[] };
        let metrics = self.fetch.get("/metrics", query).ok().map(|b| parse_metrics(&b));
        let slots =
            self.fetch.get("/slots", query).ok().and_then(|b| parse_slots(&b).ok()).unwrap_or_default();
        (metrics, slots)
    }

    /// `live_gen_tps` for one model: the growth of `n_decoded` of its first processing slot since the
    /// previous round, or `None`. No processing slot drops the stored reference.
    fn live_gen_tps(&mut self, id: &str, slots: &[SlotInfo], now: Instant) -> Option<f64> {
        let Some(slot) = slots.iter().find(|s| s.is_processing) else {
            self.live.remove(id);
            return None;
        };
        let prev = self.live.get(id).map(|r| (r.id_task, r.n_decoded, r.instant));
        let tps = match prev {
            Some((task, decoded, instant))
                if task == slot.id_task
                    && slot.n_decoded > decoded
                    && now.duration_since(instant) >= Duration::from_millis(1) =>
            {
                let secs = now.duration_since(instant).as_secs_f64();
                Some((slot.n_decoded as f64 - decoded as f64) / secs)
            }
            _ => None,
        };
        self.live.insert(
            id.to_string(),
            LiveRef { id_task: slot.id_task, n_decoded: slot.n_decoded, instant: now },
        );
        tps
    }
}

/// Runs `poller.poll` every `interval` on a new thread and hands each snapshot to `sink`.
/// Stops when `stop` is set or when `sink` returns `false`. Sleeps in slices of at most 100 ms so that
/// a stop request is honoured quickly.
pub fn spawn<F>(
    poller: Poller<F>,
    interval: Duration,
    stop: Arc<AtomicBool>,
    mut sink: impl FnMut(LlamaSnapshot) -> bool + Send + 'static,
) -> JoinHandle<()>
where
    F: Fetch + Send + 'static,
{
    thread::spawn(move || {
        let mut poller = poller;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !sink(poller.poll(Instant::now())) {
                break;
            }
            let mut slept = Duration::ZERO;
            while slept < interval {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let slice = (interval - slept).min(Duration::from_millis(100));
                thread::sleep(slice);
                slept += slice;
            }
        }
    })
}
