# T43 — The activity map widget: braille dots, stations, spokes, pulses, context ring

| | |
|---|---|
| **Batch** | 8 — the Agents map (design validated by Florian on 2026-11-13) |
| **Depends on** | T42 |
| **Read first** | `src/collect/activity.rs` (whole file); `src/ui/theme.rs` (`Role`, `rgb`, `lerp_rgb`, `gauge_color`, `PCT_THRESHOLDS`, `ColorMode`); `src/ui/text.rs` (`tps`); `tests/agents_screen.rs` (the golden mechanism) |
| **Deliverable** | `src/ui/dots.rs`, `src/ui/map.rs`, `tests/map.rs`, `tests/snapshots/map-86x23.txt`; `pub mod dots; pub mod map;` in `src/ui/mod.rs` |

## Why

The Agents screen shows the followed OpenCode session as a map, in the spirit of the "lot-map" mod for Claude
Code: the model in the centre inside a thin context ring, eight stations around it, a spoke to each, a pulse
flying along a spoke when a step starts or ends. Florian validated the look on a mockup on 2026-11-13, after
rejecting three others: what made the first one look clumsy ("ça fait pâté") was a doubled ring, an inner
circle, a cross-shaped pulse, a braille clock around the node and too little room. **Follow the rules below
exactly**; they are what makes it look clean. This sheet draws the map alone, in any rectangle; T44 puts it
on the Agents screen.

## Contract

### `src/ui/dots.rs`

```rust
/// A braille canvas: 2 × 4 dots per cell. Dots are OR-ed; each cell has one colour, the one painted with
/// the highest priority (on a tie, the last painted).
pub struct Dots { /* private */ }

impl Dots {
    /// A blank canvas of `width × height` cells.
    pub fn new(width: u16, height: u16) -> Self;
    /// Size in dots: `(width * 2, height * 4)`.
    pub fn size(&self) -> (i32, i32);
    /// Sets the dot at `(x, y)` (dots, origin top-left, y down). Outside the canvas: ignored.
    pub fn plot(&mut self, x: i32, y: i32, color: Option<Color>, priority: u8);
    /// A straight line: `n = max(|dx|, |dy|).ceil()` steps (at least 1); point `k` (0..=n) is
    /// `from + (to - from) * k / n`, rounded; with `every > 1`, only points with `k % every == 0`.
    pub fn line(&mut self, from: (f64, f64), to: (f64, f64), color: Option<Color>, priority: u8, every: usize);
    /// An arc of radius `r` around `centre`: angle 0 is up, clockwise; from `start` turns over `turns`
    /// turns (1.0 = full circle); `n = (r * turns * 2π * 4).ceil() + 1` points, rounded.
    pub fn arc(&mut self, centre: (f64, f64), r: f64, start: f64, turns: f64, color: Option<Color>, priority: u8);
    /// Writes every cell that has at least one dot into `buf` at `area`'s top-left: the braille symbol
    /// (`U+2800` + bits) and, when the cell's colour is `Some`, that foreground. Cells without dots are untouched.
    pub fn render(&self, area: Rect, buf: &mut Buffer);
}
```

Bits of a cell, dot `(dx, dy)` with `dx` in 0..2 and `dy` in 0..4: `(0,0)=0x01 (0,1)=0x02 (0,2)=0x04
(1,0)=0x08 (1,1)=0x10 (1,2)=0x20 (0,3)=0x40 (1,3)=0x80`.

### `src/ui/map.rs`

```rust
/// The centre of the map: what llama.cpp says about the model serving the session.
pub struct Core {
    /// Short model name, e.g. `Coder-35B`.
    pub model: String,
    pub tps: Option<f64>,
    /// Context used, 0.0–1.0.
    pub ctx_ratio: Option<f64>,
}

/// Smallest area the map draws in.
pub const MIN_W: u16 = 60;
pub const MIN_H: u16 = 18;

/// The cell of each station inside `area` (relative to its top-left), in `Station::ALL` order.
pub fn station_cells(area: Rect) -> [(u16, u16); 8];

/// Draws the map of `activity` at wall-clock `now_ms` in `area`. Smaller than `MIN_W × MIN_H`: draws
/// nothing. Never panics.
pub fn render(activity: &Activity, core: &Core, now_ms: i64, theme: &Theme, area: Rect, buf: &mut Buffer);
```

