# T08 — The system sampler and `--dump`

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T03, T04, T06, T07 |
| **Read first** | `docs/01-architecture.md` § 4 and § 5 |
| **Deliverable** | `src/collect/sampler.rs`, `src/dump.rs`, `src/main.rs` (the `--dump` branch), `tests/sampler_dump.rs`; `pub mod dump;` in `src/lib.rs`, one `pub mod` line in `src/collect/mod.rs` |

## Why

This sheet closes the data layer: one `SystemSnapshot` assembling T02–T04, and `llamatop --dump`, which prints
**everything every collector sees** as JSON and exits. Florian runs it on the aibox to check each value
against the real machine before any screen is drawn. It is the acceptance test of batch 1.

## Reuse (do not rewrite) — check the exact signatures with `grep -n "pub " src/collect/<file>.rs`

- `system::{CpuReader, CpuSample, MemSample, read_mem, read_uptime, read_hostname}`
- `sensors::{TempReading, read_temps, cpu_temp, nvme_temp}`, `gpu::{GpuSample, read_gpus}`
- `disk::{DiskSample, read_disks}`, `net::{NetReader, NetSample}`, `process::{ProcSample, read_server_procs}`
- `llama_poll::{Poller, LlamaSnapshot}`, `logs::{follow, LogLine}`
- `crate::http::Client::new(base_url, api_key, timeout)`, `crate::config::Cli` (fields `url`, `sysroot`,
  `mounts`, `log_cmd`, method `api_key()`).

## Contract

### `src/collect/sampler.rs`

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SystemSnapshot {
    pub hostname: Option<String>,
    pub uptime_s: Option<u64>,
    pub cpu: CpuSample,
    pub cpu_temp_c: Option<f32>,
    pub nvme_temp_c: Option<f32>,
    pub mem: Option<MemSample>,
    pub gpus: Vec<GpuSample>,
    pub temps: Vec<TempReading>,
    pub disks: Vec<DiskSample>,
    pub nets: Vec<NetSample>,
    pub procs: Vec<ProcSample>,
}

/// Owns what must persist between two samples (CPU and network references).
pub struct Sampler { /* private */ }

impl Sampler {
    pub fn new(sysroot: PathBuf, mounts: Vec<PathBuf>) -> Self;
    /// Reads every system source once. Never panics; missing sources give `None` / empty.
    pub fn sample(&mut self, now: Instant) -> SystemSnapshot;
}

/// Same thread contract as `llama_poll::spawn`: one sample per `interval`, stop on `stop` or when `sink`
/// returns `false`, sleeps in slices of at most 100 ms.
pub fn spawn(
    sampler: Sampler,
    interval: Duration,
    stop: Arc<AtomicBool>,
    sink: impl FnMut(SystemSnapshot) -> bool + Send + 'static,
) -> JoinHandle<()>;
```

### `src/dump.rs`

```rust
/// Builds the `--dump` report: pretty-printed JSON, one object with exactly these keys:
/// `system` (SystemSnapshot), `llama` (LlamaSnapshot), `log_tail` (array of LogLine), `log_error`
/// (null, or the error text when the log command could not be started).
pub fn run(cli: &Cli) -> anyhow::Result<String>;
```

Steps of `run`:
1. `Sampler::sample` twice, 300 ms apart; keep the second (it has CPU usage and network rates).
2. One `Poller::poll` with `http::Client::new(&cli.url, cli.api_key(), 3 s)`.
3. `logs::follow(&cli.log_cmd, …)`: collect lines for 700 ms (or until the `log source ended` line), stop
   the follower, keep the **last 20** lines. If `follow` returns `Err`: `log_tail` empty, `log_error` =
   its message.
4. Serialize with `serde_json::to_string_pretty`.

`src/main.rs`: when `cli.dump` is set, print `dump::run(&cli)?` and exit 0. Leave the other branch (the
"not implemented yet (batch 2)" error) as it is.

## Pitfalls

- The API key must not appear in the output: nothing in the snapshots holds it; do not add the `Cli` to the
  report.
- `Health::Down(String)` serializes as `{"Down": "…"}` and `Health::Ok` as `"Ok"` (serde's default enum
  representation). Keep the default; the test relies on it.
- `--dump` must return in about 1.5 s even when the server is down and the journal is silent: the HTTP
  timeout is 3 s at most, the log wait is bounded, the follower is stopped (T07 `stop` kills the command).
- Port 9 on 127.0.0.1 refuses connections immediately: that is what the test uses for "server down".

## Tests — `tests/sampler_dump.rs`

`sysroot()` helper as before. Build a `Cli` with `Cli::parse_from([...])` (`use clap::Parser`).

- `snapshot_of_the_fixture_machine`: `Sampler::new(sysroot(), vec!["/".into()])`, `sample(t0)` → hostname
  `aibox-flo`, uptime 356400, `cpu.usage_pct` `None` (first sample), mem total 33554432000, 1 GPU with busy
  97.0, cpu_temp 52.0, nvme_temp 41.85, 9 temps, 1 disk, 1 net (`wlp9s0`, rates `None`), 2 procs.
- `second_sample_keeps_references`: a second `sample(t0 + 1 s)` on the same `Sampler` → `nets[0].rx_bps` is
  `Some(0.0)` (the counters did not move), `cpu.per_core_pct` has 16 entries, all 0.0, and `cpu.usage_pct` is
  `None` (the fixture `proc/stat` did not change, so the total did not grow).
- `empty_machine`: `Sampler::new("/nonexistent-llamatop".into(), vec![])` → hostname `None`, mem `None`, gpus,
  temps, disks, nets, procs all empty, no panic.
- `spawn_delivers_and_stops`: 50 ms interval, stop after 300 ms → joins within 1 s, sink called ≥ 2 times.
- `dump_with_server_down`: `Cli::parse_from(["llamatop", "--dump", "--url", "http://127.0.0.1:9",
  "--sysroot", <fixture sysroot>, "--mount", "/", "--log-cmd", "cat <absolute journal fixture>"])` →
  `run` is `Ok`; parse the text as `serde_json::Value`: the top-level keys are exactly `system`, `llama`,
  `log_tail`, `log_error`; `system.hostname` = `aibox-flo`; `system.gpus[0].busy_pct` = 97.0;
  `llama.health.Down` is a string; `llama.models` is empty; `log_tail` has 20 items, the last one has the text `log source ended: cat`; `log_error` is null.
  The call takes less than 5 s.
- `dump_with_missing_log_program`: same with `--log-cmd "llamatop-no-such-program"` → `log_tail` empty,
  `log_error` is a non-empty string.
- `dump_never_shows_the_key`: write `sk-test-SECRET-123` to a temp file, pass `--api-key-file` → the output
  does not contain `SECRET`.

## Acceptance criteria

- [x] Exact public API above; `cargo run -- --dump --sysroot tests/fixtures/sysroot --url http://127.0.0.1:9
      --log-cmd "cat tests/fixtures/journal/llama-server.txt"` prints the JSON report and exits 0.
- [x] The 7 tests exist under these names and pass.
- [x] No fixture modified; no collector module of T02–T07 changed (if one of them has a bug, STOP and report).
- [x] `bash .devbox/check` is green.
