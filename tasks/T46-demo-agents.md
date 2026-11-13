# T46 — `--demo` shows a simulated OpenCode session on the Agents screen

| | |
|---|---|
| **Batch** | 8 — the Agents map (follow-up) |
| **Depends on** | T45 |
| **Read first** | `src/app/demo.rs` (header, `LOADED_ID`, `app`); `src/app/run.rs` (`demo_loop`); `src/main.rs` (the `--snapshot` path); `src/config.rs` (`screen`); `src/collect/opencode.rs` (`Session`, `Step`, `StepKind`, `OpenCodeSnapshot`, `SNAPSHOT_STEPS`); `src/collect/activity.rs` (`Activity::update`, `Station`); `tests/agents_map.rs` (how the map view is built and its golden) |
| **Deliverable** | `src/app/demo_agents.rs`, `tests/demo_agents.rs`, `tests/snapshots/demo-agents-140x40.txt`; `pub mod demo_agents;` in `src/app/mod.rs`; changes to `src/app/run.rs`, `src/main.rs`, `src/config.rs`, `README.md` |

## Why

The Agents map (T41–T44) only moves while an OpenCode job runs. `llamatop --demo` shows every other screen with
generated data; it should also show the Agents screen with a simulated session, so anyone (a GitHub visitor,
Florian without a running job) sees the map come alive: pulses, the running clock, a failing then passing
check. `--snapshot --screen 5` renders the same scene for the README.

## Contract

### `src/app/demo_agents.rs`

```rust
/// Fixed demo wall-clock start for tests and `--snapshot` (2026-11-13 ~21:20 UTC).
pub const DEMO_START_MS: i64 = 1_794_600_000_000;
/// Length of one scripted cycle, seconds.
pub const AGENT_CYCLE_S: f64 = 60.0;
/// The demo time `--snapshot --screen 5` renders: the check has just started, its pulse is half way.
pub const SNAPSHOT_T: f64 = 24.3;

/// A simulated OpenCode server: one running session that replays the script below every cycle.
/// Deterministic: the same `start_ms` and the same rounds give the same snapshots.
pub struct DemoAgents { /* private: start_ms, an Activity */ }

impl DemoAgents {
    /// Demo time 0 is wall-clock `start_ms`.
    pub fn new(start_ms: i64) -> Self;
    /// One poll round at demo time `t` seconds (rounds come with increasing `t`).
    pub fn round(&mut self, t: f64) -> OpenCodeSnapshot;
}

/// Rounds at `t` = 0, 1, …, floor(t), then at `t` itself when it is not a whole number; the last snapshot.
pub fn replay(start_ms: i64, t: f64) -> OpenCodeSnapshot;
```

**The script** (one cycle; `start` and `dur` in seconds from the cycle start; `ok` once ended):

| # | start | dur | kind | summary | detail | ok |
|---|---|---|---|---|---|---|
| 0 | 0 | 0 | `User` | `Execute tasks/T12-demo-feature.md` | — | — (`None`) |
| 1 | 1 | 4 | `Think` | `Read the sheet first, then the cited sections.` | — | — |
| 2 | 5 | 0.5 | `Read` | `tasks/T12-demo-feature.md` | — | true |
| 3 | 6 | 3 | `Think` | `The renderer lives in src/ui; find its entry point.` | — | — |
| 4 | 9 | 0.5 | `Search` | `pub fn render in src/ui` | — | true |
| 5 | 10 | 0.5 | `Read` | `src/missing.rs` | `File not found` | **false** |
| 6 | 11 | 2 | `Other("webfetch")` | `webfetch` | `docs.rs/ratatui` | true |
| 7 | 14 | 4 | `Think` | `Plan: one widget, one test, then the check.` | — | — |
| 8 | 18 | 0.3 | `Other("todowrite")` | `todowrite` | `3 todos` | true |
| 9 | 19 | 0.5 | `Edit` | `src/ui/demo.rs` | `+12 −3` | true |
| 10 | 20 | 0.3 | `Write` | `tests/demo.rs` | `24 lines` | true |
| 11 | 21 | 2 | `Shell` | `cargo fmt` | — | true |
| 12 | 24 | 6 | `Shell` | `cargo test --test demo` | `test result: FAILED. 7 passed; 1 failed` | **false** |
| 13 | 31 | 5 | `Think` | `One assertion expects the old width; fix the widget.` | — | — |
| 14 | 36 | 0.5 | `Edit` | `src/ui/demo.rs` | `+2 −1` | true |
| 15 | 37 | 6 | `Other("task")` | `task` | `review the change` | true |
| 16 | 44 | 8 | `Shell` | `bash .devbox/check` | `test result: ok. 8 passed` | true |
| 17 | 53 | 0 | `Say` | `Done: the check is green.` | — | — |

Rules for a round at demo time `t`, with `now = start_ms + t * 1000` (rounded to ms):

- A scripted step of cycle `c` (0, 1, …) **exists** when `c * 60 + start <= t`. Its `Step`: `id` =
  `demo-<c>-<#>`; `time_ms` = `start_ms + (c * 60 + start) * 1000`; it has **ended** when
  `c * 60 + start + dur <= t`: then `end_ms = Some(time_ms + dur * 1000)` and `ok` from the table (`None` for
  rows with "—"), else `end_ms = None` and `ok = None`; `summary` and `detail` from the table (`None` for "—").
