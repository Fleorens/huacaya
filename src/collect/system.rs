//! CPU, memory, load and uptime, read from `proc/` and `sys/` under the sysroot.
//!
//! Parsing (`parse_*`) is pure and unit-tested; the `read_*`/`CpuReader` layer only does I/O.
//! Values use the data-model units: bytes, degrees Celsius, watts, percent 0–100, MHz, seconds.

use std::path::Path;

use serde::Serialize;

use crate::collect::{read_parsed, read_trimmed};

/// Jiffies of one `cpu` line. `idle` = idle + iowait (columns 4 and 5).
/// `total` = the sum of the first 8 columns (user … steal); guest columns are already in user.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

/// The `cpu ` aggregate and one entry per `cpu<N>` core, in file order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuStat {
    pub all: CpuTimes,
    pub cores: Vec<CpuTimes>,
}

/// Parses `/proc/stat`. `all` is the `cpu ` line, `cores` the `cpu<N>` lines in file order.
/// `None` when there is no aggregate line. Lines with fewer than 5 numbers are skipped.
pub fn parse_proc_stat(text: &str) -> Option<CpuStat> {
    let mut all: Option<CpuTimes> = None;
    let mut cores: Vec<CpuTimes> = Vec::new();

    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let Some(label) = tokens.next() else { continue };
        if label != "cpu" && !is_cpu_label(label) {
            continue; // skips `intr`, `ctxt`, … without parsing their numbers.
        }
        // A malformed token would shift the columns: skip the whole line instead.
        let Ok(nums) = tokens.map(str::parse::<u64>).collect::<Result<Vec<u64>, _>>() else {
            continue;
        };
        // Need at least idle + iowait (columns 3 and 4) to be useful.
        if nums.len() < 5 {
            continue;
        }
        let times = CpuTimes {
            total: nums.iter().take(8).sum(),
            idle: nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0),
        };
        if label == "cpu" {
            all = Some(times);
        } else {
            cores.push(times);
        }
    }

    all.map(|all| CpuStat { all, cores })
}

/// A directory name like `cpu0`…`cpu15`: `cpu` followed by digits only.
fn is_cpu_label(label: &str) -> bool {
    label.strip_prefix("cpu").is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// Busy share between two readings, 0–100. `None` when `total` did not grow.
pub fn usage_pct(prev: CpuTimes, cur: CpuTimes) -> Option<f32> {
    if cur.total <= prev.total {
        return None;
    }
    Some(core_usage_pct(prev, cur))
}

/// Per-core busy share, 0–100. `0.0` when the core's `total` did not grow.
fn core_usage_pct(prev: CpuTimes, cur: CpuTimes) -> f32 {
    if cur.total <= prev.total {
        return 0.0;
    }
    let delta_total = cur.total - prev.total;
    // iowait can go backwards on Linux: saturate rather than underflow, then clamp.
    let delta_idle = cur.idle.saturating_sub(prev.idle);
    // Compute in f64 so large counters keep precision, return the data-model unit (f32).
    (100.0 * (1.0 - delta_idle as f64 / delta_total as f64)).clamp(0.0, 100.0) as f32
}

/// All fields in bytes. `used` = total − available.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemSample {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

/// Parses `/proc/meminfo` (values in kB). `None` without `MemTotal` or `MemAvailable`.
/// Missing swap lines count as 0.
pub fn parse_meminfo(text: &str) -> Option<MemSample> {
    let mut total: Option<u64> = None;
    let mut available: Option<u64> = None;
    let mut cached: Option<u64> = None;
    let mut swap_total: Option<u64> = None;
    let mut swap_free: Option<u64> = None;

    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else { continue };
        let Some(kb) = fields.next().and_then(|v| v.parse().ok()) else { continue };
        let key = key.trim_end_matches(':');
        match key {
            "MemTotal" => total = Some(kb),
            "MemAvailable" => available = Some(kb),
            "Cached" => cached = Some(kb),
            "SwapTotal" => swap_total = Some(kb),
            "SwapFree" => swap_free = Some(kb),
            _ => {}
        }
    }

    let total = total?;
    let available = available?;
    let swap_total = swap_total.unwrap_or(0);
    let swap_used = swap_total.saturating_sub(swap_free.unwrap_or(0));

    Some(MemSample {
        total: total.saturating_mul(1024),
        available: available.saturating_mul(1024),
        used: total.saturating_sub(available).saturating_mul(1024),
        cached: cached.unwrap_or(0).saturating_mul(1024),
        swap_total: swap_total.saturating_mul(1024),
        swap_used: swap_used.saturating_mul(1024),
    })
}

/// Averages of the 1, 5 and 15 minute load.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadAvg {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

