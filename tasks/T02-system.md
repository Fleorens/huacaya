# T02 — CPU, memory, load, uptime

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T01 |
| **Read first** | `docs/01-architecture.md` § 2 (table only) and § 3; `AGENTS.md` § Conventions is already in your instructions |
| **Deliverable** | `src/collect/mod.rs`, `src/collect/system.rs`, `tests/system.rs`; `pub mod collect;` in `src/lib.rs` |

## Why

The CPU and memory panels. Everything is read from `proc/` and `sys/` under a `sysroot`, so the tests run
on the fake machine `tests/fixtures/sysroot` (16 CPU threads, 32 GB).

## Fixtures (do not edit)

- `tests/fixtures/proc-stat-t0.txt`, `tests/fixtures/proc-stat-t1.txt`: two readings of `/proc/stat`, 1 s apart.
  `tests/fixtures/sysroot/proc/stat` is identical to `t1`.
- `tests/fixtures/sysroot/proc/{meminfo,loadavg,uptime}`, `proc/sys/kernel/hostname`.
- `tests/fixtures/sysroot/sys/devices/system/cpu/cpu<N>/cpufreq/scaling_cur_freq` (kHz), N = 0…15.
  The same directory also holds `cpufreq/` and `cpuidle/`, which are **not** CPUs.

## Contract — `src/collect/system.rs`

Every struct derives `Debug, Clone, PartialEq, serde::Serialize`.

```rust
/// Jiffies of one `cpu` line. `idle` = idle + iowait (columns 4 and 5).
/// `total` = the sum of the first 8 columns (user … steal); guest columns are already in user.
#[derive(Copy)]
pub struct CpuTimes { pub total: u64, pub idle: u64 }

pub struct CpuStat { pub all: CpuTimes, pub cores: Vec<CpuTimes> }

/// Parses `/proc/stat`. `all` is the `cpu ` line, `cores` the `cpu<N>` lines in file order.
/// `None` when there is no aggregate line. Lines with fewer than 5 numbers are skipped.
pub fn parse_proc_stat(text: &str) -> Option<CpuStat>;

/// Busy share between two readings, 0–100. `None` when `total` did not grow.
pub fn usage_pct(prev: CpuTimes, cur: CpuTimes) -> Option<f32>;

/// All fields in bytes. `used` = total − available.
pub struct MemSample {
    pub total: u64, pub available: u64, pub used: u64, pub cached: u64,
    pub swap_total: u64, pub swap_used: u64,
}

/// Parses `/proc/meminfo` (values in kB). `None` without `MemTotal` or `MemAvailable`.
/// Missing swap lines count as 0.
pub fn parse_meminfo(text: &str) -> Option<MemSample>;

pub struct LoadAvg { pub one: f32, pub five: f32, pub fifteen: f32 }
pub fn parse_loadavg(text: &str) -> Option<LoadAvg>;

pub struct CpuFreq { pub avg_mhz: f32, pub max_mhz: f32 }

/// Average and maximum of `sys/devices/system/cpu/cpu<N>/cpufreq/scaling_cur_freq` (kHz in the file).
/// Only directories named `cpu` followed by digits only. `None` when no file can be read.
pub fn read_cpu_freq(sysroot: &Path) -> Option<CpuFreq>;

/// Whole seconds from `proc/uptime`.
pub fn read_uptime(sysroot: &Path) -> Option<u64>;
/// Trimmed content of `proc/sys/kernel/hostname`; `None` when missing or empty.
pub fn read_hostname(sysroot: &Path) -> Option<String>;
pub fn read_mem(sysroot: &Path) -> Option<MemSample>;

pub struct CpuSample {
    /// `None` on the first sample (no previous reading) and when `proc/stat` is unreadable.
    pub usage_pct: Option<f32>,
    /// One entry per core, same order as `proc/stat`; empty on the first sample.
    pub per_core_pct: Vec<f32>,
    pub freq: Option<CpuFreq>,
    pub load: Option<LoadAvg>,
}

/// Keeps the previous `/proc/stat` reading between samples.
#[derive(Debug, Default)]
pub struct CpuReader { /* private */ }

impl CpuReader {
    pub fn new() -> Self;
    /// Reads `proc/stat`, `proc/loadavg` and cpufreq under `sysroot`.
    pub fn sample(&mut self, sysroot: &Path) -> CpuSample;
    /// Same computation from an already-read `/proc/stat` text (what `sample` calls; used by tests).
    pub fn usage_from(&mut self, stat_text: &str) -> (Option<f32>, Vec<f32>);
}
```

A core whose `total` did not grow between two readings counts as 0 % in `per_core_pct`. If the number of
cores changed between two readings, `per_core_pct` is empty for that sample.

`src/collect/mod.rs` only declares the submodules (`pub mod system;`) and a private-to-crate helper that later
sheets reuse:

```rust
/// Reads a file under `sysroot` and trims it. `None` on any error.
pub(crate) fn read_trimmed(sysroot: &Path, relative: &str) -> Option<String>;
/// `read_trimmed` then `parse::<T>()`.
pub(crate) fn read_parsed<T: std::str::FromStr>(sysroot: &Path, relative: &str) -> Option<T>;
```

## Pitfalls

- `Path::join` with an **absolute** argument replaces the base: always join relative paths (`"proc/stat"`).
- `cpu  10000 …` has two spaces after `cpu`; split on whitespace, never on a fixed column.
- `MemTotal:       32768000 kB`: the unit is kB = 1024 bytes.
- A fresh `CpuReader` has no previous reading: do not report 0 %, report `None`.

## Tests — `tests/system.rs` (integration test, uses `llamatop::collect::system::*`)

Helper: `fn sysroot() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot") }`.
Compare floats with a tolerance of 0.01.

- `proc_stat_parses_16_cores`: `t0` → 16 cores; `all.total` = 160400, `all.idle` = 136320;
  `cores[0].total` = 10025, `cores[0].idle` = 8520.
- `usage_between_t0_and_t1`: feed `t0` then `t1` to `usage_from` → first call `(None, [])`, second call usage
  8.75, `per_core[0]` = 90.0, `per_core[1]` = 50.0, `per_core[2]` = 0.0, 16 entries.
- `same_reading_twice_gives_none`: `t1` then `t1` → usage `None`, 16 entries all 0.0.
- `usage_pct_guards`: `usage_pct` with equal totals → `None`.
- `meminfo`: `read_mem(sysroot)` → total 33554432000, available 12582912000, used 20971520000,
  cached 9216000000, swap_total 8589930496, swap_used 268435456.
- `meminfo_without_available_is_none`: `parse_meminfo("MemTotal: 1 kB\n")` → `None`.
- `loadavg`: 1.25, 0.80, 0.45.
- `cpu_freq`: avg 1218.75, max 4700.0.
- `uptime_and_hostname`: 356400 and `aibox-flo`.
- `missing_sysroot_gives_none`: on `/nonexistent-llamatop`: `read_mem`, `read_uptime`, `read_hostname`,
  `read_cpu_freq` are `None`; `CpuReader::new().sample(..)` has `usage_pct` `None`, `load` `None`, no panic.

## Acceptance criteria

- [ ] Exact public API above; all structs serialize with serde.
- [ ] The 10 tests exist under these names and pass, with the values above.
- [ ] No fixture modified (`git status --short tests/fixtures` is empty).
- [ ] `bash .devbox/check` is green.
