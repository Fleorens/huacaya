//! Integration tests for `collect::logs`: the line classifier `parse_line` and the command follower
//! `follow`. Values come from `tests/fixtures/journal/llama-server.txt` (real capture + 5 hand-written
//! lines); floats compare within 0.01.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use huacaya::collect::logs::*;

/// The fixture, as a vector of lines (1-based indexing below).
fn fixture_lines() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journal/llama-server.txt");
    let content = std::fs::read_to_string(path).expect("fixture present");
    content.split('\n').map(|s| s.trim_end_matches('\r').to_string()).collect()
}

/// Parse fixture line `n` (1-based).
fn fixture_line(n: usize) -> LogLine {
    parse_line(&fixture_lines()[n - 1])
}

/// Absolute path of the journal fixture, for `cat`.
fn fixture_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/journal/llama-server.txt")
        .display()
        .to_string()
}

/// True when two floats differ by less than the 0.01 tolerance.
fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// Poll `lines` until it holds `target` entries or 3 s elapse, never sleeping longer than 50 ms.
fn wait_for(lines: &Arc<Mutex<Vec<LogLine>>>, target: usize) {
    let start = Instant::now();
    while lines.lock().unwrap().len() < target {
        if start.elapsed() > Duration::from_secs(3) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn router_line() {
    let l = fixture_line(1);
    assert_eq!(l.time.as_deref(), Some("10:28:39"));
    assert_eq!(l.source.as_deref(), Some("llama-server"));
    assert_eq!(l.instance, None);
    assert_eq!(l.task, None);
    assert_eq!(l.level, Level::Info);
    assert!(l.text.starts_with("srv  proxy_reques: proxying request"));
    assert_eq!(
        l.event,
        Some(LogEvent::Proxy {
            model: "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M".to_string(),
            port: 35335,
        })
    );
}

#[test]
fn instance_line() {
    let l = fixture_line(3);
    assert_eq!(l.instance, Some(35335));
    assert_eq!(l.task, Some(9137));
    assert!(l.text.starts_with("slot launch_slot_:"));
    assert_eq!(l.event, None);

    assert_eq!(fixture_line(2).task, Some(-1));
}

#[test]
fn gen_progress() {
    match fixture_line(4).event.expect("event") {
        LogEvent::GenProgress { n_gen, tps, tps_3s } => {
            assert_eq!(n_gen, 161);
            assert!(approx(tps, 52.96));
            assert!(approx(tps_3s, 53.29));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn prompt_timing() {
    match fixture_line(8).event.expect("event") {
        LogEvent::PromptTiming { tokens, tps } => {
            assert_eq!(tokens, 48);
            assert!(approx(tps, 85.26));
        }
        other => panic!("unexpected {other:?}"),
    }
    match fixture_line(24).event.expect("event") {
        LogEvent::PromptTiming { tokens, tps } => {
            assert_eq!(tokens, 10229);
            assert!(approx(tps, 891.00));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn gen_timing() {
    match fixture_line(9).event.expect("event") {
        LogEvent::GenTiming { tokens, tps } => {
            assert_eq!(tokens, 627);
            assert!(approx(tps, 46.63));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn lines_without_event() {
    for n in [10, 11, 12] {
        let l = fixture_line(n);
        assert_eq!(l.event, None, "line {n}");
        assert_eq!(l.level, Level::Info, "line {n}");
    }
}

#[test]
fn draft_acceptance() {
    match fixture_line(13).event.expect("event") {
        LogEvent::DraftAcceptance { accepted, generated } => {
            assert_eq!(accepted, 369);
            assert_eq!(generated, 771);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn released() {
    match fixture_line(14).event.expect("event") {
        LogEvent::Released { n_tokens } => assert_eq!(n_tokens, 57724),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn prompt_progress() {
    match fixture_line(18).event.expect("event") {
        LogEvent::PromptProgress { n_tokens, progress, tps } => {
            assert_eq!(n_tokens, 4096);
            assert!(approx(progress as f64, 0.40));
            assert!(approx(tps, 1004.65));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn warning_letter() {
    let l = fixture_line(31);
    assert_eq!(l.level, Level::Warn);
    assert!(l.text.starts_with("srv    load_model:"));
}

#[test]
fn error_letter() {
    assert_eq!(fixture_line(32).level, Level::Error);
}

#[test]
fn other_unit_uses_keywords() {
    let l = fixture_line(33);
    assert_eq!(l.source.as_deref(), Some("systemd"));
    assert_eq!(l.instance, None);
    assert_eq!(l.level, Level::Error);
}

#[test]
fn request_line() {
    let l = fixture_line(34);
    assert_eq!(
        l.event,
        Some(LogEvent::Request {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            status: 401,
        })
    );
    assert_eq!(l.level, Level::Warn);
}

#[test]
fn line_without_prefix() {
    let l = fixture_line(35);
    assert_eq!(l.time, None);
    assert_eq!(l.source, None);
    assert_eq!(l.text, "this line has no journald prefix at all");
    assert_eq!(l.level, Level::Info);
    assert_eq!(l.event, None);
}

#[test]
fn letter_wins_over_keywords() {
    let l = parse_line("2026-11-11T10:00:00.000000+01:00 h llama-server[1]: [1] 1.2 I srv x: no error here");
    assert_eq!(l.level, Level::Info);
}

#[test]
fn weird_input_never_panics() {
    let big = "a".repeat(100_000);
    let inputs: Vec<&str> = vec![
        "",
        "   ",
        "é",
        "éééééééééééééééééééééé x y: z",
        &big,
        "request: GET",
        "eval time = x tokens per second",
        "n_gen = a, tg = b",
        "[99999999] I",
    ];
    for raw in inputs {
        assert_eq!(parse_line(raw).event, None, "input {raw:?}");
    }
}

#[test]
fn follows_a_command_to_its_end() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    {
        let sink_lines = lines.clone();
        let mut follower = follow(&format!("cat {}", fixture_path()), move |line| {
            sink_lines.lock().unwrap().push(line);
            true
        })
        .expect("cat runs");
        wait_for(&lines, 36);
        follower.stop();
    }

    let collected = lines.lock().unwrap().clone();
    assert_eq!(collected.len(), 36);
    assert_eq!(collected[0].time.as_deref(), Some("10:28:39"));
    assert_eq!(collected[35].level, Level::Warn);
    assert_eq!(collected[35].text, "log source ended: cat");
    assert_eq!(collected[35].time, None);
    assert_eq!(collected[35].source, None);
}

#[test]
fn stop_kills_a_silent_command() {
    let mut follower = follow("sleep 30", |_line| true).expect("sleep runs");
    let start = Instant::now();
    follower.stop();
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn drop_stops_the_command() {
    let follower = follow("sleep 30", |_line| true).expect("sleep runs");
    let start = Instant::now();
    drop(follower);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn missing_program_is_an_error() {
    assert!(follow("huacaya-no-such-program --x", |_line| true).is_err());
    assert!(follow("  ", |_line| true).is_err());
}

#[test]
fn sink_can_refuse() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    {
        let sink_lines = lines.clone();
        let mut follower = follow(&format!("cat {}", fixture_path()), move |line| {
            sink_lines.lock().unwrap().push(line);
            false
        })
        .expect("cat runs");
        wait_for(&lines, 1);
        follower.stop();
    }

    assert_eq!(lines.lock().unwrap().len(), 1);
}
