# T32 — TUI v2: one-line header, layout, Requests table, key bar

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T31 |
| **Read first** | `src/ui/layout.rs`; `src/ui/draw.rs`; `src/ui/panels/header.rs`; `src/ui/panels/request.rs`; `src/ui/panels/log.rs` (`grep -n "pub \|fn render"`); `src/app/state.rs` (`requests`) |
| **Deliverable** | changes to `src/ui/layout.rs`, `src/ui/draw.rs`, `src/ui/panels/header.rs`, `src/ui/panels/request.rs`, `src/ui/panels/log.rs`; `src/ui/keybar.rs`; tests and golden files updated |

## Why

The validated mockups give the Overview a new skeleton: a one-line header instead of a three-line box, a
model band across the whole width, the GPU / CPU / Memory panels as three equal columns, the log renamed
Activity, the last requests as a table, and a key bar on the last line. This sheet builds the skeleton and the
two panels that change little (Requests, Activity). T33 redraws the model band and the machine panels inside
the rectangles placed here: until then the model panel is simply cut to its new height.

## Contract

### `src/ui/panels/header.rs` — one line

`pub const HEIGHT: u16 = 1;` and `render` draws, on the first row of `area`, no frame:

- left: ` llamatop ` (`Role::BorderFocused` background: `Style::default().fg(<terminal background>)` is not
  available, so use `theme.fg(Role::BorderFocused)` plus `Modifier::REVERSED | Modifier::BOLD`), then
  ` <hostname>  up <duration>  <dot> <status>  <build>` (hostname and build in `Text`, `up …` in `Dim`, the
  status with its role, the same status texts as today; build omitted when unknown);
- right-aligned: `PAUSED  ` (`Warning`, bold) when paused, then `<clock>  ? help` (clock `Text`, help `Dim`);
- tabs between them: ` <n> <title> ` per screen separated by one space, the current one with
  `BorderFocused` + `REVERSED | BOLD`, the others `Dim`; placed so they are centred in the space left between
  the two parts; when that space is too small, the right part is dropped first, then the tabs.

### `src/ui/keybar.rs`

```rust
/// The key bar, last row of the Overview: `q quit · ? help · p pause · +/- refresh <s> s · / filter · e level
/// · r raw · x proxy`, `<s>` = `interval_ms / 1000` with one decimal, all `Dim`, cut at the width.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

### `src/ui/layout.rs`

`OverviewLayout` keeps its fields (`model` is the model band, `request` the Requests table, `log` Activity) and
gains `pub keybar: Option<Rect>`. New rules (`header` is row 0, height 1; the key bar is the last row; the
body is between them):

- **Wide** — `graphs` true. Model band: 6 rows, full width. Machine row: 9 rows, three columns `gpu`, `cpu`,
  `memory` of widths `a = w / 3`, `b = (w - a) / 2`, `c = w - a - b`. Then `log` (`w * 63 / 100` wide) and
  `request` (the rest) side by side on the remaining rows.
- **Medium** — `graphs` true. Model band 6 rows; machine row 8 rows, same three columns; `log` full width on
  the rest; no `request`.
- **Narrow** — `graphs` false. Model band 6 rows; `gpu` 6 rows full width; `cpu` | `memory` 5 rows (halves);
  `log` the rest.
- Degrading as today: when `log` would get fewer than 3 rows, drop in order `request`, the `cpu` | `memory`
  row (Narrow), then `log`. `keybar` is `None` when the body would have fewer than 9 rows.

### `src/ui/panels/request.rs` — the Requests table

```rust
/// Height needed, borders included (header row and one request).
pub const MIN_HEIGHT: u16 = 4;
/// The Requests panel: `frame::panel("Requests", Role::Server, false, theme)`, right title `last <n>` (`Dim`),
/// a header row and the newest `app.requests` first, as many as fit, keeping the last inner row for the
/// summary when there are at least 3 inner rows.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Columns (one space between): `time` (8, left), `prompt` (7, right), `gen` (7, right), `t/s` (6, right:
`text::tps(gen_tps)`), `draft` (6, right: `text::pct(draft_pct)`); header in `Dim`, values `Text`, a missing
value `-`. Numbers of tokens with a thin grouping: `1 043`, `10 229` (a space every three digits). Summary row
(`Dim`): `avg <mean of the gen_tps known, text::tps> t/s · <n> req · <count(sum of prompt + gen tokens)> tok`.
No request yet: one `Dim` row `no request yet`.

### `src/ui/panels/log.rs`

The frame title becomes `Activity` (everything else unchanged). The Logs screen keeps calling the same
renderer: its title becomes `Activity` too, which is fine.

### `src/ui/draw.rs`

Draw the header on `layout.header`, the panels on their rectangles, and `keybar::render` on `layout.keybar`.
The other screens use the body under the one-line header, as they do today under the three-line one.

## Tests to update, and new ones

Existing tests that encode the old skeleton are updated to the contract above — never weakened:
`tests/layout.rs` (`wide_layout`, `medium_layout`, `narrow_layout`, `compact_rows` untouched),
`tests/panels_model.rs` (`header_generating`, `header_states`, `header_current_tab`, `request_panel`), and every
test that renders a whole frame. Golden files: regenerate with `UPDATE_GOLDEN=1`, then read them.

New expected values:

- `wide_layout` (140 × 40): header `(0,0,140,1)`, model `(0,1,140,6)`, gpu `(0,7,46,9)`, cpu `(46,7,47,9)`,
  memory `(93,7,47,9)`, log `(0,16,88,23)`, request `(88,16,52,23)`, keybar `(0,39,140,1)`.
- `medium_layout` (100 × 30): model `(0,1,100,6)`, gpu `(0,7,33,8)`, cpu `(33,7,33,8)`, memory `(66,7,34,8)`,
  log `(0,15,100,14)`, no request, keybar `(0,29,100,1)`.
- `narrow_layout` (80 × 24): model `(0,1,80,6)`, gpu `(0,7,80,6)`, cpu `(0,13,40,5)`, memory `(40,13,40,5)`,
  log `(0,18,80,5)`, keybar `(0,23,80,1)`.
- `header_generating`: `demo::app(Generating)` at 140 × 1 → the row contains `llamatop`, `gpu-box`, `up 4d 3h`,
  `● server ok`, `b12151`, `1 Overview`, `4 System`, `10:21:03`, `? help`, and not `5 Agents` (4 screens until T35).
- `request_panel`: `demo::app(Generating)`, 50 × 6 → rows contain `time`, `prompt`, `8 192`, `1 040`, `52.0`,
  `70 %`, and `avg 52.0 t/s · 1 req · 9.2K tok`.
- New, in `tests/tui2_layout.rs`: `keybar_text` (`App::new(500)`, 100 × 1 → contains `+/- refresh 0.5 s` and
  `x proxy`), `requests_newest_first` (an `App` fed the fixture journal, Requests 50 × 6 → the row of `10 229`
  is above the row of `48`), `thousands` (the helper you write for `1 043`: `pub fn group_thousands(n: u64) ->
  String` in `src/ui/text.rs`: 0 → `0`, 999 → `999`, 1043 → `1 043`, 1234567 → `1 234 567`).

## Pitfalls

- Every frame-level golden file changes; regenerate them only after the rectangles are right.
- The model panel (until T33) needs 8 rows and gets 6: it already drops the rows that do not fit.
- The other screens' bodies start one row higher than before: their tests that look at absolute rows change.

## Acceptance criteria

- [ ] The contract above is implemented; the Overview has the new skeleton at every tier.
- [ ] The updated and new tests pass; golden files regenerated and read.
- [ ] `bash .devbox/check` is green.
