//! Deterministic demo data for every dashboard state (T11).
//!
//! Pure: the same `t` always yields the same snapshot, so the golden files of T13 are stable. No
//! clock, no randomness, no I/O — every value is a formula of `t`. The live `--demo` mode of T14
//! replays these same functions against the clock.

use crate::app::demo_log::{cycle_lines, raw_line};
use crate::app::{App, Event};
use crate::collect::disk::DiskSample;
use crate::collect::gpu::GpuSample;
use crate::collect::llama::{Metrics, ModelInfo, ModelState, SlotInfo};
use crate::collect::llama_poll::{ActiveModel, Health, LlamaSnapshot};
use crate::collect::logs::{LogLine, parse_line};
use crate::collect::net::NetSample;
use crate::collect::process::{ProcSample, ServerRole};
use crate::collect::sampler::SystemSnapshot;
use crate::collect::sensors::TempReading;
use crate::collect::system::{CpuFreq, CpuInfo, CpuSample, LoadAvg, MemSample};

/// The model id of the loaded one (model 4 below).
pub(crate) const LOADED_ID: &str = "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M";

/// The states the demo can show. `clap::ValueEnum` so that T13 can take it as `--demo-state <state>`
/// (values `idle`, `prompt`, `generating`, `loading`, `down`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DemoState {
    Idle,
    Prompt,
    Generating,
    Loading,
    Down,
}

/// Length of one live demo cycle, in seconds.
pub const CYCLE_S: f64 = 40.0;

/// Live demo state at `t` seconds, by `c = t.rem_euclid(CYCLE_S)`: `[0, 8)` Idle, `[8, 12)` Prompt,
/// `[12, 32)` Generating, `[32, 40)` Idle. `Loading` and `Down` only appear through `--demo-state`.
pub fn state_at(t: f64) -> DemoState {
    let c = t.rem_euclid(CYCLE_S);
    if c < 8.0 {
        DemoState::Idle
    } else if c < 12.0 {
        DemoState::Prompt
    } else if c < 32.0 {
        DemoState::Generating
    } else {
        DemoState::Idle
    }
}

/// `sin(1.3 t) + 0.5 sin(0.37 t)`, the wobble every "± k" value is scaled by (within ±1.5).
fn wobble(t: f64) -> f64 {
    (1.3 * t).sin() + 0.5 * (0.37 * t).sin()
}

