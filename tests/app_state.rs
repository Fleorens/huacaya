//! Tests for the pure state machine (`src/app`), per `tasks/T09-app-state.md`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use huacaya::app::{App, Event, Screen};
use huacaya::collect::gpu::GpuSample;
use huacaya::collect::llama::{ModelInfo, ModelState, SlotInfo};
use huacaya::collect::llama_poll::{ActiveModel, Health, LlamaSnapshot};
use huacaya::collect::logs::{Level, LogEvent, LogLine, parse_line};
use huacaya::collect::sampler::SystemSnapshot;
use huacaya::collect::system::{CpuInfo, CpuSample};
// --- Key + snapshot builders (the sheet forbids these in `src/`) -------------------------------
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}
fn code(k: KeyCode) -> KeyEvent {
    KeyEvent::new(k, KeyModifiers::NONE)
}
fn ctrl_c() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
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
fn model(id: &str, state: ModelState) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        state,
        ctx_train: None,
        ctx_size: None,
        n_cpu_moe: None,
        n_gpu_layers: None,
        threads: None,
        spec_type: None,
        model_path: None,
        vision: false,
        size_bytes: None,
        n_params: None,
        ftype: None,
    }
}
fn slot(is_processing: bool) -> SlotInfo {
    SlotInfo {
        id: 0,
        id_task: 1,
        n_ctx: 0,
        is_processing,
        n_prompt_tokens: 0,
        n_decoded: 0,
        n_remain: 0,
        speculative: false,
    }
}
fn llama_models(n: usize) -> LlamaSnapshot {
    LlamaSnapshot {
        health: Health::Ok,
        router: true,
        build_info: None,
        models: (0..n).map(|i| model(&format!("model{i}"), ModelState::Loaded)).collect(),
        active: vec![],
    }
}
fn llama_active(live_gen_tps: Option<f64>, is_processing: bool) -> LlamaSnapshot {
    let mut snap = llama_models(1);
    snap.active = vec![ActiveModel {
        id: "model0".into(),
        metrics: None,
        slots: vec![slot(is_processing)],
        live_gen_tps,
    }];
    snap
}

