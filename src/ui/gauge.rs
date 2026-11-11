//! A one-row horizontal gauge with a gradient fill.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::ui::theme::{Role, Theme, gauge_color};

// Partial cells, one to seven eighths.
const PARTIALS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// The bar's characters: `width` cells. With `e = (ratio * width * 8).round()` eighths (`ratio`
/// clamped to 0..=1, NaN counts as 0): `e / 8` cells `█`, then, when `e % 8 > 0`, one cell from
/// `▏▎▍▌▋▊▉` (index `e % 8`, 1-based), then `░` up to `width`. Always exactly `width` characters.
pub fn bar_cells(ratio: f64, width: u16) -> String {
    let ratio = ratio.clamp(0.0, 1.0);
    let e = (ratio * width as f64 * 8.0).round() as u32;
    let mut s = String::with_capacity(width as usize);
    let full = e / 8;
    let partial = e % 8;
    for i in 0..width as u32 {
        if i < full {
            s.push('█');
        } else if i == full && partial > 0 {
            s.push(PARTIALS[(partial as usize) - 1]);
        } else {
            s.push('░');
        }
    }
    s
}

/// A one-row horizontal gauge. Draws `bar_cells(ratio, area.width)` on the **first row** of `area`
/// (nothing when the area has no width or no height); other rows are untouched.
/// Cell `i` of the filled part (`█` and the partial cell) gets its foreground from
/// `gauge_color(theme, accent, (i as f32 + 0.5) / width as f32 * 100.0, thresholds)`; `░` cells take
/// `theme.color(Role::Dim)`. With no colour (`None`), the cell keeps the buffer's own foreground.
pub struct Gauge<'t> {
    pub ratio: f64,
    pub accent: Role,
    pub thresholds: (f32, f32),
    pub theme: &'t Theme,
}

impl Widget for Gauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let chars: Vec<char> = bar_cells(self.ratio, area.width).chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            let x = area.x + i as u16;
            let y = area.y;
            let fg = if c == '░' {
                self.theme.color(Role::Dim)
            } else {
                gauge_color(
                    self.theme,
                    self.accent,
                    (i as f32 + 0.5) / area.width as f32 * 100.0,
                    self.thresholds,
                )
            };
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(c);
                if let Some(color) = fg {
                    cell.fg = color;
                }
            }
        }
    }
}
