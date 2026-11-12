// Cell-by-cell tests for the extra GPU and Memory detail rows, shown only when the panel is tall
// enough (T21). The model has no visual access, so every panel is rendered into a `Buffer` and checked
// row by row, with the same helpers as `tests/panels_model.rs`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::ui::panels::{Ctx, gpu, memory};
use llamatop::ui::theme::{ColorMode, Theme};

/// Render `f` into a `w × h` buffer and return each row's symbols concatenated.
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

// --------------------------------------------------------------------------- GPU details

#[test]
fn gpu_details_when_tall() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    // Inner height 14 (>= 10): the two detail rows render, the graph keeps the rows left of them.
    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 70, 16);
    let all: String = rows.join("\n");
    assert!(all.contains("volt  917 mV   GTT 175 M"), "{:?}", rows);
    assert!(all.contains("link  16.0 GT/s PCIe x16"), "{:?}", rows);
    // The last inner row still holds the braille load graph.
    let last_inner = rows.len() - 2;
    assert!(
        rows[last_inner].chars().any(|c| (0x2800..=0x28ff).contains(&(c as u32))),
        "{:?}",
        rows[last_inner]
    );
}

#[test]
fn gpu_no_details_when_short() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    // Inner height 9 (< 10): no detail row.
    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 70, 11);
    let all: String = rows.join("\n");
    assert!(!all.contains("clock"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Memory details

#[test]
fn memory_details_when_tall() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    // Inner height 6: the two detail rows render.
    let rows = draw(|area, buf| memory::render(&app, &ctx, area, buf), 40, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("avail 9.5 G   cached 18.2 G"), "{:?}", rows);
    assert!(all.contains("router 287 M"), "{:?}", rows);

    // Inner height 3: neither detail row.
    let rows = draw(|area, buf| memory::render(&app, &ctx, area, buf), 40, 5);
    let all: String = rows.join("\n");
    assert!(!all.contains("avail"), "{:?}", rows);
    assert!(!all.contains("router"), "{:?}", rows);
}

#[test]
fn memory_down_has_no_router_row() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Down);
    // Row 3 is the router itself, so there is no separate router row.
    let rows = draw(|area, buf| memory::render(&app, &ctx, area, buf), 40, 8);
    let all: String = rows.join("\n");
    assert!(all.contains("server 287 M"), "{:?}", rows);
    assert!(!rows.iter().any(|r| r.starts_with("│router")), "{:?}", rows);
}
