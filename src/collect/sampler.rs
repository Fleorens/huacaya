//! Assembles one `SystemSnapshot` from every T02–T04 collector, and the thread that samples it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::collect::disk::{DiskSample, auto_mounts, read_disks};
use crate::collect::gpu::{GpuSample, read_gpus};
use crate::collect::net::{NetReader, NetSample};
use crate::collect::nvidia::{SmiBackoff, parse_smi, run_smi};
use crate::collect::process::{ProcSample, read_server_procs};
use crate::collect::sensors::{TempReading, cpu_temp, nvme_temp, read_temps};
use crate::collect::system::{CpuReader, CpuSample, MemSample, read_hostname, read_mem, read_uptime};

/// One snapshot of the machine, assembled from every system collector (T02–T04).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
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

/// Owns what must persist between two samples: the CPU and network references used to derive the
/// rates, whether `nvidia-smi` was found missing (so we stop spawning it), and its failure backoff. Everything else is read
/// fresh each time; a missing source yields `None`/empty, never a panic.
pub struct Sampler {
    sysroot: PathBuf,
    mounts: Vec<PathBuf>,
    cpu: CpuReader,
    net: NetReader,
    nvidia_missing: bool,
    nvidia_backoff: SmiBackoff,
}

impl Sampler {
    /// A sampler rooted at `sysroot`, reading the given `mounts` for disk usage.
    pub fn new(sysroot: PathBuf, mounts: Vec<PathBuf>) -> Self {
        Self {
            sysroot,
            mounts,
            cpu: CpuReader::new(),
            net: NetReader::new(),
            nvidia_missing: false,
            nvidia_backoff: SmiBackoff::default(),
        }
    }

    /// Reads every system source once. Never panics; missing sources give `None`/empty. The first
    /// sample has no baseline, so `cpu.usage_pct` and the network rates are `None`.
    pub fn sample(&mut self, now: Instant) -> SystemSnapshot {
        let sysroot = &self.sysroot;

        // AMD cards first, then the NVIDIA ones (only on a real machine, `sysroot == /`, never for a
        // test sysroot). Only a spawn that fails with `NotFound` stops the tries for good; other
        // failures are retried, once a minute after 3 in a row (`SmiBackoff`).
        let mut gpus = read_gpus(sysroot);
        if !self.nvidia_missing && sysroot.as_path() == Path::new("/") && self.nvidia_backoff.should_try(now)
        {
            let result = run_smi();
            self.nvidia_backoff.record(result.is_ok(), now);
            match result {
                Ok(text) => gpus.extend(parse_smi(&text)),
                Err(kind) => self.nvidia_missing = kind == std::io::ErrorKind::NotFound,
            }
        }

        let temps = read_temps(sysroot);
        SystemSnapshot {
            hostname: read_hostname(sysroot),
            uptime_s: read_uptime(sysroot),
            cpu: self.cpu.sample(sysroot),
            cpu_temp_c: cpu_temp(&temps),
            nvme_temp_c: nvme_temp(&temps),
            mem: read_mem(sysroot),
            gpus,
            temps,
            disks: if self.mounts.is_empty() {
                read_disks(&auto_mounts(sysroot))
            } else {
                read_disks(&self.mounts)
            },
            nets: self.net.sample(sysroot, now),
            procs: read_server_procs(sysroot),
        }
    }
}

/// Runs `sampler.sample` every `interval` on a new thread and hands each snapshot to `sink`. Stops when
/// `stop` is set or when `sink` returns `false`, sleeping in slices of at most 100 ms so a stop request
/// is honoured quickly.
pub fn spawn(
    sampler: Sampler,
    interval: Duration,
    stop: Arc<AtomicBool>,
    mut sink: impl FnMut(SystemSnapshot) -> bool + Send + 'static,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut sampler = sampler;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !sink(sampler.sample(Instant::now())) {
                break;
            }
            let mut slept = Duration::ZERO;
            while slept < interval {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let slice = (interval - slept).min(Duration::from_millis(100));
                thread::sleep(slice);
                slept += slice;
            }
        }
    })
}
