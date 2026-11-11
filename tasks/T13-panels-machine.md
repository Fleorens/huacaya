# T13 — GPU, CPU, Memory and Log panels

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T12 |
| **Read first** | `docs/02-ui.md` § 2.1, § 2.2, § 4; `grep -n "pub " src/ui/*.rs src/ui/panels/*.rs src/app/state.rs` |
| **Deliverable** | `src/ui/panels/gpu.rs`, `src/ui/panels/cpu.rs`, `src/ui/panels/memory.rs`, `src/ui/panels/log.rs`, `tests/panels_machine.rs`; four `pub mod` lines (and the shared helper below) in `src/ui/panels/mod.rs` |

## Why

The machine side of the Overview: GPU, CPU and memory with their gauges and graphs, and the live log.
Same rules as T12: each panel draws only inside its `Rect`, into a `Buffer`, from `App` and `Ctx`, and is
tested by rendering `app::demo::app(state)` into `Buffer::empty`.

## Contract

### Shared helper, in `src/ui/panels/mod.rs`

```rust
/// One `<label><gauge>  <value>` row at `(x, y)`, `width` columns: `label` (`Role::Dim`, written as given,
/// so pad it yourself), then a `gauge::Gauge` filling the columns left after two spaces and `value`
/// (`Role::Text`), then the value. When fewer than 4 columns are left for the gauge, it is omitted and
/// the value follows the label directly.
pub fn gauge_row(
    buf: &mut Buffer, x: u16, y: u16, width: u16, label: &str,
    ratio: f64, accent: Role, thresholds: (f32, f32), value: &str, theme: &Theme,
);
```

If T12 already wrote an equivalent private helper for the Model panel's `ctx` row, make it this one and
use it there too, without changing what the Model panel shows.

### `src/ui/panels/gpu.rs`

```rust
/// Height needed, borders included (4 rows inside).
pub const MIN_HEIGHT: u16 = 6;
/// The GPU panel of `system.gpus[0]`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Frame: `frame::panel(title, Role::Gpu, false, theme)`, title `GPU  <name>` with a leading `Radeon `
removed from the name (`GPU  RX 7800 XT`). No system snapshot or no GPU: title `GPU`, one `Dim` row
`no AMD GPU`. Rows (labels padded to 6):

1. `gauge_row("load  ", busy / 100, Gpu, PCT_THRESHOLDS, pct(busy))`
2. `gauge_row("VRAM  ", used / total, Gpu, PCT_THRESHOLDS, bytes_pair(used, total))` (`n/a`, no gauge,
   when either is missing)
3. `temp  <temp(junction)><mark> junction   <temp(edge)> edge` — `mark` = `hot_mark(severity(junction,
   GPU_JUNCTION_THRESHOLDS))`; the junction value and its mark in `Warning` / `Critical` when the severity
   is, `Text` otherwise.
4. `power <W> W / <cap> W   fan <rpm> rpm` — watts with no decimal; each missing value `n/a`.
5. When `ctx.graphs`: a `BrailleGraph` of `app.gpu_load` (`max` 100, accent `Gpu`) on **all** the rows left.

### `src/ui/panels/cpu.rs`

```rust
/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Frame `frame::panel("CPU", Role::Cpu, false, theme)`. Rows:

1. `gauge_row("usage ", usage / 100, Cpu, PCT_THRESHOLDS, pct(usage))`
2. `temp  <temp(cpu_temp_c)><mark>   <GHz> GHz avg` — `mark` from `CPU_TEMP_THRESHOLDS`, coloured as for
   the GPU; GHz = `freq.avg_mhz / 1000` with one decimal (`n/a` without `freq`).
3. `load  <one> <five> <fifteen>` with two decimals (`n/a` without `load`).
4. When `ctx.graphs`: a `BrailleGraph` of `app.cpu_usage` (`max` 100, accent `Cpu`) on all the rows left.

No system snapshot: the three rows show `n/a` values and no gauge.

### `src/ui/panels/memory.rs`

