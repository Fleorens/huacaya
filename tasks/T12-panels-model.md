# T12 — Text helpers, header, Model and Last request panels

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T10, T11 |
| **Read first** | `docs/02-ui.md` § 2.1, § 2.2; `grep -n "pub " src/ui/*.rs src/app/*.rs src/util/format.rs` |
| **Deliverable** | `src/ui/text.rs`, `src/ui/panels/mod.rs`, `src/ui/panels/header.rs`, `src/ui/panels/model.rs`, `src/ui/panels/request.rs`, `tests/panels_model.rs`; `pub mod panels; pub mod text;` in `src/ui/mod.rs` |

## Why

The Overview screen is a set of panels. This sheet writes the server-side ones (the header with the tabs,
the Model panel, the Last request panel) and the small text helpers every panel shares. T13 writes the
machine-side panels, T14 lays them out. Each panel draws into a `ratatui::buffer::Buffer` inside the `Rect`
it is given and never outside, so it is tested by rendering into a `Buffer::empty`.

The data comes from `App` (T09) and, in the tests, from `app::demo::app(state)` (T11).

## Contract

### `src/ui/text.rs`

```rust
/// Tokens per second: one decimal below 100 (`62.5`), none from 100 (`643`); `None` → `n/a`.
pub fn tps(v: Option<f64>) -> String;
/// A percentage, no decimal, a space before `%`: `96 %`; `None` → `n/a`.
pub fn pct(v: Option<f64>) -> String;
/// A temperature, no decimal: `70 °C`; `None` → `n/a`.
pub fn temp(v: Option<f32>) -> String;
/// `s` cut to at most `width` characters (chars, not bytes); when it is cut, its last kept character is
/// replaced by `…`. `width` 0 → empty.
pub fn fit(s: &str, width: usize) -> String;
/// `used/total U` in the unit of `total`: the largest of B, K, M, G, T (powers of 1024) that `total`
/// reaches; one decimal for K and above (`14.8/16.0 G`), integers for B (`500/1000 B`).
pub fn bytes_pair(used: u64, total: u64) -> String;
```

### `src/ui/panels/mod.rs`

```rust
pub mod header;
pub mod model;
pub mod request;

/// What every panel needs besides the state.
#[derive(Debug, Clone, Copy)]
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    /// `false` on narrow terminals: panels drop their braille graphs.
    pub graphs: bool,
    /// Local wall-clock time shown in the header, `HH:MM:SS`; given by the caller.
    pub clock: &'a str,
}
```

Every `render` below draws only inside `area`, and does nothing when `area` is smaller than 3 × 3. Text is
written with `Buffer::set_line` / `set_span` with a `max_width`, so nothing overflows; lines that do not fit
in the panel's height are dropped from the bottom.

### `src/ui/panels/header.rs`

```rust
/// Height of the header, borders included.
pub const HEIGHT: u16 = 3;
/// The header: a `frame::panel` with an empty title (border `Role::Border`) and two titles on the top
/// border, then the tabs on the inner row.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

- Left title: ` llamatop ─ <hostname> ─ up <util::duration(uptime_s)> ` (`?` for a missing hostname, `up n/a`
  for a missing uptime), `llamatop` in `Role::BorderFocused` bold, the rest `Role::Text`.
- Right title (right-aligned, `Line::from(…).right_aligned()` through `Block::title_top`):
  ` <dot> <status>  <build> ─ <clock> `, where the build is `build_info` up to its first `-` (omitted with its
  two spaces when `None`) and `<dot> <status>` is, in this order of checks:
  no llama snapshot → `○ waiting` (`Dim`); `Health::Down` → `● server down` (`Critical`);
  `Health::Unauthorized` → `● no API key` (`Warning`); `Health::Ok` → `● server ok` (`Good`).
- Inner row: for each `Screen::ALL[i]`, ` <i + 1> <title> ` followed by one space; the current screen's
  segment in `Role::BorderFocused` with `Modifier::BOLD | Modifier::REVERSED`, the others `Role::Dim`.
  At the right end of the row: `PAUSED  ` (`Warning`, bold) when `app.paused`, then `? help  q ` (`Dim`).
  When the row is too narrow for both, the tabs win.

### `src/ui/panels/model.rs`

```rust
/// Height needed, borders included (6 rows inside).
pub const MIN_HEIGHT: u16 = 8;

