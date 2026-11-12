# T14 — Responsive layout, Overview screen, `--snapshot`, golden files

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T13 |
| **Read first** | `docs/02-ui.md` § 3 and § 6; `grep -n "pub " src/ui/*.rs src/ui/panels/*.rs src/app/*.rs src/config.rs` |
| **Deliverable** | `src/ui/layout.rs`, `src/ui/compact.rs`, `src/ui/draw.rs`, `src/ui/snapshot.rs`, `tests/layout.rs`, `tests/snapshots/*.txt`; `pub mod` lines in `src/ui/mod.rs`; five flags in `src/config.rs`; the `--snapshot` branch in `src/main.rs` |

## Why

The panels exist (T12, T13); this sheet places them. The layout is recomputed from the terminal size on every
frame and degrades as `docs/02-ui.md` § 3 says. Because nobody can look at the screen, the sheet also adds
`llamatop --snapshot WxH`, which renders one frame of the demo to standard output, and golden files: one
rendered frame per size, compared in `cargo test`. The orchestrator reviews the golden files themselves.

## Contract

### `src/ui/layout.rs`

```rust
/// The layout family chosen from the terminal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { Wide, Medium, Narrow, Compact, TooSmall }

/// First match: `w < 30 || h < 8` → `TooSmall`; `w < 60 || h < 20` → `Compact`; `w < 100` → `Narrow`;
/// `w >= 140 && h >= 40` → `Wide`; else `Medium`.
pub fn tier(width: u16, height: u16) -> Tier;

/// Where each Overview panel goes; `None` = not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverviewLayout {
    pub header: Rect,
    pub model: Option<Rect>,
    pub request: Option<Rect>,
    pub log: Option<Rect>,
    pub gpu: Option<Rect>,
    pub cpu: Option<Rect>,
    pub memory: Option<Rect>,
    /// Passed to the panels as `Ctx::graphs`.
    pub graphs: bool,
}

/// The Overview layout of `area` for the `Wide`, `Medium` and `Narrow` tiers (rules below). Any other
/// tier returns the `Narrow` layout; callers check `tier` first.
pub fn overview(area: Rect) -> OverviewLayout;
```

Rules. `header` is always the top 3 rows; `body` is the rest. Splits of a width `w` in two give the left
part `w / 2` (floor) and the right part the rest. Heights are exact, in rows:

- **Wide** — `graphs` true. Body in two columns. Left: `model` 8 rows, `request` 5 rows, `log` the rest.
  Right: `gpu` `body.height * 55 / 100` rows, then `cpu` | `memory` side by side on the rest.
- **Medium** — `graphs` true. Band 1, 8 rows: `model` | `gpu`. Band 2, 6 rows: `cpu` | `memory`. `log`: the
  rest, full width. No `request`.
- **Narrow** — `graphs` false. One column: `model` 8, `gpu` 6, `cpu` | `memory` 5, `log` the rest.

Degrading, when the body is too short: a `log` under 3 rows is not enough. Then drop, one at a time and
recomputing after each, in this order until `log` has 3 rows: `request`, then the `cpu` | `memory` band
(both), then `log` itself. `model` and `gpu` are never dropped; if they do not fit, they are cut to the body.
Rectangles never overlap and never leave `area`.

### `src/ui/compact.rs`

