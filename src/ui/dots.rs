//! A braille dot canvas (T43): paint dots, lines, arcs and cells, then render them to a buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

/// A braille canvas: 2 × 4 dots per cell. Dots are OR-ed; each cell keeps one colour, the one painted
/// with the highest priority (on a tie, the last painted).
#[derive(Debug, Clone)]
pub struct Dots {
    width: usize,
    height: usize,
    bits: Vec<u8>,
    colors: Vec<Option<Color>>,
    priorities: Vec<u8>,
}

impl Dots {
    /// A blank canvas of `width × height` cells.
    pub fn new(width: u16, height: u16) -> Self {
        let w = width as usize;
        let h = height as usize;
        Dots {
            width: w,
            height: h,
            bits: vec![0; w * h],
            colors: vec![None; w * h],
            priorities: vec![0; w * h],
        }
    }

    /// Size in dots: `(width * 2, height * 4)`.
    pub fn size(&self) -> (i32, i32) {
        (self.width as i32 * 2, self.height as i32 * 4)
    }

    /// Sets the dot at `(x, y)` (dots, origin top-left, y down). Outside the canvas: ignored.
    pub fn plot(&mut self, x: i32, y: i32, color: Option<Color>, priority: u8) {
        let (dw, dh) = self.size();
        if x < 0 || y < 0 || x >= dw || y >= dh {
            return;
        }
        let col = x / 2;
        let row = y / 4;
        let bit = match (x - col * 2, y - row * 4) {
            (0, 0) => 0x01,
            (0, 1) => 0x02,
            (0, 2) => 0x04,
            (1, 0) => 0x08,
            (1, 1) => 0x10,
            (1, 2) => 0x20,
            (0, 3) => 0x40,
            (1, 3) => 0x80,
            _ => 0,
        };
        let idx = row as usize * self.width + col as usize;
        self.bits[idx] |= bit;
        // Highest priority wins; on a tie the last painted wins.
        if priority >= self.priorities[idx] {
            self.colors[idx] = color;
            self.priorities[idx] = priority;
        }
    }

    /// A straight line: `n = max(|dx|, |dy|).ceil()` steps (at least 1); point `k` (0..=n) is
    /// `from + (to - from) * k / n`, rounded; with `every > 1`, only points with `k % every == 0`.
    pub fn line(
        &mut self,
        from: (f64, f64),
        to: (f64, f64),
        color: Option<Color>,
        priority: u8,
        every: usize,
    ) {
        let n = (to.0 - from.0).abs().max((to.1 - from.1).abs()).ceil().max(1.0) as usize;
        for k in 0..=n {
            if every == 1 || k % every == 0 {
                let t = k as f64 / n as f64;
                let px = (from.0 + (to.0 - from.0) * t).round();
                let py = (from.1 + (to.1 - from.1) * t).round();
                self.plot(px as i32, py as i32, color, priority);
            }
        }
    }

    /// An arc of radius `r` around `centre`: angle 0 is up, clockwise; from `start` turns over `turns`
    /// turns (1.0 = full circle); `n = (r * turns * 2π * 4).ceil() + 1` points, rounded.
    pub fn arc(
        &mut self,
        centre: (f64, f64),
        r: f64,
        start: f64,
        turns: f64,
        color: Option<Color>,
        priority: u8,
    ) {
        let n = (r * turns * 2.0 * std::f64::consts::PI * 4.0).ceil() as usize + 1;
        for k in 0..=n {
            let angle = start + turns * (k as f64 / n as f64) * 2.0 * std::f64::consts::PI;
            let px = (centre.0 + r * angle.sin()).round();
            let py = (centre.1 - r * angle.cos()).round();
            self.plot(px as i32, py as i32, color, priority);
        }
    }

    /// Writes every cell that has at least one dot into `buf` at `area`'s top-left: the braille symbol
    /// (`U+2800` + bits) and, when the cell's colour is `Some`, that foreground. Cells without dots are
    /// untouched.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for row in 0..self.height as u16 {
            for col in 0..self.width as u16 {
                let idx = row as usize * self.width + col as usize;
                let bits = self.bits[idx];
                if bits == 0 {
                    continue;
                }
                let x = area.x + col;
                let y = area.y + row;
                if x >= area.width || y >= area.height {
                    continue;
                }
                let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                let cell = &mut buf[(x, y)];
                cell.set_char(ch);
                if let Some(color) = self.colors[idx] {
                    cell.fg = color;
                }
            }
        }
    }
}
