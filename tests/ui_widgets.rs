// Cell-by-cell tests for the theme and dashboard widgets. The model has no visual access, so the
// buffer is the only eye: every widget is rendered into a `Buffer` and checked cell by cell.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Widget;

use llamatop::ui::frame::{badge, panel};
use llamatop::ui::gauge::{Gauge, bar_cells};
use llamatop::ui::graph::{BrailleGraph, braille_rows};
use llamatop::ui::theme::*;

/// Each row's symbols concatenated.
fn lines(buf: &Buffer) -> Vec<String> {
    let area = buf.area();
    (0..area.height).map(|y| (0..area.width).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

// --------------------------------------------------------------------------- Theme

#[test]
fn mode_from_env() {
    assert_eq!(ColorMode::from_env_values(Some("1"), Some("truecolor")), ColorMode::NoColor);
    assert_eq!(ColorMode::from_env_values(Some(""), Some("truecolor")), ColorMode::TrueColor);
    assert_eq!(ColorMode::from_env_values(None, Some("24bit")), ColorMode::TrueColor);
    assert_eq!(ColorMode::from_env_values(None, Some("TrueColor")), ColorMode::TrueColor);
    assert_eq!(ColorMode::from_env_values(None, Some("xterm")), ColorMode::Ansi16);
    assert_eq!(ColorMode::from_env_values(None, None), ColorMode::Ansi16);
}

#[test]
fn palette_by_mode() {
    assert_eq!(rgb(Role::Gpu), (0xbb, 0x9a, 0xf7));
    let t = Theme::new(ColorMode::TrueColor);
    let a = Theme::new(ColorMode::Ansi16);
    let n = Theme::new(ColorMode::NoColor);
    assert_eq!(t.color(Role::Critical), Some(Color::Rgb(0xf7, 0x76, 0x8e)));
    assert_eq!(a.color(Role::Gpu), Some(Color::Magenta));
    assert_eq!(a.color(Role::Warning), Some(Color::LightYellow));
    assert_eq!(n.color(Role::Gpu), None);
    assert_eq!(n.fg(Role::Gpu), Style::default());
    assert_eq!(t.fg(Role::Cpu), Style::default().fg(Color::Rgb(0x7a, 0xa2, 0xf7)));
}

#[test]
fn severity_and_mark() {
    assert_eq!(severity(74.9, 75.0, 90.0), Severity::Normal);
    assert_eq!(severity(75.0, 75.0, 90.0), Severity::Warning);
    assert_eq!(severity(89.9, 75.0, 90.0), Severity::Warning);
    assert_eq!(severity(90.0, 75.0, 90.0), Severity::Critical);
    assert_eq!(severity(f32::NAN, 75.0, 90.0), Severity::Normal);
    assert_eq!(hot_mark(Severity::Critical), "▲");
    assert_eq!(hot_mark(Severity::Warning), "▲");
    assert_eq!(hot_mark(Severity::Normal), "");
}

#[test]
fn lerp_clamps_and_rounds() {
    assert_eq!(lerp_rgb((0, 0, 0), (255, 255, 255), 0.5), (128, 128, 128));
    assert_eq!(lerp_rgb((0, 0, 0), (255, 255, 255), -1.0), (0, 0, 0));
    assert_eq!(lerp_rgb((0, 0, 0), (255, 255, 255), 2.0), (255, 255, 255));
}

#[test]
fn gauge_colour_zones() {
    let t = Theme::new(ColorMode::TrueColor);
    let a = Theme::new(ColorMode::Ansi16);
    let n = Theme::new(ColorMode::NoColor);
    let accent = Role::Gpu;
    let th = PCT_THRESHOLDS;
    assert_eq!(gauge_color(&t, accent, 50.0, th), Some(Color::Rgb(0xbb, 0x9a, 0xf7)));
    assert_eq!(gauge_color(&t, accent, 82.5, th), Some(Color::Rgb(206, 165, 176)));
    assert_eq!(gauge_color(&t, accent, 95.0, th), Some(Color::Rgb(0xf7, 0x76, 0x8e)));
    assert_eq!(gauge_color(&a, accent, 50.0, th), Some(Color::Magenta));
    assert_eq!(gauge_color(&a, accent, 82.5, th), Some(Color::LightYellow));
    assert_eq!(gauge_color(&a, accent, 95.0, th), Some(Color::Red));
    assert_eq!(gauge_color(&n, accent, 95.0, th), None);
}

// --------------------------------------------------------------------------- Gauge

#[test]
fn bar_cells_values() {
    assert_eq!(bar_cells(0.5, 10), "█████░░░░░");
    assert_eq!(bar_cells(0.97, 20), "███████████████████▍");
    assert_eq!(bar_cells(0.06, 10), "▋░░░░░░░░░");
    assert_eq!(bar_cells(0.0, 5), "░░░░░");
    assert_eq!(bar_cells(1.5, 4), "████");
    assert_eq!(bar_cells(f64::NAN, 3), "░░░");
    assert_eq!(bar_cells(0.5, 0), "");
}

#[test]
fn gauge_renders_first_row() {
    let t = Theme::new(ColorMode::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    let gauge = Gauge { ratio: 0.5, accent: Role::Cpu, thresholds: PCT_THRESHOLDS, theme: &t };
    gauge.render(*buf.area(), &mut buf);
    assert_eq!(lines(&buf), ["█████░░░░░", "          "]);
    assert_eq!(buf[(0, 0)].fg, Color::Rgb(0x7a, 0xa2, 0xf7));
    assert_eq!(buf[(9, 0)].fg, Color::Rgb(0x56, 0x5f, 0x89));
}

#[test]
fn gauge_colours_past_thresholds() {
    let t = Theme::new(ColorMode::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
    let gauge = Gauge { ratio: 1.0, accent: Role::Gpu, thresholds: PCT_THRESHOLDS, theme: &t };
    gauge.render(*buf.area(), &mut buf);
    assert_eq!(buf[(9, 0)].fg, Color::Rgb(0xf7, 0x76, 0x8e));
    assert_eq!(buf[(0, 0)].fg, Color::Rgb(0xbb, 0x9a, 0xf7));
}

#[test]
fn gauge_without_colour() {
    let n = Theme::new(ColorMode::NoColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    let gauge = Gauge { ratio: 0.5, accent: Role::Cpu, thresholds: PCT_THRESHOLDS, theme: &n };
    gauge.render(*buf.area(), &mut buf);
    assert_eq!(lines(&buf), ["█████░░░░░", "          "]);
    assert_eq!(buf[(0, 0)].fg, Color::Reset);
}

// --------------------------------------------------------------------------- Graph

#[test]
fn braille_one_row() {
    // A 0 sample shows the baseline dot; the slot with no sample yet stays blank.
    assert_eq!(braille_rows(&[0.0, 1.0, 2.0, 3.0, 4.0], 4.0, 3, 1), ["⢀⣠⣾".to_string()]);
    assert_eq!(braille_rows(&[1.0, 2.0, 3.0, 4.0], 4.0, 3, 1), [" ⣠⣾".to_string()]);
}

#[test]
fn braille_two_rows() {
    assert_eq!(braille_rows(&[8.0, 4.0, 1.0, 0.0], 8.0, 2, 2), ["⡇ ".to_string(), "⣿⣀".to_string()]);
}

#[test]
fn braille_small_and_clamped() {
    assert_eq!(braille_rows(&[0.1], 100.0, 1, 1), ["⢀".to_string()]);
    assert_eq!(braille_rows(&[20.0], 10.0, 1, 1), ["⢸".to_string()]);
}

#[test]
fn braille_degenerate() {
    assert_eq!(braille_rows(&[5.0], 0.0, 2, 1), ["  ".to_string()]);
    assert_eq!(braille_rows(&[f32::NAN, -3.0], 10.0, 1, 1), [" ".to_string()]);
    assert_eq!(braille_rows(&[1.0], 1.0, 0, 3), Vec::<String>::new());
}

#[test]
fn graph_renders_with_accent() {
    let t = Theme::new(ColorMode::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 3, 1));
    let graph = BrailleGraph { values: &[1.0, 2.0, 3.0, 4.0], max: 4.0, accent: Role::Gpu, theme: &t };
    graph.render(*buf.area(), &mut buf);
    assert_eq!(lines(&buf), [" ⣠⣾".to_string()]);
    assert_eq!(buf[(1, 0)].fg, Color::Rgb(0xbb, 0x9a, 0xf7));
    assert_eq!(buf[(0, 0)].fg, Color::Reset);
}

// --------------------------------------------------------------------------- Frame

#[test]
fn panel_rounded_frame() {
    let t = Theme::new(ColorMode::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
    let block = panel("GPU", Role::Gpu, false, &t);
    block.render(*buf.area(), &mut buf);
    assert_eq!(lines(&buf), ["╭ GPU ─────────────╮", "│                  │", "╰──────────────────╯"]);
    assert_eq!(buf[(0, 0)].fg, Color::Rgb(0x54, 0x5c, 0x7e));
    assert_eq!(buf[(2, 0)].fg, Color::Rgb(0xbb, 0x9a, 0xf7));
    assert!(buf[(2, 0)].modifier.contains(Modifier::BOLD));
}

#[test]
fn panel_focused() {
    let t = Theme::new(ColorMode::TrueColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
    let block = panel("GPU", Role::Gpu, true, &t);
    block.render(*buf.area(), &mut buf);
    assert_eq!(buf[(0, 0)].fg, Color::Rgb(0x7a, 0xa2, 0xf7));
}

#[test]
fn badge_states() {
    let t = Theme::new(ColorMode::TrueColor);
    let n = Theme::new(ColorMode::NoColor);
    let lit: Span = badge("GENERATING", Role::Server, &t, true);
    assert_eq!(&*lit.content, "◉ GENERATING");
    assert_eq!(lit.style, Style::default().fg(Color::Rgb(0xff, 0x9e, 0x64)).add_modifier(Modifier::BOLD));
    let dim: Span = badge("GENERATING", Role::Server, &t, false);
    assert_eq!(&*dim.content, "○ GENERATING");
    let none: Span = badge("GENERATING", Role::Server, &n, true);
    assert_eq!(none.style, Style::default().add_modifier(Modifier::BOLD));
}
