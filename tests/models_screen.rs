// Cell-by-cell tests for the Models screen (T16). The model has no visual access, so every screen is
// rendered into a `Buffer` and checked row by row; the golden files capture the whole frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, Screen};
use llamatop::ui::panels::Ctx;
use llamatop::ui::screens::models;
use llamatop::ui::snapshot::snapshot;
use llamatop::ui::theme::{ColorMode, Theme};

/// Render `models::render` over a `w × h` area and return each row's symbols concatenated.
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

/// An app on the Models screen with the given model selected.
fn models_app(state: DemoState, selected: usize) -> App {
    let mut app = demo_app(state);
    app.screen = Screen::Models;
    app.model_selected = selected;
    app
}

// --------------------------------------------------------------------------- params()

#[test]
fn params_format() {
    assert_eq!(models::params(35_505_251_456), "35.5B");
    assert_eq!(models::params(8_190_000), "8.2M");
    assert_eq!(models::params(1_000_000_000), "1.0B");
}

// --------------------------------------------------------------------------- The table

#[test]
fn table_lists_all_models() {
    let theme = true_color();
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Idle, 0), &make_ctx(&theme), area, buf),
        120,
        26,
    );
    let all: String = rows.join("\n");

    // The header row lists every column label.
    let header = rows.iter().find(|r| r.contains("moe")).unwrap();
    for label in ["state", "model", "quant", "ctx", "train", "moe", "spec", "img"] {
        assert!(header.contains(label), "header missing {label}");
    }

    // Every model appears.
    for name in
        ["Ornith-1.5-35B-A3B", "gsq-qwen3.8-27b", "muse-glimmer-30b", "Cyber-Tiel-Coder-35B-A3B", "Qwen3-8B"]
    {
        assert!(all.contains(name), "missing model {name}");
    }

    // The loaded model is `● loaded` with its spec; the vision model shows `img`.
    let cybertiel = rows.iter().find(|r| r.contains("Cyber-Tiel-Coder-35B-A3B")).unwrap();
    assert!(cybertiel.contains("● loaded"));
    assert!(cybertiel.contains("draft-mtp"));
    assert!(cybertiel.contains("262.1K"));

    let ornith = rows.iter().find(|r| r.contains("Ornith-1.5-35B-A3B")).unwrap();
    assert!(ornith.contains("○ unloaded"));
    assert!(ornith.contains("img"));
}

#[test]
fn selection_marker() {
    let theme = true_color();
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Idle, 3), &make_ctx(&theme), area, buf),
        120,
        26,
    );
    let selected: Vec<_> = rows.iter().filter(|r| r.contains("▶")).collect();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].contains("Cyber-Tiel-Coder-35B-A3B"));
}

// --------------------------------------------------------------------------- The details

#[test]
fn details_of_loaded_model() {
    let theme = true_color();
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Generating, 3), &make_ctx(&theme), area, buf),
        120,
        30,
    );
    let all: String = rows.join("\n");
    // Table right title.
    assert!(all.contains("5 known · 1 loaded · VRAM 15.9/16.0 G"));
    // Identity: id, file (cut), size line.
    // The full id row is 64 columns; Identity's interior is 57 at 120 wide, so it is cut with `…`.
    assert!(all.contains("id      peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q…"));
    assert!(all.contains("/srv/models/direct/Cyber-Tiel-Coder-35B-A3B"));
    assert!(all.contains("size    21.0 G   params 35.5B   type Q4_K - Medium"));
    // Runtime.
    assert!(all.contains("196.6K of 262.1K trained"));
    assert!(all.contains("offload ngl 99   n-cpu-moe 21   threads 8"));
    assert!(all.contains("spec    draft-mtp"));
    assert!(all.contains("port    35335   pid 6873   rss 19.2 G"));
    // Performance.
    assert!(all.contains("since load"));
    assert!(all.contains("gen     56.7 t/s avg"));
    assert!(all.contains("prompt  591 t/s avg"));
    assert!(all.contains("98 % reused"));
    assert!(all.contains("64 % accepted"));
    // Slot.
    assert!(all.contains("Slot 0"));
    assert!(all.contains("◉ generating"));
    assert!(all.contains("63.8K / 196.6K"));
    // Hint row.
    assert!(all.contains("↑↓ select · Home/End"));
}

#[test]
fn details_of_unloaded_model() {
    let theme = true_color();
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Idle, 0), &make_ctx(&theme), area, buf),
        120,
        26,
    );
    let all: String = rows.join("\n");
    assert!(all.contains("loaded models only"));
    // No live metrics when the selected model is not the active one.
    assert!(all.contains("not loaded"));
    assert!(all.contains("no slot"));
    assert!(!all.contains("tokens "));
}

#[test]
fn details_four_panels() {
    let theme = true_color();
    // 120 × 30: the four panels side by side in two columns.
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Generating, 3), &make_ctx(&theme), area, buf),
        120,
        30,
    );
    let all: String = rows.join("\n");
    for title in ["Identity", "Runtime", "Performance", "Slot 0"] {
        assert!(all.contains(title), "missing panel title {title}");
    }
    // Identity and Performance are on the same row (two columns side by side).
    let identity_row = rows.iter().find(|r| r.contains("Identity")).unwrap();
    let performance_row = rows.iter().find(|r| r.contains("Performance")).unwrap();
    assert_eq!(identity_row, performance_row);
}

#[test]
fn details_unloaded_model() {
    let theme = true_color();
    // Generating demo, but `Qwen3-8B` (index 4) is not the active model.
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Generating, 4), &make_ctx(&theme), area, buf),
        120,
        26,
    );
    let all: String = rows.join("\n");
    assert!(all.contains("not loaded"));
    assert!(all.contains("no slot"));
    // No instance process for `Qwen3-8B`: no `port ` row.
    assert!(!all.contains("port "));
}

