# T44 — Agents screen: the map view (`m`), its feed and its "now" line

| | |
|---|---|
| **Batch** | 8 — the Agents map (design validated by Florian on 2026-11-13) |
| **Depends on** | T43 |
| **Read first** | `src/ui/screens/agents.rs` (whole file); `src/ui/map.rs` (`Core`, `render`, `MIN_W`, `MIN_H`); `src/collect/activity.rs` (`Activity`, `Station`); `src/app/state.rs` (`App`, `apply`, `animating`); `src/app/keys.rs` (the Agents keys); `src/app/run.rs` (`draw_frame`); `src/ui/panels/model.rs` (`speed`, `ctx_ratio`); `src/ui/help.rs`; `docs/02-ui.md` § 2 and § 5 |
| **Deliverable** | `src/ui/screens/agents_map.rs`, `tests/agents_map.rs`, `tests/snapshots/agents-map-140x40.txt`; `pub mod agents_map;` in `src/ui/screens/mod.rs`; changes to `src/ui/screens/agents.rs`, `src/app/state.rs`, `src/app/keys.rs`, `src/app/run.rs`, `src/app/clock.rs`, `src/ui/panels/model.rs`, `src/ui/help.rs`, `docs/02-ui.md` |

## Why

T43 draws the activity map; this sheet puts it on the Agents screen, as in the mockup Florian validated on
2026-11-13: the Queue stays on the left; on the right, **one panel** replaces the session panel and the
timeline — the map on top, full width, then a thin rule, then the feed (the timeline's rows with a duration
column), then a one-line "now" status. `m` switches between this map view and today's list view.

## Contract

### State, clock and keys

- `App` gains `pub agent_map: bool` (`App::new`: `true`) and `pub now_ms: i64` (wall-clock Unix ms of the
  frame being drawn; `App::new`: 0).
- `src/app/clock.rs` gains `/// Wall-clock Unix ms, 0 when the clock cannot be read. pub fn now_ms() -> i64`.
  `run.rs` sets `app.now_ms = clock::now_ms()` right before each `draw_frame`.
- On the Agents screen, `m` toggles `agent_map`. Other screens ignore `m`.
- `App::animating()` is also true when `screen == Agents`, `agent_map`, and the snapshot's activity has a
  pulse or a station with `running > 0` (so pulses move and the running clock ticks at the 100 ms tick).

### `src/ui/panels/model.rs`

Two helpers become public (same behaviour as the private code they come from):

```rust
/// The speed the band shows: live while slot 0 decodes, else the average generation speed.
pub fn speed_value(m: &ActiveModel) -> Option<f64>;
/// Context used by slot 0, 0.0–1.0; `None` without a slot or a context size.
pub fn ctx_ratio_of(m: &ActiveModel) -> Option<f64>;
```

### `src/ui/screens/agents.rs`

`render` keeps its signature. After the Queue, when `app.agent_map` and the right side (the rectangle right of
the Queue, full body height) is at least `map::MIN_W + 2` wide and `map::MIN_H + 12` tall, it calls
`agents_map::render` on that rectangle instead of drawing the session panel and the timeline. Otherwise the
list view, exactly as today. `render_timeline_row` becomes `pub(crate)` (like `step_glyph` already is) so the feed reuses it.

### `src/ui/screens/agents_map.rs`

```rust
/// The map view of the followed session in `area` (the right side of the Agents screen).
pub fn render(app: &App, snapshot: &OpenCodeSnapshot, session: &Session, theme: &Theme, area: Rect, buf: &mut Buffer);
```

