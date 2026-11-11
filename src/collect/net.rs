//! Network traffic, read from `proc/net/dev` under the sysroot.
//!
//! Parsing (`parse_net_dev`) is pure; `NetReader::rates` computes rates from stored baselines and
//! `sample` only does I/O.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::collect::read_trimmed;
use crate::util::Rate;

/// Cumulative bytes on one interface, as read from `/proc/net/dev`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetCounters {
    pub iface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// One interface's cumulative counters. The name ends at the first `:` (the kernel may print
/// `eth0:123` with no space); the columns after it start with rx bytes, then tx bytes 9 later.
pub fn parse_net_dev(text: &str) -> Vec<NetCounters> {
    text.lines()
        .skip(2) // the two header lines
        .filter_map(|line| {
            let (iface, rest) = line.split_once(':')?;
            let cols: Vec<&str> = rest.split_whitespace().collect();
            let rx_bytes = cols.first()?.parse::<u64>().ok()?;
            let tx_bytes = cols.get(8)?.parse::<u64>().ok()?;
            Some(NetCounters { iface: iface.trim().to_string(), rx_bytes, tx_bytes })
        })
        .filter(|c| c.iface != "lo")
        .collect()
}

/// One interface's rate over the last sample plus its cumulative totals.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetSample {
    pub iface: String,
    /// Bytes per second; `None` on the first sample of an interface.
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    pub rx_total: u64,
    pub tx_total: u64,
}

/// Keeps one pair of `Rate` per interface, so each interface has its own baseline.
#[derive(Debug, Default)]
pub struct NetReader {
    rx: HashMap<String, Rate>,
    tx: HashMap<String, Rate>,
}

impl NetReader {
    /// A reader with no interface seen yet.
    pub fn new() -> Self {
        NetReader { rx: HashMap::new(), tx: HashMap::new() }
    }

    /// Reads `proc/net/dev` under `sysroot`, dropping silent interfaces (rx + tx == 0).
    pub fn sample(&mut self, sysroot: &Path, now: Instant) -> Vec<NetSample> {
        let Some(text) = read_trimmed(sysroot, "proc/net/dev") else {
            return Vec::new();
        };
        let counters: Vec<NetCounters> =
            parse_net_dev(&text).into_iter().filter(|c| c.rx_bytes != 0 || c.tx_bytes != 0).collect();
        self.rates(counters, now)
    }

    /// Rates from already-parsed counters, using each interface's stored baseline.
    pub fn rates(&mut self, counters: Vec<NetCounters>, now: Instant) -> Vec<NetSample> {
        counters
            .into_iter()
            .map(|c| {
                let rx_bps = self.rx.entry(c.iface.clone()).or_default().update(c.rx_bytes as f64, now);
                let tx_bps = self.tx.entry(c.iface.clone()).or_default().update(c.tx_bytes as f64, now);
                NetSample { iface: c.iface, rx_bps, tx_bps, rx_total: c.rx_bytes, tx_total: c.tx_bytes }
            })
            .collect()
    }
}
