//! Integration tests for `collect::nvidia` (the `nvidia-smi` collector), its place in the sampler, and
//! the GPU panel on an NVIDIA card. Values come from `tests/fixtures/nvidia/smi.csv`; floats compare
//! within 0.01.

use std::path::PathBuf;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::App;
use llamatop::collect::nvidia::parse_smi;
use llamatop::collect::sampler::{Sampler, SystemSnapshot};
use llamatop::collect::system::CpuSample;
use llamatop::ui::panels::{Ctx, gpu};
use llamatop::ui::theme::{ColorMode, Theme};

const MIB: u64 = 1_048_576;

/// The `smi.csv` fixture, read at runtime (never baked in).
fn smi_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nvidia/smi.csv");
    std::fs::read_to_string(path).expect("read tests/fixtures/nvidia/smi.csv")
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

/// Render `f` into a `w × h` buffer and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

/// The fixture machine's sysroot (`tests/fixtures/sysroot`).
fn sysroot() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sysroot")
}

#[test]
fn parse_two_gpus() {
    let gpus = parse_smi(&smi_text());
    assert_eq!(gpus.len(), 2);

    // First: a fully reported RTX 4090.
    let a = &gpus[0];
    assert_eq!(a.card.as_str(), "nvidia0");
    assert_eq!(a.name.as_str(), "NVIDIA GeForce RTX 4090");
    assert!(approx(a.busy_pct.expect("busy"), 97.0));
    assert_eq!(a.vram_used.expect("vram used"), 20480 * MIB);
    assert_eq!(a.vram_total.expect("vram total"), 24564 * MIB);
    assert!(approx(a.temp_edge_c.expect("edge"), 71.0));
    assert_eq!(a.temp_junction_c, None);
    assert!(approx(a.power_w.expect("power"), 352.47));
    assert!(approx(a.power_cap_w.expect("cap"), 450.0));
    assert!(approx(a.fan_pct.expect("fan"), 63.0));
    assert!(approx(a.sclk_mhz.expect("sclk"), 2520.0));
    assert_eq!(a.link.as_deref(), Some("PCIe gen 4 x16"));

    // Second: a card with a few `[N/A]` / `[Not Supported]` fields.
    let b = &gpus[1];
    assert_eq!(b.card.as_str(), "nvidia1");
    assert_eq!(b.name.as_str(), "NVIDIA RTX A2000 12GB");
    assert_eq!(b.busy_pct, None);
    assert_eq!(b.power_w, None);
    assert_eq!(b.power_cap_w, None);
    assert_eq!(b.link.as_deref(), Some("PCIe gen 3 x8"));
    // Still read where they are defined.
    assert_eq!(b.vram_used, Some(512 * MIB));
    assert!(approx(b.temp_edge_c.expect("edge"), 38.0));
}

#[test]
fn parse_garbage() {
    // None of these panics.
    for text in ["", "\n\n", "a, b", "x,y,z,1,2,3,4,5,6,7,8,9,10"] {
        let _ = parse_smi(text);
    }

    // Fewer than 12 fields each → no sample.
    assert!(parse_smi("").is_empty());
    assert!(parse_smi("\n\n").is_empty());
    assert!(parse_smi("a, b").is_empty());

    // Twelve-or-more fields: one sample, whose non-numeric numeric fields are `None`.
    let gpus = parse_smi("x,y,z,1,2,3,4,5,6,7,8,9,10");
    assert_eq!(gpus.len(), 1);
    assert_eq!(gpus[0].card.as_str(), "nvidia");
    assert_eq!(gpus[0].name.as_str(), "y");
    assert_eq!(gpus[0].busy_pct, None);
    assert_eq!(gpus[0].vram_used, Some(MIB));
    assert_eq!(gpus[0].vram_total, Some(2 * MIB));
    assert!(approx(gpus[0].temp_edge_c.expect("edge"), 3.0));
    assert_eq!(gpus[0].link.as_deref(), Some("PCIe gen 8 x9"));
}

#[test]
fn fixture_sysroot_never_calls_nvidia_smi() {
    // A `Sampler` on the test sysroot (not `/`) never spawns `nvidia-smi`: exactly the one AMD card.
    let mut sampler = Sampler::new(sysroot(), vec![PathBuf::from("/")]);
    let snap = sampler.sample(Instant::now());

    assert_eq!(snap.gpus.len(), 1);
    assert_eq!(snap.gpus[0].card.as_str(), "card1");
}

#[test]
fn gpu_panel_nvidia() {
    let theme = true_color();
    let ctx = Ctx { theme: &theme, graphs: true, clock: "10:21:03" };

    // An `App` whose system snapshot carries the first fixture GPU (the RTX 4090).
    let gpus = parse_smi(&smi_text());
    let mut app = App::new(1000);
    app.system = Some(SystemSnapshot {
        hostname: None,
        uptime_s: None,
        cpu: CpuSample { usage_pct: None, per_core_pct: Vec::new(), freq: None, load: None },
        cpu_temp_c: None,
        nvme_temp_c: None,
        mem: None,
        gpus,
        temps: Vec::new(),
        disks: Vec::new(),
        nets: Vec::new(),
        procs: Vec::new(),
    });

    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 60, 9);
    let all: String = rows.join("\n");
    assert!(all.contains("GPU  RTX 4090"), "{:?}", rows[0]);
    assert!(all.contains("temp  71 °C"), "{:?}", rows);
    assert!(!all.contains("junction"), "{:?}", rows);
}
