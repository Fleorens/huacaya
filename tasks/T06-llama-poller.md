# T06 — Polling the llama.cpp server

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T05 |
| **Read first** | `docs/01-architecture.md` § 2.1, **§ 2.2 "The safety rule"**, § 4 |
| **Deliverable** | `src/collect/llama_poll.rs`, `tests/llama_poll.rs`; one `pub mod` line in `src/collect/mod.rs` |

## Why

One polling round turns five HTTP calls into one immutable `LlamaSnapshot`. The round must be **safe**: on a
router, asking `/metrics?model=X` for a model that is not loaded makes the server load it, which evicts the
model in use and kills the running job. Read § 2.2 before writing a line.

## Reuse (do not rewrite)

- `crate::http::Client::get(&self, path, query) -> anyhow::Result<String>` — already written. A non-2xx
  answer is an `Err` whose message contains `HTTP <code>` (e.g. `GET /v1/models: HTTP 401`).
- `crate::collect::llama::{parse_models, parse_props, parse_metrics, parse_slots, ModelInfo, ModelState,
  Metrics, SlotInfo}` (T05).

## Contract — `src/collect/llama_poll.rs`

Data types derive `Debug, Clone, PartialEq, serde::Serialize`.

```rust
/// What the poller needs from an HTTP client; lets the tests run without a network.
pub trait Fetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String>;
}
impl Fetch for crate::http::Client { /* delegates to Client::get */ }

pub enum Health {
    Ok,
    /// `/health` failed; the message is the error text (it never contains the key).
    Down(String),
    /// `/health` is fine but `/v1/models` answered 401: missing or wrong API key.
    Unauthorized,
}

pub struct ActiveModel {
    pub id: String,
    pub metrics: Option<Metrics>,
    pub slots: Vec<SlotInfo>,
    /// Tokens per second generated right now, see "Live speed".
    pub live_gen_tps: Option<f64>,
}

pub struct LlamaSnapshot {
    pub health: Health,
    pub router: bool,
    pub build_info: Option<String>,
    pub models: Vec<ModelInfo>,
    /// One entry per model in state `Loaded`, same order as `models`.
    pub active: Vec<ActiveModel>,
}

pub struct Poller<F: Fetch> { /* private */ }

impl<F: Fetch> Poller<F> {
    pub fn new(fetch: F) -> Self;
    /// One round. Never panics, never returns an error: failures end up in `health` or as `None`s.
    pub fn poll(&mut self, now: std::time::Instant) -> LlamaSnapshot;
}

/// Runs `poller.poll` every `interval` on a new thread and hands each snapshot to `sink`.
/// Stops when `stop` is set or when `sink` returns `false`. Sleeps in slices of at most 100 ms so that
/// a stop request is honoured quickly.
pub fn spawn<F: Fetch + Send + 'static>(
    poller: Poller<F>,
    interval: std::time::Duration,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    sink: impl FnMut(LlamaSnapshot) -> bool + Send + 'static,
) -> std::thread::JoinHandle<()>;
```

### One round, in this order

1. `GET /health`. On error: return `health: Down(msg)`, `router` and `build_info` as last known, `models` and
   `active` empty; forget the cached props and the live-speed references. **Stop the round here.**
2. `GET /props` — only when no props are cached. On success cache `router` and `build_info`. On error:
   `router = false`, `build_info = None`, nothing cached (retried next round).
3. `GET /v1/models`. On an error whose message contains `HTTP 401`: return `health: Unauthorized`, empty
   `models` and `active`. On any other error: `health: Down(msg)`, empty lists.
4. For each model whose state is `ModelState::Loaded`, **and only those**:
   - router: `GET /metrics` and `GET /slots` with query `[("model", id)]`;
   - not a router: the same two calls with an empty query.
   A failed call gives `metrics: None` / `slots: vec![]` for that model; the round goes on.
5. Compute `live_gen_tps` per active model, return `health: Ok`.

### Live speed

Per model id, remember `(id_task, n_decoded, instant)` of the **first slot that `is_processing`**.
`live_gen_tps` = `(n_decoded − previous n_decoded) / seconds elapsed` when the previous reference exists,
has the same `id_task`, `n_decoded` grew, and at least 1 ms elapsed. Otherwise `None`. No processing slot:
`None`, and the reference is dropped.