```rust
/// The compact tier: no frames, one line per subsystem, then the log.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Rows: 0 — `llamatop · <hostname> · <dot> <status>` (same status texts and roles as the header of T12);
1 — `<badge> <short model name> <tps> t/s` (the Model panel's badge and speed rules; a missing model shows
the badge alone); 2 — `GPU <busy>% <junction>°C <vram used, util::bytes> │ CPU <usage>% │ RAM <used,
util::bytes>` (no space before `%` and `°C` here, to save room; `n/a` for missing values); then the end of
`visible_logs()` on the remaining rows, `<time> <text>`, coloured by level as in the Log panel. Every row cut
at the width.

### `src/ui/draw.rs`

```rust
/// Draws one whole frame of `app` into `area`.
pub fn draw(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

By `tier(area.width, area.height)`: `TooSmall` → `terminal too small` centred (`Dim`), and on the next row,
when it fits, `<w>×<h>, need 30×8`; `Compact` → `compact::render`; otherwise `header::render` on
`layout.header`, then each present panel on its rectangle, with `Ctx { graphs: layout.graphs, ..*ctx }`.
Until T16–T18 add their screens, the Models, Logs and System screens show the Overview body too (the header
already shows which tab is selected). Never panics, whatever the size (0 × 0 included).

### `src/ui/snapshot.rs`

```rust
/// Renders one frame of `app` at `width × height` with `draw`, clock `10:21:03`.
/// Plain (`ansi` false): each row's symbols with trailing spaces removed, rows joined by `\n`, final `\n`.
/// ANSI: the same rows, each cell preceded by an SGR sequence whenever its style differs from the previous
/// cell's (`\x1b[0m` then `1` for bold, `7` for reversed, `38;2;r;g;b` for an `Rgb` foreground,
/// `30`–`37` / `90`–`97` for the 16 named colours), each row ending with `\x1b[0m`; trailing spaces kept.
pub fn snapshot(app: &App, theme: &Theme, width: u16, height: u16, ansi: bool) -> String;

/// Parses `WxH` (`140x40`; `X` accepted too). Both between 1 and 1000, else `None`.
pub fn parse_size(text: &str) -> Option<(u16, u16)>;
```

### `src/config.rs` — five new flags (and nothing else changed)

```rust
/// Show generated data instead of the real server and machine.
#[arg(long)]
pub demo: bool,
/// With `--snapshot`: the demo state to render.
#[arg(long, value_enum, default_value = "generating")]
pub demo_state: crate::app::demo::DemoState,
/// Render one frame of the demo at `WxH` (e.g. `140x40`) on standard output, then exit.
#[arg(long, value_name = "WxH")]
pub snapshot: Option<String>,
/// With `--snapshot`: keep the colours as ANSI escape sequences.
#[arg(long)]
pub ansi: bool,
/// With `--snapshot`: the screen to render, 1–4 (Overview, Models, Logs, System).
#[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
pub screen: u8,
```

The `defaults` test of `config.rs` must keep passing.

### `src/main.rs`

After the `--dump` branch: when `cli.snapshot` is set, parse it with `parse_size` (an invalid size is an
error: `invalid --snapshot size, expected WxH`), build `demo::app(cli.demo_state)` with
`screen = Screen::ALL[cli.screen - 1]`, and print `snapshot(&app, &Theme::new(ColorMode::detect()), w, h,
cli.ansi)` with `print!`; exit 0. The last branch (dashboard not implemented) stays for T15.

## Golden files

`tests/snapshots/overview-<W>x<H>.txt` for `140x40`, `100x30`, `80x24`, `50x15` and `20x6`: plain snapshots
of `demo::app(DemoState::Generating)`, `TrueColor` theme. The test compares the file with a fresh render.
When the environment variable `UPDATE_GOLDEN` is `1`, the test writes the files instead and passes: run it
once that way to create them, then **read each file** and check it against `docs/02-ui.md` § 2.1 and § 3
(right panels, nothing cut in the middle of a number, borders closed). Describe what you checked in the report.

## Pitfalls

- `Rect::new(x, y, w, h)` is cheap: build rectangles by hand with the integer rules above, not with
  `Layout` constraints, whose solver may round differently from the rules.
- Subtract with `saturating_sub` everywhere: the layout is called on every size from 0 × 0.
- `ColorMode::detect()` reads the environment: the tests must call `snapshot` with an explicit theme, never
  `main`.
- A golden file is data written by the test, not a fixture: it may be regenerated when the rendering
  changes on purpose, never edited by hand.

## Tests

`tests/layout.rs`, exactly these names.

- `tiers`: `tier(29, 40)` TooSmall; `(30, 7)` TooSmall; `(30, 8)` Compact; `(59, 40)` Compact; `(120, 19)`
  Compact; `(60, 20)` Narrow; `(99, 50)` Narrow; `(100, 30)` Medium; `(160, 39)` Medium; `(140, 40)` Wide.
- `wide_layout`: `overview(Rect::new(0, 0, 140, 40))` → header `(0, 0, 140, 3)`, model `(0, 3, 70, 8)`,
  request `(0, 11, 70, 5)`, log `(0, 16, 70, 24)`, gpu `(70, 3, 70, 20)`, cpu `(70, 23, 35, 17)`,
  memory `(105, 23, 35, 17)`, graphs true.
- `medium_layout`: `(100, 30)` → model `(0, 3, 50, 8)`, gpu `(50, 3, 50, 8)`, cpu `(0, 11, 50, 6)`,
  memory `(50, 11, 50, 6)`, log `(0, 17, 100, 13)`, no request.
- `narrow_layout`: `(80, 24)` → model `(0, 3, 80, 8)`, gpu `(0, 11, 80, 6)`;
  the log would get 2 rows, too few, so the cpu | memory band is dropped: cpu and memory `None`,
  log `(0, 17, 80, 7)`; graphs false. At `(80, 30)`: cpu `(0, 17, 40, 5)`, memory `(40, 17, 40, 5)`,
  log `(0, 22, 80, 8)`.
- `layout_invariants`: for every `w` in `30..=200` step 5 and `h` in `20..=60` step 4 (tiers Narrow and
  above): every present rectangle lies inside the area, no two overlap, `model` and `gpu` are present,
  a present `log` has at least 3 rows, and present panels have at least their `MIN_HEIGHT` rows.
- `draw_never_panics`: `draw` of `demo::app(Generating)` and of `App::new(1000)` into every size `w` in
  `0..=160` step 7, `h` in `0..=50` step 3.
- `too_small_message`: 20 × 6 → a row contains `terminal too small` and another `20×6, need 30×8`.
- `compact_rows`: `demo::app(Generating)` at 50 × 15 → row 0 contains `llamatop · aibox-flo · ● server ok`,
  row 1 contains `GENERATING` and `t/s`, row 2 contains `GPU 95% 73°C 15.9 G │ CPU 50% │ RAM 21.6 G`,
  the last row contains `all slots are idle`.
- `parse_sizes`: `140x40` → `(140, 40)`; `80X24` → `(80, 24)`; `0x10`, `10`, `axb`, `2000x10` → `None`.
- `ansi_snapshot`: `snapshot(&demo::app(Generating), &T, 100, 30, true)` contains `\x1b[38;2;255;158;100m`
  (the `Server` colour) and `\x1b[0m`; the plain snapshot contains no `\x1b`.
- `golden_files`: the five golden files described above.

## Acceptance criteria

- [x] The files exist with the exact public API above; `src/config.rs` gains exactly the five fields.
- [x] `cargo run -- --snapshot 140x40` prints the generating demo; `--snapshot 80x24 --demo-state down`
      prints the Down state; `--snapshot 10` fails with the message above.
- [x] The 11 tests above exist under these names and pass; the five golden files are committed.
- [x] `bash .devbox/check` is green.
- [ ] (Orchestrator) the golden files read right.
