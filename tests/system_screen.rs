// Cell-by-cell tests for the System screen (T18, T38). The model has no visual access, so every screen
// is rendered into a `Buffer` and checked row by row; the golden files capture the whole frame.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use llamatop::app::demo::{self, DemoState, app as demo_app};
use llamatop::app::keys::handle;
use llamatop::app::{App, Event, Screen};
use llamatop::collect::net::NetSample;
use llamatop::ui::panels::Ctx;
use llamatop::ui::screens::system;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::theme::{ColorMode, Role, Theme, rgb};

/// Render `system::render` over a `w × h` area and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

/// The shared `Ctx`: true-colour theme, graphs on, the fixed clock.
fn make_ctx<'a>(theme: &'a Theme) -> Ctx<'a> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

/// Whether the `needles` all appear in `row`, left to right (in order, not necessarily adjacent).
fn ordered(row: &str, needles: &[&str]) -> bool {
    let mut pos = 0;
    for n in needles {
        match row[pos..].find(n) {
            Some(i) => pos += i + n.chars().count(),
            None => return false,
        }
    }
    true
}

// --------------------------------------------------------------------------- sensor_thresholds()

#[test]
fn thresholds_by_chip() {
    assert_eq!(system::sensor_thresholds("amdgpu"), (85.0, 95.0));
    assert_eq!(system::sensor_thresholds("coretemp"), (80.0, 95.0));
    assert_eq!(system::sensor_thresholds("k10temp"), (80.0, 95.0));
    assert_eq!(system::sensor_thresholds("nvme"), (70.0, 80.0));
    assert_eq!(system::sensor_thresholds("acpitz"), (80.0, 95.0));
}

// --------------------------------------------------------------------------- CPU title + heatmap

#[test]
fn cpu_title_and_heatmap() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    let all: String = rows.join("\n");

    // The CPU title and its right title (temp · freq).
    assert!(all.contains("CPU  Intel Core i7-11700K · 8 cores / 16 threads"), "{:?}", rows);
    assert!(all.contains("60 °C · 3.5 GHz"), "{:?}", rows);

    // A heatmap row shows the first eight cores of the snapshot, rounded, in order.
    let per_core = &app.system.as_ref().expect("demo snapshot").cpu.per_core_pct;
    let expected: Vec<String> = per_core.iter().take(8).map(|p| format!("{:.0}", p.round())).collect();
    let needles: Vec<&str> = expected.iter().map(String::as_str).collect();
    assert_eq!(needles.len(), 8);
    assert!(rows.iter().any(|r| ordered(r, &needles)), "no heatmap row with {needles:?}: {rows:?}");
}

#[test]
fn heatmap_colours() {
    // True colour: the 0 % cell is `Selection`, the 100 % cell is `Cpu`.
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);
    if let Some(sys) = &mut app.system {
        sys.cpu.per_core_pct = vec![0.0, 100.0, 50.0, 25.0, 10.0, 20.0, 30.0, 40.0];
    }
    let mut buf = Buffer::empty(Rect::new(0, 0, 140, 40));
    system::render(&app, &ctx, Rect::new(0, 0, 140, 40), &mut buf);
    // CPU panel inner is (1,1); cell width 7, 8 per row, 1-col gap. Core 0 ("0") at x=4, core 1 ("100") at x=11.
    let (r, g, b) = rgb(Role::Selection);
    assert_eq!(buf[(4, 1)].style().bg, Some(Color::Rgb(r, g, b)), "0 % cell bg");
    let (r, g, b) = rgb(Role::Cpu);
    assert_eq!(buf[(11, 1)].style().bg, Some(Color::Rgb(r, g, b)), "100 % cell bg");

    // No colour: the 95 % number is bold.
    let theme = Theme::new(ColorMode::NoColor);
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);
    if let Some(sys) = &mut app.system {
        sys.cpu.per_core_pct = vec![0.0, 50.0, 95.0, 25.0, 10.0, 20.0, 30.0, 40.0];
    }
    let mut buf = Buffer::empty(Rect::new(0, 0, 140, 40));
    system::render(&app, &ctx, Rect::new(0, 0, 140, 40), &mut buf);
    // Core 2 ("95") at x=19.
    assert!(buf[(19, 1)].style().add_modifier.contains(Modifier::BOLD), "95 % number should be bold");
}

// --------------------------------------------------------------------------- Sensors

#[test]
fn sensors_and_marks() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");
    // The sensors are grouped by chip: the `GPU · amdgpu` header and the `junction` reading.
    assert!(all.contains("GPU · amdgpu"), "{:?}", rows);
    assert!(all.contains("junction"), "{:?}", rows);
    assert!(all.contains("73 °C"), "{:?}", rows);
    assert!(!all.contains("▲"), "{:?}", rows);

    // Force the junction hot: it must show its value with a critical mark.
    let mut app = demo_app(DemoState::Generating);
    if let Some(sys) = &mut app.system {
        for t in &mut sys.temps {
            if t.chip == "amdgpu" && t.label == "junction" {
                t.celsius = 96.0;
            }
        }
    }
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");
    assert!(all.contains("96 °C▲"), "{:?}", rows);

    // At 80 columns the value and its mark keep their room: the gauge shrinks instead.
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 80, 37);
    assert!(rows.join("\n").contains("96 °C▲"), "{:?}", rows);
    // At 60 columns the sensor rows stay inside the left column (its right border intact).
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 60, 37);
    for r in &rows[1..rows.len() - 1] {
        assert!(matches!(r.chars().nth(29), Some('│' | '╮' | '╯' | '╭' | '╰' | '┤')), "{r:?}");
    }
}

