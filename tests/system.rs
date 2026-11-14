//! Integration tests for `collect::system`: CPU, memory, load and uptime.
//!
//! Values come from `tests/fixtures/` (a fake 16-thread, 32 GB machine). Floats compare within 0.01.

use std::path::PathBuf;

use huacaya::collect::system::*;

fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

/// Reads a fixture that lives directly under `tests/fixtures/` (not under `sysroot/`).
fn stat_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{}", name));
    std::fs::read_to_string(path).expect("fixture present")
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

#[test]
fn proc_stat_parses_16_cores() {
    let stat = parse_proc_stat(&stat_text("proc-stat-t0.txt")).expect("proc/stat parses");
    assert_eq!(stat.cores.len(), 16);
    assert_eq!(stat.all.total, 160400);
    assert_eq!(stat.all.idle, 136320);
    assert_eq!(stat.cores[0].total, 10025);
    assert_eq!(stat.cores[0].idle, 8520);
}

#[test]
fn usage_between_t0_and_t1() {
    let mut reader = CpuReader::new();

    // First sample: no previous reading, so nothing to compare against.
    let (usage, per_core) = reader.usage_from(&stat_text("proc-stat-t0.txt"));
    assert_eq!(usage, None);
    assert!(per_core.is_empty());

    // Second sample: aggregate and per-core busy shares.
    let (usage, per_core) = reader.usage_from(&stat_text("proc-stat-t1.txt"));
    assert!(approx(usage.expect("usage grew"), 8.75));
    assert!(approx(per_core[0], 90.0));
    assert!(approx(per_core[1], 50.0));
    assert!(approx(per_core[2], 0.0));
    assert_eq!(per_core.len(), 16);
}

#[test]
fn same_reading_twice_gives_none() {
    let mut reader = CpuReader::new();
    reader.usage_from(&stat_text("proc-stat-t1.txt"));

    let (usage, per_core) = reader.usage_from(&stat_text("proc-stat-t1.txt"));
    assert_eq!(usage, None);
    assert_eq!(per_core.len(), 16);
    assert!(per_core.iter().all(|v| approx(*v, 0.0)));
}

#[test]
fn usage_pct_guards() {
    let prev = CpuTimes { total: 100, idle: 50 };
    let cur = CpuTimes { total: 100, idle: 40 };
    assert_eq!(usage_pct(prev, cur), None);
}

#[test]
fn meminfo() {
    let mem = read_mem(&sysroot()).expect("meminfo parses");
    assert_eq!(mem.total, 33554432000);
    assert_eq!(mem.available, 12582912000);
    assert_eq!(mem.used, 20971520000);
    assert_eq!(mem.cached, 9216000000);
    assert_eq!(mem.swap_total, 8589930496);
    assert_eq!(mem.swap_used, 268435456);
}

#[test]
fn meminfo_without_available_is_none() {
    assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
}

#[test]
fn loadavg() {
    let path = sysroot().join("proc/loadavg");
    let text = std::fs::read_to_string(path).expect("loadavg fixture");
    let load = parse_loadavg(&text).expect("loadavg parses");
    assert!(approx(load.one, 1.25));
    assert!(approx(load.five, 0.80));
    assert!(approx(load.fifteen, 0.45));
}

#[test]
fn cpu_freq() {
    let freq = read_cpu_freq(&sysroot()).expect("cpufreq readable");
    assert!(approx(freq.avg_mhz, 1218.75));
    assert!(approx(freq.max_mhz, 4700.0));
}

#[test]
fn uptime_and_hostname() {
    assert_eq!(read_uptime(&sysroot()), Some(356400));
    assert_eq!(read_hostname(&sysroot()), Some("aibox-flo".to_string()));
}

#[test]
fn missing_sysroot_gives_none() {
    let bad = PathBuf::from("/nonexistent-huacaya");
    assert_eq!(read_mem(&bad), None);
    assert_eq!(read_uptime(&bad), None);
    assert_eq!(read_hostname(&bad), None);
    assert_eq!(read_cpu_freq(&bad), None);

    let mut reader = CpuReader::new();
    let sample = reader.sample(&bad);
    assert_eq!(sample.usage_pct, None);
    assert_eq!(sample.load, None);
}

#[test]
fn structs_serialize_with_serde() {
    let sample = CpuSample {
        usage_pct: Some(1.0),
        per_core_pct: vec![1.0, 2.0],
        freq: Some(CpuFreq { avg_mhz: 1000.0, max_mhz: 2000.0 }),
        load: Some(LoadAvg { one: 1.0, five: 1.0, fifteen: 1.0 }),
    };
    let _ = serde_json::to_string(&sample).expect("CpuSample serializes");

    let _ = serde_json::to_string(&CpuTimes { total: 1, idle: 1 }).expect("CpuTimes serializes");
    let _ = serde_json::to_string(&MemSample {
        total: 1,
        available: 1,
        used: 1,
        cached: 1,
        swap_total: 1,
        swap_used: 1,
    })
    .expect("MemSample serializes");
}

#[test]
fn idle_going_backwards_does_not_panic() {
    let mut reader = CpuReader::new();
    // iowait (column 5) drops between readings while total grows.
    let t0 = "cpu  100 0 100 500 300 0 0 0\ncpu0 100 0 100 500 300 0 0 0\n";
    let t1 = "cpu  400 0 200 500 100 0 0 0\ncpu0 400 0 200 500 100 0 0 0\n";
    reader.usage_from(t0);
    let (usage, per_core) = reader.usage_from(t1);
    let usage = usage.expect("total grew");
    assert!((0.0..=100.0).contains(&usage));
    assert_eq!(per_core.len(), 1);
    assert!((0.0..=100.0).contains(&per_core[0]));
}

#[test]
fn cpu_freq_skips_core_without_cpufreq() {
    let root = std::env::temp_dir().join(format!("huacaya-freq-{}", std::process::id()));
    let cpu = root.join("sys/devices/system/cpu");
    std::fs::create_dir_all(cpu.join("cpu0/cpufreq")).expect("create cpu0");
    std::fs::create_dir_all(cpu.join("cpu1")).expect("create cpu1"); // offline: no cpufreq/
    std::fs::write(cpu.join("cpu0/cpufreq/scaling_cur_freq"), "3600000\n").expect("write freq");

    let freq = read_cpu_freq(&root);
    std::fs::remove_dir_all(&root).ok();

    let freq = freq.expect("one readable core is enough");
    assert!(approx(freq.avg_mhz, 3600.0));
    assert!(approx(freq.max_mhz, 3600.0));
}
