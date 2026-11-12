// Cell-by-cell tests for the machine-side Overview panels: GPU, CPU, Memory and Log (T13). The model
// has no visual access, so every panel is rendered into a `Buffer` and checked row by row.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use llamatop::app::demo::{DemoState, app as demo_app};
use llamatop::app::{App, Event};
use llamatop::collect::logs::parse_line;
use llamatop::ui::big::level_bars;
use llamatop::ui::panels::{Ctx, cpu, gauge_row, gpu, log, memory};
use llamatop::ui::theme::{ColorMode, PCT_THRESHOLDS, Role, Theme};

/// Render `f` into a `w × h` buffer and return each row's symbols concatenated.
fn draw<F>(f: F, w: u16, h: u16) -> Vec<String>
where
    F: Fn(Rect, &mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(Rect::new(0, 0, w, h), &mut buf);
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
}

/// Render `f` into a `w × h` buffer and return the buffer, for the cells' colours.
fn render_into<F>(f: F, w: u16, h: u16) -> Buffer
where
    F: Fn(&mut Buffer),
{
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    f(&mut buf);
    buf
}

fn rows_of(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect())
        .collect()
}

/// The shared `Ctx`: true-colour theme, graphs on, the fixed clock.
fn make_ctx<'a>(theme: &'a Theme) -> Ctx<'a> {
    Ctx { theme, graphs: true, clock: "10:21:03" }
}

fn true_color() -> Theme {
    Theme::new(ColorMode::TrueColor)
}

// --------------------------------------------------------------------------- gauge_row

#[test]
fn gauge_row_layout() {
    let theme = true_color();

    // 30 columns: `load  ` + 18 gauge cells + `  50 %`, cell 6 is filled.
    let rows = draw(
        |_area, buf| gauge_row(buf, 0, 0, 30, "load  ", 0.5, Role::Gpu, PCT_THRESHOLDS, "50 %", &theme),
        30,
        1,
    );
    let chars: Vec<char> = rows[0].chars().collect();
    assert_eq!(chars.len(), 30);
    assert_eq!(&chars[0..6].iter().collect::<String>(), "load  ");
    assert_eq!(chars[6], '█');
    assert_eq!(&chars[24..26].iter().collect::<String>(), "  ");
    assert_eq!(&chars[26..30].iter().collect::<String>(), "50 %");

    // 12 columns: fewer than 4 left for the gauge, so it is omitted.
    let rows = draw(
        |_area, buf| gauge_row(buf, 0, 0, 12, "load  ", 0.5, Role::Gpu, PCT_THRESHOLDS, "50 %", &theme),
        12,
        1,
    );
    let chars: Vec<char> = rows[0].chars().collect();
    assert_eq!(&chars[0..10].iter().collect::<String>(), "load  50 %");
    assert!(chars[10..].iter().all(|c| *c == ' '));
}

// --------------------------------------------------------------------------- GPU

#[test]
fn gpu_generating() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 50, 9);
    let all: String = rows.join("\n");
    assert!(all.contains("GPU  RX 7800 XT"), "{:?}", rows[0]);
    assert!(all.contains("95 %"), "{:?}", rows);
    assert!(all.contains("15.9/16.0 G"), "{:?}", rows);
    assert!(all.contains("temp  73 °C junction  62 °C edge  76 °C mem"), "{:?}", rows);
    assert!(all.contains("power 117 W / 212 W   fan 34 %   clk 2204 MHz"), "{:?}", rows);
    // The last inner row holds the braille load graph.
    let last_inner = rows.len() - 2;
    assert!(
        rows[last_inner].chars().any(|c| (0x2800..=0x28ff).contains(&(c as u32))),
        "{:?}",
        rows[last_inner]
    );
}

#[test]
fn gpu_hot_mark() {
    let theme = true_color();
    let ctx = make_ctx(&theme);

    // junction ~82, below 85: no mark.
    let app = demo_app(DemoState::Prompt);
    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 50, 9);
    let temp_y = rows.iter().position(|r| r.contains("junction")).unwrap();
    assert!(!rows[temp_y].contains("▲"), "{:?}", rows[temp_y]);

    // junction raised to 96 (Critical): mark and Critical colour on the value.
    let mut app = demo_app(DemoState::Prompt);
    app.system.as_mut().unwrap().gpus[0].temp_junction_c = Some(96.0);
    let buf = render_into(|b| gpu::render(&app, &ctx, Rect::new(0, 0, 50, 9), b), 50, 9);
    let rows = rows_of(&buf);
    let temp_y = rows.iter().position(|r| r.contains("96 °C")).unwrap() as u16;
    let row: Vec<char> = rows[temp_y as usize].chars().collect();
    let needle: Vec<char> = "96 °C".chars().collect();
    let start = row.windows(needle.len()).position(|w| w == needle.as_slice()).unwrap() as u16;
    assert!(rows[temp_y as usize].contains("96 °C▲ junction"), "{:?}", rows[temp_y as usize]);
    assert_eq!(buf[(start, temp_y)].fg, Color::Rgb(0xf7, 0x76, 0x8e));
}

