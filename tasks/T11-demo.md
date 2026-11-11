# T11 — Demo data

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T09 |
| **Read first** | `docs/02-ui.md` § 6; `src/app/state.rs` (`grep -n "pub " src/app/state.rs`) |
| **Deliverable** | `src/app/demo.rs`, `tests/demo.rs`; one `pub mod demo;` line in `src/app/mod.rs` |

## Why

Nobody can look at the screen while the dashboard is written, and the real server cannot be put in every
state on demand. The demo provides plausible data for each state the screens must show — idle, prompt
processing, generating, loading a model, server down — **deterministically**: the same `t` always gives the
same snapshot, so the golden files of T13 are stable. The live `--demo` mode of T14 replays the same
functions against the clock.

Every value comes from a formula of `t`: no random numbers, no clock, no I/O.

## Contract — `src/app/demo.rs`

```rust
use crate::app::App;
use crate::collect::llama_poll::LlamaSnapshot;
use crate::collect::logs::LogLine;
use crate::collect::sampler::SystemSnapshot;

/// The states the demo can show. `clap::ValueEnum` so that T13 can take it as `--demo-state <state>`
/// (values `idle`, `prompt`, `generating`, `loading`, `down`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DemoState { Idle, Prompt, Generating, Loading, Down }

/// Length of one live demo cycle, in seconds.
pub const CYCLE_S: f64 = 40.0;

/// Live demo state at `t` seconds, by `c = t.rem_euclid(CYCLE_S)`: `[0, 8)` Idle, `[8, 12)` Prompt,
/// `[12, 32)` Generating, `[32, 40)` Idle. `Loading` and `Down` only appear through `--demo-state`.
pub fn state_at(t: f64) -> DemoState;

/// The aibox in `state` at `t` (table below). Pure.
pub fn system(t: f64, state: DemoState) -> SystemSnapshot;

/// The router in `state` at `t` (below). Pure.
pub fn llama(t: f64, state: DemoState) -> LlamaSnapshot;

/// The log lines of the live cycle whose event time `e` (below) satisfies `t0 <= e < t1`, in time order,
/// each built as a raw journal line and passed through `collect::logs::parse_line`.
pub fn log_lines(t0: f64, t1: f64) -> Vec<LogLine>;

/// An `App` as it looks after two minutes in `state`, built only through `App::apply`:
/// `App::new(1000)`; for `s` in `0..120`: `apply(Event::System(system(s as f64, state)))` then
/// `apply(Event::Llama(llama(s as f64, state)))`; then every line of `log_lines(0.0, CYCLE_S)` as
/// `Event::Log`. Two calls give equal results.
pub fn app(state: DemoState) -> App;
```

### Wobble

`w(t) = (1.3 * t).sin() + 0.5 * (0.37 * t).sin()` (in `f64`, within ±1.5). Values below marked "± k" are
`base + k * w(t)`; percentages are then clamped to 0–100.

### `system(t, state)`

| | Idle | Prompt | Generating | Loading | Down |
|---|---|---|---|---|---|
| GPU `busy_pct` | 2 ± 1 | 99 ± 1 | 96 ± 2 | 30 ± 5 | 0 |
| GPU `vram_used` (GiB) | 15.6 | 15.9 | 15.9 | 8.0 | 0.6 |
| GPU `power_w` | 18 ± 2 | 190 ± 6 | 120 ± 4 | 60 ± 5 | 12 |
| GPU `temp_junction_c` | 48 ± 1 | 82 ± 1 | 74 ± 1 | 55 ± 1 | 42 |
| CPU `usage_pct` | 4 ± 2 | 35 ± 3 | 52 ± 3 | 12 ± 3 | 3 ± 1 |
| `mem.used` (GiB) | 21.5 | 21.6 | 21.6 | 14.0 | 6.0 |

Everything else is fixed or derived, the same in every state:
- `hostname` `aibox-flo`; `uptime_s` = `356_400 + t as u64`;
- CPU: `per_core_pct` 16 entries, entry `i` = `usage + ((i % 4) as f32 - 1.5) * 4.0` clamped 0–100;
  `freq` avg `1200 + 45 * usage` MHz, max `4700`; `load` = `usage / 6`, `usage / 7`, `usage / 9`;
  `cpu_temp_c` = `40 + 0.4 * usage`;
- GPU, one entry: `card1`, `Radeon RX 7800 XT`, `vram_total` 17 163 091 968, `gtt_used` 183 365 632,
  `temp_edge_c` = junction − 11, `temp_mem_c` = junction + 3, `power_cap_w` 212, `fan_rpm` =
  `400 + 25 * (junction - 40)` (as `u32`), `fan_pct` = `fan_rpm / 3600 * 100`, `sclk_mhz` = `500 + 18 * busy`,
  `vddgfx_mv` 917, `link` `16.0 GT/s PCIe x16`;
- `nvme_temp_c` 41.85; `temps`: `coretemp`/`Package id 0` (= `cpu_temp_c`), `nvme`/`Composite`,
  `amdgpu`/`edge`, `amdgpu`/`junction`, `amdgpu`/`mem`, in that order;