#[test]
fn sensors_grouped() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    let all: String = rows.join("\n");

    // The groups appear in order: GPU, CPU, Storage.
    let i_gpu = all.find("GPU · amdgpu").unwrap();
    let i_cpu = all.find("CPU · coretemp").unwrap();
    let i_store = all.find("Storage · nvme").unwrap();
    assert!(i_gpu < i_cpu && i_cpu < i_store, "groups out of order: {all:?}");

    // The summary readings, each label with its value on one row.
    assert!(rows.iter().any(|r| ordered(r, &["junction", "73 °C"])), "{:?}", rows);
    assert!(rows.iter().any(|r| ordered(r, &["package", "60 °C"])), "{:?}", rows);
}

#[test]
fn sensors_all_toggle() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);
    app.screen = Screen::System;
    // `all_sensors` starts false: the right title offers `s: all`.
    assert!(!app.all_sensors);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    assert!(rows.join("\n").contains("s: all"), "{:?}", rows);

    // `s` on the System screen sets it; the right title then offers `s: summary`.
    handle(&mut app, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(app.all_sensors);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    assert!(rows.join("\n").contains("s: summary"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Disks, network, processes

#[test]
fn disks_network_processes() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");

    for needle in [
        "/srv/models",
        "72.1/117.6 G",
        "wlp9s0",
        "2.1 K/s",
        "12.7 K/s",
        "total 187 M",
        "total 188 M",
        "1194",
        "router",
        "8080",
        "287 M",
        "6873",
        "instance",
        "35335",
        "19.2 G",
        "Cyber-Tiel-Coder-35B-A3B",
    ] {
        assert!(all.contains(needle), "missing {needle} in {{\n{all}\n}}");
    }
}

#[test]
fn network_two_rows() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    let all: String = rows.join("\n");

    // The down row carries the iface and the down arrow; the up row has a blank iface column and the
    // up arrow; each row carries its own `total`.
    assert!(all.contains("wlp9s0  ↓ "), "{:?}", rows);
    assert!(all.contains("        ↑ "), "{:?}", rows);
    assert!(all.contains("total"), "{:?}", rows);
}

#[test]
fn gpu_summary_line() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 40);
    let all: String = rows.join("\n");

    // The GPU summary line in the Processes panel.
    assert!(all.contains("GPU  RX 7800 XT"), "{:?}", rows);
    assert!(all.contains("917 mV"), "{:?}", rows);
}

#[test]
fn net_rates_history() {
    let mut app = App::new(1000);

    // Two events with rates for one interface: one history entry, two samples each.
    for _ in 0..2 {
        let mut s = demo::system(0.0, DemoState::Generating);
        s.nets = vec![NetSample {
            iface: "wlp9s0".into(),
            rx_bps: Some(2500.0),
            tx_bps: Some(15_000.0),
            rx_total: 187_000_000,
            tx_total: 188_000_000,
        }];
        app.apply(Event::System(s));
    }
    assert_eq!(app.net_rates.len(), 1);
    assert_eq!(app.net_rates[0].0, "wlp9s0");
    assert_eq!(app.net_rates[0].1.len(), 2);
    assert_eq!(app.net_rates[0].2.len(), 2);

    // A snapshot without the interface drops its entry.
    let mut s = demo::system(0.0, DemoState::Generating);
    s.nets = vec![];
    app.apply(Event::System(s));
    assert!(app.net_rates.is_empty());
}

// --------------------------------------------------------------------------- The "no data" cases

#[test]
fn system_without_data() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let rows = draw(|area, buf| system::render(&App::new(1000), &ctx, area, buf), 140, 37);
    assert!(rows.join("\n").contains("no data yet"));

    let rows = draw(|area, buf| system::render(&demo_app(DemoState::Down), &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");
    assert!(all.contains("router"), "{:?}", rows);
    assert!(!all.contains("instance"), "{:?}", rows);

    // First sample: no per-core values yet, the heatmap shows `cores n/a`.
    let mut app = demo_app(DemoState::Generating);
    if let Some(sys) = &mut app.system {
        sys.cpu.per_core_pct.clear();
    }
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    assert!(rows.join("\n").contains("cores n/a"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Bounds

#[test]
fn system_small() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);

    for (w, h) in [(60u16, 10u16), (20u16, 4u16)] {
        // Rendered at an offset, nothing must spill past the area.
        let mut buf = Buffer::empty(Rect::new(0, 0, w + 4, h + 4));
        let area = Rect::new(2, 2, w, h);
        system::render(&app, &ctx, area, &mut buf);
        for y in 0..(h + 4) {
            for x in 0..(w + 4) {
                let inside = (2..2 + w).contains(&x) && (2..2 + h).contains(&y);
                if !inside {
                    assert_eq!(buf[(x, y)].symbol(), " ", "cell ({x},{y}) outside the area");
                }
            }
        }
    }

    // 60 × 10 still shows the first core's value.
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 60, 10);
    assert!(rows.join("\n").contains("44"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Golden files

#[test]
fn system_golden() {
    let theme = Theme::new(ColorMode::TrueColor);
    for (w, h) in [(140u16, 40u16), (100, 30)] {
        let mut app = demo_app(DemoState::Generating);
        app.screen = Screen::System;
        let path = format!("tests/snapshots/system-{w}x{h}.txt");
        let fresh = snapshot(&app, &theme, w, h, false);
        if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
            std::fs::write(&path, &fresh).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap();
        assert_eq!(expected, fresh, "golden file {path} is stale");
    }
}
