//! Data and widgets added by `tasks/T31` (CPU info, request history, task timer, big digits, level
//! bars). Expected values come from `tests/fixtures/sysroot`.

use std::path::Path;
use std::time::Instant;

use llamatop::app::demo::{self, DemoState};
use llamatop::app::{App, Event};
use llamatop::collect::logs::parse_line;
use llamatop::collect::sampler::{Sampler, SystemSnapshot};
use llamatop::collect::system::{parse_cpuinfo, read_cpuinfo, short_cpu_name};
use llamatop::ui::big::{big_digits, level_bars};

const SYSROOT: &str = "tests/fixtures/sysroot";

/// `short_cpu_name` on the names the sheet gives.
#[test]
fn cpu_names() {
    assert_eq!(short_cpu_name("11th Gen Intel(R) Core(TM) i7-11700K @ 3.60GHz"), "Intel Core i7-11700K");
    assert_eq!(short_cpu_name("AMD Ryzen 5 5625U with Radeon Graphics"), "AMD Ryzen 5 5625U");
    assert_eq!(short_cpu_name("AMD Ryzen 9 7950X 16-Core Processor"), "AMD Ryzen 9 7950X");
    assert_eq!(short_cpu_name("Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz"), "Intel Xeon CPU E5-2680 v4");
    assert_eq!(short_cpu_name(""), "");
}

/// The fixture `proc/cpuinfo` parses to the aibox CPU; empty text gives all `None`.
#[test]
fn cpuinfo_fixture() {
    let cpu = read_cpuinfo(Path::new(SYSROOT));
    assert_eq!(cpu.model.as_deref(), Some("Intel Core i7-11700K"));
    assert_eq!(cpu.cores, Some(8));
    assert_eq!(cpu.threads, Some(16));
    assert_eq!(parse_cpuinfo(""), parse_cpuinfo(""));
    assert!(
        parse_cpuinfo("").model.is_none()
            && parse_cpuinfo("").cores.is_none()
            && parse_cpuinfo("").threads.is_none()
    );
}

/// A sampler on the fixture sysroot reads the CPU name; a snapshot JSON without the `cpu_info` key
/// deserializes with a default `CpuInfo`.
#[test]
fn snapshot_has_cpu_info() {
    let mut sampler = Sampler::new(Path::new(SYSROOT).to_path_buf(), Vec::new());
    let snap = sampler.sample(Instant::now());
    assert_eq!(snap.cpu_info.model.as_deref(), Some("Intel Core i7-11700K"));

    let json = serde_json::to_value(demo::system(0.0, DemoState::Idle)).expect("serialize");
    let mut fields = json.as_object().expect("object").clone();
    fields.remove("cpu_info");
    let back: SystemSnapshot = serde_json::from_value(serde_json::Value::Object(fields)).expect("parse");
    assert_eq!(back.cpu_info, Default::default());
}

/// The block digits for "52.4" (and an empty string), exactly.
#[test]
fn big_digits_glyphs() {
    assert_eq!(big_digits("52.4"), ["█▀▀ ▀▀█   █ █", "▀▀█ █▀▀   ▀▀█", "▀▀▀ ▀▀▀ ▀   ▀"]);
    assert_eq!(big_digits(""), ["", "", ""]);
}

/// The level bars for the values the sheet gives.
#[test]
fn level_bars_values() {
    assert_eq!(level_bars(&[0.0, 50.0, 100.0, f32::NAN, 120.0]), "▁▅█ █");
}

/// `App::requests` follows `last_request`: two tasks from the fixture journal, then capped at
/// `REQUEST_HISTORY` over 60 demo cycles.
#[test]
fn request_history() {
    let mut app = App::new(1000);
    for line in
        include_str!("fixtures/journal/llama-server.txt").lines().filter(|l| !l.is_empty()).map(parse_line)
    {
        app.apply(Event::Log(line));
    }
    assert_eq!(app.requests.len(), 2);
    let tasks: Vec<Option<i64>> = app.requests.iter().map(|r| r.task).collect();
    assert_eq!(tasks, vec![Some(9137), Some(9397)]);
    assert_eq!(app.requests.back(), app.last_request.as_ref());

    let mut app = App::new(1000);
    for k in 0..60 {
        for line in demo::log_lines(40.0 * k as f64, 40.0 * (k + 1) as f64) {
            app.apply(Event::Log(line));
        }
    }
    assert_eq!(app.requests.len(), 50);
    assert_eq!(app.requests.back(), app.last_request.as_ref());
}

/// `task_elapsed_s` counts the ticks since the processing slot 0 started, and is `None` when idle.
#[test]
fn task_timer() {
    let mut app = App::new(1000);
    app.apply(Event::Llama(demo::llama(20.0, DemoState::Generating)));
    for _ in 0..25 {
        app.apply(Event::Tick);
    }
    app.apply(Event::Llama(demo::llama(20.0, DemoState::Generating)));
    assert_eq!(app.task_elapsed_s(), Some(2));

    let mut app = App::new(1000);
    app.apply(Event::Llama(demo::llama(0.0, DemoState::Idle)));
    assert_eq!(app.task_elapsed_s(), None);
}