#[test]
fn gpu_missing() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = App::new(1000);
    let rows = draw(|area, buf| gpu::render(&app, &ctx, area, buf), 40, 6);
    let all: String = rows.join("\n");
    assert!(all.contains("no GPU"), "{:?}", rows);
}

// --------------------------------------------------------------------------- CPU

#[test]
fn cpu_generating() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| cpu::render(&app, &ctx, area, buf), 48, 9);
    let all: String = rows.join("\n");
    assert!(rows[0].contains("CPU  Intel Core i7-11700K"), "{:?}", rows[0]);
    let per_core = app.system.as_ref().unwrap().cpu.per_core_pct.clone();
    assert!(all.contains(&format!("cores {}  16 thr", level_bars(&per_core))), "{:?}", rows);
    assert!(all.contains("temp  60 °C   freq 3.5 GHz"), "{:?}", rows);
    assert!(all.contains("load  8.33 7.14 5.56"), "{:?}", rows);

    // With graphs off, no braille character anywhere.
    let ctx = Ctx { theme: &theme, graphs: false, clock: "10:21:03" };
    let rows = draw(|area, buf| cpu::render(&app, &ctx, area, buf), 48, 9);
    let all: String = rows.join("\n");
    assert!(!all.chars().any(|c| (0x2800..=0x28ff).contains(&(c as u32))), "{:?}", rows);
}

// --------------------------------------------------------------------------- Memory

#[test]
fn memory_generating() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| memory::render(&app, &ctx, area, buf), 60, 9);
    let all: String = rows.join("\n");
    assert!(all.contains("RAM   "), "{:?}", rows);
    assert!(all.contains("21.6/31.1 G"), "{:?}", rows);
    assert!(all.contains("swap  "), "{:?}", rows);
    assert!(all.contains("0.0/8.0 G"), "{:?}", rows);
    assert!(all.contains("model 19.2 G rss   router 287 M"), "{:?}", rows);
    assert!(all.contains("avail 9.5 G   cached 18.2 G"), "{:?}", rows);
    assert!(all.contains("disk  / 61 %   /srv/models 61 %   /srv/data 11 %"), "{:?}", rows);

    // Down: router only.
    let app = demo_app(DemoState::Down);
    let rows = draw(|area, buf| memory::render(&app, &ctx, area, buf), 60, 9);
    let all: String = rows.join("\n");
    assert!(all.contains("server 287 M"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Log

#[test]
fn log_follow() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let app = demo_app(DemoState::Generating);
    let rows = draw(|area, buf| log::render(&app, &ctx, area, buf), 70, 6);
    assert!(rows[0].contains("follow ●"), "{:?}", rows[0]);
    // The last inner row is the newest visible line.
    let last_inner = rows.len() - 2;
    assert!(rows[last_inner].contains("10:00:32"), "{:?}", rows[last_inner]);
    assert!(rows[last_inner].contains("all slots are idle"), "{:?}", rows[last_inner]);
    let all: String = rows.join("\n");
    assert!(!all.contains("proxying request"), "{:?}", rows);
}

#[test]
fn log_scroll_and_filter() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);

    // Scrolled up by 3: the indicator and the newest line is hidden.
    app.log_view.follow = false;
    app.log_view.scroll = 3;
    let rows = draw(|area, buf| log::render(&app, &ctx, area, buf), 70, 6);
    assert!(rows[0].contains("scroll −3"), "{:?}", rows[0]);
    let all: String = rows.join("\n");
    assert!(!all.contains("all slots are idle"), "{:?}", rows);

    // Filtered on "n_gen": the indicator and every shown line.
    let mut app = demo_app(DemoState::Generating);
    app.log_view.follow = true;
    app.log_view.filter = "n_gen".to_string();
    let rows = draw(|area, buf| log::render(&app, &ctx, area, buf), 70, 6);
    assert!(rows[0].contains("/n_gen"), "{:?}", rows[0]);
    for row in &rows[1..rows.len() - 1] {
        if !row.trim().is_empty() {
            assert!(row.contains("n_gen"), "{:?}", row);
        }
    }
}

