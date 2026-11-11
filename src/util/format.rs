/// Binary units, one letter: `512 B`, `1.5 K`, `150 M`, `14.8 G`, `1.2 T`.
/// One decimal when the scaled value is below 100, none from 100 up. Always one space before the unit.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = n as f64;
    let mut i = 0;
    while value >= 1024.0 && i < UNITS.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    // Whole bytes never carry a decimal: `5 B`, not `5.0 B`.
    if i == 0 {
        return format!("{} B", n);
    }
    let scaled = if value < 100.0 { format!("{:.1}", value) } else { format!("{:.0}", value) };
    format!("{} {}", scaled, UNITS[i])
}

/// Decimal count: `999`, `61.7K`, `196.6K`, `2.7M`. Below 1000: the integer. No space before the unit.
pub fn count(n: u64) -> String {
    const UNITS: [&str; 3] = ["", "K", "M"];
    let mut value = n as f64;
    let mut i = 0;
    while value >= 1000.0 && i < UNITS.len() - 1 {
        value /= 1000.0;
        i += 1;
    }
    if i == 0 { format!("{}", n) } else { format!("{:.1}{}", value, UNITS[i]) }
}

/// The two largest units: `59s`, `2m 5s`, `1h 2m`, `4d 3h`. `0` gives `0s`.
pub fn duration(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }
    // From largest to smallest unit; keep the first two that are non-zero.
    let units =
        [(seconds / 86_400, "d"), (seconds / 3_600 % 24, "h"), (seconds / 60 % 60, "m"), (seconds % 60, "s")];
    let mut parts: Vec<String> = Vec::with_capacity(2);
    for (value, label) in units {
        if value > 0 {
            parts.push(format!("{}{}", value, label));
        }
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(" ")
}

/// Splits a model id into a short name and its quantization tag.
/// Keep what follows the last `/`, cut the tag at the last `:`, drop a trailing `-GGUF` from the name.
pub fn short_model_name(id: &str) -> (String, Option<String>) {
    // Keep the last path segment: the model file name, not the repo / collection.
    let file = id.rsplit('/').next().unwrap_or(id);

    let (name, tag) = match file.rsplit_once(':') {
        Some((name, quant)) => (name, Some(quant.to_string())),
        None => (file, None),
    };

    // A `-GGUF` suffix carries no information for the label.
    let name = name.strip_suffix("-GGUF").unwrap_or(name);
    (name.to_string(), tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_units() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 K");
        assert_eq!(bytes(157_286_400), "150 M");
        assert_eq!(bytes(15_894_315_008), "14.8 G");
        assert_eq!(bytes(17_163_091_968), "16.0 G");
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(5), "5 B");
    }

    #[test]
    fn count_units() {
        assert_eq!(count(999), "999");
        assert_eq!(count(61_700), "61.7K");
        assert_eq!(count(196_608), "196.6K");
        assert_eq!(count(2_673_440), "2.7M");
    }

    #[test]
    fn duration_units() {
        assert_eq!(duration(0), "0s");
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(125), "2m 5s");
        assert_eq!(duration(3725), "1h 2m");
        assert_eq!(duration(356_400), "4d 3h");
    }

    #[test]
    fn model_names() {
        assert_eq!(
            short_model_name("peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M"),
            ("Cyber-Tiel-Coder-35B-A3B".to_string(), Some("Q4_K_M".to_string()))
        );
        assert_eq!(short_model_name("muse-glimmer-30b"), ("muse-glimmer-30b".to_string(), None));
        assert_eq!(
            short_model_name("unsloth/Qwen3-8B-GGUF:Q4_K_M"),
            ("Qwen3-8B".to_string(), Some("Q4_K_M".to_string()))
        );
    }
}
