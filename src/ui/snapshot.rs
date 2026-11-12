//! Render one demo frame to a string, for golden files and `--snapshot`.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier, Style};

use crate::app::App;
use crate::ui::draw;
use crate::ui::panels::Ctx;
use crate::ui::theme::Theme;

/// Renders one frame of `app` at `width × height` with `draw`, clock `10:21:03`.
///
/// Plain (`ansi` false): each row's symbols with trailing spaces removed, rows joined by `\n`, final
/// `\n`. ANSI: the same rows, each cell preceded by an SGR sequence whenever its style differs from the
/// previous cell's (`\x1b[0m` then `1` for bold, `7` for reversed, `38;2;r;g;b` for an `Rgb` foreground,
/// `30`–`37` / `90`–`97` for the 16 named colours), each row ending with `\x1b[0m`; trailing spaces kept.
pub fn snapshot(app: &App, theme: &Theme, width: u16, height: u16, ansi: bool) -> String {
    let backend = TestBackend::new(width, height);
    // `TestBackend` never errors, so the `Err` branch is unreachable but keeps us off `unwrap`.
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    // `draw` returns the rendered `CompletedFrame`; `TestBackend` never errors, but we match to stay
    // off `unwrap`. The returned frame owns the buffer we read below.
    let frame = match terminal.draw(|f| {
        let area = f.area();
        let ctx = Ctx { theme, graphs: true, clock: "10:21:03" };
        draw::draw(app, &ctx, area, f.buffer_mut());
    }) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let buffer = &frame.buffer;
    if ansi { ansi_rows(buffer, width, height) } else { plain_rows(buffer, width, height) }
}

/// Plain rows: symbols with trailing spaces removed, joined by `\n` with a final `\n`.
fn plain_rows(buffer: &ratatui::buffer::Buffer, width: u16, height: u16) -> String {
    let mut out = String::new();
    for y in 0..height {
        let mut row = String::new();
        for x in 0..width {
            row.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

/// ANSI rows: an SGR before each cell whose style changed, `\x1b[0m` at the end of each row, trailing
/// spaces kept.
fn ansi_rows(buffer: &ratatui::buffer::Buffer, width: u16, height: u16) -> String {
    let mut out = String::new();
    for y in 0..height {
        let mut row = String::new();
        let mut prev: Option<Style> = None;
        for x in 0..width {
            let style = buffer[(x, y)].style();
            if prev.as_ref() != Some(&style) {
                row.push_str(&sgr(&style));
            }
            prev = Some(style);
            row.push_str(buffer[(x, y)].symbol());
        }
        row.push_str("\x1b[0m");
        out.push_str(&row);
        out.push('\n');
    }
    out
}

/// The SGR sequence for `style`: `\x1b[0m`, then `1` for bold, `7` for reversed, then the foreground
/// colour code (`38;2;r;g;b` for `Rgb`, `3x`/`9x` for the 16 named colours).
fn sgr(style: &Style) -> String {
    let mut params = String::new();
    if style.add_modifier.contains(Modifier::BOLD) {
        params.push('1');
    }
    if style.add_modifier.contains(Modifier::REVERSED) {
        params.push('7');
    }
    if let Some(fg) = style.fg
        && let Some(code) = color_code(fg)
    {
        if !params.is_empty() {
            params.push(';');
        }
        params.push_str(&code);
    }
    if params.is_empty() { "\x1b[0m".to_string() } else { format!("\x1b[{}m", params) }
}

/// The SGR foreground colour code for the 16 named colours and true colour; `None` for anything else.
fn color_code(c: Color) -> Option<String> {
    match c {
        Color::Rgb(r, g, b) => Some(format!("38;2;{r};{g};{b}")),
        Color::Indexed(i) => match i {
            0..=7 => Some(format!("3{i}")),
            8..=15 => Some(format!("9{}", i - 8)),
            _ => None,
        },
        _ => None,
    }
}

/// Parses `WxH` (`140x40`; `X` accepted too). Both between 1 and 1000, else `None`.
pub fn parse_size(text: &str) -> Option<(u16, u16)> {
    let (a, b) = text.split_once('x').or_else(|| text.split_once('X'))?;
    let w: u16 = a.parse().ok()?;
    let h: u16 = b.parse().ok()?;
    if (1..=1000).contains(&w) && (1..=1000).contains(&h) { Some((w, h)) } else { None }
}