/// Parses `proc/loadavg`'s leading three numbers. `None` when they are missing or malformed.
pub fn parse_loadavg(text: &str) -> Option<LoadAvg> {
    let mut fields = text.split_whitespace();
    let one: f32 = fields.next()?.parse().ok()?;
    let five: f32 = fields.next()?.parse().ok()?;
    let fifteen: f32 = fields.next()?.parse().ok()?;
    Some(LoadAvg { one, five, fifteen })
}

/// Average and maximum CPU frequency, in MHz.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuFreq {
    pub avg_mhz: f32,
    pub max_mhz: f32,
}

/// Average and maximum of `sys/devices/system/cpu/cpu<N>/cpufreq/scaling_cur_freq` (kHz in the file).
/// Only directories named `cpu` followed by digits only. `None` when no file can be read.
pub fn read_cpu_freq(sysroot: &Path) -> Option<CpuFreq> {
    let cpu_dir = sysroot.join("sys/devices/system/cpu");
    let entries = std::fs::read_dir(&cpu_dir).ok()?;

    let mut freqs_khz: Vec<u64> = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !is_cpu_label(&name) {
            continue; // skips `cpufreq/`, `cpuidle/` and any sibling directory.
        }
        if !entry.file_type().ok().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        // A core without a readable cpufreq (offline, no driver) is skipped, not fatal.
        let relative = format!("sys/devices/system/cpu/{name}/cpufreq/scaling_cur_freq");
        let Some(khz) = read_parsed::<u64>(sysroot, &relative) else { continue };
        freqs_khz.push(khz);
    }

    if freqs_khz.is_empty() {
        return None;
    }
    let sum: u64 = freqs_khz.iter().sum();
    let avg_mhz = sum as f64 / freqs_khz.len() as f64 / 1000.0;
    let max_mhz = *freqs_khz.iter().max()? as f64 / 1000.0;
    Some(CpuFreq { avg_mhz: avg_mhz as f32, max_mhz: max_mhz as f32 })
}

/// Whole seconds from `proc/uptime`.
pub fn read_uptime(sysroot: &Path) -> Option<u64> {
    let text = read_trimmed(sysroot, "proc/uptime")?;
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(seconds as u64)
}

/// Trimmed content of `proc/sys/kernel/hostname`; `None` when missing or empty.
pub fn read_hostname(sysroot: &Path) -> Option<String> {
    read_trimmed(sysroot, "proc/sys/kernel/hostname").filter(|h| !h.is_empty())
}

/// Reads and parses `proc/meminfo`.
pub fn read_mem(sysroot: &Path) -> Option<MemSample> {
    let text = read_trimmed(sysroot, "proc/meminfo")?;
    parse_meminfo(&text)
}

/// Reads `proc/loadavg`.
fn read_load(sysroot: &Path) -> Option<LoadAvg> {
    let text = read_trimmed(sysroot, "proc/loadavg")?;
    parse_loadavg(&text)
}

/// One sample of the machine's CPU, memory, load and uptime.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
pub struct CpuReader {
    prev: Option<CpuStat>,
}

impl CpuReader {
    /// A reader with no previous reading yet.
    pub fn new() -> Self {
        CpuReader { prev: None }
    }

    /// Reads `proc/stat`, `proc/loadavg` and cpufreq under `sysroot`.
    pub fn sample(&mut self, sysroot: &Path) -> CpuSample {
        let (usage_pct, per_core_pct) = read_trimmed(sysroot, "proc/stat")
            .map(|text| self.usage_from(&text))
            .unwrap_or((None, Vec::new()));
        CpuSample { usage_pct, per_core_pct, freq: read_cpu_freq(sysroot), load: read_load(sysroot) }
    }

    /// Same computation from an already-read `/proc/stat` text (what `sample` calls; used by tests).
    pub fn usage_from(&mut self, stat_text: &str) -> (Option<f32>, Vec<f32>) {
        let Some(cur) = parse_proc_stat(stat_text) else {
            return (None, Vec::new());
        };

        let Some(prev) = self.prev.take() else {
            // First sample: keep the reading for next time, report nothing yet.
            self.prev = Some(cur);
            return (None, Vec::new());
        };

        let prev_cores = prev.cores.len();
        let per_core_pct = if cur.cores.len() != prev_cores {
            // Core count changed: nothing to compare against.
            Vec::new()
        } else {
            prev.cores.iter().zip(cur.cores.iter()).map(|(p, c)| core_usage_pct(*p, *c)).collect()
        };

        let usage = usage_pct(prev.all, cur.all);
        self.prev = Some(cur);
        (usage, per_core_pct)
    }
}