#### Geometry (in dots, `dw = width * 2`, `dh = height * 4`, all `f64`)

- Centre: `cx = floor(dw / 4) * 2`, `cy = floor(dh / 8) * 4 + 1`.
- Radii of the station ellipse: `rx = min(dh / 2 * 1.25, dw / 2 - 30)`, `ry = dh / 2 - 10`.
- Station `k` (index in `Station::ALL`) at angle `a = 2π k / 8` (0 = up, clockwise):
  `x = cx + rx sin a`, `y = cy - ry cos a`, then snapped: `x = round(x / 2) * 2`, `y = floor(y / 4) * 4 + 1`.
  Its cell is `(x / 2, y / 4)` — what `station_cells` returns.
- Ring radius `ring = min(19, ry * 0.53)`.
- Spoke `k`: unit vector `u` from the centre to the station; from `centre + u * (ring + 4)` to
  `(x - u.x * 3, y - u.y * 4)`.

#### Colours

Station colours (`Role`): You `Text`, Model `Server`, Files `Cpu`, Shell `Memory`, Checks `Good`, Web `Gpu`,
Agents `Warning`, Tools `Text`. In `TrueColor`, with `bg = (0x1a, 0x1b, 0x26)`:
`faint = lerp_rgb(bg, rgb(Border), 0.7)`, and a station's recency
`r = clamp(1 - (now_ms - last_ms) / 120_000, 0, 1)` (0 without `last_ms`). In `Ansi16`, "faint" is
`theme.color(Role::Dim)` and a spoke takes the station colour when `r > 0.75`, else `Dim`. In `NoColor`, every
colour is `None`.

#### What is drawn, in this order (priorities in brackets)

1. **Spokes**. A station with `calls == 0`: a dotted line (`every = 3`) in faint [1]. Otherwise a solid line
   (`every = 1`) in `lerp_rgb(faint, station, 0.35 + 0.65 * r)` [6]. One dot wide; never doubled.
2. **Pulses** (`activity.pulses` with `0 <= now_ms - at_ms < PULSE_MS`). Progress `t = (now_ms - at_ms) /
   PULSE_MS`; the head is at `t` along the spoke for an outbound pulse and at `1 - t` for an inbound one.
   Head: the dots `(hx, hy)`, `(hx + 1, hy)`, `(hx, hy + 1)`, `(hx + 1, hy + 1)` in white `(255, 255, 255)`
   [20] — `Critical` for a failed inbound pulse. Tail: the 5 dots behind the head **on the spoke itself**
   (steps of one dot, towards where the pulse came from), dot `k` (1..=5) in
   `lerp_rgb(station, white, 0.5 - 0.08 k)` [19]. (`Ansi16`: head `White`, tail the station colour.)
3. **Context ring**: one full `arc` of radius `ring`, faint [3]; then the lit part from angle 0 over
   `ctx_ratio` turns [9] in `gauge_color(theme, Role::Good, ratio * 100, PCT_THRESHOLDS)`. Without
   `ctx_ratio`, only the faint ring. One circle, one dot wide — **no second ring, no inner circle**.
4. `Dots::render` into `area`.
5. **Nodes** — a text glyph on the station's cell, bold: `○` in `Dim` when `calls == 0`; `◉` in the station
   colour when `running > 0`; `●` in the station colour otherwise.
6. **Core text**, centred on column `round(cx / 2)` (a string of `n` chars starts `n / 2` columns left of
   it): row `floor(cy / 4) - 1` the model name (`Server`, bold); next row `<tps> t/s` (`text::tps`, `Text`);
   next row `ctx <n>%` (`n` = `ctx_ratio * 100` rounded, no space before `%`; `ctx n/a` without a ratio) in
   the ring's lit colour (`Dim` without a ratio).