/// The state badge's text and role (first match wins):
/// no llama snapshot → `("WAITING", Dim)`; `Health::Down` → `("SERVER DOWN", Critical)`;
/// `Health::Unauthorized` → `("UNAUTHORIZED", Warning)`; a model `Loading` → `("LOADING", Warning)`;
/// no active model → `("NO MODEL", Dim)`; slot 0 of the first active model processing with
/// `n_decoded == 0` → `("PROMPT", Server)`; processing → `("GENERATING", Good)`; else `("IDLE", Dim)`.
pub fn badge_state(app: &App) -> (&'static str, Role);

/// The Model panel: `frame::panel("Model", Role::Server, false, theme)`, then the rows below.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

The badge is `frame::badge(text, role, theme, lit)` with `lit = app.frame / 5 % 2 == 0` for `PROMPT`,
`GENERATING` and `LOADING`, `lit = true` otherwise. Labels are `Role::Dim`, padded to 7 characters; values
`Role::Text`. With an active model `m` (`app.llama.active[0]`, its `ModelInfo` `info` found by id in
`models`, its slot `s = m.slots.first()`, `metrics = m.metrics`):

1. `<short name>   <quant>` — `util::short_model_name(&m.id)`; name `Text` bold, quant `Dim`.
2. the badge, then `   task <s.id_task>` (`Dim`) when the slot is processing.
3. `gen    <tps> t/s` with `m.live_gen_tps` while generating; otherwise `gen    <tps(avg_gen_tps)> t/s avg`.
   When `ctx.graphs` and at least 8 columns are left after the text plus 2 spaces, a one-row
   `BrailleGraph` of `app.gen_tps` (all of it; the graph keeps the last values) with
   `max = gen_tps.max().unwrap_or(0).max(1.0)` and accent `Server` fills the rest of the row.
4. `prompt <tps(avg_prompt_tps)> t/s   cache <pct(cache_reuse_pct)>`.
5. `ctx    <gauge>  <count(used)> / <count(s.n_ctx)>` with `used = s.n_prompt_tokens + s.n_decoded`; the
   gauge (`Role::Server`, `PCT_THRESHOLDS`, ratio `used / n_ctx`) takes the width left between the label
   and two spaces before the value; no gauge under 4 columns; no slot → `ctx    n/a`.
6. `draft  <pct(draft_acceptance_pct)>` then, for each one present in `info`, `   ctx-size <count>`,
   `   n-cpu-moe <n>`, `   threads <n>`; the whole row passed through `text::fit`.

Without an active model: row 1 is the name of the `Loading` model when there is one (else empty), row 2
the badge, row 3 a `Dim` message: `Down(msg)` → `msg`; `Unauthorized` → `set LLAMA_API_KEY or
--api-key-file`; `LOADING` → `loading <short name>…`; `NO MODEL` → `no model loaded — <n> available`;
`WAITING` → `waiting for the first poll…`.

### `src/ui/panels/request.rs`

```rust
/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;
/// The Last request panel: `frame::panel("Last request", Role::Server, false, theme)` and three rows:
/// `prompt <tokens> tok   <tps> t/s`, `gen    <tokens> tok   <tps> t/s`,
/// `draft  <pct>   ctx <count(ctx_tokens)>   at <time>` (`n/a` for each missing value; labels `Dim`,
/// padded to 7). Without a last request: one `Dim` row `no request yet`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

## Pitfalls

- `Block::inner(area)` gives the rectangle inside the borders; render the block first, then the rows.
- `title_top(Line::from(spans).right_aligned())` puts a second title at the right of the top border in
  ratatui 0.30; the left title is a plain `title_top(Line::from(spans))`.
- `Buffer::set_line(x, y, &line, max_width)` truncates at `max_width` columns: always pass the space left.
- Mind `°` and `…` and `─`: they are one column but several bytes. Use `.chars().count()`, never `.len()`,
  to measure text.
- The demo `App` has `frame == 0`, so every badge is lit (`◉`) in the tests.
- Expected numbers below come from the T11 formulas: `828751 / 1402.01 = 591.1`,
  `41518700 / 42347451 = 98.0 %`, `333880 / 517905 = 64.5 %`, `507212 / 8948.33 = 56.7`, and at
  `t = 119` the generating slot holds `57651 + 6189 = 63840` tokens.

## Tests

`tests/panels_model.rs`, exactly these names. Helper in the test file: `fn draw(f, w, h) -> Vec<String>`
rendering into `Buffer::empty(Rect::new(0, 0, w, h))` and returning each row's symbols concatenated.
`T` = `Theme::new(ColorMode::TrueColor)`, `ctx` = `Ctx { theme: &T, graphs: true, clock: "10:21:03" }`.

- `text_helpers`: `tps(Some(62.54))` `62.5`; `tps(Some(643.2))` `643`; `tps(None)` `n/a`;
  `pct(Some(96.4))` `96 %`; `temp(Some(70.4))` `70 °C`; `fit("abcdef", 4)` `abc…`; `fit("abc", 4)` `abc`;
  `fit("héllo", 3)` `hé…`; `fit("abc", 0)` empty; `bytes_pair(15_894_315_008, 17_163_091_968)`
  `14.8/16.0 G`; `bytes_pair(1_310_720, 8_589_930_496)` `0.0/8.0 G`; `bytes_pair(500, 1000)` `500/1000 B`.
- `header_generating`: `demo::app(Generating)`, 100 × 3 → row 0 contains `llamatop ─ aibox-flo ─ up 4d 3h`,
  `● server ok  b12151 ─ 10:21:03` and ends with `╮`; row 1 contains `1 Overview`, `4 System`, `? help`.
- `header_states`: `App::new(1000)` → row 0 contains `○ waiting`; `demo::app(Down)` → `● server down`;
  `demo::app(Generating)` with `paused = true` → row 1 contains `PAUSED`.
- `header_current_tab`: `demo::app(Idle)` with `screen = Screen::Logs`: the cell under the `3` of
  `3 Logs` has `Modifier::REVERSED`, the cell under the `1` of `1 Overview` does not.
- `badge_states`: `badge_state` of `demo::app(s)` for Idle, Prompt, Generating, Loading, Down →
  `IDLE`, `PROMPT`, `GENERATING`, `LOADING`, `SERVER DOWN`; of `App::new(1000)` → `WAITING`.
- `model_generating`: `demo::app(Generating)`, 60 × 8 → rows contain `Cyber-Tiel-Coder-35B-A3B   Q4_K_M`,
  `◉ GENERATING   task 9000`, ` t/s` on the `gen` row (and no ` avg`), `prompt 591 t/s   cache 98 %`,
  `63.8K / 196.6K`, `draft  64 %`, `n-cpu-moe 21`.
- `model_idle`: `demo::app(Idle)`, 60 × 8 → `◉ IDLE`, `gen    56.7 t/s avg`, `9.2K / 196.6K`.
- `model_without_active`: `demo::app(Down)` 60 × 8 → `SERVER DOWN` and `Connection refused`;
  `demo::app(Loading)` → `LOADING` and `loading Cyber-Tiel-Coder-35B-A3B…`.
- `model_graph_toggle`: `demo::app(Generating)`, 60 × 8: with `graphs: true` the `gen` row holds at least one
  braille character (`'\u{2800}'..='\u{28FF}'`); with `graphs: false` it holds none.
- `request_panel`: `demo::app(Generating)`, 50 × 5 → `prompt 8192 tok   891 t/s`,
  `gen    1040 tok   52.0 t/s`, `draft  70 %   ctx 9.2K   at 10:00:12`; `App::new(1000)` →
  `no request yet`.
- `panels_stay_inside`: for each of the three panels and each demo state, render into a 30 × 6 area at
  `(5, 2)` of a 40 × 10 buffer: every cell outside that area is still a space.

## Acceptance criteria

- [x] The files exist with the exact public API above; `src/ui/mod.rs` declares `pub mod panels;` and
      `pub mod text;`.
- [x] Nothing outside the deliverable changed (except `tasks/`).
- [x] The 11 tests above exist under these names and pass.
- [x] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the header and the Model panel read well in a real terminal.
