# T31 — TUI v2: data and widgets

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T30 |
| **Read first** | `src/app/state.rs`; `src/collect/sampler.rs`; `src/collect/system.rs` (`grep -n "pub "`); `src/ui/text.rs` |
| **Deliverable** | `src/ui/big.rs`, `tests/tui2_data.rs`, `tests/fixtures/sysroot/proc/cpuinfo`; `pub mod big;` in `src/ui/mod.rs`; changes to `src/app/state.rs`, `src/collect/system.rs`, `src/collect/sampler.rs`, `src/app/demo.rs`, `tests/app_state.rs` |

## Why

The new screens (T32 onwards) need a few facts the data model does not keep yet — the history of requests,
how long the current task has been running, the CPU's name and core count — and two text widgets: big digits
for the live speed, and one-character level bars for the cores. This sheet adds them without changing any
screen.

## Contract

### `src/app/state.rs`

```rust
/// Requests kept in `App::requests`.
pub const REQUEST_HISTORY: usize = 50;
```

`App` gains two public fields (doc comments as given):

- `/// Every request seen in the log, oldest first, at most REQUEST_HISTORY; the last one is last_request.`
  `pub requests: VecDeque<LastRequest>`
- `/// (id_task, frame) when the first active model's slot 0 started processing that task.`
  `pub task_started: Option<(i64, u64)>`

and one method:

```rust
/// Seconds since `task_started`, from the 100 ms ticks (`(frame - start) / 10`); `None` without a task.
pub fn task_elapsed_s(&self) -> Option<u64>;
```

Rules:
- `requests` follows `last_request` exactly: every time `update_last_request` starts a new `LastRequest`, push it
  at the back (dropping the front beyond `REQUEST_HISTORY`); every time it fills a field of the current one,
  replace the back with the updated value. `last_request` keeps its meaning and stays equal to
  `requests.back()`.
- On `Event::Llama(s)`: if `s.active[0].slots[0]` exists and `is_processing`, with `id_task = t`: when
  `task_started` is not `Some((t, _))`, set it to `Some((t, self.frame))`; otherwise leave it. When there is no
  processing slot 0, set it to `None`.

### `src/collect/system.rs`

```rust
/// The CPU's name and physical core count, from `proc/cpuinfo`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CpuInfo {
    /// `short_cpu_name` of the first `model name`.
    pub model: Option<String>,
    /// The first `cpu cores` value.
    pub cores: Option<u32>,
    /// The number of `processor` lines.
    pub threads: Option<u32>,
}

/// Parses `/proc/cpuinfo`. Missing fields are `None` (`threads` is `None` when there is no `processor` line).
pub fn parse_cpuinfo(text: &str) -> CpuInfo;

/// Reads `proc/cpuinfo` under `sysroot`; all `None` when unreadable.
pub fn read_cpuinfo(sysroot: &Path) -> CpuInfo;

/// A short CPU name: remove `(R)`, `(TM)`, `(tm)`; cut from ` @ ` on; cut from ` with ` on (`with Radeon
/// Graphics`); remove a trailing `<N>-Core Processor`, `Processor` or `CPU`; remove a leading
/// `<N>th Gen ` / `<N>st Gen ` / `<N>nd Gen ` / `<N>rd Gen `; collapse runs of spaces; trim.
pub fn short_cpu_name(raw: &str) -> String;
```

### `src/collect/sampler.rs`

`SystemSnapshot` gains `#[serde(default)] pub cpu_info: CpuInfo` (`CpuInfo` also derives `Default`), filled by
`read_cpuinfo(sysroot)` in `sample`. `#[serde(default)]` keeps an agent of an older version readable.
Update every `SystemSnapshot { … }` literal (`src/app/demo.rs`: model `Intel Core i7-11700K`, 8 cores, 16
threads; `tests/app_state.rs`: `CpuInfo::default()`).

### `tests/fixtures/sysroot/proc/cpuinfo`

