//! Cell-by-cell tests for the Agents activity map (T43). The model has no visual access, so the map
//! is rendered into a `Buffer` and checked row by row; the golden file captures the whole frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use huacaya::collect::activity::Activity;
use huacaya::collect::opencode::{Step, StepKind, parse_steps};
use huacaya::ui::dots::Dots;
use huacaya::ui::map::{Core, render, station_cells};
use huacaya::ui::theme::{ColorMode, PCT_THRESHOLDS, Role, Theme, gauge_color, lerp_rgb, rgb};

// --- Builders (the sheet forbids these in `src/`) -----------------------------------------------

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/opencode/{name}")).expect("fixture present")
}

fn step(
    id: &str,
    kind: StepKind,
    summary: &str,
    time_ms: i64,
    end_ms: Option<i64>,
    ok: Option<bool>,
) -> Step {
    Step { time_ms, kind, id: id.to_string(), end_ms, summary: summary.to_string(), ok, detail: None }
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

fn nc_theme() -> Theme {
    Theme::new(ColorMode::NoColor)
}

/// The test `Activity`: the real history folded in, then a live round where a shell step starts.
fn activity() -> Activity {
    let steps = parse_steps(&fixture("messages-real.json"), Some("/home/dev/work/.wt/llamatop-145")).unwrap();
    let mut a = Activity::default();
    a.update("s1", &steps, 1794586720000);
    a.update("s1", &[step("x1", StepKind::Shell, "cargo test", 1794586721000, None, None)], 1794586722000);
    a
}

fn core() -> Core {
    Core { model: "Coder-35B".into(), tps: Some(62.5), ctx_ratio: Some(0.617) }
}

const NOW: i64 = 1794586722300;

/// Render the map into a `Buffer` so its cell styles can be inspected.
fn render_buf(theme: &Theme, w: u16, h: u16, now_ms: i64) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    render(&activity(), &core(), now_ms, theme, Rect::new(0, 0, w, h), &mut buf);
    buf
}

/// Render the map and return each row's symbols (trailing spaces removed), one line per row.
fn map_rows(theme: &Theme, w: u16, h: u16, now_ms: i64) -> Vec<String> {
    let buf = render_buf(theme, w, h, now_ms);
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string())
        .collect()
}

// --------------------------------------------------------------------------- Dots::new / plot

#[test]
fn braille_bits() {
    // Render one cell so we can read the braille symbol after each plot.
    fn symbol(d: &Dots) -> String {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        d.render(Rect::new(0, 0, 1, 1), &mut buf);
        buf[(0, 0)].symbol().to_string()
    }

    let mut d = Dots::new(1, 1);
    d.plot(0, 0, None, 0);
    assert_eq!(symbol(&d), "⠁");

    d.plot(1, 0, None, 0);
    assert_eq!(symbol(&d), "⠉");

    let mut d = Dots::new(1, 1);
    d.plot(0, 3, None, 0);
    assert_eq!(symbol(&d), "⡀");

    let mut d = Dots::new(1, 1);
    d.plot(1, 3, None, 0);
    assert_eq!(symbol(&d), "⢀");

    // Outside the canvas: nothing changes.
    let mut d = Dots::new(1, 1);
    d.plot(2, 0, None, 0);
    d.plot(-1, 0, None, 0);
    assert_eq!(symbol(&d), " ");
}

#[test]
fn dots_priority() {
    let mut d = Dots::new(1, 1);
    let red = Color::Rgb(255, 0, 0);
    let blue = Color::Rgb(0, 0, 255);
    let green = Color::Rgb(0, 255, 0);
    d.plot(0, 0, Some(red), 5);
    d.plot(0, 0, Some(blue), 2);
    let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
    d.render(Rect::new(0, 0, 1, 1), &mut buf);
    assert_eq!(buf[(0, 0)].style().fg, Some(red));

    d.plot(0, 0, Some(green), 5);
    let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
    d.render(Rect::new(0, 0, 1, 1), &mut buf);
    assert_eq!(buf[(0, 0)].style().fg, Some(green));
}

// --------------------------------------------------------------------------- station_cells

#[test]
fn cells_86x23() {
    assert_eq!(
        station_cells(Rect::new(0, 0, 86, 23)),
        [(43, 2), (63, 4), (71, 11), (63, 17), (43, 20), (23, 17), (15, 11), (23, 4)]
    );
}

// --------------------------------------------------------------------------- Labels + core

#[test]
fn labels_and_core() {
    let rows = map_rows(&true_color(), 86, 23, NOW);
    let all: String = rows.join("\n");
    for s in [
        "Model 14",
        "Files 5",
        "Tools 1",
        "Checks 2 ◷ 0:01",
        "Web 0",
        "Agents 0",
        "You 0",
        "Coder-35B",
        "62.5 t/s",
        "ctx 62%",
    ] {
        assert!(all.contains(s), "missing {:?} in:\n{}", s, rows.join("\n"));
    }
}

// --------------------------------------------------------------------------- Node glyphs

#[test]
fn nodes() {
    let theme = true_color();
    let buf = render_buf(&theme, 86, 23, NOW);
    assert_eq!(buf[(43, 20)].symbol(), "◉");
    assert_eq!(buf[(71, 11)].symbol(), "●");
    assert_eq!(buf[(23, 17)].symbol(), "○");
}

