// The responsive layout, the Overview screen, the compact tier, `--snapshot` and the golden files. The
// model has no visual access, so every frame is rendered into a `Buffer` and read row by row.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::App;
use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::ui::panels::Ctx;
use llamatop::ui::snapshot::{parse_size, snapshot};
use llamatop::ui::theme::{ColorMode, Theme};

/// Render `f` into a `w × h` buffer, return each row's symbols with trailing spaces removed.
fn rows<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(*buf.area(), &mut buf);
    (0..buf.area().height)
        .map(|y| (0..buf.area().width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
        .map(|s| s.trim_end().to_string())
        .collect()
}

/// Rect `inner` lies fully inside `outer` (top-left inclusive, bottom-right exclusive).
fn inside(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && inner.x + inner.width <= outer.x + outer.width
        && outer.y <= inner.y
        && inner.y + inner.height <= outer.y + outer.height
}

// --------------------------------------------------------------------------- Tier

#[test]
fn tiers() {
    assert_eq!(llamatop::ui::layout::tier(29, 40), llamatop::ui::layout::Tier::TooSmall);
    assert_eq!(llamatop::ui::layout::tier(30, 7), llamatop::ui::layout::Tier::TooSmall);
    assert_eq!(llamatop::ui::layout::tier(30, 8), llamatop::ui::layout::Tier::Compact);
    assert_eq!(llamatop::ui::layout::tier(59, 40), llamatop::ui::layout::Tier::Compact);
    assert_eq!(llamatop::ui::layout::tier(120, 19), llamatop::ui::layout::Tier::Compact);
    assert_eq!(llamatop::ui::layout::tier(60, 20), llamatop::ui::layout::Tier::Narrow);
    assert_eq!(llamatop::ui::layout::tier(99, 50), llamatop::ui::layout::Tier::Narrow);
    assert_eq!(llamatop::ui::layout::tier(100, 30), llamatop::ui::layout::Tier::Medium);
    assert_eq!(llamatop::ui::layout::tier(160, 39), llamatop::ui::layout::Tier::Medium);
    assert_eq!(llamatop::ui::layout::tier(140, 40), llamatop::ui::layout::Tier::Wide);
}

// --------------------------------------------------------------------------- Overview geometry

#[test]
fn wide_layout() {
    let l = llamatop::ui::layout::overview(Rect::new(0, 0, 140, 40));
    assert_eq!(l.header, Rect::new(0, 0, 140, 3));
    assert_eq!(l.model, Some(Rect::new(0, 3, 70, 8)));
    assert_eq!(l.request, Some(Rect::new(0, 11, 70, 5)));
    assert_eq!(l.log, Some(Rect::new(0, 16, 70, 24)));
    assert_eq!(l.gpu, Some(Rect::new(70, 3, 70, 20)));
    assert_eq!(l.cpu, Some(Rect::new(70, 23, 35, 17)));
    assert_eq!(l.memory, Some(Rect::new(105, 23, 35, 17)));
    assert!(l.graphs);
}

#[test]
fn medium_layout() {
    let l = llamatop::ui::layout::overview(Rect::new(0, 0, 100, 30));
    assert_eq!(l.model, Some(Rect::new(0, 3, 50, 8)));
    assert_eq!(l.gpu, Some(Rect::new(50, 3, 50, 8)));
    assert_eq!(l.cpu, Some(Rect::new(0, 11, 50, 6)));
    assert_eq!(l.memory, Some(Rect::new(50, 11, 50, 6)));
    assert_eq!(l.log, Some(Rect::new(0, 17, 100, 13)));
    assert_eq!(l.request, None);
}

#[test]
fn narrow_layout() {
    // (80, 24): the log would get 2 rows, too few, so the cpu | memory band is dropped.
    let l = llamatop::ui::layout::overview(Rect::new(0, 0, 80, 24));
    assert_eq!(l.model, Some(Rect::new(0, 3, 80, 8)));
    assert_eq!(l.gpu, Some(Rect::new(0, 11, 80, 6)));
    assert_eq!(l.cpu, None);
    assert_eq!(l.memory, None);
    assert_eq!(l.log, Some(Rect::new(0, 17, 80, 7)));
    assert!(!l.graphs);

    // (80, 30): the band fits, the log gets 8 rows.
    let l = llamatop::ui::layout::overview(Rect::new(0, 0, 80, 30));
    assert_eq!(l.cpu, Some(Rect::new(0, 17, 40, 5)));
    assert_eq!(l.memory, Some(Rect::new(40, 17, 40, 5)));
    assert_eq!(l.log, Some(Rect::new(0, 22, 80, 8)));
}

// --------------------------------------------------------------------------- Invariants

#[test]
fn layout_invariants() {
    for w in (30..=200).step_by(5) {
        for h in (20..=60).step_by(4) {
            let area = Rect::new(0, 0, w, h);
            // Only the real tiers; Compact and TooSmall fall back to the Narrow layout on a tiny body.
            if !matches!(
                llamatop::ui::layout::tier(w, h),
                llamatop::ui::layout::Tier::Narrow
                    | llamatop::ui::layout::Tier::Medium
                    | llamatop::ui::layout::Tier::Wide
            ) {
                continue;
            }
            let l = llamatop::ui::layout::overview(area);
            let present: Vec<Rect> =
                [l.model, l.request, l.log, l.gpu, l.cpu, l.memory].into_iter().flatten().collect();

            // Every present rectangle lies inside the area.
            for r in &present {
                assert!(inside(area, *r), "{r:?} outside {area:?}");
            }
            // No two present rectangles overlap.
            for a in &present {
                for b in &present {
                    if a != b {
                        assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
                    }
                }
            }
            // Model and GPU are never dropped; a present log has at least 3 rows; present panels keep
            // their minimum height.
            assert!(l.model.is_some(), "model dropped at {w}x{h}");
            assert!(l.gpu.is_some(), "gpu dropped at {w}x{h}");
            if let Some(r) = l.model {
                assert!(r.height >= llamatop::ui::panels::model::MIN_HEIGHT);
            }
            if let Some(r) = l.gpu {
                assert!(r.height >= llamatop::ui::panels::gpu::MIN_HEIGHT);
            }
            if let Some(r) = l.request {
                assert!(r.height >= llamatop::ui::panels::request::MIN_HEIGHT);
            }
            if let Some(r) = l.cpu {
                assert!(r.height >= llamatop::ui::panels::cpu::MIN_HEIGHT);
            }
            if let Some(r) = l.memory {
                assert!(r.height >= llamatop::ui::panels::memory::MIN_HEIGHT);
            }
            if let Some(r) = l.log {
                assert!(r.height >= llamatop::ui::panels::log::MIN_HEIGHT);
            }
        }
    }
}

// --------------------------------------------------------------------------- draw

#[test]
fn draw_never_panics() {
    let theme = Theme::new(ColorMode::TrueColor);
    let apps = [demo_app(DemoState::Generating), App::new(1000)];
    for app in apps {
        for w in (0..=160).step_by(7) {
            for h in (0..=50).step_by(3) {
                let ctx = Ctx { theme: &theme, graphs: true, clock: "10:21:03" };
                llamatop::ui::draw::draw(
                    &app,
                    &ctx,
                    Rect::new(0, 0, w, h),
                    &mut Buffer::empty(Rect::new(0, 0, w, h)),
                );
            }
        }
    }
}

#[test]
fn too_small_message() {
    let theme = Theme::new(ColorMode::TrueColor);
    let ctx = Ctx { theme: &theme, graphs: true, clock: "10:21:03" };
    let rows =
        rows(|area, buf| llamatop::ui::draw::draw(&demo_app(DemoState::Generating), &ctx, area, buf), 20, 6);
    assert!(rows.iter().any(|r| r.contains("terminal too small")));
    assert!(rows.iter().any(|r| r.contains("20×6, need 30×8")));
}

// --------------------------------------------------------------------------- Compact

#[test]
fn compact_rows() {
    let theme = Theme::new(ColorMode::TrueColor);
    let ctx = Ctx { theme: &theme, graphs: true, clock: "10:21:03" };
    let rows =
        rows(|area, buf| llamatop::ui::draw::draw(&demo_app(DemoState::Generating), &ctx, area, buf), 50, 15);
    assert!(rows[0].contains("llamatop · aibox-flo · ● server ok"));
    assert!(rows[1].contains("GENERATING"));
    assert!(rows[1].contains("t/s"));
    assert!(rows[2].contains("GPU 95% 73°C 15.9 G │ CPU 50% │ RAM 21.6 G"));
    assert!(rows.last().unwrap().contains("all slots are idle"));
}

// --------------------------------------------------------------------------- parse_size

#[test]
fn parse_sizes() {
    assert_eq!(parse_size("140x40"), Some((140, 40)));
    assert_eq!(parse_size("80X24"), Some((80, 24)));
    assert_eq!(parse_size("0x10"), None);
    assert_eq!(parse_size("10"), None);
    assert_eq!(parse_size("axb"), None);
    assert_eq!(parse_size("2000x10"), None);
}

// --------------------------------------------------------------------------- snapshot colours

#[test]
fn ansi_snapshot() {
    let app = demo_app(DemoState::Generating);
    let theme = Theme::new(ColorMode::TrueColor);
    let ansi = snapshot(&app, &theme, 100, 30, true);
    // The Server accent is #ff9e64 = Rgb(255, 158, 100); the reset is always present.
    assert!(ansi.contains("\x1b[38;2;255;158;100m"));
    assert!(ansi.contains("\x1b[0m"));
    let plain = snapshot(&app, &theme, 100, 30, false);
    assert!(!plain.contains("\x1b"));
}

#[test]
fn ansi_snapshot_ansi16() {
    let app = demo_app(DemoState::Generating);
    let theme = Theme::new(ColorMode::Ansi16);
    let ansi = snapshot(&app, &theme, 100, 30, true);
    // Server = Yellow (33), Border = DarkGray (90): named colours map to 30–37 / 90–97.
    assert!(ansi.contains("\x1b[0m\x1b[33m"));
    assert!(ansi.contains("\x1b[0m\x1b[90m"));
}

#[test]
fn ansi_sgr_params_are_separated() {
    let app = demo_app(DemoState::Generating);
    let theme = Theme::new(ColorMode::TrueColor);
    let ansi = snapshot(&app, &theme, 100, 30, true);
    // Bold + reversed must never be glued into the invalid `17`.
    assert!(!ansi.contains("\x1b[17"));
    assert!(ansi.contains("\x1b[0m\x1b[1;"));
}

// --------------------------------------------------------------------------- Golden files

#[test]
fn golden_files() {
    let app = demo_app(DemoState::Generating);
    let theme = Theme::new(ColorMode::TrueColor);
    for (w, h) in [(140u16, 40u16), (100, 30), (80, 24), (50, 15), (20, 6)] {
        let path = format!("tests/snapshots/overview-{w}x{h}.txt");
        let fresh = snapshot(&app, &theme, w, h, false);
        if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
            std::fs::write(&path, &fresh).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap();
        assert_eq!(expected, fresh, "golden file {path} is stale");
    }
}
