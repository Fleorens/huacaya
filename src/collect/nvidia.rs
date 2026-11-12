//! The NVIDIA GPUs, read from `nvidia-smi` (shipped with the driver). Parsing is pure and unit-tested;
//! the `read_nvidia` layer only does I/O. A missing program, a failure, or a timeout yields no card.

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant};

use crate::collect::gpu::GpuSample;

/// The query passed to `nvidia-smi`, in this column order.
pub const QUERY: &str = "index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,\
power.limit,fan.speed,clocks.sm,pcie.link.gen.current,pcie.link.width.current";

/// One MiB, to turn `memory.used` / `memory.total` (MiB) into bytes.
const MIB: u64 = 1_048_576;

/// The 2 s ceiling on `nvidia-smi`, which `Command` runs without any timeout of its own.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Parses the output of `nvidia-smi --query-gpu=<QUERY> --format=csv,noheader,nounits`: one line per
/// GPU, comma-separated, each field trimmed. A field that is empty, `[N/A]`, `[Not Supported]` or not a
/// number is `None`. Lines with fewer than 12 fields are skipped. Never panics.
pub fn parse_smi(text: &str) -> Vec<GpuSample> {
    text.lines().filter_map(parse_line).collect()
}

/// One CSV line into a `GpuSample`, or `None` when it has fewer than 12 fields.
fn parse_line(line: &str) -> Option<GpuSample> {
    let f: Vec<&str> = line.split(',').map(str::trim).collect();
    if f.len() < 12 {
        return None;
    }

    let link_gen = num::<u32>(f[10]);
    let width = num::<u32>(f[11]);
    let link = match (link_gen, width) {
        (Some(g), Some(w)) => Some(format!("PCIe gen {g} x{w}")),
        _ => None,
    };

    Some(GpuSample {
        card: match f[0].parse::<u64>().ok() {
            Some(i) => format!("nvidia{i}"),
            None => "nvidia".to_string(),
        },
        name: f[1].to_string(),
        busy_pct: num(f[2]),
        vram_used: mib(f[3]),
        vram_total: mib(f[4]),
        gtt_used: None,
        temp_edge_c: num(f[5]),
        temp_junction_c: None,
        temp_mem_c: None,
        power_w: num(f[6]),
        power_cap_w: num(f[7]),
        fan_rpm: None,
        fan_pct: num(f[8]),
        sclk_mhz: num(f[9]),
        vddgfx_mv: None,
        link,
    })
}

/// A field that is empty, `[N/A]`, `[Not Supported]` or not a number is `None`.
fn num<T: FromStr>(field: &str) -> Option<T> {
    if field.is_empty() || field == "[N/A]" || field == "[Not Supported]" {
        return None;
    }
    field.parse().ok()
}

/// A `memory.used` / `memory.total` value (MiB) in bytes.
fn mib(field: &str) -> Option<u64> {
    num::<u64>(field).and_then(|m| m.checked_mul(MIB))
}

/// Runs `nvidia-smi` with [QUERY], waits at most `TIMEOUT` (then kills it), and parses its output. Empty
/// when the program is missing, fails, or times out.
pub fn read_nvidia() -> Vec<GpuSample> {
    run_smi().map(|text| parse_smi(&text)).unwrap_or_default()
}

/// Spawns `nvidia-smi` and returns its stdout. The error kind lets the sampler tell a missing program
/// (`NotFound`, stop trying) from a transient failure or a timeout (`TimedOut`, `Other`: try again).
pub(crate) fn run_smi() -> Result<String, ErrorKind> {
    let mut child = Command::new("nvidia-smi")
        .arg("--query-gpu")
        .arg(QUERY)
        .arg("--format=csv,noheader,nounits")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.kind())?;

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                kill_detached(child);
                return Err(ErrorKind::TimedOut);
            }
            Err(e) => {
                kill_detached(child);
                return Err(e.kind());
            }
        }
    };
    if !status.success() {
        return Err(ErrorKind::Other);
    }

    let mut stdout = child.stdout.take().ok_or(ErrorKind::Other)?;
    let mut text = String::new();
    stdout.read_to_string(&mut text).map_err(|e| e.kind())?;
    Ok(text)
}

/// Kills `child` and reaps it on a detached thread: a `nvidia-smi` stuck in the kernel (state D, hung
/// driver) survives `kill`, and waiting for it here would freeze the sampler thread and its `join`.
fn kill_detached(mut child: Child) {
    let _ = child.kill();
    // If the thread cannot be created, the child is dropped unreaped: a zombie beats a frozen sampler.
    let _ = std::thread::Builder::new().name("nvidia-smi-reap".into()).spawn(move || {
        let _ = child.wait();
    });
}

/// Consecutive failures after which `nvidia-smi` is only retried once per [BACKOFF_PERIOD].
pub const BACKOFF_AFTER: u32 = 3;

/// The retry period once `nvidia-smi` has failed [BACKOFF_AFTER] times in a row.
pub const BACKOFF_PERIOD: Duration = Duration::from_secs(60);

/// Spaces out `nvidia-smi` runs after repeated failures (error, non-zero exit, timeout), so a hung
/// driver does not add 2 s to every sample. Pure: the caller passes the time.
#[derive(Debug, Clone, Default)]
pub struct SmiBackoff {
    failures: u32,
    last_try: Option<Instant>,
}

impl SmiBackoff {
    /// Whether `nvidia-smi` should run at `now`: always below [BACKOFF_AFTER] consecutive failures,
    /// then only once [BACKOFF_PERIOD] has passed since the last try.
    pub fn should_try(&self, now: Instant) -> bool {
        if self.failures < BACKOFF_AFTER {
            return true;
        }
        self.last_try.is_none_or(|t| now.saturating_duration_since(t) >= BACKOFF_PERIOD)
    }

    /// Records the outcome of a try made at `now`: a success resets the failure count.
    pub fn record(&mut self, ok: bool, now: Instant) {
        self.last_try = Some(now);
        self.failures = if ok { 0 } else { self.failures.saturating_add(1) };
    }
}