- `mem`: `total` 33 444 237 312, `available` = total − used, `cached` 19 519 881 216, swap 8 589 930 496
  total and 1 310 720 used;
- `disks`: `/` 65 026 052 096 total / 39 769 993 216 used / 21 919 703 040 available; `/srv/models`
  126 227 120 128 / 77 434 490 880 / 42 333 401 088; `/srv/data` 63 090 503 680 / 7 026 569 216 /
  52 825 931 776;
- `nets`: one `wlp9s0`, `rx_bps` `Some(2_500 ± 500)`, `tx_bps` `Some(15_000 ± 3_000)`, totals
  `195_718_003 + 2_500 * t` and `195_824_906 + 15_000 * t` (as `u64`);
- `procs`: the router (pid 1194, `Router`, model `None`, port 8080, rss 300 736 512, 23 threads), and, except
  in `Down`, the instance (pid 6873, `Instance`, model = the loaded id below, port 35335, 30 threads, rss
  20 577 497 088, or 9 000 000 000 in `Loading`).

GiB = 1024³ bytes; convert with `(x * 1024.0 * 1024.0 * 1024.0) as u64`.

### `llama(t, state)`

The five models of the aibox, in this order (`ctx_train` / `ctx_size` / `n_cpu_moe` / `spec_type` /
`vision`; every one has `n_gpu_layers` 99 and `threads` 8):

1. `bartowski/Ornith-1.5-35B-A3B-GGUF:Q4_K_M` — 262144 / 163840 / 16 / none / vision
2. `gsq-qwen3.8-27b` — 262144 / 65536 / none / `draft-mtp`
3. `muse-glimmer-30b` — 131072 / 131072 / none / `draft-dflash`
4. `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M` — 262144 / 196608 / 21 / `draft-mtp`; the
   **loaded** one: `size_bytes` 22 512 939 520, `n_params` 35 505 251 456, `ftype` `Q4_K - Medium`,
   `model_path` `/srv/models/direct/Cyber-Tiel-Coder-35B-A3B-MTP-UD-Q4_K_M.gguf`
5. `unsloth/Qwen3-8B-GGUF:Q4_K_M` — 40960 / 32768 / none / none

`model_path` is `None` for the others; `size_bytes`, `n_params`, `ftype` are `None` unless loaded.

- `Down`: `health` `Health::Down("GET /health: Connection refused")`, `router` false, `build_info` `None`,
  no models, no active.
- Otherwise `health` `Ok`, `router` true, `build_info` `Some("b12151-10d4846a5")`, the five models, all
  `Unloaded` except model 4: `Loading` in state `Loading` (with `active` empty), `Loaded` otherwise, with one
  `ActiveModel` for it:
  - `metrics`: `prompt_tokens_total` 828 751, `prompt_tokens_cached_total` 41 518 700, `prompt_seconds_total`
    1402.01, `tokens_predicted_total` 507 212, `tokens_predicted_seconds_total` 8948.33, `n_decode_total`
    186 120, `n_tokens_max` 99 007, `requests_processing` 1 when the slot is processing else 0,
    `requests_deferred` 0, draft 517 905 drafted / 333 880 accepted;
  - one slot: `id` 0, `id_task` 9000, `n_ctx` 196 608, `speculative` true, `n_remain` −1, and
    Idle: not processing, `n_prompt_tokens` 9 232, `n_decoded` 0 —
    Prompt: processing, `n_prompt_tokens` 8 192, `n_decoded` 0 —
    Generating: processing, `n_prompt_tokens` 57 651, `n_decoded` = `(52.0 * t) as u64 % 30_000 + 1`;
  - `live_gen_tps`: `Some(52 ± 4)` while Generating, `None` otherwise.

### `log_lines(t0, t1)` — the live cycle

For cycle `n` (`n = (e / CYCLE_S).floor()`), `task = 9000 + n`. Times inside a cycle `c`, in seconds; at
equal `c` the lines keep the order listed. Each raw line is
`<ts> aibox-flo llama-server[1194]: <body>`, where `<ts>` is `2026-11-12T<HH:MM:SS>.000000+01:00` for
`36_000 + e.floor()` seconds of the day (modulo 86 400). `M` = model 4's id. A body starting with
`[35335]` is an instance line, the others are router lines:

| `c` | Body |
|---|---|
| 8 | `90.25.429.894 I srv  proxy_reques: proxying request to model M on port 35335` |
| 8 | `[35335] 60.27.289.353 I slot launch_slot_: id  0 \| task T \| processing task, is_child = 0` |
| 9, 10, 11 (k = c − 8) | `[35335] 60.27.289.353 I slot update_slots: id  0 \| task T \| prompt processing, n_tokens = <2048 k>, progress = <0.25 k, 2 decimals>, t = <k>.00 s / 890.00 tokens per second` |
| 12 | `[35335] 60.27.289.353 I slot print_timing: id  0 \| task T \| prompt eval time =    9195.00 ms /  8192 tokens (    1.12 ms per token,   891.00 tokens per second)` |
| 15, 18, … 30 | `[35335] 60.27.289.353 I slot print_timing: id  0 \| task T \| n_gen = <52 (c − 12)>, tg =  52.10 t/s, tg_3s =  53.00 t/s` |
| 32 | `[35335] 60.27.289.353 I slot print_timing: id  0 \| task T \|        eval time =   20000.00 ms /  1040 tokens (   19.23 ms per token,    52.00 tokens per second)` |
| 32 | `[35335] 60.27.289.353 I slot print_timing: id  0 \| task T \|       total time =   29195.00 ms /  9232 tokens` |
| 32 | `[35335] 60.27.289.353 I slot print_timing: id  0 \| task T \| draft acceptance = 0.70000 (  728 accepted /  1040 generated), mean len =  2.80` |
| 32 | `[35335] 60.27.289.353 I slot      release: id  0 \| task T \| stop processing: n_tokens = 9232, truncated = 0` |
| 32 | `90.25.429.894 I srv  log_server_r: request: POST /v1/chat/completions 192.168.1.42 200` |
| 32 | `[35335] 60.27.289.353 I srv  update_slots: all slots are idle` |

(`\|` is a plain `|` in the line; the table escapes it.) Iterate over the cycles that intersect
`[t0, t1)` and keep the lines whose `e = n * CYCLE_S + c` is in range; nothing when `t1 <= t0`.

## Pitfalls

- Build `LlamaSnapshot`, `ModelInfo`, `GpuSample`… with struct literals: every field is `pub`. Do not change
  any type of `src/collect/`.
- `t` is `f64`; the snapshot fields are `f32` or `u64`: convert once, at the end of each formula.
- `clap::ValueEnum` derive needs `use clap::ValueEnum;` only where `value_variants` is called; the derive
  attribute alone works with the `derive` feature already enabled.
- `log_lines` must not depend on how the range is split: `log_lines(0, 40)` equals `log_lines(0, 20)`
  followed by `log_lines(20, 40)`.
- 400-line limit: if `demo.rs` grows past it, move the log templates into `src/app/demo_log.rs`, declared
  as a private `mod demo_log;` in `src/app/mod.rs`.

## Tests

`tests/demo.rs`, exactly these names.

- `cycle_states`: `state_at` at 0 → Idle, 7.9 → Idle, 8 → Prompt, 11.9 → Prompt, 12 → Generating,
  31.9 → Generating, 32 → Idle, 48 → Prompt, −1 → Idle (39 in the cycle).
- `system_is_plausible`: for every state and `t` in `0, 0.5, … 200`: one GPU with `busy_pct` in 0–100,
  `vram_used <= vram_total`, `mem.used <= mem.total`, `cpu.usage_pct` in 0–100, 16 `per_core_pct`,
  3 disks.
- `system_follows_state`: at `t = 10.0`, GPU `busy_pct` Generating > Idle + 50; `power_w` Prompt > Idle.
- `deterministic`: `system(17.3, Generating) == system(17.3, Generating)`, same for `llama`, and
  `format!("{:?}", app(Generating)) == format!("{:?}", app(Generating))`.
- `llama_states`: Generating → one active, its slot processing, `live_gen_tps` > 0; Idle → slot not
  processing, `live_gen_tps` `None`; Loading → no active and exactly one model `Loading`; Down →
  `Health::Down(_)`, no models.
- `active_models_are_loaded`: for every state and `t` in `0..60`, every `active[i].id` is the id of a model
  whose state is `Loaded` (the safety rule's invariant, mirrored in the demo).
- `log_cycle_events`: `log_lines(0.0, 40.0)` has 1 `Proxy`, 3 `PromptProgress`, 1 `PromptTiming`,
  6 `GenProgress`, 1 `GenTiming`, 1 `DraftAcceptance`, 1 `Released`, 1 `Request` (status 200) events; its
  first line's `time` is `Some("10:00:08")`; every line has `instance` `Some(35335)` or `None`.
- `log_split_invariant`: `log_lines(0, 40)` == `log_lines(0, 20)` + `log_lines(20, 40)`;
  `log_lines(40, 80)` has the same length as `log_lines(0, 40)` and its lines carry task 9001;
  `log_lines(5.0, 5.0)` is empty.
- `demo_app`: `app(Generating)`: `gen_tps.len()` 120, `cpu_usage.len()` 120, `last_request` with
  `prompt_tokens` `Some(8192)`, `gen_tokens` `Some(1040)`, `ctx_tokens` `Some(9232)`, `animating()` true;
  `app(Idle).animating()` false; `app(Down)`: llama health `Down`, `gen_tps` empty.

## Acceptance criteria

- [ ] `src/app/demo.rs` exists with the exact public API above; `src/app/mod.rs` declares `pub mod demo;`.
- [ ] Nothing outside the deliverable changed (except `tasks/`).
- [ ] The 9 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
