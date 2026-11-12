//! Deterministic demo data (`src/app/demo.rs`), per `tasks/T11-demo.md`. Values are exact.

use llamatop::app::demo::{CYCLE_S, DemoState, app, llama, log_lines, state_at, system};
use llamatop::collect::llama::ModelState;
use llamatop::collect::llama_poll::Health;
use llamatop::collect::logs::LogEvent;

/// The five demo states, in the order the sheet lists them.
const STATES: [DemoState; 5] =
    [DemoState::Idle, DemoState::Prompt, DemoState::Generating, DemoState::Loading, DemoState::Down];

#[test]
fn cycle_states() {
    assert_eq!(state_at(0.0), DemoState::Idle);
    assert_eq!(state_at(7.9), DemoState::Idle);
    assert_eq!(state_at(8.0), DemoState::Prompt);
    assert_eq!(state_at(11.9), DemoState::Prompt);
    assert_eq!(state_at(12.0), DemoState::Generating);
    assert_eq!(state_at(31.9), DemoState::Generating);
    assert_eq!(state_at(32.0), DemoState::Idle);
    assert_eq!(state_at(48.0), DemoState::Prompt);
    assert_eq!(state_at(-1.0), DemoState::Idle); // -1 rem_euclid 40 = 39, in [32, 40) Idle
}

#[test]
fn system_is_plausible() {
    for state in STATES {
        for i in 0..=400 {
            let t = i as f64 / 2.0;
            let s = system(t, state);

            assert_eq!(s.gpus.len(), 1, "one GPU in {state:?}");
            let gpu = &s.gpus[0];
            let busy = gpu.busy_pct.unwrap();
            assert!((0.0..=100.0).contains(&busy), "busy {busy} in 0..=100 in {state:?}");
            assert!(gpu.vram_used.unwrap() <= gpu.vram_total.unwrap(), "vram in {state:?}");

            let mem = s.mem.unwrap();
            assert!(mem.used <= mem.total, "mem in {state:?}");

            let usage = s.cpu.usage_pct.unwrap();
            assert!((0.0..=100.0).contains(&usage), "usage {usage} in 0..=100 in {state:?}");
            assert_eq!(s.cpu.per_core_pct.len(), 16, "16 cores in {state:?}");
            assert_eq!(s.disks.len(), 3, "3 disks in {state:?}");
        }
    }
}

#[test]
fn system_follows_state() {
    let t = 10.0;
    let idle = system(t, DemoState::Idle);
    let idle_busy = idle.gpus[0].busy_pct.unwrap();
    let idle_power = idle.gpus[0].power_w.unwrap();

    let gen_snap = system(t, DemoState::Generating);
    assert!(gen_snap.gpus[0].busy_pct.unwrap() > idle_busy + 50.0, "Generating busy > Idle + 50");

    let prompt = system(t, DemoState::Prompt);
    assert!(prompt.gpus[0].power_w.unwrap() > idle_power, "Prompt power > Idle power");
}

#[test]
fn deterministic() {
    assert_eq!(system(17.3, DemoState::Generating), system(17.3, DemoState::Generating));
    assert_eq!(llama(17.3, DemoState::Generating), llama(17.3, DemoState::Generating));
    assert_eq!(format!("{:?}", app(DemoState::Generating)), format!("{:?}", app(DemoState::Generating)));
}

#[test]
fn llama_states() {
    // Generating: one active, its slot processing, live t/s > 0.
    let gen_snap = llama(10.0, DemoState::Generating);
    assert_eq!(gen_snap.active.len(), 1);
    assert!(gen_snap.active[0].slots[0].is_processing);
    assert!(gen_snap.active[0].live_gen_tps.unwrap() > 0.0);

    // Idle: slot not processing, no live t/s.
    let idle = llama(10.0, DemoState::Idle);
    assert!(!idle.active[0].slots[0].is_processing);
    assert_eq!(idle.active[0].live_gen_tps, None);

    // Loading: no active, exactly one model Loading.
    let loading = llama(10.0, DemoState::Loading);
    assert!(loading.active.is_empty());
    let loading_models = loading.models.iter().filter(|m| m.state == ModelState::Loading).collect::<Vec<_>>();
    assert_eq!(loading_models.len(), 1);

    // Down: health Down, no models.
    let down = llama(10.0, DemoState::Down);
    assert!(matches!(down.health, Health::Down(_)));
    assert!(down.models.is_empty());
}

