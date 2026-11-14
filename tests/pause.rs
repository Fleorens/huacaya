//! Tests for the pure state machine (`src/app`), per `tasks/T09-app-state.md`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use huacaya::app::{App, Event, Screen};
use huacaya::collect::gpu::GpuSample;
use huacaya::collect::logs::parse_line;
use huacaya::collect::sampler::SystemSnapshot;
use huacaya::collect::system::{CpuInfo, CpuSample};
// --- Key + snapshot builders (the sheet forbids these in `src/`) -------------------------------
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}
fn gpu(busy: Option<f32>) -> GpuSample {
    GpuSample {
        card: "0".into(),
        name: "RX 7800 XT".into(),
        busy_pct: busy,
        vram_used: None,
        vram_total: None,
        gtt_used: None,
        temp_edge_c: None,
        temp_junction_c: None,
        temp_mem_c: None,
        power_w: None,
        power_cap_w: None,
        fan_rpm: None,
        fan_pct: None,
        sclk_mhz: None,
        vddgfx_mv: None,
        link: None,
    }
}
fn cpu_sample(usage_pct: Option<f32>) -> CpuSample {
    CpuSample { usage_pct, per_core_pct: vec![], freq: None, load: None }
}
fn system(busy: Option<f32>, cpu: Option<f32>) -> SystemSnapshot {
    SystemSnapshot {
        hostname: None,
        uptime_s: None,
        cpu: cpu_sample(cpu),
        cpu_temp_c: None,
        nvme_temp_c: None,
        mem: None,
        gpus: vec![gpu(busy)],
        temps: vec![],
        disks: vec![],
        nets: vec![],
        procs: vec![],
        cpu_info: CpuInfo::default(),
    }
}
const LINE: &str = "2026-11-11T10:38:21.429833+01:00 aibox-flo llama-server[1194]: [35335] 70.09.526.815 I slot      release: id  0 | task 9397 | stop processing: n_tokens = 10314, truncated = 0";
fn busy(app: &App) -> Option<f32> {
    app.system.as_ref().and_then(|s| s.gpus[0].busy_pct)
}
#[test]
fn pause_freezes_the_shown_data() {
    let mut app = App::new(1000);
    app.apply(Event::System(system(Some(10.0), Some(1.0))));
    app.apply(Event::Key(key('p')));
    app.apply(Event::System(system(Some(90.0), Some(2.0))));
    app.apply(Event::Log(parse_line(LINE)));
    let shown = app.shown();
    assert_eq!(busy(&shown), Some(10.0));
    assert_eq!(shown.logs.len(), 0);
    assert_eq!(shown.cpu_usage.len(), 1);
    assert_eq!(busy(&app), Some(90.0));
    assert_eq!(app.logs.len(), 1);
}
#[test]
fn pause_keeps_navigation_live() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('p')));
    app.apply(Event::Key(key('2')));
    assert_ne!(app.screen, Screen::Overview);
    assert_eq!(app.shown().screen, app.screen);
}
#[test]
fn resume_shows_live_data() {
    let mut app = App::new(1000);
    app.apply(Event::System(system(Some(10.0), None)));
    app.apply(Event::Key(key('p')));
    app.apply(Event::System(system(Some(90.0), None)));
    app.apply(Event::Key(key('p')));
    let shown = app.shown();
    assert!(matches!(shown, std::borrow::Cow::Borrowed(_)));
    assert_eq!(busy(&shown), Some(90.0));
}
#[test]
fn pause_keeps_the_frozen_log_position() {
    let mut app = App::new(1000);
    for _ in 0..30 {
        app.apply(Event::Log(parse_line(LINE)));
    }
    // Scroll up (follow off), then pause: new lines move the live position, not the frozen one.
    app.apply(Event::Key(key('3')));
    app.apply(Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)));
    let before = app.log_view.scroll;
    assert!(!app.log_view.follow);
    app.apply(Event::Key(key('p')));
    for _ in 0..5 {
        app.apply(Event::Log(parse_line(LINE)));
    }
    assert_eq!(app.log_view.scroll, before + 5);
    assert_eq!(app.shown().log_view.scroll, before);
}
