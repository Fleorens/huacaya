//! Tests for T04: disks, network and llama-server processes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use huacaya::collect::disk::{disk_usage, read_disks};
use huacaya::collect::net::{NetCounters, NetReader};
use huacaya::collect::process::{ProcSample, ServerRole, read_server_procs};

fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

#[test]
fn disk_usage_of_root() {
    let Some(disk) = disk_usage(Path::new("/")) else {
        panic!("disk_usage(/) is None");
    };
    assert_eq!(disk.mount, "/");
    assert!(disk.total > 0);
    assert!(disk.used <= disk.total);
}

#[test]
fn disk_usage_of_missing_path() {
    assert!(disk_usage(Path::new("/nonexistent-huacaya")).is_none());
    let mounts = [PathBuf::from("/"), PathBuf::from("/nonexistent-huacaya")];
    assert_eq!(read_disks(&mounts).len(), 1);
}

#[test]
fn net_dev_parsing() {
    let text = std::fs::read_to_string(sysroot().join("proc/net/dev")).unwrap();
    let counters = huacaya::collect::net::parse_net_dev(&text);
    assert_eq!(counters.len(), 2);
    assert_eq!(
        counters[0],
        NetCounters { iface: "wlp9s0".into(), rx_bytes: 9876543210, tx_bytes: 1234567890 }
    );
    assert_eq!(counters[1], NetCounters { iface: "enp8s0".into(), rx_bytes: 0, tx_bytes: 0 });
}

#[test]
fn net_dev_without_space_after_colon() {
    let text = "h1\nh2\neth0:100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0\n";
    let counters = huacaya::collect::net::parse_net_dev(text);
    assert_eq!(counters.len(), 1);
    assert_eq!(counters[0].rx_bytes, 100);
    assert_eq!(counters[0].tx_bytes, 200);
}

#[test]
fn net_rates() {
    let mut reader = NetReader::new();
    let t0 = Instant::now();
    let first = reader.rates(vec![NetCounters { iface: "wlp9s0".into(), rx_bytes: 1000, tx_bytes: 500 }], t0);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].rx_bps, None);
    assert_eq!(first[0].tx_bps, None);

    let second = reader.rates(
        vec![NetCounters { iface: "wlp9s0".into(), rx_bytes: 3000, tx_bytes: 1500 }],
        t0 + Duration::from_secs(2),
    );
    assert_eq!(second[0].rx_bps, Some(1000.0));
    assert_eq!(second[0].tx_bps, Some(500.0));
}

#[test]
fn silent_interfaces_are_hidden() {
    let mut reader = NetReader::new();
    let samples = reader.sample(Path::new(&sysroot()), Instant::now());
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].iface, "wlp9s0");
}

#[test]
fn server_processes() {
    let procs = read_server_procs(Path::new(&sysroot()));
    assert_eq!(procs.len(), 2);

    assert_eq!(
        procs[0],
        ProcSample {
            pid: 1201,
            role: ServerRole::Router,
            model: None,
            port: Some(8080),
            rss: 83886080,
            threads: 12,
        }
    );
    assert_eq!(
        procs[1],
        ProcSample {
            pid: 48211,
            role: ServerRole::Instance,
            model: Some("peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M".into()),
            port: Some(35335),
            rss: 9663676416,
            threads: 34,
        }
    );
}

#[test]
fn no_processes_on_empty_root() {
    let procs = read_server_procs(Path::new("/nonexistent-huacaya"));
    assert!(procs.is_empty());
}
