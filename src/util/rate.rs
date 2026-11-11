use std::time::Instant;

/// Turns successive readings of a monotonic counter into a per-second rate.
#[derive(Debug, Clone, Default)]
pub struct Rate {
    // Reference reading: the previous (value, instant). Kept so the next delta has a baseline.
    prev: Option<(f64, Instant)>,
}

impl Rate {
    pub fn new() -> Self {
        Rate { prev: None }
    }

    /// Returns `(value - previous) / elapsed seconds`.
    /// `None` on the first call, when `value` is lower than the previous one (counter reset), and when
    /// less than 1 ms elapsed. In the first two cases the new reading becomes the reference; in the
    /// third the old reference is kept.
    pub fn update(&mut self, value: f64, now: Instant) -> Option<f64> {
        let Some((prev_value, prev_time)) = self.prev else {
            self.prev = Some((value, now));
            return None;
        };

        let elapsed = now.duration_since(prev_time).as_secs_f64();

        // Too close to measure: keep the old reference so the next reading has a baseline.
        if elapsed < 1e-3 {
            return None;
        }

        // Counter went down (restart / reload): drop it, start fresh from this reading.
        if value < prev_value {
            self.prev = Some((value, now));
            return None;
        }

        self.prev = Some((value, now));
        Some((value - prev_value) / elapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn first_reading_gives_none() {
        let mut r = Rate::new();
        let t0 = Instant::now();
        assert_eq!(r.update(1000.0, t0), None);
    }

    #[test]
    fn rate_per_second() {
        let mut r = Rate::new();
        let t0 = Instant::now();
        r.update(1000.0, t0);
        let rate = r.update(1500.0, t0 + Duration::from_secs(2));
        assert_eq!(rate, Some(250.0));
    }

    #[test]
    fn counter_reset_gives_none_then_recovers() {
        let mut r = Rate::new();
        let t0 = Instant::now();
        r.update(1000.0, t0);
        assert_eq!(r.update(10.0, t0 + Duration::from_secs(1)), None);
        assert_eq!(r.update(110.0, t0 + Duration::from_secs(2)), Some(100.0));
    }

    #[test]
    fn too_close_keeps_reference() {
        let mut r = Rate::new();
        let t0 = Instant::now();
        r.update(0.0, t0);
        assert_eq!(r.update(5.0, t0 + Duration::from_micros(100)), None);
        assert_eq!(r.update(100.0, t0 + Duration::from_secs(1)), Some(100.0));
    }
}
