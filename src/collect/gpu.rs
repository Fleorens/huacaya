//! The AMD GPU (Radeon RX 7800 XT), read from `sys/class/drm/card*/device` and its `amdgpu` hwmon.
//!
//! Parsing is pure and unit-tested; the `read_*` layer only does I/O. A missing file gives `None` for
//! that field only, so the dashboard shows "n/a" rather than crashing on a partial card.

use std::path::Path;

use serde::Serialize;

use crate::collect::{read_parsed, read_trimmed};

/// A snapshot of one AMD GPU.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuSample {
    /// Directory name under `sys/class/drm`, e.g. `card1`.
    pub card: String,
    /// From the PCI device id: `0x747e` → `Radeon RX 7800 XT`; unknown id → `AMD GPU 1002:<id without 0x>`.
    pub name: String,
    pub busy_pct: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub gtt_used: Option<u64>,
    pub temp_edge_c: Option<f32>,
    pub temp_junction_c: Option<f32>,
    pub temp_mem_c: Option<f32>,
    /// `power1_average`, else `power1_input`, in watts.
    pub power_w: Option<f32>,
    pub power_cap_w: Option<f32>,
    pub fan_rpm: Option<u32>,
    /// `pwm1 / pwm1_max × 100`; `pwm1_max` missing counts as 255.
    pub fan_pct: Option<f32>,
    pub sclk_mhz: Option<f32>,
    pub vddgfx_mv: Option<u32>,
    /// `"<current_link_speed> x<current_link_width>"`, e.g. `16.0 GT/s PCIe x16`.
    pub link: Option<String>,
}

