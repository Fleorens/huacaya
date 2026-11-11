//! CPU, NVMe and GPU temperatures, read from the generic `hwmon` sysfs tree.
//!
//! Parsing is pure and unit-tested; the `read_*` layer only does I/O. Temperatures are in degrees
//! Celsius. A value that cannot be read is skipped, never fatal.

use std::path::Path;

use serde::Serialize;

use crate::collect::{read_parsed, read_trimmed};

/// One `temp<N>_input` of one `hwmon` chip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TempReading {
    /// Content of the chip's `name` file (`coretemp`, `nvme`, `amdgpu`, …).
    pub chip: String,
    /// Content of `temp<N>_label`, or `temp<N>` when there is no label file.
    pub label: String,
    pub celsius: f32,
}

/// A `hwmon<N>` directory name: `hwmon` followed by digits only.
fn hwmon_index(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("hwmon")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// The numeric part of a `temp<N>_input` / `temp<N>_label` file name.
fn temp_index(name: &str, suffix: &str) -> Option<u64> {
    let rest = name.strip_prefix("temp")?.strip_suffix(suffix)?;
    rest.parse().ok()
}

/// Every `temp<N>_input` of every `sys/class/hwmon/hwmon<M>`, ordered by M then N (numeric order).
/// A chip without a `name` file is skipped. Unreadable or unparsable inputs are skipped, and so is a
/// reading below −40 °C (an unconnected probe: the real board reports −55 °C on two of them).
pub fn read_temps(sysroot: &Path) -> Vec<TempReading> {
    let hwmon_dir = sysroot.join("sys/class/hwmon");
    let Some(entries) = std::fs::read_dir(&hwmon_dir).ok() else {
        return Vec::new();
    };

    let mut readings: Vec<(u64, u64, TempReading)> = Vec::new();
    for entry in entries.flatten() {
        let name: String = entry.file_name().to_string_lossy().into_owned();
        let Some(m) = hwmon_index(&name) else { continue };
        // A chip without a `name` file is skipped (e.g. a kernel virtual chip).
        let Some(chip) = read_trimmed(sysroot, &format!("sys/class/hwmon/{name}/name")) else {
            continue;
        };

        // Collect this chip's inputs and labels, keyed by N.
        let temp_dir = hwmon_dir.join(&name);
        let Some(temp_entries) = std::fs::read_dir(&temp_dir).ok() else { continue };
        let mut inputs: Vec<(u64, f32)> = Vec::new();
        let mut labels: Vec<(u64, String)> = Vec::new();
        for te in temp_entries.flatten() {
            let tn: String = te.file_name().to_string_lossy().into_owned();
            if let Some(n) = temp_index(&tn, "_input")
                && let Some(milli) = read_parsed::<i32>(sysroot, &format!("sys/class/hwmon/{name}/{tn}"))
            {
                // Millidegrees → Celsius; an unparsable value is skipped.
                inputs.push((n, milli as f32 / 1000.0));
            } else if let Some(n) = temp_index(&tn, "_label")
                && let Some(l) = read_trimmed(sysroot, &format!("sys/class/hwmon/{name}/{tn}"))
            {
                labels.push((n, l));
            }
        }

        for (n, celsius) in inputs {
            // An unconnected probe reports a very low temperature: skip it.
            if celsius < -40.0 {
                continue;
            }
            let label = labels
                .iter()
                .find(|(n2, _)| *n2 == n)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| format!("temp{n}"));
            readings.push((m, n, TempReading { chip: chip.clone(), label, celsius }));
        }
    }

    readings.sort_by_key(|(m, n, _)| (*m, *n));
    readings.into_iter().map(|(_, _, r)| r).collect()
}

/// CPU package temperature: chip `coretemp` label `Package id 0`; else chip `k10temp` label `Tctl`;
/// else the hottest `coretemp` / `k10temp` reading; else `None`.
pub fn cpu_temp(temps: &[TempReading]) -> Option<f32> {
    if let Some(t) = temps.iter().find(|t| t.chip == "coretemp" && t.label == "Package id 0") {
        return Some(t.celsius);
    }
    if let Some(t) = temps.iter().find(|t| t.chip == "k10temp" && t.label == "Tctl") {
        return Some(t.celsius);
    }
    temps
        .iter()
        .filter(|t| t.chip == "coretemp" || t.chip == "k10temp")
        .map(|t| t.celsius)
        .max_by(|a, b| a.total_cmp(b))
}

/// First reading of chip `nvme` labelled `Composite`, else the first `nvme` reading.
pub fn nvme_temp(temps: &[TempReading]) -> Option<f32> {
    temps
        .iter()
        .find(|t| t.chip == "nvme" && t.label == "Composite")
        .or_else(|| temps.iter().find(|t| t.chip == "nvme"))
        .map(|t| t.celsius)
}