- **Panel**: rounded border (`Role::Border`), left title ` #<job> · <short_title> ` (`Server`, bold — the same
  text as the list view's session panel), right title ` m list ` (`Dim`) followed by `· follow ● `
  (`Good`) when `app.agent_focus` is `None`.
- **Map**: the first `clamp(inner.height * 3 / 5, map::MIN_H, 23)` rows of the inner area, full inner width:
  `map::render(&snapshot.activity, &core, app.now_ms, theme, …)`. `core`: with an active llama.cpp model
  (`app.llama…active.first()`): `model` = `short_model_name(id).0`, `tps` = `speed_value`, `ctx_ratio` =
  `ctx_ratio_of`; without one: `model` = the session's model through `short_model_name`, or `""`, and `None`
  for both values.
- **Rule**: the next row, `─` across the inner width, in `lerp_rgb((0x1a,0x1b,0x26), rgb(Border), 0.6)`
  (`Border` outside `TrueColor`).
- **Feed**: the rows between the rule and the last inner row: the newest steps that fit, oldest at the top,
  each drawn by `render_timeline_row` on the inner width minus 5, then its duration right-aligned in the last 4
  columns: `end_ms - time_ms` (running: `app.now_ms - time_ms`) as `<s>s` below 60 s, `<m>m` below an hour,
  `<h>h` otherwise (negative → `0s`); `Dim`, and `Good` for a running step.
- **Now line**, the last inner row: when `activity.now` is `Some((station, summary, start))`:
  `◉ <station name>  <summary>  <m:ss since start>` (the glyph and the name in the station's colour as on the
  map, bold; the summary `Text`; the time `Dim`); else `○ idle` (`Dim`). Then, `Dim`:
  `   in <count(tokens_in)>  out <count(tokens_out)> · <duration>` (the session's tokens and time as the list
  view's session panel computes them), then `   step <sum of calls over all stations>`. Cut at the width.

### Help and docs

- `src/ui/help.rs`: a row `m` → `Agents: map / list view` (box height + 1).
- `docs/02-ui.md`: in § 2, the Agents screen's map view (a short ASCII sketch and the rules above); in § 5,
  the `m` key.

## Pitfalls

- **Make room first, in a separate commit** (`refactor: …`, no behaviour change, `bash .devbox/check` green
  before going on): `src/app/state.rs` and `src/app/run.rs` are both at the 400-line limit.
  - Move the `App` methods `update_last_request`, `task_elapsed_s` and `sync_request` (from the doc comment
    `/// Merges one classified line …` to the end of `impl App`) into a new file `src/app/requests.rs` holding a
    second `impl App` block; `update_last_request` and `sync_request` become `pub(super)`; add `mod requests;`
    to `src/app/mod.rs`. Callers (`app.task_elapsed_s()` in the UI and `tests/tui2_data.rs`) do not change.
  - Move `agent_loop`, `reachability_line` and `agent_message` from `src/app/run.rs` into
    `src/app/threads.rs` (beside `opencode_loop`); `agent_loop` becomes `pub(super)`; `run.rs` imports it like
    `opencode_loop`.
- Keep the list view byte-for-byte as it is: the existing Agents golden must not change. Its test app has
  `agent_map` set to `false` explicitly (update the fixture builder in `tests/agents_screen.rs`, nothing else).
- `now_ms` is in `App` so draws stay pure: tests set it, only `run.rs` reads the clock.

## Tests

`tests/agents_map.rs`, exactly these names. Build the app like `tests/agents_screen.rs` does, then replace the
snapshot's `steps` by `parse_steps(messages-real.json, Some("/home/dev/work/.wt/llamatop-145"))` and its
`activity` by the T43 test activity (history at 1794586720000, live round with the running `cargo test` step
`x1` at 1794586722000); `app.now_ms = 1794586722300`; `agent_map = true`; no llama snapshot.

- `m_toggles`: `m` on the Agents screen flips `agent_map`; `m` on the Overview leaves it as it is.
- `map_view`: a 140 × 40 frame contains `#131 · T24-setup-wizard`, `m list`, `Model 14`, `Files 5`,
  `Checks 2 ◷ 0:01`, a feed row with `src/app/state.rs`, and the now line `◉ Checks  cargo test  0:01`.
- `feed_durations`: the edit step's row (`src/app/state.rs`, 1794586228054 → 1794586230621) ends with `2s`; the
  running think (the last step) shows `5s` in `Good`.
- `list_when_small`: at 100 × 30 the right side is too small: the frame shows the `Timeline` panel title, not
  `m list`.
- `list_when_off`: `agent_map = false` at 140 × 40 → the list view (`Timeline` title present).
- `animating_map`: with the activity above, `animating()` is true on the Agents screen with the map on, false
  with `agent_map = false` (and no llama snapshot).
- `agents_map_golden`: `tests/snapshots/agents-map-140x40.txt`, same `UPDATE_GOLDEN=1` mechanism; read it and
  describe it in your report.

## Acceptance criteria

- [x] The contract above is implemented; the list view is unchanged.
- [x] Every earlier test passes; the tests above pass.
- [x] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) a running job's map moves: pulses on each step, the running clock ticks,
      counts grow; `m` switches views.