```rust
/// Height needed, borders included (3 rows inside).
pub const MIN_HEIGHT: u16 = 5;
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Frame `frame::panel("Memory", Role::Memory, false, theme)`. Rows:

1. `gauge_row("RAM   ", used / total, Memory, PCT_THRESHOLDS, bytes_pair(used, total))`
2. `gauge_row("swap  ", swap_used / swap_total, …, bytes_pair(swap_used, swap_total))`; a swap total of 0
   gives `swap  none`.
3. `model <util::bytes(rss)>` for the first `ServerRole::Instance` of `system.procs`; without one,
   `server <util::bytes(rss)>` for the router; without any, `model n/a`.

### `src/ui/panels/log.rs`

```rust
/// Height needed, borders included (1 row inside).
pub const MIN_HEIGHT: u16 = 3;
/// The log panel: the end of `app.visible_logs()`, as positioned by `app.log_view`.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);
```

Frame `frame::panel("Log", Role::Server, false, theme)` plus, right-aligned on the top border:
` /<filter> ` (`Warning`) when the filter is not empty, then ` follow ● ` (`Good`) when `log_view.follow`,
else ` scroll −<scroll> ` (`Warning`). With `v = visible_logs()` and `h` the inner height, the rows show
`v[end - h .. end]` (fewer when there are fewer lines) with `end = v.len() - min(scroll, v.len())`, oldest
at the top. Each row: `<time> ` in `Dim` (nothing for a line without time), then the text in `Text`
(`Info`), `Warning` (`Warn`) or `Critical` (`Error`), cut at the panel's width. No line: one `Dim` row
`no log line yet`.

## Pitfalls

- Expected values below come from T11 at `t = 119`, state Generating: `w(119) = −0.6665`, so GPU busy
  `94.67` → `95 %`, junction `73.33` → `73 °C`, edge `62 °C`, power `117 W`, fan `1233 rpm`, CPU usage
  `50.0` → `50 %`, CPU temperature `60 °C`, frequency `3450 MHz` → `3.5 GHz`, load `8.33 7.14 5.56`.
- `−` in ` scroll −3 ` is U+2212, one column; `●` is one column.
- `visible_logs()` hides the `Proxy` lines (T09): the demo's single `Proxy` line never appears here.
- 400-line limit per file; each panel is its own file.

## Tests

`tests/panels_machine.rs`, exactly these names; same `draw` helper, `T` and `ctx` as in
`tests/panels_model.rs` (copy them, a test file cannot import another).

- `gauge_row_layout`: into a 30 × 1 buffer, `gauge_row(…, 0, 0, 30, "load  ", 0.5, Gpu, PCT_THRESHOLDS,
  "50 %", &T)` → the row is `load  ` + 18 gauge cells + `  50 %` (30 columns), and cell 6 is `█`; with
  width 12 → `load  50 %` followed by spaces (no gauge).
- `gpu_generating`: `demo::app(Generating)`, 50 × 9 → top row contains `GPU  RX 7800 XT`; rows contain
  `95 %`, `15.9/16.0 G`, `temp  73 °C junction   62 °C edge`, `power 117 W / 212 W   fan 1233 rpm`; the last
  inner row holds at least one braille character.
- `gpu_hot_mark`: `demo::app(Prompt)` (junction 82 ± 1, below 85) has no `▲`; the same app with
  `system.gpus[0].temp_junction_c = Some(96.0)` → `96 °C▲ junction`, and the cell of `9` has the
  `Critical` colour `Color::Rgb(0xf7, 0x76, 0x8e)`.
- `gpu_missing`: `App::new(1000)` → `no AMD GPU`.
- `cpu_generating`: `demo::app(Generating)`, 40 × 6 → `50 %`, `temp  60 °C   3.5 GHz avg`,
  `load  8.33 7.14 5.56`; with `graphs: false` no braille character anywhere.
- `memory_generating`: `demo::app(Generating)`, 40 × 5 → `RAM   `, `21.6/31.1 G`, `swap  `, `0.0/8.0 G`,
  `model 19.2 G`; `demo::app(Down)` → `server 286.8 M` (the router only).
- `log_follow`: `demo::app(Generating)`, 70 × 6 → top row contains `follow ●`; the last inner row contains
  `10:00:32` and `all slots are idle`; no row contains `proxying request`.
- `log_scroll_and_filter`: same app with `log_view.follow = false`, `log_view.scroll = 3` → top row contains
  `scroll −3`, no row contains `all slots are idle`; with `log_view.filter = "n_gen"` (follow on) → top row
  contains `/n_gen`, every non-empty inner row contains `n_gen`.
- `log_levels`: an `App::new(1000)` fed the fixture `tests/fixtures/journal/llama-server.txt` (each line
  through `parse_line`, as `Event::Log`), 120 × 40 → the row holding `out of memory` has its text cells in
  `Color::Rgb(0xf7, 0x76, 0x8e)`; `App::new(1000)` alone → `no log line yet`.
- `machine_panels_stay_inside`: for the four panels and each demo state, render into a 30 × 6 area at
  `(5, 2)` of a 40 × 10 buffer: every cell outside is still a space.

## Acceptance criteria

- [ ] The files exist with the exact public API above; `src/ui/panels/mod.rs` declares the four modules and
      `gauge_row`.
- [ ] Nothing outside the deliverable changed (except `tasks/`, and the Model panel reusing `gauge_row`).
- [ ] The 10 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the four panels read well in a real terminal.