/// The aibox in `state` at `t`. Pure.
pub fn system(t: f64, state: DemoState) -> SystemSnapshot {
    let w = wobble(t);

    // Per-state base and wobble amplitude: (busy, busy_a, vram GiB, power, power_a, junction, j_a,
    // usage, usage_a, mem GiB). Amplitude 0 means the value is fixed.
    let (busy, busy_a, vram, power, power_a, junction, j_a, usage, usage_a, mem) = match state {
        DemoState::Idle => (2.0, 1.0, 15.6, 18.0, 2.0, 48.0, 1.0, 4.0, 2.0, 21.5),
        DemoState::Prompt => (99.0, 1.0, 15.9, 190.0, 6.0, 82.0, 1.0, 35.0, 3.0, 21.6),
        DemoState::Generating => (96.0, 2.0, 15.9, 120.0, 4.0, 74.0, 1.0, 52.0, 3.0, 21.6),
        DemoState::Loading => (30.0, 5.0, 8.0, 60.0, 5.0, 55.0, 1.0, 12.0, 3.0, 14.0),
        DemoState::Down => (0.0, 0.0, 0.6, 12.0, 0.0, 42.0, 0.0, 3.0, 1.0, 6.0),
    };
    let busy = ((busy + busy_a * w) as f32).clamp(0.0, 100.0);
    let power = (power + power_a * w) as f32;
    let junction = (junction + j_a * w) as f32;
    let usage = (usage + usage_a * w) as f32;
    let vram_used = (vram * 1024.0 * 1024.0 * 1024.0) as u64;
    let mem_used = (mem * 1024.0 * 1024.0 * 1024.0) as u64;

    // CPU, derived the same in every state.
    let per_core: Vec<f32> =
        (0..16).map(|i| (usage + ((i % 4) as f32 - 1.5) * 4.0).clamp(0.0, 100.0)).collect();
    let cpu_temp_c = 40.0 + 0.4 * usage;
    let cpu = CpuSample {
        usage_pct: Some(usage),
        per_core_pct: per_core,
        freq: Some(CpuFreq { avg_mhz: 1200.0 + 45.0 * usage, max_mhz: 4700.0 }),
        load: Some(LoadAvg { one: usage / 6.0, five: usage / 7.0, fifteen: usage / 9.0 }),
    };

    // The one AMD GPU, derived from junction and busy.
    let temp_edge = junction - 11.0;
    let temp_mem = junction + 3.0;
    let fan_rpm = (400.0 + 25.0 * (junction - 40.0)) as u32;
    let gpu = GpuSample {
        card: "card1".into(),
        name: "Radeon RX 7800 XT".into(),
        busy_pct: Some(busy),
        vram_used: Some(vram_used),
        vram_total: Some(17_163_091_968),
        gtt_used: Some(183_365_632),
        temp_edge_c: Some(temp_edge),
        temp_junction_c: Some(junction),
        temp_mem_c: Some(temp_mem),
        power_w: Some(power),
        power_cap_w: Some(212.0),
        fan_rpm: Some(fan_rpm),
        fan_pct: Some(fan_rpm as f32 / 3600.0 * 100.0),
        sclk_mhz: Some(500.0 + 18.0 * busy),
        vddgfx_mv: Some(917),
        link: Some("16.0 GT/s PCIe x16".into()),
    };

    let mem_total = 33_444_237_312;
    let mem = MemSample {
        total: mem_total,
        available: mem_total - mem_used,
        used: mem_used,
        cached: 19_519_881_216,
        swap_total: 8_589_930_496,
        swap_used: 1_310_720,
    };

    let mut procs = vec![ProcSample {
        pid: 1194,
        role: ServerRole::Router,
        model: None,
        port: Some(8080),
        rss: 300_736_512,
        threads: 23,
    }];
    if state != DemoState::Down {
        let rss = if state == DemoState::Loading { 9_000_000_000 } else { 20_577_497_088 };
        procs.push(ProcSample {
            pid: 6873,
            role: ServerRole::Instance,
            model: Some(LOADED_ID.into()),
            port: Some(35335),
            rss,
            threads: 30,
        });
    }

    SystemSnapshot {
        hostname: Some("gpu-box".into()),
        uptime_s: Some(356_400 + t as u64),
        cpu,
        cpu_temp_c: Some(cpu_temp_c),
        nvme_temp_c: Some(41.85),
        mem: Some(mem),
        gpus: vec![gpu],
        temps: vec![
            TempReading { chip: "coretemp".into(), label: "Package id 0".into(), celsius: cpu_temp_c },
            TempReading { chip: "nvme".into(), label: "Composite".into(), celsius: 41.85 },
            TempReading { chip: "amdgpu".into(), label: "edge".into(), celsius: temp_edge },
            TempReading { chip: "amdgpu".into(), label: "junction".into(), celsius: junction },
            TempReading { chip: "amdgpu".into(), label: "mem".into(), celsius: temp_mem },
        ],
        disks: vec![
            DiskSample {
                mount: "/".into(),
                total: 65_026_052_096,
                used: 39_769_993_216,
                available: 21_919_703_040,
            },
            DiskSample {
                mount: "/srv/models".into(),
                total: 126_227_120_128,
                used: 77_434_490_880,
                available: 42_333_401_088,
            },
            DiskSample {
                mount: "/srv/data".into(),
                total: 63_090_503_680,
                used: 7_026_569_216,
                available: 52_825_931_776,
            },
        ],
        nets: vec![NetSample {
            iface: "wlp9s0".into(),
            rx_bps: Some(2500.0 + 500.0 * w),
            tx_bps: Some(15000.0 + 3000.0 * w),
            rx_total: (195_718_003.0 + 2500.0 * t) as u64,
            tx_total: (195_824_906.0 + 15000.0 * t) as u64,
        }],
        procs,
        cpu_info: CpuInfo { model: Some("Intel Core i7-11700K".into()), cores: Some(8), threads: Some(16) },
    }
}

/// The router in `state` at `t`. Pure.
pub fn llama(t: f64, state: DemoState) -> LlamaSnapshot {
    match state {
        DemoState::Down => LlamaSnapshot {
            health: Health::Down("GET /health: Connection refused".into()),
            router: false,
            build_info: None,
            models: vec![],
            active: vec![],
        },
        _ => {
            let state_for =
                if state == DemoState::Loading { ModelState::Loading } else { ModelState::Loaded };
            LlamaSnapshot {
                health: Health::Ok,
                router: true,
                build_info: Some("b12151-10d4846a5".into()),
                models: models(state_for),
                active: if state == DemoState::Loading { vec![] } else { vec![active_model(state, t)] },
            }
        }
    }
}

