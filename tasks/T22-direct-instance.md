# T22 — Poll the loaded instance directly, not through the router

| | |
|---|---|
| **Batch** | 3 — after the first look at the real screen |
| **Depends on** | T21 |
| **Read first** | `docs/01-architecture.md` § 2.1, § 2.2; `src/collect/llama_poll.rs`; `src/http.rs`; `grep -n "pub " src/collect/process.rs`; the head of `tests/llama_poll.rs` (the `FakeFetch`) |
| **Deliverable** | changes to `src/http.rs`, `src/collect/llama_poll.rs`, `src/app/run.rs`, `src/dump.rs`, `docs/01-architecture.md`; `tests/llama_direct.rs` |

## Why

Every polling round asks the router for `/metrics?model=<id>` and `/slots?model=<id>`; the router proxies them
to the model's instance and prints `proxying request to model … on port …` for each: two journal lines per
second, which pushed the real log lines out of the journal window on the aibox. The instance itself listens
on `127.0.0.1:<port>` (the router starts it with `--host 127.0.0.1 --port <port> --alias <model id>`), answers
`/metrics` and `/slots` without `?model=` (with the same API key), and prints **nothing** for a successful
request (measured on the aibox, 2026-11-12). Asking it directly removes the noise.

It is also safe: an instance process exists only for a model that is loaded, so a direct request can never
load a model. The safety rule of § 2.2 stays as it is: direct or not, only models `Loaded` in this round's
`/v1/models` are polled.

## Contract

### `src/http.rs`

```rust
impl Client {
    /// GET `http://127.0.0.1:<port><path>` with the same key and timeout as `get`; same error rules.
    pub fn get_local(&self, port: u16, path: &str) -> Result<String>;
}

/// `true` when the URL's host is `127.0.0.1`, `localhost` or `[::1]` (scheme and port ignored, ASCII
/// case-insensitive): the instances' local ports are then reachable.
pub fn is_loopback_url(url: &str) -> bool;
```

### `src/collect/llama_poll.rs`

```rust
pub trait Fetch {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<String>;
    /// GET `path` on the local instance listening on `port`. The default refuses, so fetchers that cannot
    /// reach instances (and the existing test doubles) keep working unchanged.
    fn get_local(&self, port: u16, path: &str) -> anyhow::Result<String> {
        anyhow::bail!("GET :{port}{path}: no direct access")
    }
}

impl<F: Fetch> Poller<F> {
    /// Enables direct polling: each round reads the `llama-server` processes under `sysroot`
    /// (`process::read_server_procs`) and, for a `Loaded` model whose id equals an `Instance`'s `model`
    /// (its `--alias`) and that has a `port`, asks `/metrics` and `/slots` with `get_local(port, …)`
    /// instead of `get(…, [("model", id)])`.
    pub fn with_local_instances(self, sysroot: PathBuf) -> Self;
}
```

- `impl Fetch for Client` overrides `get_local` with `Client::get_local`.
- In a round with direct polling enabled: the process list is read **once per round**, after `/v1/models`.
  For each `Loaded` model (and only those, as today): instance found → `/metrics` then `/slots` through
  `get_local`; if **either** direct request fails, that model falls back to the router path (`get` with
  `model=`) for both, in the same round. No instance → the router path, exactly as today.
- Without `with_local_instances`, nothing changes (every existing test must pass unchanged).
- `ActiveModel`, `LlamaSnapshot` and every other public type are unchanged.

### `src/app/run.rs` and `src/dump.rs`

Build the poller with `.with_local_instances(cli.sysroot.clone())` when `http::is_loopback_url(&cli.url)`,
with `Poller::new` alone otherwise.

### `docs/01-architecture.md`

In § 2.2, add a paragraph: on the server's own machine, llamatop asks the loaded instance directly
(`127.0.0.1:<port>`, port read from the instance's command line), which prints nothing in the journal; the
router path with `model=` is the fallback and the only path from another machine. The rule above holds for
both.

## Pitfalls

- The fixture sysroot has the router (pid 1201, port 8080) and one instance (pid 48211, `--port 35335`,
  `--alias peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`), which is the model `loaded` in
  `tests/fixtures/llama/models.json`.
- `tests/llama_direct.rs` needs its own fake fetcher (a test file cannot import another): copy the
  `FakeFetch` of `tests/llama_poll.rs` and give it a `get_local` that records `(port, path)` and answers
  from a second map, or an error when told to.
- `is_loopback_url`: `http://127.0.0.1:8080`, `http://localhost:8080/`, `http://[::1]:8080` → true;
  `http://192.168.1.203:8080`, `http://127.0.0.1.example.com` → false. Parse the host by hand (between
  `://` and the next `:` or `/`, brackets kept for IPv6); no new dependency.
- Do not log, print or return the key; `get_local` errors carry the port and path only.

## Tests

`tests/llama_direct.rs`, exactly these names. `sysroot` = `tests/fixtures/sysroot`; the router answers as in
`router_setup` of `tests/llama_poll.rs`; the direct answers are the same fixture bodies.

- `loopback_urls`: the five URLs of the pitfall above.
- `direct_round_skips_the_router`: `Poller::new(fake).with_local_instances(sysroot)`, one round → the loaded
  model's `metrics` and `slots` are filled; `get_local` was called with `(35335, "/metrics")` and
  `(35335, "/slots")`; no `get` call carries a `model` query.
- `direct_failure_falls_back`: same, but `get_local` answers an error for `/slots` → the round still fills
  `metrics` and `slots` (from the router), and `get` was called with `/metrics` and `/slots` with
  `model = <the loaded id>`.
- `no_instance_uses_the_router`: `with_local_instances` on an empty temp dir as sysroot → no `get_local` call,
  the router path with `model=` as today.
- `without_direct_nothing_changes`: `Poller::new(fake)` alone → no `get_local` call.
- `direct_never_targets_unloaded_models`: models.json with the instance's model id changed to `Unloaded` (edit
  the JSON string in the test, not the fixture file) → no `get_local` call and no `/metrics` or `/slots` call
  at all.

## Acceptance criteria

- [ ] The public API above exists; `run.rs` and `dump.rs` enable direct polling on a loopback URL only.
- [ ] Every test of `tests/llama_poll.rs` passes unchanged; the 6 tests above exist and pass.
- [ ] `docs/01-architecture.md` § 2.2 describes the direct path.
- [ ] `bash .devbox/check` is green.
- [ ] (Orchestrator, on the aibox) with llamatop running, the journal no longer gets `proxying request` lines
      from it.
