# T35 — Agents screen (5): OpenCode sessions and the live timeline

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T34 |
| **Read first** | `src/collect/opencode.rs`; `src/app/state.rs` (`Screen`, `agent_focus`); `src/app/keys.rs`; `src/ui/draw.rs`; `src/ui/screens/models.rs` (a screen's shape); `src/app/clock.rs` |
| **Deliverable** | `src/ui/screens/agents.rs`, `tests/agents_screen.rs`, `tests/snapshots/agents-*.txt`; `pub mod agents;` in `src/ui/screens/mod.rs`; changes to `src/app/state.rs`, `src/app/keys.rs`, `src/app/run.rs`, `src/collect/opencode.rs`, `src/ui/panels/header.rs`, `src/ui/draw.rs`, `src/ui/help.rs`, `docs/02-ui.md` |

## Why

The fifth screen of the validated mockup: on the left, the OpenCode sessions (one per `dev` job), on the right
the session followed — its model, tokens, state, and a timeline of every step (files read and edited, commands
and their results, the model's thinking). T34 delivers the data; this sheet shows it and adds the keys.

## Contract

### State and keys

- `Screen` gains `Agents` (fifth: `Screen::ALL` has 5 entries, title `Agents`); key `5` selects it; `Tab` /
  `BackTab` cycle through five.
- `App` gains `pub agent_selected: usize` (index into `opencode.sessions`) and
  `pub utc_offset_s: i32` (seconds east of UTC, for the timeline's clock; `App::new` sets 0, `run.rs` sets
  `clock::local_utc_offset()`).
- On the Agents screen: `Up` / `Down` move `agent_selected` within the sessions (no wrap) and set
  `agent_focus = Some(<that session's id>)`; `Home` / `End` likewise; `a` sets `agent_focus = None` (follow the
  running session automatically). Other screens ignore these keys as before.
- On `Event::OpenCode(s)`: clamp `agent_selected` to the sessions; when `agent_focus` is `None`, set
  `agent_selected` to the index of `s.focus` in the sessions (the poller's automatic choice).

### `src/collect/opencode.rs`

`OpenCodeSnapshot` gains `#[serde(default)] pub fetched_ms: i64`: the wall-clock time of the round in Unix
milliseconds (`SystemTime::now()`, 0 when it cannot be read), set by `OpenCodePoller::poll`.

### `src/ui/screens/agents.rs`

```rust
/// The Agents screen in `area` (the body below the header).
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);

/// A session's short title: `Execute tasks/T30-nvidia.md` → `T30-nvidia`; a leading `#<n> ` is removed first;
/// other titles are kept.
pub fn short_title(title: &str) -> String;
```

Without OpenCode (`app.opencode` is `None`): one panel `Agents` over the area with a `Dim` row
`OpenCode is not configured: llamatop setup, or --opencode <URL>`. When `error` is set, the last row of the
Queue panel shows it in `Warning`.

Layout: a **Queue** panel on the left (`min(52, area.width * 2 / 5)` columns), and on the right, from the top, a
**session** panel of 5 rows and a **Timeline** panel on the rest.

- **Queue** — `frame::panel("Queue", Role::Server, …)`, right title `opencode` (`Dim`). Header row
  `  #     state        age   task` (`Dim`). One row per session, in the snapshot's order: `▶ ` for the selected
  one (else two spaces), the job number padded to 5 (or `-`), the state padded to 12 — `◉ running` (`Server`),
  `✓ done` (`Good`) for `succeeded`, `✖ failed` (`Critical`) for `failed`, `○ <outcome>` (`Dim`) otherwise — the
  age `util::duration((fetched_ms - created_ms) / 1000)` padded to 6, then `short_title` cut at the width. The
  selected row has the `REVERSED` modifier only on its `▶`. Last row (`Dim`):
  `<n> sessions · <k> running`.
- **Session** — `frame::panel("#<job> · <short_title>", …)` (just the short title without a job). Rows:
  `model   <short_model_name(model).0>  · <agent>`; `tokens  in <count(in)>  out <count(out)>  cache
  <count(cache)>`; `state   ◉ working` (running) or the outcome line as in the Queue; `time    <duration>` (running:
  `fetched - created`; done: `idle - created`).
- **Timeline** — `frame::panel("Timeline", …)`, right title `follow ●` (`Good`). The last steps that fit, oldest
  at the top: `<HH:MM:SS> <glyph> <summary>  <detail>` with the time from `clock::hms(time_ms / 1000,
  utc_offset_s)` (`Dim`), and the glyphs (padded to 8): `▸ read`, `✎ edit`, `✚ write`, `$ shell`, `⌕ grep`,
  `◉ think`, `» say`, `• <name>` — `Read`/`Search` in `Cpu`, `Edit`/`Write` in `Gpu`, `Shell` in `Memory`, `Think`
  / `Say` in `Server`; summary `Text` (`Dim` for `Think`); detail `Critical` when `ok == Some(false)`, `Good` when a
  `Shell` has `ok == Some(true)`, `Dim` otherwise. The row is cut at the width.

### Header, help, draw, docs

- `src/ui/panels/header.rs`: the tab `5 Agents` gets a trailing ` ●` (`Server`) when a session is running.
- `src/ui/help.rs`: a row `a` → `Agents: follow the running session` (box height + 1).
- `src/ui/draw.rs`: route `Screen::Agents` to `screens::agents::render` on the body.
- `docs/02-ui.md`: screen 5 in § 2 and the `5` / `a` keys in § 5.

## Pitfalls

- `Screen::ALL` grows: every test that counts screens or cycles with `Tab` from `System` now lands on `Agents`;
  update those tests to the new order (never weaken them).
- Build the test `App` from the fixtures: `parse_sessions(sessions.json)`, `parse_steps(messages.json,
  Some("/work/.wt/demo-131"))`, `fetched_ms = 1794514000000`, `focus` = the done session's id.
- Times in tests: `utc_offset_s = 0`, so `clock::hms` gives UTC: the fixture's edit step (1794501001000 ms) is
  `16:30:01`.

## Tests

`tests/agents_screen.rs`, exactly these names.

- `short_titles`: `#136 Execute tasks/T30-nvidia.md` → `T30-nvidia`; `Execute tasks/T24-setup-wizard.md` →
  `T24-setup-wizard`; `Refactor the parser` → itself.
- `queue_rows`: body 140 × 30 → rows contain `136`, `◉ running`, `T30-nvidia`, `131`, `✓ done`, `✖ failed`,
  `Refactor the parser`, `3 sessions · 1 running`.
- `session_panel`: the focused session is `#131`: a row contains `#131 · T24-setup-wizard` (the panel title),
  `in 145.0K  out 112.2K`, `✓ done`.
- `timeline_rows`: rows contain `16:30:01`, `✎ edit`, `src/setup.rs`, `+3 −1`, `$ shell`, `cargo test --test setup`,
  `test result: FAILED. 8 passed; 1 failed`, `◉ think`; the `FAILED` detail cells are `Critical`.
- `not_configured`: `App::new(1000)` on the Agents screen → `OpenCode is not configured`.
- `agents_keys`: `5` → Agents; `Down` → `agent_selected` 1 and `agent_focus` the second session's id; `a` →
  `agent_focus` `None`; `Tab` from `Agents` → `Overview`.
- `running_dot`: the header (140 × 1) of an app with the fixture snapshot contains `5 Agents ●`.
- `agents_golden`: `tests/snapshots/agents-140x40.txt` (the fixture app, Agents screen), same `UPDATE_GOLDEN=1`
  mechanism; read it and describe it in the report.

## Acceptance criteria

- [ ] The contract above is implemented; with `--opencode`, screen 5 follows the running session live.
- [ ] Every earlier test passes (updated for five screens); the 8 tests above pass.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the timeline of a running job moves on the real devbox.