/// Every non-empty fixture line, parsed.
fn fixture_lines() -> Vec<LogLine> {
    include_str!("fixtures/journal/llama-server.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(parse_line)
        .collect()
}
fn is_proxy(l: &LogLine) -> bool {
    matches!(l.event, Some(LogEvent::Proxy { .. }))
}
const VISIBLE_LINE: &str = "2026-11-11T10:38:21.429833+01:00 aibox-flo llama-server[1194]: [35335] 70.09.526.815 I slot      release: id  0 | task 9397 | stop processing: n_tokens = 10314, truncated = 0";
// --- Tests ---------------------------------------------------------------------------------------
#[test]
fn new_state() {
    let app = App::new(1000);
    assert_eq!(app.screen, Screen::Overview);
    assert!(app.log_view.follow);
    assert!(!app.log_view.show_proxy);
    assert_eq!(app.frame, 0);
    assert!(!app.quit);
    assert_eq!(app.gpu_load.capacity(), 300);
}
#[test]
fn screen_keys() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('2')));
    assert_eq!(app.screen, Screen::Models);
    app.apply(Event::Key(code(KeyCode::Tab)));
    assert_eq!(app.screen, Screen::Logs);
    app.apply(Event::Key(code(KeyCode::BackTab)));
    app.apply(Event::Key(code(KeyCode::BackTab)));
    assert_eq!(app.screen, Screen::Overview);
    // BackTab wraps onto the last screen, Agents.
    app.apply(Event::Key(code(KeyCode::BackTab)));
    assert_eq!(app.screen, Screen::Agents);
    app.apply(Event::Key(key('5')));
    app.apply(Event::Key(code(KeyCode::Tab)));
    assert_eq!(app.screen, Screen::Overview);
}
#[test]
fn quit_keys() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('q')));
    assert!(app.quit);

    let mut app = App::new(1000);
    app.apply(Event::Key(ctrl_c()));
    assert!(app.quit);

    let mut app = App::new(1000);
    app.apply(Event::Key(key('3')));
    app.apply(Event::Key(key('/')));
    app.apply(Event::Key(key('q')));
    assert_eq!(app.log_view.filter, "q");
    assert!(!app.quit);
    app.apply(Event::Key(ctrl_c()));
    assert!(app.quit);
}
#[test]
fn help_overlay() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('?')));
    assert!(app.help);
    app.apply(Event::Key(key('1')));
    assert!(app.help);
    assert_eq!(app.screen, Screen::Overview);
    app.apply(Event::Key(code(KeyCode::Esc)));
    assert!(!app.help);
    app.apply(Event::Key(key('?')));
    assert!(app.help);
    app.apply(Event::Key(key('q')));
    assert!(app.quit);
}
#[test]
fn interval_steps() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('+')));
    assert_eq!(app.interval_ms, 500);
    app.apply(Event::Key(key('+')));
    assert_eq!(app.interval_ms, 200);
    app.apply(Event::Key(key('+')));
    assert_eq!(app.interval_ms, 200);
    app.apply(Event::Key(key('-')));
    assert_eq!(app.interval_ms, 500);

    let mut app = App::new(750);
    app.apply(Event::Key(key('+')));
    assert_eq!(app.interval_ms, 500);
    let mut app = App::new(750);
    app.apply(Event::Key(key('-')));
    assert_eq!(app.interval_ms, 1000);

    let mut app = App::new(10000);
    app.apply(Event::Key(key('-')));
    assert_eq!(app.interval_ms, 10000);
}
#[test]
fn pause_stops_data_redraws() {
    let mut app = App::new(1000);
    app.apply(Event::Key(key('p')));
    assert!(app.paused);
    let before = app.cpu_usage.len();
    assert!(!app.apply(Event::System(system(Some(10.0), Some(20.0)))));
    assert!(app.system.is_some());
    assert_eq!(app.cpu_usage.len(), before + 1);
    app.apply(Event::Key(key('p')));
    assert!(!app.paused);
    assert!(app.apply(Event::System(system(Some(10.0), Some(20.0)))));
}
#[test]
fn histories() {
    let mut app = App::new(1000);
    app.apply(Event::System(system(Some(97.0), Some(38.0))));
    assert_eq!(app.gpu_load.last(), Some(97.0));
    assert_eq!(app.cpu_usage.last(), Some(38.0));

    let before = (app.gpu_load.len(), app.cpu_usage.len());
    app.apply(Event::System(system(None, None)));
    assert_eq!((app.gpu_load.len(), app.cpu_usage.len()), before);

    app.apply(Event::Llama(llama_active(Some(62.5), false)));
    assert_eq!(app.gen_tps.last(), Some(62.5));
    app.apply(Event::Llama(llama_active(None, false)));
    assert_eq!(app.gen_tps.last(), Some(0.0));

    let before_gen = app.gen_tps.len();
    app.apply(Event::Llama(llama_models(1)));
    assert_eq!(app.gen_tps.len(), before_gen);
}
#[test]
fn tick_redraws_only_when_animating() {
    let mut app = App::new(1000);
    assert!(!app.apply(Event::Tick));
    assert_eq!(app.frame, 1);

    let mut app = App::new(1000);
    app.apply(Event::Llama(llama_active(None, true)));
    assert!(app.apply(Event::Tick));

    let mut app = App::new(1000);
    app.apply(Event::Llama(llama_active(None, true)));
    app.apply(Event::Key(key('p')));
    assert!(!app.apply(Event::Tick));
}
#[test]
fn last_request_from_fixture() {
    let mut app = App::new(1000);
    for line in fixture_lines() {
        app.apply(Event::Log(line));
    }
    let req = app.last_request.expect("a last request");
    assert_eq!(req.task, Some(9397));
    assert_eq!(req.time.as_deref(), Some("10:38:21"));
    assert_eq!(req.prompt_tokens, Some(10229));
    assert_eq!(req.prompt_tps, Some(891.0));
    assert_eq!(req.gen_tokens, Some(86));
    assert_eq!(req.gen_tps, Some(75.83));
    assert_eq!(req.ctx_tokens, Some(10314));
    assert!((req.draft_pct.unwrap() - 61.0 / 72.0 * 100.0).abs() < 1e-6);
}
#[test]
fn log_capacity() {
    let mut app = App::new(1000);
    for n in 0..2005 {
        app.apply(Event::Log(parse_line(&format!("line {n}"))));
    }
    assert_eq!(app.logs.len(), 2000);
    assert_eq!(app.logs.front().map(|l| l.text.as_str()), Some("line 5"));
}
#[test]
fn proxy_lines_hidden() {
    let mut app = App::new(1000);
    for line in fixture_lines() {
        app.apply(Event::Log(line));
    }
    assert!(!app.visible_logs().into_iter().any(is_proxy));

    app.apply(Event::Key(key('3')));
    app.apply(Event::Key(key('x')));
    assert!(app.visible_logs().into_iter().any(is_proxy));

    app.apply(Event::Key(key('x')));
    assert!(!app.visible_logs().into_iter().any(is_proxy));
}
#[test]
fn level_and_text_filter() {
    let mut app = App::new(1000);
    for line in fixture_lines() {
        app.apply(Event::Log(line));
    }
    app.apply(Event::Key(key('3')));

    app.apply(Event::Key(key('e')));
    let vis = app.visible_logs();
    assert!(!vis.is_empty());
    assert!(vis.iter().all(|l| matches!(l.level, Level::Warn | Level::Error)));

    app.apply(Event::Key(key('e')));
    let vis = app.visible_logs();
    assert!(!vis.is_empty());
    assert!(vis.iter().all(|l| l.level == Level::Error));

    app.apply(Event::Key(key('e')));
    assert!(app.visible_logs().iter().any(|l| l.level == Level::Info));

    app.apply(Event::Key(key('/')));
    for c in "TASK 9397".chars() {
        app.apply(Event::Key(key(c)));
    }
    app.apply(Event::Key(code(KeyCode::Enter)));
    assert!(!app.log_view.editing_filter);
    let needle = "TASK 9397".to_ascii_lowercase();
    let vis = app.visible_logs();
    assert!(!vis.is_empty());
    assert!(vis.iter().all(|l| l.text.to_ascii_lowercase().contains(&needle)));

    app.apply(Event::Key(code(KeyCode::Esc)));
    assert!(app.log_view.filter.is_empty());
}
#[test]
fn scrolling() {
    let mut app = App::new(1000);
    for line in fixture_lines() {
        app.apply(Event::Log(line));
    }
    app.apply(Event::Key(key('3')));

    app.apply(Event::Key(code(KeyCode::Up)));
    assert!(!app.log_view.follow);
    assert_eq!(app.log_view.scroll, 1);

    app.apply(Event::Key(code(KeyCode::PageUp)));
    assert_eq!(app.log_view.scroll, 11);

    app.apply(Event::Key(code(KeyCode::End)));
    assert_eq!(app.log_view.scroll, 0);
    assert!(app.log_view.follow);

    app.apply(Event::Key(code(KeyCode::Up)));
    app.apply(Event::Log(parse_line(VISIBLE_LINE)));
    assert_eq!(app.log_view.scroll, 2);

    app.apply(Event::Key(code(KeyCode::Down)));
    app.apply(Event::Key(code(KeyCode::Down)));
    assert_eq!(app.log_view.scroll, 0);
    assert!(app.log_view.follow);

    app.apply(Event::Key(code(KeyCode::Home)));
    assert_eq!(app.log_view.scroll, app.visible_logs().len() - 1);
}
#[test]
fn model_selection() {
    let mut app = App::new(1000);
    app.apply(Event::Llama(llama_models(5)));
    app.apply(Event::Key(key('2')));

    for _ in 0..6 {
        app.apply(Event::Key(code(KeyCode::Down)));
    }
    assert_eq!(app.model_selected, 4);
    app.apply(Event::Key(code(KeyCode::Up)));
    assert_eq!(app.model_selected, 3);
    app.apply(Event::Key(code(KeyCode::Home)));
    assert_eq!(app.model_selected, 0);
    app.apply(Event::Key(code(KeyCode::End)));
    assert_eq!(app.model_selected, 4);

    app.apply(Event::Llama(llama_models(2)));
    assert_eq!(app.model_selected, 1);

    app.apply(Event::Key(key('1')));
    let before = app.model_selected;
    app.apply(Event::Key(code(KeyCode::Down)));
    assert_eq!(app.model_selected, before);
}

#[test]
fn first_models_list_selects_the_loaded_model() {
    use huacaya::app::demo::{DemoState, llama as demo_llama};
    let mut app = App::new(1000);
    app.apply(Event::Llama(demo_llama(0.0, DemoState::Idle)));
    assert_eq!(app.model_selected, 3, "the demo's loaded model is the 4th");
    // Later lists keep the user's choice.
    app.model_selected = 0;
    app.apply(Event::Llama(demo_llama(1.0, DemoState::Idle)));
    assert_eq!(app.model_selected, 0);
}
