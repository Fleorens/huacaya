//! The NVIDIA GPUs, read from `nvidia-smi` (shipped with the driver). Parsing is pure and unit-tested;
//! the `read_nvidia` layer only does I/O. A missing program, a failure, or a timeout yields no card.

use std::io::{ErrorKind, Read};
use std::process::{Command, Stdio};
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
                let _ = child.kill();
                let _ = child.wait();
                return Err(ErrorKind::TimedOut);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
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
