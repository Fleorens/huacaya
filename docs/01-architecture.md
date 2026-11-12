# llamatop — architecture

## 1. What it is

One static-ish Rust binary, run **on the aibox** in a terminal (SSH or local screen). It shows, live:

- the llama.cpp server: models and their state, the loaded model's settings, generation and prompt speed,
  context fill, prompt-cache reuse, speculative-decoding acceptance, requests in progress;
- the machine: GPU (load, VRAM, temperatures, power, fan), CPU (load, per-core, temperature, frequency),
  RAM and swap, disks, network, the `llama-server` processes;
- the server log, live, coloured, filterable.

It is **read-only**: it never loads, unloads or configures anything.

It also runs anywhere else with `--url http://192.168.1.203:8080`: the server panels work over HTTP, the
machine panels then describe the local machine (or show "n/a"), and the log panel shows whatever `--log-cmd`
prints (for example `ssh aibox journalctl …`).

## 2. Data sources

| Source | Read from | Period | Module | Sheet |
|---|---|---|---|---|
| CPU, RAM, load, uptime | `proc/stat`, `proc/meminfo`, `proc/loadavg`, `proc/uptime`, `cpufreq` | 1 s | `collect::system` | T02 |
| Temperatures, GPU | `sys/class/hwmon/*`, `sys/class/drm/card*/device` (amdgpu) | 1 s | `collect::sensors`, `collect::gpu` | T03 |
| Disks, network, processes | `statvfs`, `proc/net/dev`, `proc/<pid>/{comm,status,cmdline}` | 1 s | `collect::disk`, `collect::net`, `collect::process` | T04 |
| Server state | `GET /health`, `/props`, `/v1/models`, `/metrics?model=`, `/slots?model=` | 1 s | `collect::llama` (parsing), `collect::llama_poll` (HTTP) | T05, T06 |
| Server log | standard output of `--log-cmd` (default: `journalctl -u llama-server -f …`) | stream | `collect::logs` | T07 |
| Everything, assembled | the modules above | 1 s | `collect::sampler`, `--dump` | T08 |

All paths are relative to `--sysroot` (default `/`). Tests use `tests/fixtures/sysroot`.

### 2.1 The llama.cpp router — facts measured on the aibox (build b12151, 2026-11-11)

- `GET /health` → `{"status":"ok"}`, **without** key. Every other endpoint needs `Authorization: Bearer <key>`
  and answers 401 without it.
- `GET /props` (no `model`) → `{"role":"router", "build_info":"b12151-10d4846a5", "max_instances":1, …}`.
  A single-model server has no `"role":"router"`.
- `GET /v1/models` → `data[]`, one entry per model: `id`, `status.value` (`"unloaded"`, `"loading"`,
  `"loaded"`, sometimes others), `status.args` (the full command line of the instance), `context_length`
  (training context), and, **only when loaded**, `meta` (`n_ctx`, `n_params`, `size`, `ftype`).
- `GET /metrics` and `GET /slots` **need `?model=<id>`** on a router; without it: HTTP 400.
- `/metrics` is Prometheus text; names are prefixed `llamacpp:`; values may use exponent notation
  (`2.67344e+06`); some lines carry labels (`…{position="0"} 6654`).
- `/slots` is a JSON array; live generation progress is `next_token[0].n_decoded`.

Fixtures of all these answers: `tests/fixtures/llama/`.

### 2.2 The safety rule

**Never send a request with `model=<id>` for a model that is not `loaded`.** On a router with
`models_autoload`, such a request **loads the model**: ~20 GB read, the current model evicted, the job in
progress on the single GPU killed. `/metrics` and `/slots` are therefore polled only for models whose
`status.value` is exactly `"loaded"` in the `/v1/models` answer of the **same** polling round. This rule has a
test in T06 and must survive every refactoring.

On the server's own machine, when `--url` is a loopback address (`127.0.0.1`, `localhost` or `[::1]`),
llamatop silences the journal as well. Each round, after `/v1/models`, it reads the `llama-server` processes
under `--sysroot` (`collect::process::read_server_procs`); the instance that serves a loaded model listens on
`127.0.0.1:<port>` (the port read from its `--port`, its `--alias` being the model id), and answering
`/metrics` and `/slots` directly on that port — with the same API key — prints **nothing** in the journal,
whereas the router prints `proxying request to model … on port …` for each of the two calls. When a direct
answer fails, the round falls back to the router path (with `model=`) for both; when no instance is found, or
when `--url` is a remote address, the router path is the only one. The safety rule above holds for both paths:
direct or not, only models `loaded` in this round are polled. Known limit: the instance is matched by name and
`--alias` only, not by owner, so on a multi-user machine another account's `llama-server` with the same alias
would receive the API key; the aibox has a single user.

## 3. Derived values

| Value | Formula |
|---|---|
| CPU usage | `1 − Δidle / Δtotal` between two `proc/stat` samples, `idle` = idle + iowait |
| RAM used | `MemTotal − MemAvailable` |
| Context fill | `slot.n_prompt_tokens / slot.n_ctx` (plus `n_decoded` while generating) |
| Live generation speed | `Δ next_token[0].n_decoded / Δt` between two `/slots` samples of the same `id_task` |
| Average generation speed | `tokens_predicted_total / tokens_predicted_seconds_total` |
| Average prompt speed | `prompt_tokens_total / prompt_seconds_total` |
| Prompt-cache reuse | `cached / (cached + prompt_tokens_total)` |
| Draft acceptance | `spec_decode_num_accepted_tokens_total / spec_decode_num_draft_tokens_total` |
| Network rate | `Δ bytes / Δt` per interface |

A rate over a counter that went **down** (server restarted, model reloaded) is `None` for that round, never a
negative number.

## 4. Threads

```
sampler thread   ── every interval ──▶  SystemSnapshot ─┐
llama poll thread ── every interval ──▶  LlamaSnapshot ─┼─▶ mpsc::Sender<Event> ─▶ UI thread (batch 2)
log thread        ── one line at a time ▶ LogLine ──────┘
```

No async runtime. Each collector thread owns its previous sample (for deltas), sends immutable snapshots, and
stops when the receiver is dropped (`send` fails) or when its `Arc<AtomicBool>` stop flag is set. The UI thread
never blocks on I/O.

## 5. Layout of the source tree

```
src/main.rs              CLI entry: --dump, or the dashboard
src/config.rs            Cli (clap)                         — written, do not change without a sheet
src/http.rs              Client::get                        — written, the only user of ureq
src/util/                history ring, rates, formatting    — T01
src/collect/system.rs    CPU, RAM, load                     — T02
src/collect/sensors.rs   hwmon scan                         — T03
src/collect/gpu.rs       amdgpu                             — T03
src/collect/disk.rs  net.rs  process.rs                     — T04
src/collect/llama.rs     parsing of the server answers      — T05
src/collect/llama_poll.rs  one polling round + thread       — T06
src/collect/logs.rs      log follower + line classifier     — T07
src/collect/sampler.rs   SystemSnapshot + thread            — T08
src/dump.rs              --dump                             — T08
src/app/  src/ui/        state, screens                     — batch 2
```

## 6. What is deliberately out

- No control of the server (load / unload / restart): the dashboard cannot break a running job.
- No TLS, no remote agent, no daemon, no configuration file: flags and environment only.
- No NVIDIA / Intel GPU support: the GPU panel is written for amdgpu sysfs and shows "n/a" elsewhere.
- No history on disk: the graphs cover the last minutes, kept in memory.
