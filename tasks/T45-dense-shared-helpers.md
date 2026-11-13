# T45 — The dense view uses the model band's helpers instead of copies

| | |
|---|---|
| **Batch** | 8 — the Agents map (follow-up) |
| **Depends on** | T44 |
| **Read first** | `src/ui/panels/model.rs` (`generating`, `speed_value`, `speed`, `ctx_used`, `ctx_ratio`, `ctx_ratio_of`); `src/ui/dense.rs` (its private `ctx_used`, `ctx_ratio`, `speed`, `generating` at the end of the file, and their callers) |
| **Deliverable** | changes to `src/ui/panels/model.rs` and `src/ui/dense.rs` only |

## Why

T39 wrote `src/ui/dense.rs` (the 80×24 Overview) with private copies of four helpers of
`src/ui/panels/model.rs`: `ctx_used`, `ctx_ratio`, `speed` and `generating`. They are identical today; two
copies will drift apart. `dense.rs` is also at the 400-line limit. Pure refactor: **no visible change**.

## Contract

### `src/ui/panels/model.rs`

These three become `pub` (same body, a one-line doc comment each):

```rust
/// The context used by slot 0, `(used, n_ctx)`; `(0, 0)` without a slot.
pub fn ctx_used(slot: Option<&SlotInfo>) -> (u64, u64);
/// Context used by slot 0, 0.0–1.0; `0.0` without a slot or a context size (the gauges' ratio).
pub fn ctx_ratio(slot: Option<&SlotInfo>) -> f64;
/// The speed to show, formatted: live while generating, else the average.
pub fn speed(m: &ActiveModel) -> String;
```

`generating`, `speed_value` and `ctx_ratio_of` keep their visibility. Nothing else changes in the file.

### `src/ui/dense.rs`

Delete its private `ctx_used`, `ctx_ratio`, `speed` and `generating`, and call the `model.rs` ones
(`use crate::ui::panels::model::{ctx_ratio, ctx_used, speed};` or qualified paths). Remove the imports that
become unused. Nothing else changes in the file.

## Pitfalls

- The golden frames must not change: if `UPDATE_GOLDEN=1` would rewrite any file, you changed behaviour —
  stop and look. Do not run with `UPDATE_GOLDEN=1`.
- `dense.rs`'s `speed` and `model.rs`'s `speed` must be compared before deleting: they are the same today
  (`tps` of the live speed while generating, else of the average). If you find a difference, STOP and report.
- Touch no test file.

## Tests

No new test: the existing ones are the proof, in particular `tests/dense.rs` and the golden
`tests/snapshots/overview-80x24.txt`, which must pass unchanged.

## Acceptance criteria

- [x] `src/ui/dense.rs` has no copy of `ctx_used`, `ctx_ratio`, `speed` or `generating`.
- [x] Every test passes unchanged; no golden file changes.
- [x] `bash .devbox/check` is green.