/// The five models, in order. `loaded_state` is `Loaded` or `Loading` for the loaded one.
fn models(loaded_state: ModelState) -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "bartowski/Ornith-1.5-35B-A3B-GGUF:Q4_K_M".into(),
            state: ModelState::Unloaded,
            ctx_train: Some(262144),
            ctx_size: Some(163840),
            n_cpu_moe: Some(16),
            n_gpu_layers: Some(99),
            threads: Some(8),
            spec_type: None,
            model_path: None,
            vision: true,
            size_bytes: None,
            n_params: None,
            ftype: None,
        },
        ModelInfo {
            id: "gsq-qwen3.8-27b".into(),
            state: ModelState::Unloaded,
            ctx_train: Some(262144),
            ctx_size: Some(65536),
            n_cpu_moe: None,
            n_gpu_layers: Some(99),
            threads: Some(8),
            spec_type: Some("draft-mtp".into()),
            model_path: None,
            vision: false,
            size_bytes: None,
            n_params: None,
            ftype: None,
        },
        ModelInfo {
            id: "muse-glimmer-30b".into(),
            state: ModelState::Unloaded,
            ctx_train: Some(131072),
            ctx_size: Some(131072),
            n_cpu_moe: None,
            n_gpu_layers: Some(99),
            threads: Some(8),
            spec_type: Some("draft-dflash".into()),
            model_path: None,
            vision: false,
            size_bytes: None,
            n_params: None,
            ftype: None,
        },
        ModelInfo {
            id: LOADED_ID.into(),
            state: loaded_state,
            ctx_train: Some(262144),
            ctx_size: Some(196608),
            n_cpu_moe: Some(21),
            n_gpu_layers: Some(99),
            threads: Some(8),
            spec_type: Some("draft-mtp".into()),
            model_path: Some("/srv/models/direct/Cyber-Tiel-Coder-35B-A3B-MTP-UD-Q4_K_M.gguf".into()),
            vision: false,
            size_bytes: Some(22_512_939_520),
            n_params: Some(35_505_251_456),
            ftype: Some("Q4_K - Medium".into()),
        },
        ModelInfo {
            id: "unsloth/Qwen3-8B-GGUF:Q4_K_M".into(),
            state: ModelState::Unloaded,
            ctx_train: Some(40960),
            ctx_size: Some(32768),
            n_cpu_moe: None,
            n_gpu_layers: Some(99),
            threads: Some(8),
            spec_type: None,
            model_path: None,
            vision: false,
            size_bytes: None,
            n_params: None,
            ftype: None,
        },
    ]
}

/// The one `ActiveModel` of the loaded model: its metrics, one slot and the live generation speed.
fn active_model(state: DemoState, t: f64) -> ActiveModel {
    let (is_processing, n_prompt_tokens, n_decoded) = match state {
        DemoState::Idle => (false, 9232, 0),
        DemoState::Prompt => (true, 8192, 0),
        DemoState::Generating => (true, 57651, ((52.0 * t) as u64 % 30_000) + 1),
        DemoState::Loading | DemoState::Down => (false, 0, 0),
    };
    let slot = SlotInfo {
        id: 0,
        id_task: 9000,
        n_ctx: 196608,
        is_processing,
        n_prompt_tokens,
        n_decoded,
        n_remain: -1,
        speculative: true,
    };
    ActiveModel {
        id: LOADED_ID.into(),
        metrics: Some(Metrics {
            prompt_tokens_total: 828_751.0,
            prompt_tokens_cached_total: 41_518_700.0,
            prompt_seconds_total: 1402.01,
            tokens_predicted_total: 507_212.0,
            tokens_predicted_seconds_total: 8948.33,
            n_decode_total: 186_120.0,
            n_tokens_max: 99_007.0,
            requests_processing: if is_processing { 1.0 } else { 0.0 },
            requests_deferred: 0.0,
            draft_tokens_total: Some(517_905.0),
            draft_accepted_total: Some(333_880.0),
        }),
        slots: vec![slot],
        live_gen_tps: if state == DemoState::Generating { Some(52.0 + 4.0 * wobble(t)) } else { None },
    }
}

/// An `App` as it looks after two minutes in `state`, built only through `App::apply`. Two calls give
/// equal results.
pub fn app(state: DemoState) -> App {
    let mut app = App::new(1000);
    for s in 0..120 {
        let s = s as f64;
        app.apply(Event::System(system(s, state)));
        app.apply(Event::Llama(llama(s, state)));
    }
    for line in log_lines(0.0, CYCLE_S) {
        app.apply(Event::Log(line));
    }
    app
}

/// The log lines of the live cycle whose event time `e` satisfies `t0 <= e < t1`, in time order;
/// empty when `t1 <= t0`. Pure, and independent of how the range is split.
pub fn log_lines(t0: f64, t1: f64) -> Vec<LogLine> {
    // A non-finite bound would make the cycle range below astronomically long.
    if t1 <= t0 || !t0.is_finite() || !t1.is_finite() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in (t0 / CYCLE_S).floor() as u64..=(t1 / CYCLE_S).floor() as u64 {
        for (c, body) in cycle_lines(n) {
            let e = n as f64 * CYCLE_S + c;
            if e >= t0 && e < t1 {
                out.push(parse_line(&raw_line(e, &body)));
            }
        }
    }
    out
}