## Pitfalls

- **The safety rule (§ 2.2).** Never call `/metrics` or `/slots` for a model that is `Unloaded`, `Loading`
  or `Other`. Do not "try anyway and ignore the error": the request itself triggers the load.
- The model id contains `/` and `:`. Pass it as a query **pair**; `http::Client` encodes it. Never build the
  URL by hand.
- `Fetch::get` takes `&self`: a recording fake needs a `Mutex<Vec<…>>` inside.
- The thread of `spawn` must not sleep `interval` in one go (a 10 s interval would delay quitting by 10 s).

## Tests — `tests/llama_poll.rs`

Write a `FakeFetch` in the test file: a `HashMap<String, Result<String, String>>` of canned answers keyed by
path, answering an error `GET <path>: HTTP 404` for unknown paths, and recording every call as
`(path, Vec<(String, String)>)` in an `Arc<Mutex<Vec<…>>>` that the test keeps a clone of. Load the canned
bodies from `tests/fixtures/llama/`. `TIEL` = `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`.

- `healthy_router_round`: `/health`, `/props` (router fixture), `/v1/models`, `/metrics`, `/slots` (idle) →
  health `Ok`, router true, build `b12151-10d4846a5`, 5 models, 1 active with id `TIEL`, metrics `Some`
  with tokens_predicted_total 24107, 1 slot, live_gen_tps `None`.
- `only_loaded_models_are_queried`: same setup → among the recorded calls, those to `/metrics` and `/slots`
  are exactly 2, and each has the query `[("model", TIEL)]`. No recorded call anywhere carries another
  model id.
- `nothing_loaded_means_no_model_query`: `/v1/models` answering the fixture with every `"value":"loaded"`
  replaced by `"value":"unloaded"` (string replace in the test) → `active` empty, **zero** calls to
  `/metrics` and `/slots`.
- `loading_model_is_not_queried`: same with `"value":"loading"` → zero calls to `/metrics` and `/slots`.
- `server_down`: `/health` answers an error → health `Down(_)`, models empty, and the only recorded call is
  `/health`.
- `unauthorized`: `/health` ok, `/props` ok, `/v1/models` → error `GET /v1/models: HTTP 401` → health
  `Unauthorized`, models empty.
- `props_are_fetched_once`: two rounds → exactly one recorded call to `/props`.
- `metrics_failure_is_not_fatal`: no `/metrics` answer (404) but `/slots` ok → health `Ok`, 1 active,
  metrics `None`, 1 slot.
- `single_model_server`: `/props` → `{}`, `/v1/models` → `{"data":[{"id":"solo"}]}`, `/metrics` and `/slots`
  fixtures → 1 active `solo`, and the `/metrics` and `/slots` calls carry an **empty** query.
- `live_speed`: round 1 at `t0` with `slots-busy.json` → live `None`; then replace the `/slots` answer by the
  same text with `"n_decoded":427` changed to `"n_decoded":552`, round 2 at `t0 + 2 s` → live `Some(62.5)`.
  (Give `FakeFetch` a way to change an answer between rounds, e.g. answers behind an `Arc<Mutex<…>>`.)
- `live_speed_resets_on_new_task`: as above, but round 2 also changes `"id_task":8040` to `"id_task":8041`
  → live `None`.
- `spawn_stops_on_flag`: `spawn` with a 50 ms interval and a sink that counts; set `stop` after 300 ms; the
  thread joins within 1 s and the sink was called at least twice.
- `spawn_stops_when_sink_refuses`: sink returns `false` on the first call → the thread joins within 1 s.

## Acceptance criteria

- [x] Exact public API above; `impl Fetch for http::Client` exists.
- [x] The 13 tests exist under these names and pass.
- [x] No request with `model=` is ever sent for a model that is not `Loaded` (tests 2, 3, 4).
- [x] No fixture modified; `src/http.rs` and `src/collect/llama.rs` unchanged.
- [x] `bash .devbox/check` is green.
