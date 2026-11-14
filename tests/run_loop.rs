//! The run loop: clock parsing/formatting and the `--demo` event generator (`tasks/T15-run-loop.md`).

use huacaya::app::clock::{hms, local_utc_offset, parse_utc_offset};
use huacaya::app::demo;
use huacaya::app::run::demo_events;
use huacaya::app::{App, Event};
use huacaya::collect::logs::{LogEvent, LogLine};

#[test]
fn utc_offsets() {
    assert_eq!(parse_utc_offset("+0100\n"), Some(3600));
    assert_eq!(parse_utc_offset("-0530"), Some(-19800));
    assert_eq!(parse_utc_offset("+0000"), Some(0));
    for bad in ["0100", "+01", "", "+01:00"] {
        assert_eq!(parse_utc_offset(bad), None, "{bad:?}");
    }
}

#[test]
fn clock_format() {
    assert_eq!(hms(0, 0), "00:00:00");
    assert_eq!(hms(3_725, 0), "01:02:05");
    assert_eq!(hms(86_399, 3600), "00:59:59");
    assert_eq!(hms(0, -3600), "23:00:00");
    assert_eq!(hms(1_762_934_400, 3600), "09:00:00");
}

#[test]
fn local_offset_is_sane() {
    assert!(local_utc_offset().abs() <= 14 * 3600);
}

#[test]
fn demo_events_data_period() {
    let events = demo_events(0.95, 1.05, 1.0);
    assert_eq!(
        events.iter().filter(|e| matches!(e, Event::System(_))).count(),
        1,
        "one System when the data period is crossed"
    );
    assert_eq!(
        events.iter().filter(|e| matches!(e, Event::Llama(_))).count(),
        1,
        "one Llama when the data period is crossed"
    );

    let events = demo_events(1.05, 1.15, 1.0);
    assert!(
        !events.iter().any(|e| matches!(e, Event::System(_) | Event::Llama(_))),
        "no data when the period is not crossed"
    );
}

#[test]
fn demo_events_logs() {
    // Summing the log events over the 0.1 s windows gives the whole cycle, split independently of the
    // windowing (the half-open windows partition [0, 40)).
    let mut sum = 0usize;
    for k in 0..400 {
        let n = demo_events(k as f64 * 0.1, (k + 1) as f64 * 0.1, 1.0)
            .iter()
            .filter(|e| matches!(e, Event::Log(_)))
            .count();
        sum += n;
    }
    assert_eq!(sum, demo::log_lines(0.0, 40.0).len());

    // The two log lines at event time 8: the Proxy line, then the launch_slot line.
    let events = demo_events(7.95, 8.05, 1.0);
    let logs: Vec<&LogLine> = events
        .iter()
        .filter_map(|e| match e {
            Event::Log(l) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(logs.len(), 2);
    assert!(matches!(logs[0].event, Some(LogEvent::Proxy { .. })));
    assert!(logs[1].text.contains("launch_slot_"));
}

#[test]
fn demo_drives_app() {
    let mut app = App::new(1000);
    // 0..20 s in 0.1 s steps: by 20 s the demo is Generating.
    for k in 0..200 {
        let t0 = k as f64 * 0.1;
        let t1 = (k + 1) as f64 * 0.1;
        for event in demo_events(t0, t1, 1.0) {
            app.apply(event);
        }
    }
    assert!(app.apply(Event::Tick), "Generating animates the tick");

    // On to 40 s: the request that finished at event time 32 is the last one.
    for k in 200..400 {
        let t0 = k as f64 * 0.1;
        let t1 = (k + 1) as f64 * 0.1;
        for event in demo_events(t0, t1, 1.0) {
            app.apply(event);
        }
    }
    let req = app.last_request.clone().expect("a last request");
    assert_eq!(req.gen_tokens, Some(1040));
    assert!(app.gen_tps.len() >= 39, "one sample per second of generation");
}