- `steps` = the existing steps in time order, only the newest `SNAPSHOT_STEPS`.
- `activity.update(<running session id>, &steps, now)` on the `DemoAgents`'s own `Activity`; the snapshot carries
  a clone.
- `sessions`, newest first: the running one — id `ses_demo_running`, title `#42 Execute tasks/T12-demo-feature.md`,
  job 42, model `demo::LOADED_ID`, agent `build`, running, `created_ms = start_ms - 600_000`,
  `tokens_in = 40_000 + 900 t`, `tokens_out = 12_000 + 250 t`, `cache_read = 1_500_000 + 30_000 t` (truncated),
  directory `/work/demo`; then `ses_demo_done` (`#41 Execute tasks/T11-demo-data.md`, job 41, outcome
  `succeeded`, not running, `created_ms = start_ms - 3_600_000`, `idle_ms = Some(start_ms - 1_800_000)`); then
  `ses_demo_failed` (`Refactor the parser`, no job, outcome `failed`, not running,
  `created_ms = start_ms - 86_400_000`, `idle_ms = Some(start_ms - 84_000_000)`). The other fields: `None` / 0.
- `focus = Some("ses_demo_running")`, `error = None`, `fetched_ms = now`.

### `src/app/run.rs`

`demo_loop` owns a `DemoAgents::new(clock::now_ms())` and, whenever its demo clock crosses a whole second
(`prev.floor() != now.floor()`, and once at the start), sends `Event::OpenCode(Box::new(agents.round(now)))`.
`demo_events` does not change.

### `--snapshot --screen 5`

- `src/config.rs`: `screen` accepts 1–5 (doc: `1–5 (Overview, Models, Logs, System, Agents)`).
- `src/main.rs`: when the snapshot's screen is 5, after building the demo app: `app.opencode =
  Some(replay(DEMO_START_MS, SNAPSHOT_T))`, `app.now_ms = DEMO_START_MS + 24_300`, `app.utc_offset_s = 0`.
  Screens 1–4 do not change (their goldens must not move).

### `README.md`

One sentence where `--demo` is described: the Agents screen (5) shows a simulated OpenCode session in the
demo.

## Pitfalls

- Pure and deterministic: no clock and no randomness in `demo_agents.rs`; only `run.rs` reads the clock.
- Float times: compute `c * 60 + start` and compare in seconds as `f64`; convert to ms with `.round() as i64`.
- `src/app/run.rs` must stay under 400 lines. Keep `demo_agents.rs` under 400 lines too (the script is a
  `const` array of a small private struct).
- Existing goldens must not change; regenerate only the new one.

## Tests

`tests/demo_agents.rs`, exactly these names. `S = DEMO_START_MS`.

- `steps_at_12`: `replay(S, 12.0).steps` has 7 steps, kinds `User, Think, Read, Think, Search, Read,
  Other("webfetch")`; `steps[5].ok == Some(false)` with detail `File not found`; `steps[6].end_ms == None`;
  `steps[1].end_ms == Some(S + 5_000)`; ids `demo-0-0` … `demo-0-6`.
- `all_stations_lit`: `replay(S, 59.0).activity.stations`, calls per station: You 1, Model 5, Files 6, Shell 1,
  Checks 2, Web 1, Agents 1, Tools 1; errors: Files 1, Checks 1, the others 0; no station running.
- `pulse_when_check_starts`: `replay(S, 24.0).activity.pulses` contains an outbound pulse at `Station::Checks`
  with `at_ms == S + 24_000`.
- `now_line`: `replay(S, 26.0).activity.now == Some((Station::Checks, "cargo test --test demo".into(),
  S + 24_000))`.
- `second_cycle`: `replay(S, 119.0)`: Files calls 12, Checks errors 2; the newest step's id is `demo-1-17`.
- `queue`: `replay(S, 30.0)`: 3 sessions in the order above; `focus == Some("ses_demo_running")`; the running
  one has job 42 and `tokens_in == 67_000`.
- `deterministic`: two `replay(S, 37.5)` are equal.
- `screen_five`: `Cli::try_parse_from(["llamatop", "--snapshot", "140x40", "--screen", "5"])` is `Ok`;
  `--screen 6` is an error.
- `demo_agents_golden`: the app `main.rs` builds for `--snapshot 140x40 --screen 5` (build it the same way in
  the test: `demo::app(DemoState::Generating)`, screen `Agents`, then the three fields above), rendered with
  `ui::snapshot::snapshot(&app, &theme, 140, 40, false)` → `tests/snapshots/demo-agents-140x40.txt`, same
  `UPDATE_GOLDEN=1` mechanism; read it and describe it in your report (the map, the half-way pulse on the
  Checks spoke, the feed, the now line `◉ Checks  cargo test --test demo  0:00`).

## Acceptance criteria

- [x] The contract above is implemented; screens 1–4 and every existing golden are unchanged.
- [x] Every earlier test passes; the tests above pass.
- [x] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) `llamatop --demo`, key `5`: the map moves on its own, cycle after cycle.
