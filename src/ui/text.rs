//! Small text helpers shared by every panel: number, percentage, temperature, truncation and a
//! bytes pair. Pure, no I/O — exercised by `tests/panels_model.rs`.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_values() {
        assert_eq!(tps(Some(62.54)), "62.5");
        assert_eq!(tps(Some(643.2)), "643");
        assert_eq!(tps(None), "n/a");
    }

    #[test]
    fn pct_value() {
        assert_eq!(pct(Some(96.4)), "96 %");
        assert_eq!(pct(None), "n/a");
    }

    #[test]
    fn temp_value() {
        assert_eq!(temp(Some(70.4)), "70 °C");
        assert_eq!(temp(None), "n/a");
    }

    #[test]
    fn fit_values() {
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abc", 4), "abc");
        assert_eq!(fit("héllo", 3), "hé…");
        assert_eq!(fit("abc", 0), "");
    }

    #[test]
    fn bytes_pair_values() {
        assert_eq!(bytes_pair(15_894_315_008, 17_163_091_968), "14.8/16.0 G");
        assert_eq!(bytes_pair(1_310_720, 8_589_930_496), "0.0/8.0 G");
        assert_eq!(bytes_pair(500, 1000), "500/1000 B");
    }
}