#[test]
fn log_levels() {
    let theme = true_color();
    let ctx = make_ctx(&theme);

    // The fixture fed line by line: the "out of memory" line is an Error, coloured Critical.
    let mut app = App::new(1000);
    let text = std::fs::read_to_string("tests/fixtures/journal/llama-server.txt").unwrap();
    for line in text.lines() {
        app.apply(Event::Log(parse_line(line)));
    }
    let buf = render_into(|b| log::render(&app, &ctx, Rect::new(0, 0, 120, 40), b), 120, 40);
    let rows = rows_of(&buf);
    let y = rows.iter().position(|r| r.contains("out of memory")).unwrap() as u16;
    // Match by character (the `│` border is one column but three bytes, so byte indices would drift).
    let row: Vec<char> = rows[y as usize].chars().collect();
    let needle: Vec<char> = "out of memory".chars().collect();
    let start = row.windows(needle.len()).position(|w| w == needle.as_slice()).unwrap() as u16;
    // The text is styled Critical; ratatui renders span spaces with the default colour, so check the
    // letter cells of the matched text.
    for (k, ch) in needle.iter().enumerate() {
        if *ch != ' ' {
            assert_eq!(buf[(start + k as u16, y)].fg, Color::Rgb(0xf7, 0x76, 0x8e), "cell at {k}");
        }
    }

    // No lines yet.
    let app = App::new(1000);
    let rows = draw(|area, buf| log::render(&app, &ctx, area, buf), 60, 4);
    let all: String = rows.join("\n");
    assert!(all.contains("no log line yet"), "{:?}", rows);
}

// --------------------------------------------------------------------------- Bounds

#[test]
fn machine_panels_stay_inside() {
    let theme = true_color();
    let states =
        [DemoState::Idle, DemoState::Prompt, DemoState::Generating, DemoState::Loading, DemoState::Down];
    for state in states {
        let app = demo_app(state);
        let area = Rect::new(5, 2, 30, 6);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        gpu::render(&app, &ctx, area, &mut buf);
        check_inside(&buf, area);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        cpu::render(&app, &ctx, area, &mut buf);
        check_inside(&buf, area);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        memory::render(&app, &ctx, area, &mut buf);
        check_inside(&buf, area);

        let ctx = make_ctx(&theme);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        log::render(&app, &ctx, area, &mut buf);
        check_inside(&buf, area);
    }
}

/// Every cell outside `area` is still a space.
fn check_inside(buf: &Buffer, area: Rect) {
    let (x0, x1) = (area.x as usize, (area.x + area.width) as usize);
    let (y0, y1) = (area.y as usize, (area.y + area.height) as usize);
    let w = buf.area().width as usize;
    for (y, row) in buf.content.chunks(w).enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
            if !inside {
                assert_eq!(cell.symbol(), " ", "cell ({x},{y})");
            }
        }
    }
}

/// A filter longer than the border stays inside the area, keeps the title and the right corner.
#[test]
fn log_long_filter_stays_inside() {
    let theme = true_color();
    let ctx = make_ctx(&theme);
    let mut app = demo_app(DemoState::Generating);
    app.log_view.filter = "a filter much longer than the panel".to_string();
    let area = Rect::new(5, 2, 40, 6);
    let mut buf = Buffer::empty(Rect::new(0, 0, 50, 10));
    log::render(&app, &ctx, area, &mut buf);
    check_inside(&buf, area);
    let top = &rows_of(&buf)[2];
    assert!(top.contains(" Activity "), "{top:?}");
    assert!(top.contains("follow ● ╮"), "{top:?}");
    assert!(top.contains("/a filter"), "{top:?}");
}

#[test]
fn log_all_lines_hidden() {
    // The router's proxy lines, llamatop's own polling, can fill the whole journal window: say so.
    let theme = Theme::new(ColorMode::TrueColor);
    let ctx = Ctx { theme: &theme, graphs: true, clock: "10:21:03" };
    let mut app = App::new(1000);
    let line = "2026-11-12T11:20:05.402493+01:00 aibox-flo llama-server[1194]: 1581.51.785.561 I srv  \
                proxy_reques: proxying request to model m on port 35335";
    for _ in 0..3 {
        app.apply(Event::Log(llamatop::collect::logs::parse_line(line)));
    }
    let mut buf = Buffer::empty(Rect::new(0, 0, 70, 5));
    llamatop::ui::panels::log::render(&app, &ctx, Rect::new(0, 0, 70, 5), &mut buf);
    let row: String = (0..70).map(|x| buf[(x, 1)].symbol().to_string()).collect();
    assert!(row.contains("3 lines hidden — x proxy"), "{row}");
}
