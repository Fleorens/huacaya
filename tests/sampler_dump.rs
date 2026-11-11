//! Integration tests for `collect::sampler` (the `Sampler` and its thread) and `dump` (`--dump`).
//!
//! Values come from `tests/fixtures/` (a fake 16-thread, 32 GB machine). Floats compare within 0.01.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use clap::Parser;

use llamatop::collect::sampler::{Sampler, spawn};
use llamatop::config::Cli;
use llamatop::dump;

fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

/// Absolute path of the journal fixture, for `cat`.
fn fixture_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/journal/llama-server.txt")
        .display()
        .to_string()
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

#[test]
fn snapshot_of_the_fixture_machine() {
    let mut sampler = Sampler::new(sysroot(), vec![PathBuf::from("/")]);
    let snap = sampler.sample(Instant::now());

    assert_eq!(snap.hostname.as_deref(), Some("aibox-flo"));
    assert_eq!(snap.uptime_s, Some(356400));
    // First sample: no baseline yet, so no CPU usage.
    assert_eq!(snap.cpu.usage_pct, None);
    assert_eq!(snap.mem.as_ref().map(|m| m.total), Some(33554432000u64));
    assert_eq!(snap.gpus.len(), 1);
    assert!(approx(snap.gpus[0].busy_pct.unwrap_or(0.0), 97.0));
    assert!(approx(snap.cpu_temp_c.unwrap_or(0.0), 52.0));
    assert!(approx(snap.nvme_temp_c.unwrap_or(0.0), 41.85));
    assert_eq!(snap.temps.len(), 9);
    assert_eq!(snap.disks.len(), 1);
    assert_eq!(snap.nets.len(), 1);
    assert_eq!(snap.nets[0].iface, "wlp9s0");
    // First sample: no baseline yet, so no network rate.
    assert_eq!(snap.nets[0].rx_bps, None);
    assert_eq!(snap.nets[0].tx_bps, None);
    assert_eq!(snap.procs.len(), 2);
}

#[test]
fn second_sample_keeps_references() {
    let mut sampler = Sampler::new(sysroot(), vec![PathBuf::from("/")]);
    let t0 = Instant::now();
    sampler.sample(t0);
    // Same `proc/stat`: the total did not grow, so usage stays `None`, but the per-core count is kept.
    let snap = sampler.sample(t0 + Duration::from_secs(1));

    // Counters did not move: a 0.0 rate, not `None` (the baseline from the first sample is used).
    assert_eq!(snap.nets[0].rx_bps, Some(0.0));
    assert_eq!(snap.nets[0].tx_bps, Some(0.0));
    assert_eq!(snap.cpu.per_core_pct.len(), 16);
    assert!(snap.cpu.per_core_pct.iter().all(|p| approx(*p, 0.0)));
    assert_eq!(snap.cpu.usage_pct, None);
}

#[test]
fn empty_machine() {
    let mut sampler = Sampler::new(PathBuf::from("/nonexistent-llamatop"), vec![]);
    let snap = sampler.sample(Instant::now());

    assert_eq!(snap.hostname, None);
    assert_eq!(snap.uptime_s, None);
    assert_eq!(snap.mem, None);
    assert!(snap.gpus.is_empty());
    assert!(snap.temps.is_empty());
    assert!(snap.disks.is_empty());
    assert!(snap.nets.is_empty());
    assert!(snap.procs.is_empty());
}

#[test]
fn spawn_delivers_and_stops() {
    let sampler = Sampler::new(sysroot(), vec![PathBuf::from("/")]);
    let stop = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(Mutex::new(0u32));
    let calls_for_sink = calls.clone();

    let handle: JoinHandle<()> = spawn(sampler, Duration::from_millis(50), stop.clone(), move |_snap| {
        *calls_for_sink.lock().unwrap() += 1;
        true
    });

    std::thread::sleep(Duration::from_millis(300));
    stop.store(true, Ordering::Relaxed);

    let start = Instant::now();
    handle.join().expect("the thread joins");
    // A stop request is honoured within 100 ms slices, well under 1 s.
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(*calls.lock().unwrap() >= 2, "sink called {} times", calls.lock().unwrap());
}

/// Builds a `--dump` Cli pointing at the fixture machine, a down server and a given log command.
fn dump_cli(log_cmd: &str) -> Cli {
    Cli::parse_from([
        "llamatop",
        "--dump",
        "--url",
        "http://127.0.0.1:9",
        "--sysroot",
        sysroot().to_str().unwrap(),
        "--mount",
        "/",
        "--log-cmd",
        log_cmd,
    ])
}

#[test]
fn dump_with_server_down() {
    let cli = dump_cli(&format!("cat {}", fixture_path()));
    let start = Instant::now();
    let text = dump::run(&cli).expect("run is Ok");
    assert!(start.elapsed() < Duration::from_secs(5), "dump took {:?}", start.elapsed());

    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    // Exactly these four top-level keys.
    let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(|s| s.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["llama", "log_error", "log_tail", "system"]);

    assert_eq!(value["system"]["hostname"], "aibox-flo");
    assert_eq!(value["system"]["gpus"][0]["busy_pct"], 97.0);
    // Default serde enum representation: `Down` carries its string.
    assert!(value["llama"]["health"]["Down"].as_str().is_some());
    assert!(value["llama"]["models"].as_array().unwrap().is_empty());
    let tail = value["log_tail"].as_array().unwrap();
    assert_eq!(tail.len(), 20);
    assert_eq!(tail[20 - 1]["text"], "log source ended: cat");
    assert_eq!(value["log_error"], serde_json::Value::Null);
}

#[test]
fn dump_with_missing_log_program() {
    let cli = dump_cli("llamatop-no-such-program");
    let text = dump::run(&cli).expect("run is Ok");
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");

    assert!(value["log_tail"].as_array().unwrap().is_empty());
    let err = value["log_error"].as_str().expect("log_error is a string");
    assert!(!err.is_empty());
}

#[test]
fn dump_never_shows_the_key() {
    // Write the key to a temp file and point `--api-key-file` at it.
    let key_path = std::env::temp_dir().join(format!("llamatop-test-key-{}.txt", std::process::id()));
    std::fs::write(&key_path, "sk-test-SECRET-123").expect("write key file");
    let cli = Cli::parse_from([
        "llamatop",
        "--dump",
        "--url",
        "http://127.0.0.1:9",
        "--sysroot",
        sysroot().to_str().unwrap(),
        "--mount",
        "/",
        "--api-key-file",
        key_path.to_str().unwrap(),
        "--log-cmd",
        "cat /dev/null",
    ]);
    let text = dump::run(&cli).expect("run is Ok");
    let _ = std::fs::remove_file(&key_path);

    assert!(!text.contains("SECRET"), "the API key leaked into the report");
}
