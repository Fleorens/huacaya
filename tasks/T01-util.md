# T01 — Utilities: history ring, rates, formatting

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | — |
| **Read first** | `docs/01-architecture.md` § 3 "Derived values" |
| **Deliverable** | `src/util/mod.rs`, `src/util/history.rs`, `src/util/rate.rs`, `src/util/format.rs`; `pub mod util;` in `src/lib.rs` |

## Why

Every panel needs the same three things: the last N values of a series (graphs), a per-second rate out of a
counter that only grows (network bytes, generated tokens), and short human-readable numbers. They are pure,
so they come first and are tested to the digit.

## Contract

### `src/util/history.rs`

```rust
/// Fixed-capacity series of samples; pushing on a full history drops the oldest sample.
#[derive(Debug, Clone)]
pub struct History { /* private */ }

impl History {
    /// `capacity` 0 is treated as 1.
    pub fn new(capacity: usize) -> Self;
    pub fn push(&mut self, value: f32);
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn capacity(&self) -> usize;
    /// Most recent sample.
    pub fn last(&self) -> Option<f32>;
    /// Largest sample, `None` when empty. NaN samples are ignored.
    pub fn max(&self) -> Option<f32>;
    /// The `n` most recent samples, oldest first. Shorter than `n` when fewer are stored.
    pub fn tail(&self, n: usize) -> Vec<f32>;
}
```

### `src/util/rate.rs`

```rust
use std::time::Instant;

/// Turns successive readings of a monotonic counter into a per-second rate.
#[derive(Debug, Clone, Default)]
pub struct Rate { /* private: previous (value, instant) */ }

impl Rate {
    pub fn new() -> Self;
    /// Returns `(value - previous) / elapsed seconds`.
    /// `None` on the first call, when `value` is lower than the previous one (counter reset), and when
    /// less than 1 ms elapsed. In the first two cases the new reading becomes the reference; in the
    /// third the old reference is kept.
    pub fn update(&mut self, value: f64, now: Instant) -> Option<f64>;
}
```

### `src/util/format.rs`

```rust
/// Binary units, one letter: `512 B`, `1.5 K`, `150 M`, `14.8 G`, `1.2 T`.
/// One decimal when the scaled value is below 100, none from 100 up. Always one space before the unit.
pub fn bytes(n: u64) -> String;

/// Decimal count: `999`, `61.7K`, `196.6K`, `2.7M`. Below 1000: the integer. No space before the unit.
pub fn count(n: u64) -> String;

/// The two largest units: `59s`, `2m 5s`, `1h 2m`, `4d 3h`. `0` gives `0s`.
pub fn duration(seconds: u64) -> String;

/// Splits a model id into a short name and its quantization tag.
/// Keep what follows the last `/`, cut the tag at the last `:`, drop a trailing `-GGUF` from the name.
pub fn short_model_name(id: &str) -> (String, Option<String>);
```

## Pitfalls

- `Instant` cannot be built from a number: in tests, take `let t0 = Instant::now();` and add `Duration`s.
- `{:.1}` rounds half to even on exact binary fractions. The expected values below avoid such ties; do not
  add your own rounding code.
- Clippy asks for `is_empty` whenever `len` exists, and for `Default` when `new()` takes no argument.

## Tests

Unit tests in each file (`#[cfg(test)] mod tests`), with exactly these names and values.

`history.rs`
- `keeps_last_capacity_samples`: capacity 3, push 1,2,3,4 → `len` 3, `tail(3)` = `[2,3,4]`, `last` = 4.
- `tail_shorter_than_n`: capacity 5, push 1,2 → `tail(4)` = `[1,2]`; `tail(1)` = `[2]`.
- `max_ignores_nan`: push 1, NaN, 3 → `max` = 3; empty history → `None`.
- `zero_capacity_is_one`: `History::new(0)`, push 7, push 8 → `tail(9)` = `[8]`.

`rate.rs`
- `first_reading_gives_none`.
- `rate_per_second`: 1000 at t0, 1500 at t0+2 s → `Some(250.0)`.
- `counter_reset_gives_none_then_recovers`: 1000 at t0, 10 at t0+1 s → `None`; 110 at t0+2 s → `Some(100.0)`.
- `too_close_keeps_reference`: 0 at t0, 5 at t0+100 µs → `None`; 100 at t0+1 s → `Some(100.0)`.

`format.rs`
- `bytes_units`: 512 → `512 B`; 1536 → `1.5 K`; 157286400 → `150 M`; 15894315008 → `14.8 G`;
  17163091968 → `16.0 G`; 0 → `0 B`.
- `count_units`: 999 → `999`; 61700 → `61.7K`; 196608 → `196.6K`; 2673440 → `2.7M`.
- `duration_units`: 0 → `0s`; 59 → `59s`; 125 → `2m 5s`; 3725 → `1h 2m`; 356400 → `4d 3h`.
- `model_names`: `peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M` →
  (`Cyber-Tiel-Coder-35B-A3B`, `Some("Q4_K_M")`); `muse-glimmer-30b` → (`muse-glimmer-30b`, `None`);
  `unsloth/Qwen3-8B-GGUF:Q4_K_M` → (`Qwen3-8B`, `Some("Q4_K_M")`).

## Acceptance criteria

- [ ] The four files exist with the exact public API above; `src/lib.rs` declares `pub mod util;`.
- [ ] The 12 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