A new fixture (adding it is part of this sheet): 16 blocks `processor : 0` … `processor : 15`, each with
`model name : 11th Gen Intel(R) Core(TM) i7-11700K @ 3.60GHz`, `siblings : 16`, `cpu cores : 8`, separated by
a blank line (tabs before the colons, as the kernel writes them).

### `src/ui/big.rs`

```rust
/// `text` in 3-row block digits for the live speed. Glyphs, each 3 columns wide, rows top to bottom:
/// `0` `█▀█` `█ █` `▀▀▀` · `1` `▀█ ` ` █ ` `▀▀▀` · `2` `▀▀█` `█▀▀` `▀▀▀` · `3` `▀▀█` ` ▀█` `▀▀▀` ·
/// `4` `█ █` `▀▀█` `  ▀` · `5` `█▀▀` `▀▀█` `▀▀▀` · `6` `█▀▀` `█▀█` `▀▀▀` · `7` `▀▀█` `  █` `  ▀` ·
/// `8` `█▀█` `█▀█` `▀▀▀` · `9` `█▀█` `▀▀█` `▀▀▀`; `.` is one column ` ` ` ` `▀`; `-` is `   ` `▀▀▀` `   `;
/// any other character is one column of spaces. One space column between two glyphs, none at the ends.
pub fn big_digits(text: &str) -> [String; 3];

/// One character per value, 0–100 → `▁▂▃▄▅▆▇█` (index `round(v / 100 × 7)`, clamped 0–7); NaN → a space.
pub fn level_bars(values: &[f32]) -> String;
```

## Pitfalls

- `update_last_request` already has four places that start or fill a request: change them through one private
  helper that keeps `requests` in step, rather than touching `requests` in four places.
- The `frame` counter only moves with `Event::Tick`; `task_elapsed_s` is therefore 0 in tests without ticks.
- `short_cpu_name` works on strings only — no regex dependency; strip the ordinal with a small loop over the
  leading digits.
- Expected widths: `big_digits("52.4")` rows are 3 + 1 + 3 + 1 + 1 + 1 + 3 = 13 columns.

## Tests

`tests/tui2_data.rs`, exactly these names.

- `cpu_names`: `11th Gen Intel(R) Core(TM) i7-11700K @ 3.60GHz` → `Intel Core i7-11700K`;
  `AMD Ryzen 5 5625U with Radeon Graphics` → `AMD Ryzen 5 5625U`; `AMD Ryzen 9 7950X 16-Core Processor` →
  `AMD Ryzen 9 7950X`; `Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz` → `Intel Xeon CPU E5-2680 v4` (`CPU` is
  removed only at the end); `""` → `""`.
- `cpuinfo_fixture`: `read_cpuinfo(tests/fixtures/sysroot)` → model `Intel Core i7-11700K`, cores 8, threads 16;
  `parse_cpuinfo("")` → all `None`.
- `snapshot_has_cpu_info`: a `Sampler` on the fixture sysroot → `cpu_info.model` is `Some("Intel Core i7-11700K")`;
  JSON of a snapshot without the `cpu_info` key parses with `CpuInfo::default()`.
- `big_digits_glyphs`: `big_digits("52.4")` → `["█▀▀ ▀▀█   █ █", "▀▀█ █▀▀   ▀▀█", "▀▀▀ ▀▀▀ ▀   ▀"]`;
  `big_digits("")` → three empty strings.
- `level_bars_values`: `[0.0, 50.0, 100.0, f32::NAN, 120.0]` → `"▁▅█ █"`.
- `request_history`: an `App` fed the fixture journal → `requests.len()` 2, tasks `9137` then `9397`,
  `requests.back() == last_request.as_ref()`; fed 60 times the demo cycle (`demo::log_lines(40·k, 40·(k+1))`
  for k in 0..60) → `requests.len()` 50.
- `task_timer`: `demo::llama(20.0, Generating)` then 25 `Event::Tick` then the same snapshot → `task_elapsed_s()`
  `Some(2)`; `demo::llama(0.0, Idle)` → `None`.

## Acceptance criteria

- [ ] The public API above exists; every existing test passes (literals updated, no behaviour changed).
- [ ] The 7 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