// --------------------------------------------------------------------------- The pulse

#[test]
fn pulse_head() {
    let theme = true_color();
    let buf = render_buf(&theme, 86, 23, NOW);
    let has_white = (13..=19).any(|y| buf[(43, y)].style().fg == Some(Color::Rgb(255, 255, 255)));
    assert!(has_white, "no white pulse head in column 43, rows 13-19:\n{:?}", map_rows(&theme, 86, 23, NOW));

    let buf = render_buf(&theme, 86, 23, 1794586722700);
    let any_white =
        (0..23).any(|y| (0..86).any(|x| buf[(x, y)].style().fg == Some(Color::Rgb(255, 255, 255))));
    assert!(!any_white, "the pulse is over at 1794586722700:\n{:?}", map_rows(&theme, 86, 23, 1794586722700));
}

// --------------------------------------------------------------------------- The context ring

#[test]
fn ring_lit() {
    let theme = true_color();
    let lit = gauge_color(&theme, Role::Good, 61.7, PCT_THRESHOLDS).unwrap();
    let buf = render_buf(&theme, 86, 23, NOW);
    let has_lit = (0..23).any(|y| (0..86).any(|x| buf[(x, y)].style().fg == Some(lit)));
    assert!(has_lit, "no lit ring cell:\n{:?}", map_rows(&theme, 86, 23, NOW));

    let faint = lerp_rgb((0x1a, 0x1b, 0x26), rgb(Role::Border), 0.7);
    let faint_color = Color::Rgb(faint.0, faint.1, faint.2);
    let has_faint = (0..11).any(|y| (0..86).any(|x| buf[(x, y)].style().fg == Some(faint_color)));
    assert!(has_faint, "no faint ring cell above row 11:\n{:?}", map_rows(&theme, 86, 23, NOW));
}

// --------------------------------------------------------------------------- Too small

#[test]
fn too_small() {
    let theme = true_color();
    for (w, h) in [(59u16, 23), (86, 17)] {
        let rows = map_rows(&theme, w, h, NOW);
        let all: String = rows.iter().flat_map(|r| r.chars()).collect();
        assert!(all.trim().is_empty(), "buffer {}x{} must stay blank:\n{}", w, h, rows.join("\n"));
    }
}

// --------------------------------------------------------------------------- NO_COLOR

#[test]
fn no_color() {
    let rows_true = map_rows(&true_color(), 86, 23, NOW);
    let rows_none = map_rows(&nc_theme(), 86, 23, NOW);
    assert_eq!(rows_true, rows_none, "symbols must match across colour modes");

    let buf = render_buf(&nc_theme(), 86, 23, NOW);
    for y in 0..23u16 {
        for x in 0..86u16 {
            assert_eq!(
                buf[(x, y)].style().fg,
                Some(Color::Reset),
                "cell ({}, {}) has a foreground colour in NoColor: {:?}",
                x,
                y,
                buf[(x, y)].style().fg
            );
        }
    }
}

// --------------------------------------------------------------------------- Golden file

#[test]
fn map_golden() {
    let path = "tests/snapshots/map-86x23.txt";
    let content = map_rows(&true_color(), 86, 23, NOW).join("\n") + "\n";
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(path, content).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap();
    assert_eq!(expected, content, "golden file {path} is stale");
}

// --------------------------------------------------------------------------- Offset area

#[test]
fn offset_area() {
    // Drawn at (5, 3) inside a larger buffer, the map must be the (0, 0) map shifted, nothing clipped.
    let theme = true_color();
    let origin = map_rows(&theme, 86, 23, NOW);
    let mut buf = Buffer::empty(Rect::new(0, 0, 91, 26));
    render(&activity(), &core(), NOW, &theme, Rect::new(5, 3, 86, 23), &mut buf);
    let shifted: Vec<String> = (3..26u16)
        .map(|y| {
            (5..91u16).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string()
        })
        .collect();
    assert_eq!(origin, shifted);

    // An area that overflows the buffer is clipped, never a panic.
    let mut small = Buffer::empty(Rect::new(0, 0, 40, 10));
    render(&activity(), &core(), NOW, &theme, Rect::new(5, 3, 86, 23), &mut small);
}

/// A real model name is longer than the ring: it loses its trailing segments (the family name stays), never
/// spills out. At 86 × 23 the ring holds 15 characters: `Cyber-Tiel`.
#[test]
fn long_model_name_fits_the_ring() {
    let core = Core { model: "Cyber-Tiel-Coder-35B-A3B".into(), tps: Some(62.5), ctx_ratio: Some(0.617) };
    let mut buf = Buffer::empty(Rect::new(0, 0, 86, 23));
    render(&activity(), &core, 1794586722300, &true_color(), Rect::new(0, 0, 86, 23), &mut buf);
    let row: String = (0..86).map(|x| buf[(x, 10)].symbol().to_string()).collect();
    assert!(row.contains("Cyber-Tiel"), "row: {row}");
    assert!(!row.contains("Coder"), "row: {row}");
}
