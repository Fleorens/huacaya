/// Fixed-capacity series of samples; pushing on a full history drops the oldest sample.
#[derive(Debug, Clone)]
pub struct History {
    buf: Vec<f32>,
    capacity: usize,
}

impl History {
    /// `capacity` 0 is treated as 1.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        History { buf: Vec::with_capacity(capacity), capacity }
    }

    pub fn push(&mut self, value: f32) {
        if self.buf.len() == self.capacity {
            self.buf.remove(0);
        }
        self.buf.push(value);
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Most recent sample.
    pub fn last(&self) -> Option<f32> {
        self.buf.last().copied()
    }

    /// Largest sample, `None` when empty. NaN samples are ignored.
    pub fn max(&self) -> Option<f32> {
        // NaN filtered out first, so `partial_cmp` never sees a NaN and returns `Some`.
        self.buf
            .iter()
            .copied()
            .filter(|&x| !x.is_nan())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// The `n` most recent samples, oldest first. Shorter than `n` when fewer are stored.
    pub fn tail(&self, n: usize) -> Vec<f32> {
        let start = self.buf.len().saturating_sub(n);
        self.buf[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_last_capacity_samples() {
        let mut h = History::new(3);
        for v in [1.0, 2.0, 3.0, 4.0] {
            h.push(v);
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.tail(3), vec![2.0, 3.0, 4.0]);
        assert_eq!(h.last(), Some(4.0));
    }

    #[test]
    fn tail_shorter_than_n() {
        let mut h = History::new(5);
        h.push(1.0);
        h.push(2.0);
        assert_eq!(h.tail(4), vec![1.0, 2.0]);
        assert_eq!(h.tail(1), vec![2.0]);
    }

    #[test]
    fn max_ignores_nan() {
        let mut h = History::new(3);
        h.push(1.0);
        h.push(f32::NAN);
        h.push(3.0);
        assert_eq!(h.max(), Some(3.0));

        let empty = History::new(3);
        assert_eq!(empty.max(), None);
    }

    #[test]
    fn zero_capacity_is_one() {
        let mut h = History::new(0);
        h.push(7.0);
        h.push(8.0);
        assert_eq!(h.tail(9), vec![8.0]);
    }
}