#[test]
fn details_one_column() {
    let theme = true_color();
    // 70 wide (< 80): the panels stacked, Identity over Runtime over Performance.
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Generating, 3), &make_ctx(&theme), area, buf),
        70,
        40,
    );
    let identity = rows.iter().position(|r| r.contains("Identity")).unwrap();
    let runtime = rows.iter().position(|r| r.contains("Runtime")).unwrap();
    let performance = rows.iter().position(|r| r.contains("Performance")).unwrap();
    assert!(identity < runtime && runtime < performance);
}

#[test]
fn tokens_row() {
    use llamatop::util::count;
    let theme = true_color();
    let app = models_app(DemoState::Generating, 3);
    let llama = app.llama.as_ref().unwrap();
    let model_id = &llama.models[3].id;
    let m = llama.active.iter().find(|a| &a.id == model_id).and_then(|a| a.metrics.as_ref()).unwrap();
    let expected = format!(
        "tokens  {} generated   {} prompted",
        count(m.tokens_predicted_total as u64),
        count(m.prompt_tokens_total as u64)
    );
    let rows = draw(|area, buf| models::render(&app, &make_ctx(&theme), area, buf), 120, 26);
    let all: String = rows.join("\n");
    assert!(all.contains(&expected), "expected tokens row: {expected}");
}

#[test]
fn selected_row_band() {
    // True colour: every cell of the selected row inside the table has the selection background.
    let theme = true_color();
    let mut buf = Buffer::empty(Rect::new(0, 0, 120, 26));
    models::render(&models_app(DemoState::Idle, 3), &make_ctx(&theme), Rect::new(0, 0, 120, 26), &mut buf);
    let sel = (0..26).find(|&y| (1..120).any(|x| buf[(x, y)].symbol() == "▶")).unwrap();
    for x in 1..119 {
        assert_eq!(buf[(x, sel)].style().bg, Some(Color::Rgb(0x28, 0x34, 0x57)), "cell ({x}, {sel})");
    }

    // No colour: every cell of the selected row inside the table has the REVERSED modifier.
    let theme = Theme::new(ColorMode::NoColor);
    let mut buf = Buffer::empty(Rect::new(0, 0, 120, 26));
    models::render(&models_app(DemoState::Idle, 3), &make_ctx(&theme), Rect::new(0, 0, 120, 26), &mut buf);
    let sel = (0..26).find(|&y| (1..120).any(|x| buf[(x, y)].symbol() == "▶")).unwrap();
    for x in 1..119 {
        assert!(buf[(x, sel)].style().add_modifier.contains(Modifier::REVERSED), "cell ({x}, {sel})");
    }
}

// --------------------------------------------------------------------------- Narrowing and the "no model" cases

#[test]
fn narrow_drops_columns() {
    let theme = true_color();
    // At 50 columns the spec, moe and vision columns are dropped; the name column is kept (cut).
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Idle, 0), &make_ctx(&theme), area, buf),
        50,
        20,
    );
    let header = rows.iter().find(|r| r.contains("quant")).unwrap();
    assert!(header.contains("state"));
    assert!(header.contains("model"));
    assert!(header.contains("quant"));
    assert!(header.contains("ctx"));
    assert!(!header.contains("spec"));
    assert!(!header.contains("moe"));
    assert!(!header.contains("img"));
    // `train` is dropped first (before spec/moe/vision).
    assert!(!header.contains("train"));
    assert!(rows.iter().all(|r| !r.contains("draft-mtp") && !r.contains("img")));
    // The name column is kept (cut), so a model name still appears.
    assert!(rows.iter().any(|r| r.contains("Cyber-Tiel")));
}

#[test]
fn narrow_drops_quant_before_ctx() {
    let theme = true_color();
    // At 42 columns (40 inside the borders) quant no longer fits, ctx still does.
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Idle, 0), &make_ctx(&theme), area, buf),
        42,
        20,
    );
    let header = rows.iter().find(|r| r.contains("model")).unwrap();
    assert!(header.contains("ctx"));
    assert!(!header.contains("quant"));
    assert!(rows.iter().all(|r| !r.contains("Q4_K_M")));
    assert!(rows.iter().any(|r| r.contains("196.6K")));
}

#[test]
fn server_down() {
    let theme = true_color();
    let rows = draw(
        |area, buf| models::render(&models_app(DemoState::Down, 0), &make_ctx(&theme), area, buf),
        120,
        26,
    );
    let all: String = rows.join("\n");
    assert!(all.contains("no model: server down"));
}

#[test]
fn waiting_for_first_poll() {
    let theme = true_color();
    let mut app = models_app(DemoState::Idle, 0);
    app.llama = None;
    let rows = draw(|area, buf| models::render(&app, &make_ctx(&theme), area, buf), 120, 26);
    let all: String = rows.join("\n");
    assert!(all.contains("no model: waiting for the first poll"));
}

// --------------------------------------------------------------------------- Golden files

#[test]
fn models_golden() {
    let theme = Theme::new(ColorMode::TrueColor);
    for (w, h) in [(120u16, 30u16), (80, 24)] {
        let mut app = demo_app(DemoState::Generating);
        app.screen = Screen::Models;
        app.model_selected = 3;
        let path = format!("tests/snapshots/models-{w}x{h}.txt");
        let fresh = snapshot(&app, &theme, w, h, false);
        if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
            std::fs::write(&path, &fresh).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap();
        assert_eq!(expected, fresh, "golden file {path} is stale");
    }
}
