//! Small text helpers shared by every panel: number, percentage, temperature, truncation, a bytes
//! pair and whole-group fitting. Pure, no I/O — exercised by `tests/panels_model.rs` and
//! `tests/tui2_layout.rs`.

/// Token count with a thin grouping, a space every three digits: `0`, `999`, `1 043`, `1 234 567`.
pub fn group_thousands(n: u64) -> String {
    let chars: Vec<char> = n.to_string().chars().collect();
    let total = chars.len();
    let mut out: String = String::with_capacity(total + total / 3);
    for (i, c) in chars.iter().enumerate() {
        out.push(*c);
        // A separator before a new group: three or more digits remain after this one.
        if i + 1 < total && (total - i - 1).is_multiple_of(3) {
            out.push(' ');
        }
    }
    out
}

/// Tokens per second: one decimal below 100 (`62.5`), none from 100 (`643`); `None` → `n/a`.
pub fn tps(v: Option<f64>) -> String {
    match v {
        Some(x) if x < 100.0 => format!("{:.1}", x),
        Some(x) => format!("{:.0}", x),
        None => "n/a".to_string(),
    }
}

/// A percentage, no decimal, a space before `%`: `96 %`; `None` → `n/a`.
pub fn pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.0} %", x),
        None => "n/a".to_string(),
    }
}

/// A temperature, no decimal: `70 °C`; `None` → `n/a`.
pub fn temp(v: Option<f32>) -> String {
    match v {
        Some(x) => format!("{:.0} °C", x),
        None => "n/a".to_string(),
    }
}

/// `s` cut to at most `width` characters (chars, not bytes); when it is cut, its last kept character
/// is replaced by `…`. `width` 0 → empty.
pub fn fit(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        return s.to_string();
    }
    let mut out: String = chars.into_iter().take(width - 1).collect();
    out.push('…');
    out
}

/// `used/total U` in the unit of `total`: the largest of B, K, M, G, T (powers of 1024) that `total`
/// reaches; one decimal for K and above (`14.8/16.0 G`), integers for B (`500/1000 B`).
pub fn bytes_pair(used: u64, total: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = total as f64;
    let mut i = 0;
    while value >= 1024.0 && i < UNITS.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    let divisor = 1024u64.pow(i as u32) as f64;
    let fmt = |n: u64| {
        if i == 0 { n.to_string() } else { format!("{:.1}", n as f64 / divisor) }
    };
    format!("{}/{} {}", fmt(used), fmt(total), UNITS[i])
}

/// The groups of spans that fit in `width`, in order, stopping at the first one that does not: a row
/// drops its least important parts whole instead of being cut mid-word. The first group is always kept
/// (`set_line` cuts it if it alone is too wide).
pub fn fit_groups<'a>(groups: Vec<Vec<ratatui::text::Span<'a>>>, width: usize) -> ratatui::text::Line<'a> {
    let mut spans = Vec::new();
    let mut used = 0;
    for (i, group) in groups.into_iter().enumerate() {
        let w: usize = group.iter().map(|s| s.width()).sum();
        if i > 0 && used + w > width {
            break;
        }
        used += w;
        spans.extend(group);
    }
    ratatui::text::Line::from(spans)
}
