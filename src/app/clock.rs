//! Wall-clock helpers for the dashboard header: the UTC offset and `HH:MM:SS` formatting.

/// Parses the output of `date +%z` (`+0100`, `-0530`, with or without a trailing newline) into seconds
/// east of UTC. `None` on anything else.
pub fn parse_utc_offset(text: &str) -> Option<i32> {
    // `date +%z` is exactly `±HHMM`; strip a trailing newline and require the whole thing to match.
    let s = text.trim_end_matches(['\n', '\r']);
    let b = s.as_bytes();
    if b.len() != 5 {
        return None;
    }
    let sign = match b[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    // Byte checks first: slicing a non-ASCII string could panic, and `parse` would accept a sign.
    if !b[1..].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let hours = s[1..3].parse::<i32>().ok()?;
    let minutes = s[3..5].parse::<i32>().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// The local UTC offset, read **once** by running `date +%z`; 0 (UTC) when it cannot be run or parsed.
pub fn local_utc_offset() -> i32 {
    let output = std::process::Command::new("date").arg("+%z").output();
    let Some(output) = output.ok() else {
        return 0;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    parse_utc_offset(&text).unwrap_or(0)
}

/// `HH:MM:SS` of `unix_seconds + offset_s`, modulo one day (negative results wrap).
pub fn hms(unix_seconds: i64, offset_s: i32) -> String {
    let secs = (unix_seconds + offset_s as i64).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only the cases `tests/run_loop.rs` does not cover.
    #[test]
    fn rejects_malformed() {
        for bad in ["+01000", "abc", "+00+1", "+0\u{e9}0"] {
            assert_eq!(parse_utc_offset(bad), None, "{bad:?}");
        }
    }
}
