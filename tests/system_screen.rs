// Cell-by-cell tests for the System screen (T18). The model has no visual access, so every screen is
// rendered into a `Buffer` and checked row by row; the golden files capture the whole frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, Screen};
use llamatop::ui::panels::Ctx;
use llamatop::ui::screens::system;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::theme::{ColorMode, Theme};

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

// --------------------------------------------------------------------------- sensor_thresholds()

#[test]
fn thresholds_by_chip() {
    assert_eq!(system::sensor_thresholds("amdgpu"), (85.0, 95.0));
    assert_eq!(system::sensor_thresholds("coretemp"), (80.0, 95.0));
    assert_eq!(system::sensor_thresholds("k10temp"), (80.0, 95.0));
    assert_eq!(system::sensor_thresholds("nvme"), (70.0, 80.0));
    assert_eq!(system::sensor_thresholds("acpitz"), (80.0, 95.0));
}

// --------------------------------------------------------------------------- Cores

#[test]
fn cores_one_per_row() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");

    assert!(all.contains("cpu0"), "{:?}", rows);
    assert!(all.contains("cpu15"), "{:?}", rows);
    assert!(all.contains("44 %"), "{:?}", rows);
    assert!(all.contains("56 %"), "{:?}", rows);
    assert!(all.contains("freq   3.5 GHz avg   4.7 GHz max"), "{:?}", rows);
    assert!(all.contains("load   8.33 7.14 5.56"), "{:?}", rows);
    assert!(all.contains("uptime 4d 3h"), "{:?}", rows);

    // cpu0 and cpu1 are on different rows (one per row).
    let row0 = rows.iter().find(|r| r.contains("cpu0")).unwrap();
    let row1 = rows.iter().find(|r| r.contains("cpu1")).unwrap();
    assert_ne!(row0, row1, "cpu0 and cpu1 should be on different rows");
}

#[test]
fn cores_two_per_row() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 15);

    // At 15 rows the cores do not fit one per row, so cpu0 and cpu1 share a row.
    let row0 = rows.iter().find(|r| r.contains("cpu0")).unwrap();
    let row1 = rows.iter().find(|r| r.contains("cpu1")).unwrap();
    assert_eq!(row0, row1, "cpu0 and cpu1 should be on the same row");
}

// --------------------------------------------------------------------------- Sensors

#[test]
fn sensors_and_marks() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 140, 37);
    let all: String = rows.join("\n");
    assert!(all.contains("coretemp/Package id 0"), "{:?}", rows);
    assert!(all.contains("amdgpu/junction"), "{:?}", rows);
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
        "↓ 2.1 K/s  ↑ 12.7 K/s",
        "total ↓ 187 M  ↑ 188 M",
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

    // 60 × 10 still shows the first core.
    let rows = draw(|area, buf| system::render(&app, &ctx, area, buf), 60, 10);
    assert!(rows.join("\n").contains("cpu0"));
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
