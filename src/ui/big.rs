//! Three-row block-digit text and one-character level bars. Pure, no I/O.

/// `text` in 3-row block digits for the live speed. Glyphs, each 3 columns wide, rows top to bottom:
/// `0` `█▀█` `█ █` `▀▀▀` · `1` `▀█ ` ` █ ` `▀▀▀` · `2` `▀▀█` `█▀▀` `▀▀▀` · `3` `▀▀█` ` ▀█` `▀▀▀` ·
/// `4` `█ █` `▀▀█` `  ▀` · `5` `█▀▀` `▀▀█` `▀▀▀` · `6` `█▀▀` `█▀█` `▀▀▀` · `7` `▀▀█` `  █` `  ▀` ·
/// `8` `█▀█` `█▀█` `▀▀▀` · `9` `█▀█` `▀▀█` `▀▀▀`; `.` is one column ` ` ` ` `▀`; `-` is `   ` `▀▀▀` `   `;
/// any other character is one column of spaces. One space column between two glyphs, none at the ends.
pub fn big_digits(text: &str) -> [String; 3] {
    let cell = |c: char| -> [&str; 3] {
        match c {
            '0' => ["█▀█", "█ █", "▀▀▀"],
            '1' => ["▀█ ", " █ ", "▀▀▀"],
            '2' => ["▀▀█", "█▀▀", "▀▀▀"],
            '3' => ["▀▀█", " ▀█", "▀▀▀"],
            '4' => ["█ █", "▀▀█", "  ▀"],
            '5' => ["█▀▀", "▀▀█", "▀▀▀"],
            '6' => ["█▀▀", "█▀█", "▀▀▀"],
            '7' => ["▀▀█", "  █", "  ▀"],
            '8' => ["█▀█", "█▀█", "▀▀▀"],
            '9' => ["█▀█", "▀▀█", "▀▀▀"],
            '.' => [" ", " ", "▀"],
            '-' => ["   ", "▀▀▀", "   "],
            _ => ["   ", "   ", "   "],
        }
    };
    let mut rows: [String; 3] = std::array::from_fn(|_| String::new());
    for (i, c) in text.chars().enumerate() {
        if i > 0 {
            for r in rows.iter_mut() {
                r.push(' ');
            }
        }
        let g = cell(c);
        for r in 0..3 {
            rows[r].push_str(g[r]);
        }
    }
    rows
}

/// One character per value, 0–100 → `▁▂▃▄▅▆▇█` (index `round(v / 100 × 7)`, clamped 0–7); NaN → a space.
pub fn level_bars(values: &[f32]) -> String {
    const GLYPHS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    values
        .iter()
        .map(|&v| {
            if v.is_nan() {
                ' '
            } else {
                let idx = (v / 100.0 * 7.0).round().clamp(0.0, 7.0) as usize;
                GLYPHS[idx]
            }
        })
        .collect()
}
