//! A filled braille graph of the last `2 * width` samples.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::ui::theme::{Role, Theme};

// Braille sub-rows of one cell, from the bottom sub-row to the top: left and right column bits.
const LEFT_BITS: [u8; 4] = [0x40, 0x04, 0x02, 0x01];
const RIGHT_BITS: [u8; 4] = [0x80, 0x20, 0x10, 0x08];

/// A filled braille graph, `height` rows of `width` characters, top row first. Each cell holds two
/// samples (left and right dot column), so the graph shows the **last** `2 * width` values, newest at
/// the right; with fewer values, the left side is blank. A value's dot height is
/// `(min(v / max, 1) * height * 4).round()`, at least 1 when `v > 0`; it is 0 when `v` is NaN or `<= 0`,
/// and every height is 0 when `max` is NaN or `<= 0`. Dots fill from the bottom of the graph upward.
/// In one cell, from its bottom dot row to its top: left column bits `0x40, 0x04, 0x02, 0x01`, right
/// column bits `0x80, 0x20, 0x10, 0x08`; the character is `U+2800 + bits`, and a cell with no dot is a
/// plain space `' '` (not `U+2800`). `width == 0` or `height == 0` → empty `Vec`.
pub fn braille_rows(values: &[f32], max: f32, width: u16, height: u16) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let max_ok = !max.is_nan() && max > 0.0;

    // The graph shows the last `2 * width` values, newest at the right; pad the left with blanks.
    let n = (width as usize) * 2;
    let start = values.len().saturating_sub(n);
    let m = values.len() - start;
    // NaN marks "no sample yet": blank, unlike a real 0 which shows the baseline dot.
    let mut samples = vec![f32::NAN; n - m];
    samples.extend_from_slice(&values[start..]);

    // `grid[row][cell]` accumulates the braille bits; row 0 is the top graph row.
    let mut grid = vec![vec![0u8; width as usize]; height as usize];
    for cell in 0..width as usize {
        for (sub_bits, v) in [(LEFT_BITS, samples[2 * cell]), (RIGHT_BITS, samples[2 * cell + 1])] {
            let h = if max_ok && v >= 0.0 {
                let raw = ((v / max).min(1.0) as f64 * height as f64 * 4.0).round() as u32;
                raw.max(1).min(height as u32 * 4)
            } else {
                0
            };
            for p in 0..h {
                let from_bottom = p as usize;
                let row = height as usize - 1 - from_bottom / 4;
                let s = from_bottom % 4;
                grid[row][cell] |= sub_bits[s];
            }
        }
    }

    grid.into_iter()
        .map(|row| {
            let mut s = String::with_capacity(width as usize);
            for bits in row {
                if bits == 0 {
                    s.push(' ');
                } else {
                    s.push(char::from_u32(0x2800 + bits as u32).unwrap_or(' '));
                }
            }
            s
        })
        .collect()
}

/// Draws `braille_rows(values, max, area.width, area.height)` into `area`, every non-space cell with
/// `theme.fg(accent)`. Space cells are left untouched (symbol and style).
pub struct BrailleGraph<'a> {
    pub values: &'a [f32],
    pub max: f32,
    pub accent: Role,
    pub theme: &'a Theme,
}

impl Widget for BrailleGraph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = braille_rows(self.values, self.max, area.width, area.height);
        let color = self.theme.color(self.accent);
        for (y, line) in rows.iter().enumerate() {
            let ry = area.y + y as u16;
            for (x, ch) in line.chars().enumerate() {
                let rx = area.x + x as u16;
                if ch == ' ' {
                    continue;
                }
                if let Some(cell) = buf.cell_mut((rx, ry)) {
                    cell.set_char(ch);
                    if let Some(color) = color {
                        cell.fg = color;
                    }
                }
            }
        }
    }
}