7. **Labels**, one line each: `<Name> <calls>`, then ` ◷ m:ss` when `running > 0` (from `running_since` to
   `now_ms`, `m:ss` with `m` unbounded), then ` ✕<errors>` when `errors > 0`. Name bold in the station colour
   (`Dim`, not bold, when `calls == 0`); the count `Dim`; the clock in the station colour; the errors bold
   `Critical`. Placement, with `a` the station's angle: `|sin a| < 0.2` → centred on the node's column, on the
   row above the node (top station) or below it (bottom); `sin a > 0` → starts 2 columns right of the node;
   otherwise it ends 2 columns left of the node. Text outside `area` is clipped, never wrapped.

## Pitfalls

- The look depends on the details above: thin single lines, one ring, text glyph nodes, a small square pulse
  head with its tail on the spoke. Do not add a route between stations, a second ring, a circle inside the
  ring, filled discs for nodes, or a braille clock around a node.
- Pixel-exact positions come from the snapped integer dots: compute in `f64`, round only where the rules say.
- `station_cells(86 × 23)` is `[(43, 2), (63, 4), (71, 11), (63, 17), (43, 20), (23, 17), (15, 11), (23, 4)]`
  — check yours against it first.
- Keep each file under 400 lines: `dots.rs` is small; if `map.rs` grows, put the label code in a private
  helper function, not a new public API.

## Tests

`tests/map.rs`, exactly these names. Build the test `Activity` with `parse_steps(messages-real.json,
Some("/home/dev/work/.wt/llamatop-145"))` and `Activity::update("s1", &steps, 1794586720000)`, then a live
round `update("s1", &[<step id "x1", StepKind::Shell, summary "cargo test", time 1794586721000, end None,
ok None>], 1794586722000)`. Core: `Core { model: "Coder-35B".into(), tps: Some(62.5), ctx_ratio:
Some(0.617) }`. Render at `now_ms = 1794586722300` into an 86 × 23 buffer, `TrueColor` unless said.

- `braille_bits`: on `Dots::new(1, 1)`, plotting `(0,0)` gives `⠁`; adding `(1,0)` gives `⠉`; a fresh canvas with
  `(0,3)` gives `⡀`, with `(1,3)` gives `⢀`; plotting `(2, 0)` or `(-1, 0)` changes nothing.
- `dots_priority`: two plots in the same cell, priority 5 red then priority 2 blue → the cell is red; then
  priority 5 green → green.
- `cells_86x23`: `station_cells(Rect::new(0, 0, 86, 23))` equals the list in Pitfalls.
- `labels_and_core`: rows contain `Model 14`, `Files 5`, `Tools 1`, `Checks 2 ◷ 0:01`, `Web 0`,
  `Agents 0`, `You 0`, `Coder-35B`, `62.5 t/s`, `ctx 62%`.
- `nodes`: the glyph at the Checks cell `(43, 20)` is `◉`, at the Files cell `(71, 11)` `●`, at the Web cell
  `(23, 17)` `○`.
- `pulse_head`: some cell of column 43 between rows 13 and 19 has foreground `Rgb(255, 255, 255)` (the
  outbound pulse to Checks, half way); at `now_ms = 1794586722700` (pulse over) no cell of the buffer has that
  foreground.
- `ring_lit`: at least one cell has the foreground `gauge_color(Good, 61.7, PCT_THRESHOLDS)` and at least one
  cell above row 11 has the faint ring colour.
- `too_small`: a 59 × 23 and an 86 × 17 buffer stay blank.
- `no_color`: rendering with `NoColor` gives the same symbols as with `TrueColor`, and no cell has a
  foreground colour.
- `map_golden`: `tests/snapshots/map-86x23.txt` (symbols only, one line per row), same `UPDATE_GOLDEN=1`
  mechanism as `tests/agents_screen.rs`; read the file and describe it in your report.

## Acceptance criteria

- [ ] The contract above is implemented.
- [ ] Every earlier test passes; the tests above pass.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the map looks like the validated mockup.