#[test]
fn active_models_are_loaded() {
    for state in STATES {
        for i in 0..60 {
            let t = i as f64;
            let snap = llama(t, state);
            for active in &snap.active {
                assert!(
                    snap.models.iter().any(|m| m.id == active.id && m.state == ModelState::Loaded),
                    "active {} not Loaded in {state:?}",
                    active.id
                );
            }
        }
    }
}

#[test]
fn log_cycle_events() {
    let lines = log_lines(0.0, CYCLE_S);

    let mut proxy = 0;
    let mut prompt_progress = 0;
    let mut prompt_timing = 0;
    let mut gen_progress = 0;
    let mut gen_timing = 0;
    let mut draft = 0;
    let mut released = 0;
    let mut request_status: Option<u16> = None;
    for l in &lines {
        if let Some(event) = &l.event {
            match event {
                LogEvent::Proxy { .. } => proxy += 1,
                LogEvent::PromptProgress { .. } => prompt_progress += 1,
                LogEvent::PromptTiming { .. } => prompt_timing += 1,
                LogEvent::GenProgress { .. } => gen_progress += 1,
                LogEvent::GenTiming { .. } => gen_timing += 1,
                LogEvent::DraftAcceptance { .. } => draft += 1,
                LogEvent::Released { .. } => released += 1,
                LogEvent::Request { status, .. } => request_status = Some(*status),
            }
        }
    }

    assert_eq!(proxy, 1);
    assert_eq!(prompt_progress, 3);
    assert_eq!(prompt_timing, 1);
    assert_eq!(gen_progress, 6);
    assert_eq!(gen_timing, 1);
    assert_eq!(draft, 1);
    assert_eq!(released, 1);
    assert_eq!(request_status, Some(200));

    // First line's time is 10:00:08 (event time 8 s into the day offset 36_000).
    assert_eq!(lines[0].time.as_deref(), Some("10:00:08"));
    // Every line is either the instance (35335) or a router line (no instance).
    for l in &lines {
        assert!(matches!(l.instance, Some(35335) | None), "instance {:?} in {:?}", l.instance, l.event);
    }
}

#[test]
fn log_split_invariant() {
    let full = log_lines(0.0, 40.0);
    let mut combined = log_lines(0.0, 20.0);
    combined.extend(log_lines(20.0, 40.0));
    assert_eq!(full, combined);

    let second = log_lines(40.0, 80.0);
    assert_eq!(second.len(), full.len());
    for l in &second {
        assert!(l.task.is_none() || l.task == Some(9001), "cycle 1 carries task 9001, got {:?}", l.task);
    }
    assert!(second.iter().any(|l| l.task == Some(9001)), "cycle 1 has a task 9001 line");

    assert!(log_lines(5.0, 5.0).is_empty());
}

#[test]
fn demo_app() {
    let gen_snap = app(DemoState::Generating);
    assert_eq!(gen_snap.gen_tps.len(), 120);
    assert_eq!(gen_snap.cpu_usage.len(), 120);
    assert_eq!(gen_snap.logs.len(), log_lines(0.0, CYCLE_S).len(), "one log cycle replayed once");

    let req = gen_snap.last_request.as_ref().unwrap();
    assert_eq!(req.prompt_tokens, Some(8192));
    assert_eq!(req.gen_tokens, Some(1040));
    assert_eq!(req.ctx_tokens, Some(9232));
    assert!(gen_snap.animating());

    assert!(!app(DemoState::Idle).animating());

    let down = app(DemoState::Down);
    let down_llama = down.llama.unwrap();
    assert!(matches!(down_llama.health, Health::Down(_)));
    assert!(down.gen_tps.is_empty());
}
