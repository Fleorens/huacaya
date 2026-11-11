# T03 — Temperatures and the AMD GPU

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T02 |
| **Read first** | `docs/01-architecture.md` § 2 (table only) |
| **Deliverable** | `src/collect/sensors.rs`, `src/collect/gpu.rs`, `tests/sensors_gpu.rs`; two `pub mod` lines in `src/collect/mod.rs` |

## Why

The GPU panel is the heart of the dashboard: load, VRAM, temperatures, power, fan of the Radeon RX 7800 XT,
read from the `amdgpu` driver's sysfs files. CPU and NVMe temperatures come from the generic `hwmon` tree.

## The sysfs layout (fixture: `tests/fixtures/sysroot/sys/class/`)

```
hwmon/hwmon0/  name=acpitz    temp1_input                      (no label file)
hwmon/hwmon1/  name=nvme      temp1_input temp1_label=Composite
hwmon/hwmon2/  name=coretemp  temp1..3_input, labels "Package id 0", "Core 0", "Core 1"
hwmon/hwmon3   -> symlink to ../drm/card1/device/hwmon/hwmon3   (name=amdgpu)
hwmon/hwmon4/  name=gigabyte_wmi  temp1_input=27000  temp2_input=-55000 (unconnected probe)
drm/card0/device/vendor = 0x8086          Intel iGPU: not ours
drm/card1/device/vendor = 0x1002          the Radeon
drm/card1-DP-1/  drm/card1-HDMI-A-1/      connectors: not cards
drm/renderD128/device/vendor = 0x1002     render node: not a card
```

Files of `drm/card1/device/`: `device` (`0x747e`), `gpu_busy_percent` (`97`), `mem_info_vram_total`,
`mem_info_vram_used`, `mem_info_gtt_used` (bytes), `current_link_speed` (`16.0 GT/s PCIe`),
`current_link_width` (`16`). Files of its `hwmon/hwmon3/`: `temp<N>_input` (millidegrees) with
`temp<N>_label` ∈ `edge`, `junction`, `mem`; `power1_average` and `power1_cap` (microwatts); `fan1_input`
(rpm); `pwm1` and `pwm1_max`; `freq<N>_input` (Hz; the one labelled `sclk` — the real card also has `mclk`); `in0_input` (millivolts, label `vddgfx`).

## Contract

Every struct derives `Debug, Clone, PartialEq, serde::Serialize`. Reuse `read_trimmed` / `read_parsed`
from `src/collect/mod.rs` (T02).

### `src/collect/sensors.rs`

```rust
pub struct TempReading {
    /// Content of the chip's `name` file (`coretemp`, `nvme`, `amdgpu`, …).
    pub chip: String,
    /// Content of `temp<N>_label`, or `temp<N>` when there is no label file.
    pub label: String,
    pub celsius: f32,
}

/// Every `temp<N>_input` of every `sys/class/hwmon/hwmon<M>`, ordered by M then N (numeric order).
/// A chip without a `name` file is skipped. Unreadable or unparsable inputs are skipped, and so is a
/// reading below −40 °C (an unconnected probe: the real board reports −55 °C on two of them).
pub fn read_temps(sysroot: &Path) -> Vec<TempReading>;

/// CPU package temperature: chip `coretemp` label `Package id 0`; else chip `k10temp` label `Tctl`;
/// else the hottest `coretemp` / `k10temp` reading; else `None`.
pub fn cpu_temp(temps: &[TempReading]) -> Option<f32>;

/// First reading of chip `nvme` labelled `Composite`, else the first `nvme` reading.
pub fn nvme_temp(temps: &[TempReading]) -> Option<f32>;
```

### `src/collect/gpu.rs`

```rust
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

/// One entry per `sys/class/drm/card<digits>` whose `device/vendor` is `0x1002`, ordered by card number.
/// The sensors come from the first `device/hwmon/hwmon*` directory of the card.
/// A missing file gives `None` for that field only. No AMD card: empty vector.
pub fn read_gpus(sysroot: &Path) -> Vec<GpuSample>;
```

## Pitfalls

- `card1-DP-1` starts with `card1`: accept a directory only when what follows `card` is digits only.
- `hwmon10` sorts before `hwmon2` as text: sort on the parsed number.
- `hwmon3` under `class/hwmon` is a symlink: use `std::fs` calls that follow links (`read_dir` +
  `Path::join` + `read_to_string` do; `DirEntry::file_type()` does **not** — do not filter on it).
- Temperatures are matched by **label** (`edge`, `junction`, `mem`), not by index.
- The vendor file reads `0x1002\n`: trim, compare case-insensitively.

## Tests — `tests/sensors_gpu.rs`

Same `sysroot()` helper as `tests/system.rs`. Float tolerance 0.01.

- `temps_are_listed_in_order`: 9 readings; chips in order: `acpitz`, `nvme`, `coretemp` ×3, `amdgpu` ×3,
  `gigabyte_wmi`. First: label `temp1`, 27.8. Second: label `Composite`, 41.85. Eighth: label `mem`, 66.0.
  Last: chip `gigabyte_wmi`, label `temp1`, 27.0 (its `temp2` at −55 °C is skipped).
- `cpu_and_nvme_temps`: `cpu_temp` = 52.0, `nvme_temp` = 41.85.
- `cpu_temp_falls_back_to_hottest_core`: hand-built slice with `coretemp` readings `Core 0` 50 and
  `Core 1` 57 only → 57.0; empty slice → `None`.
- `one_amd_gpu_found`: `read_gpus` → length 1, `card` = `card1`, `name` = `Radeon RX 7800 XT`.
- `gpu_values`: busy 97.0; vram_used 15894315008; vram_total 17163091968; gtt_used 52428800; edge 61.0;
  junction 70.0; mem 66.0; power 106.0; cap 212.0; fan_rpm 1180; fan_pct 37.65; sclk 2254.0;
  vddgfx 912; link `16.0 GT/s PCIe x16`.
- `no_gpu_on_empty_root`: `/nonexistent-llamatop` → `read_gpus` empty, `read_temps` empty, no panic.
- `partial_card`: in a `std::env::temp_dir()` subdirectory unique to the test (include the process id in
  its name, remove it at the end), create `sys/class/drm/card0/device/vendor` = `0x1002` and
  `device/device` = `0x1234` only → one GPU, name `AMD GPU 1002:1234`, every `Option` field `None`.

## Acceptance criteria

- [x] Exact public API above.
- [x] The 7 tests exist under these names and pass.
- [x] No fixture modified.
- [x] `bash .devbox/check` is green.
