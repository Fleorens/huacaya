//! Integration tests for `collect::sensors` (temps) and `collect::gpu` (AMD GPU).
//!
//! Values come from `tests/fixtures/` (a fake i7 + Radeon RX 7800 XT machine). Floats compare within 0.01.

use std::path::PathBuf;

use llamatop::collect::gpu::read_gpus;
use llamatop::collect::sensors::{TempReading, cpu_temp, nvme_temp, read_temps};

fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

#[test]
fn temps_are_listed_in_order() {
    let temps = read_temps(&sysroot());
    assert_eq!(temps.len(), 9);

    // Chips in order: acpitz, nvme, coretemp ×3, amdgpu ×3, gigabyte_wmi.
    assert_eq!(temps[0].chip.as_str(), "acpitz");
    assert_eq!(temps[1].chip.as_str(), "nvme");
    assert_eq!(temps[2].chip.as_str(), "coretemp");
    assert_eq!(temps[3].chip.as_str(), "coretemp");
    assert_eq!(temps[4].chip.as_str(), "coretemp");
    assert_eq!(temps[5].chip.as_str(), "amdgpu");
    assert_eq!(temps[6].chip.as_str(), "amdgpu");
    assert_eq!(temps[7].chip.as_str(), "amdgpu");
    assert_eq!(temps[8].chip.as_str(), "gigabyte_wmi");

    // First: label temp1 (no label file), 27.8 °C.
    assert_eq!(temps[0].label.as_str(), "temp1");
    assert!(approx(temps[0].celsius, 27.8));
    // Second: nvme Composite, 41.85 °C.
    assert_eq!(temps[1].label.as_str(), "Composite");
    assert!(approx(temps[1].celsius, 41.85));
    // Eighth: amdgpu mem, 66.0 °C.
    assert_eq!(temps[7].label.as_str(), "mem");
    assert!(approx(temps[7].celsius, 66.0));
    // Last: gigabyte_wmi temp1 at 27.0 (its temp2 at −55 °C was skipped).
    assert_eq!(temps[8].label.as_str(), "temp1");
    assert!(approx(temps[8].celsius, 27.0));
}

#[test]
fn cpu_and_nvme_temps() {
    let temps = read_temps(&sysroot());
    assert!(approx(cpu_temp(&temps).expect("cpu temp"), 52.0));
    assert!(approx(nvme_temp(&temps).expect("nvme temp"), 41.85));
}

#[test]
fn cpu_temp_falls_back_to_hottest_core() {
    let built = vec![
        TempReading { chip: "coretemp".into(), label: "Core 0".into(), celsius: 50.0 },
        TempReading { chip: "coretemp".into(), label: "Core 1".into(), celsius: 57.0 },
    ];
    assert!(approx(cpu_temp(&built).expect("hottest core"), 57.0));

    assert_eq!(cpu_temp(&[]), None);
}

#[test]
fn one_amd_gpu_found() {
    let gpus = read_gpus(&sysroot());
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].card.as_str(), "card1");
    assert_eq!(gpus[0].name.as_str(), "Radeon RX 7800 XT");
}

#[test]
fn gpu_values() {
    let gpu = read_gpus(&sysroot()).pop().expect("one gpu");
    assert!(approx(gpu.busy_pct.expect("busy"), 97.0));
    assert_eq!(gpu.vram_used.expect("vram used"), 15894315008);
    assert_eq!(gpu.vram_total.expect("vram total"), 17163091968);
    assert_eq!(gpu.gtt_used.expect("gtt used"), 52428800);
    assert!(approx(gpu.temp_edge_c.expect("edge"), 61.0));
    assert!(approx(gpu.temp_junction_c.expect("junction"), 70.0));
    assert!(approx(gpu.temp_mem_c.expect("mem"), 66.0));
    assert!(approx(gpu.power_w.expect("power"), 106.0));
    assert!(approx(gpu.power_cap_w.expect("cap"), 212.0));
    assert_eq!(gpu.fan_rpm.expect("rpm"), 1180);
    assert!(approx(gpu.fan_pct.expect("fan pct"), 37.65));
    assert!(approx(gpu.sclk_mhz.expect("sclk"), 2254.0));
    assert_eq!(gpu.vddgfx_mv.expect("vddgfx"), 912);
    assert_eq!(gpu.link.expect("link"), "16.0 GT/s PCIe x16");
}

#[test]
fn no_gpu_on_empty_root() {
    let bad = PathBuf::from("/nonexistent-llamatop");
    assert!(read_gpus(&bad).is_empty());
    assert!(read_temps(&bad).is_empty());
}

#[test]
fn partial_card() {
    // A root unique to this test, removed at the end.
    let root = std::env::temp_dir().join(format!("llamatop-partial-{}", std::process::id()));
    let device = root.join("sys/class/drm/card0/device");
    std::fs::create_dir_all(&device).expect("create device dir");
    std::fs::write(device.join("vendor"), "0x1002\n").expect("write vendor");
    std::fs::write(device.join("device"), "0x1234\n").expect("write device id");

    let gpus = read_gpus(&root);
    std::fs::remove_dir_all(&root).ok();

    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].card.as_str(), "card0");
    assert_eq!(gpus[0].name.as_str(), "AMD GPU 1002:1234");
    assert_eq!(gpus[0].busy_pct, None);
    assert_eq!(gpus[0].vram_used, None);
    assert_eq!(gpus[0].vram_total, None);
    assert_eq!(gpus[0].gtt_used, None);
    assert_eq!(gpus[0].temp_edge_c, None);
    assert_eq!(gpus[0].temp_junction_c, None);
    assert_eq!(gpus[0].temp_mem_c, None);
    assert_eq!(gpus[0].power_w, None);
    assert_eq!(gpus[0].power_cap_w, None);
    assert_eq!(gpus[0].fan_rpm, None);
    assert_eq!(gpus[0].fan_pct, None);
    assert_eq!(gpus[0].sclk_mhz, None);
    assert_eq!(gpus[0].vddgfx_mv, None);
    assert_eq!(gpus[0].link, None);
}
