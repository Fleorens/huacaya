# T17 — Logs screen and help overlay

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T16 |
| **Read first** | `docs/02-ui.md` § 2 (Logs) and § 5; `grep -n "pub " src/ui/*.rs src/ui/panels/*.rs src/ui/screens/*.rs src/app/state.rs` |
| **Deliverable** | `src/ui/screens/logs.rs`, `src/ui/help.rs`, `tests/logs_help.rs`, `tests/snapshots/logs-*.txt`, `tests/snapshots/help-*.txt`; `pub mod logs;` in `src/ui/screens/mod.rs`, `pub mod help;` in `src/ui/mod.rs`; the Logs branch and the help overlay in `src/ui/draw.rs` |

## Why

The Logs screen is the server log full screen. All its behaviour (follow, scroll, filter, level filter,
proxy lines) is already in `App` (T09) and the Log panel (T13) already draws a positioned view; this sheet
gives it the whole body and a status row that says what the view shows and how to change it. The help
overlay (`?`) lists every key.

## Contract

### `src/ui/screens/logs.rs`

```rust
/// The Logs screen in `area` (the body below the header): `panels::log::render` on every row but the last,
/// then the status row on the last one.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);

/// `all`, `warn+err`, `errors`.
pub fn level_name(level: LevelFilter) -> &'static str;
```

Status row, left to right, segments separated by three spaces:
- while `log_view.editing_filter`: only `/<filter>▏` (`Warning`) then `   Enter keep   Esc clear` (`Dim`);
- otherwise: `follow ●` (`Good`) or `scroll −<scroll>` (`Warning`); `level <level_name>`; `filter "<filter>"`
  or `filter none`; `proxy hidden` or `proxy shown`; `<visible> of <total> lines` (`visible_logs().len()`
  and `logs.len()`); labels `Dim`, values `Text`. Then, right-aligned and only when it fits after the
  segments: `f follow  / filter  e level  x proxy  ↑↓ scroll` (`Dim`).

### `src/ui/help.rs`

```rust
/// The help overlay, centred in `area`, drawn over whatever is there.
pub fn render(app: &App, theme: &Theme, area: Rect, buf: &mut Buffer);
```

A box of width `min(area.width, 56)` and height `min(area.height, 13)`, at
`x = area.x + (area.width - w) / 2`, `y = area.y + (area.height - h) / 2`; first `ratatui::widgets::Clear`
on it, then `frame::panel("Help", Role::BorderFocused, true, theme)`, then these 11 rows: the keys
(`Server`, padded to 15) then the action (`Text`):

| Keys | Action |
|---|---|
| `1-4 Tab S-Tab` | `switch screen` |
| `q Ctrl-C` | `quit` |
| `? Esc` | `close this help` |
| `p` | `pause / resume the display` |
| `+ -` | `faster / slower refresh (now <s> s)` — `<s>` = `interval_ms / 1000` with one decimal |
| `↑ ↓ PgUp PgDn` | `scroll (Logs, Models)` |
| `Home End` | `top / bottom` |
| `f` | `follow the log` |
| `/` | `filter the log, Esc clears` |
| `e` | `level filter: all, warnings, errors` |
| `x` | `show / hide the router's proxy lines` |

Rows that do not fit are dropped from the bottom.

### `src/ui/draw.rs`

On `Screen::Logs` (tiers other than `TooSmall` / `Compact`): the header, then `screens::logs::render` on the
body. Then, on every screen and tier except `TooSmall`, when `app.help`: `help::render(app, theme, area, buf)`
over the whole frame, last.

### `docs/02-ui.md`

Add the `x` key to the table of § 5: `| x | show / hide the router's proxy lines (Logs; hidden by default:
llamatop's own polling makes the router print two per round) |`.

## Pitfalls

- `Clear` is a widget: `Clear.render(rect, buf)` resets the cells so the box is not see-through.
- `▏`, `●`, `−`, `↑`, `↓` are one column each.
- The demo `App` (Generating) has 18 log lines, 17 visible (its one `Proxy` line is hidden).

## Tests

`tests/logs_help.rs`, exactly these names; same helpers as `tests/panels_model.rs`. `body` = the screen
renderer alone into a `w × h` buffer; `frame` = `draw::draw` of the whole frame.

- `level_names`: `all`, `warn+err`, `errors`.
- `logs_status_row`: `demo::app(Generating)`, body 120 × 20 → the last row contains `follow ●`, `level all`,
  `filter none`, `proxy hidden`, `17 of 18 lines` and `x proxy`; row 0 contains `Log`.
- `logs_status_states`: same app with `log_view.follow = false`, `scroll = 2`, `level = Errors`,
  `filter = "slot"`, `show_proxy = true` → the last row contains `scroll −2`, `level errors`, `filter "slot"`,
  `proxy shown`.
- `logs_editing_filter`: `editing_filter = true`, `filter = "n_g"` → the last row starts with `/n_g▏` and
  contains `Esc clear`, and not `level`.
- `logs_narrow`: body 40 × 10 → the last row contains `follow ●` and not `x proxy`; no panic at 1 × 1 and
  0 × 0.
- `help_overlay`: `demo::app(Generating)` with `help = true`, frame 100 × 30 → the cell `(22, 8)` is `╭`;
  rows contain `Help`, `switch screen`, `faster / slower refresh (now 1.0 s)`, `show / hide the router's
  proxy lines`; with `interval_ms = 500` → `(now 0.5 s)`.
- `help_small`: frame 40 × 12 with `help = true` → a row contains `Help`; no panic at 30 × 8.
- `logs_and_help_golden`: golden files `tests/snapshots/logs-120x30.txt` (`demo::app(Generating)`,
  `screen = Logs`) and `tests/snapshots/help-100x30.txt` (Overview with `help = true`), same `UPDATE_GOLDEN=1`
  mechanism as T14. Read them and describe them in the report.

## Acceptance criteria

- [x] The files exist with the exact public API above; `draw.rs` routes the Logs screen and draws the help
      overlay last; `docs/02-ui.md` has the `x` key.
- [x] Nothing outside the deliverable changed (except `tasks/`).
- [x] The 8 tests above exist under these names and pass; earlier golden files are unchanged.
- [x] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) in `--demo`: `3`, `/`, typing, `Enter`, `e`, `x`, `?` behave as described.