/// A `card<N>` directory name: `card` followed by digits only (`card1-DP-1` is not a card).
fn card_index(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("card")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// A `hwmon<N>` directory name: `hwmon` followed by digits only.
fn hwmon_index(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("hwmon")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// The numeric part of a `freq<N>_input` / `freq<N>_label` file name.
fn freq_index(name: &str, suffix: &str) -> Option<u64> {
    let rest = name.strip_prefix("freq")?.strip_suffix(suffix)?;
    rest.parse().ok()
}

/// The PCI device id → a human name. Known ids are matched case-insensitively; everything else is
/// rendered generically from the vendor (always `0x1002` here) and the device id without its `0x`.
fn gpu_name(device_id: &str) -> String {
    match device_id.trim().to_ascii_lowercase().as_str() {
        "0x747e" => "Radeon RX 7800 XT".to_string(),
        other => {
            let id = other.strip_prefix("0x").unwrap_or(other);
            format!("AMD GPU 1002:{id}")
        }
    }
}

/// The first `hwmon<N>` directory under `dev`, ordered by N (numeric). `None` when there is none.
fn first_hwmon(sysroot: &Path, hwmon_rel: &str) -> Option<String> {
    let hwmon_dir = sysroot.join(hwmon_rel);
    let entries = std::fs::read_dir(&hwmon_dir).ok()?;
    let mut found: Vec<(u64, String)> = Vec::new();
    for entry in entries.flatten() {
        let name: String = entry.file_name().to_string_lossy().into_owned();
        if let Some(n) = hwmon_index(&name) {
            found.push((n, name));
        }
    }
    found.sort_by_key(|(n, _)| *n);
    found.into_iter().next().map(|(_, name)| format!("{hwmon_rel}/{name}"))
}

/// A `temp<N>` reading matched by its `temp<N>_label`, in Celsius.
fn hwmon_temp_by_label(sysroot: &Path, hwmon: &str, label: &str) -> Option<f32> {
    let dir = sysroot.join(hwmon);
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let name: String = entry.file_name().to_string_lossy().into_owned();
        let Some(n) = temp_index(&name, "_label") else {
            continue;
        };
        if read_trimmed(sysroot, &format!("{hwmon}/{name}"))
            .map(|l| l.eq_ignore_ascii_case(label))
            .unwrap_or(false)
        {
            return read_parsed::<i32>(sysroot, &format!("{hwmon}/temp{n}_input")).map(|v| v as f32 / 1000.0);
        }
    }
    None
}

/// A `temp<N>` file name's numeric part.
fn temp_index(name: &str, suffix: &str) -> Option<u64> {
    let rest = name.strip_prefix("temp")?.strip_suffix(suffix)?;
    rest.parse().ok()
}

/// The `freq<N>` reading whose `freq<N>_label` matches, in Hz.
fn hwmon_freq_by_label(sysroot: &Path, hwmon: &str, label: &str) -> Option<f64> {
    let dir = sysroot.join(hwmon);
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut labels: Vec<(u64, String)> = Vec::new();
    let mut freqs: Vec<(u64, f64)> = Vec::new();
    for entry in entries.flatten() {
        let name: String = entry.file_name().to_string_lossy().into_owned();
        if let Some(n) = freq_index(&name, "_label")
            && let Some(l) = read_trimmed(sysroot, &format!("{hwmon}/{name}"))
        {
            labels.push((n, l));
        } else if let Some(n) = freq_index(&name, "_input")
            && let Some(hz) = read_parsed::<f64>(sysroot, &format!("{hwmon}/{name}"))
        {
            freqs.push((n, hz));
        }
    }
    freqs
        .into_iter()
        .find(|(n, _)| labels.iter().any(|(ln, l)| *n == *ln && l.eq_ignore_ascii_case(label)))
        .map(|(_, hz)| hz)
}

/// `power1_average`, else `power1_input`, in watts (the files are in microwatts).
fn power_w(sysroot: &Path, hwmon: &str) -> Option<f32> {
    read_parsed::<i64>(sysroot, &format!("{hwmon}/power1_average"))
        .or_else(|| read_parsed::<i64>(sysroot, &format!("{hwmon}/power1_input")))
        .map(|w| w as f32 / 1_000_000.0)
}

/// One entry per `sys/class/drm/card<digits>` whose `device/vendor` is `0x1002`, ordered by card number.
/// The sensors come from the first `device/hwmon/hwmon*` directory of the card.
/// A missing file gives `None` for that field only. No AMD card: empty vector.
pub fn read_gpus(sysroot: &Path) -> Vec<GpuSample> {
    let drm = "sys/class/drm";
    let drm_dir = sysroot.join(drm);
    let Some(entries) = std::fs::read_dir(&drm_dir).ok() else {
        return Vec::new();
    };

    let mut gpus: Vec<(u64, GpuSample)> = Vec::new();
    for entry in entries.flatten() {
        let name: String = entry.file_name().to_string_lossy().into_owned();
        let Some(card_num) = card_index(&name) else { continue };
        // Accept only the AMD vendor id (0x1002), reading the file with a trailing newline etc.
        if !read_trimmed(sysroot, &format!("{drm}/{name}/device/vendor"))
            .map(|v| v.eq_ignore_ascii_case("0x1002"))
            .unwrap_or(false)
        {
            continue;
        }
        let device_id = read_trimmed(sysroot, &format!("{drm}/{name}/device/device")).unwrap_or_default();
        gpus.push((card_num, gpu_from_sysroot(sysroot, &name, &device_id)));
    }

    gpus.sort_by_key(|(n, _)| *n);
    gpus.into_iter().map(|(_, s)| s).collect()
}

/// Builds one `GpuSample` from a card's `device/` and its first `hwmon`.
fn gpu_from_sysroot(sysroot: &Path, card: &str, device_id: &str) -> GpuSample {
    let dev = format!("sys/class/drm/{card}/device");
    let busy_pct = read_parsed::<f32>(sysroot, &format!("{dev}/gpu_busy_percent"));
    let vram_used = read_parsed::<u64>(sysroot, &format!("{dev}/mem_info_vram_used"));
    let vram_total = read_parsed::<u64>(sysroot, &format!("{dev}/mem_info_vram_total"));
    let gtt_used = read_parsed::<u64>(sysroot, &format!("{dev}/mem_info_gtt_used"));
    let link = match (
        read_trimmed(sysroot, &format!("{dev}/current_link_speed")),
        read_trimmed(sysroot, &format!("{dev}/current_link_width")),
    ) {
        (Some(speed), Some(width)) => Some(format!("{speed} x{width}")),
        _ => None,
    };

    let hwmon = first_hwmon(sysroot, &format!("{dev}/hwmon"));
    let (
        temp_edge_c,
        temp_junction_c,
        temp_mem_c,
        power_w,
        power_cap_w,
        fan_rpm,
        fan_pct,
        sclk_mhz,
        vddgfx_mv,
    ) = match hwmon {
        Some(hwmon) => (
            hwmon_temp_by_label(sysroot, &hwmon, "edge"),
            hwmon_temp_by_label(sysroot, &hwmon, "junction"),
            hwmon_temp_by_label(sysroot, &hwmon, "mem"),
            power_w(sysroot, &hwmon),
            read_parsed::<f32>(sysroot, &format!("{hwmon}/power1_cap")).map(|w| w / 1_000_000.0),
            read_parsed::<u32>(sysroot, &format!("{hwmon}/fan1_input")),
            read_parsed::<u32>(sysroot, &format!("{hwmon}/pwm1")).and_then(|pwm| {
                let max = read_parsed::<u32>(sysroot, &format!("{hwmon}/pwm1_max")).unwrap_or(255);
                if max == 0 { None } else { Some(pwm as f32 / max as f32 * 100.0) }
            }),
            hwmon_freq_by_label(sysroot, &hwmon, "sclk").map(|hz| hz as f32 / 1_000_000.0),
            read_parsed::<u32>(sysroot, &format!("{hwmon}/in0_input")),
        ),
        None => (None, None, None, None, None, None, None, None, None),
    };

    GpuSample {
        card: card.to_string(),
        name: gpu_name(device_id),
        busy_pct,
        vram_used,
        vram_total,
        gtt_used,
        temp_edge_c,
        temp_junction_c,
        temp_mem_c,
        power_w,
        power_cap_w,
        fan_rpm,
        fan_pct,
        sclk_mhz,
        vddgfx_mv,
        link,
    }
}
