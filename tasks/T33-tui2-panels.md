# T33 — TUI v2: model band with big digits, GPU / CPU / Memory panels

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T32 |
| **Read first** | `src/ui/panels/model.rs`, `gpu.rs`, `cpu.rs`, `memory.rs`; `src/ui/big.rs`; `src/ui/panels/mod.rs` (`gauge_row`); `src/app/state.rs` (`task_elapsed_s`) |
| **Deliverable** | changes to `src/ui/panels/model.rs`, `gpu.rs`, `cpu.rs`, `memory.rs`; tests and golden files updated |

## Why

T32 placed the new skeleton. This sheet draws the two parts of the mockup that carry the most information:
the model band across the screen, with the live speed in big digits, and the machine panels as three equal
columns. The model band is 6 rows high (4 inside its frame) on every tier.

## Contract

### `src/ui/panels/model.rs` — the model band

`MIN_HEIGHT` becomes 6. `badge_state` is unchanged. `render` draws `frame::panel("Model", Role::Server, …)` and,
with `W` the inner width, one of three layouts. The speed `S` is `live_gen_tps` while generating (`n_decoded >
0`), else `avg_gen_tps()`, written with `text::tps`; its label is `t/s now`, or `t/s avg` when not generating.

**Wide (`W >= 110`)** — three columns: left 38, right 34, the centre between them with 2 spaces on each side.
- left: row 0 `<short name>  <quant>` (name `Text` bold, quant `Dim`); row 1 the badge, then `  task #<id>`
  and, when `task_elapsed_s()` is known, ` · <util::duration(s)>` (`Dim`); row 2 the settings
  `<spec_type> · n-cpu-moe <n> · ngl <n>` (the parts that exist, ` · ` between, `Dim`, `text::fit`).
- centre: rows 0–2 `big::big_digits(S)` in `Server` bold; then, 2 columns right of the digits: row 0 the label,
  row 1 a one-row `BrailleGraph` of `app.gen_tps` (accent `Server`, `max = max(gen_tps.max, 1)`) on the
  columns left, row 2 `avg <avg_gen_tps> · 5 min` (`Dim`); row 3
  `prompt <tps(avg_prompt)> t/s   cache <pct>   draft <pct>` (labels `Dim`).
- right: row 0 `gauge_row("ctx  ", used / n_ctx, Server, PCT_THRESHOLDS, "")` with no value; row 1
  `     <count(used)> / <count(n_ctx)>  <pct(used / n_ctx × 100)>`; row 2
  `req  <requests_processing> running · <requests_deferred> queued` (integers).

**Medium (`60 <= W < 110`)** — left column 38 as above; the rest (after 2 spaces): row 0
`<S> <label>  <one-row graph>`, row 1 the prompt / cache / draft line, row 2 the ctx `gauge_row` with value
`<used> / <n_ctx>`, row 3 the req line.

**Small (`W < 60`)** — one column: row 0 name + quant, row 1 badge + task, row 2 `<S> <label>`, row 3 ctx
`gauge_row` with the value.

Without an active model the band shows, at every width, left row 0 the loading model's name (if any), row 1
the badge, row 2 the same `Dim` message as today (`set LLAMA_API_KEY…`, `loading …`, `no model loaded — <n>
available`, the down message, `waiting for the first poll…`); nothing else.

### `src/ui/panels/gpu.rs`

- Row 3: `temp  <junction> junction  <edge> edge  <mem> mem` (each part only when known; with junction
  `None`, `temp  <edge>` as T30 does), the mark as today.
- Row 4: `power <W> / <cap> W   fan <fan_pct as text::pct, else <rpm> rpm>   clk <sclk> MHz`.
- The T21 detail rows (when the inner height is at least 10) become: `volt  <mV> mV   GTT <bytes>` and
  `link  <link>` (the clock and memory temperature now live in rows 3 and 4).

### `src/ui/panels/cpu.rs`

- Title `CPU  <cpu_info.model>` (just `CPU` when unknown).
- Rows: `gauge_row("usage ", …)` as today; `cores <big::level_bars(per_core_pct)>  <threads> thr` (`thr` part
  only when known; `cores n/a` before the second sample); `temp  <temp><mark>   freq <GHz> GHz`;
  `load  <one> <five> <fifteen>`; the graph on the rows left (when `ctx.graphs`).

### `src/ui/panels/memory.rs`

- Rows: RAM; swap; `model <rss> rss   router <rss>` when an instance and the router exist, else
  `server <rss>` (router only) or `model n/a`; `avail <bytes>   cached <bytes>`; `disk  <mount> <pct>` for
  each disk, `   ` between, `text::fit` (pct = `used / total × 100`, `text::pct`). Rows that do not fit are
  dropped from the bottom.

## Tests to update, and new ones

Update to this contract, never weaken: `tests/panels_model.rs` (`model_generating`, `model_idle`,
`model_without_active`, `model_graph_toggle`), `tests/panels_machine.rs` (`gpu_generating`, `cpu_generating`,
`memory_generating`), `tests/panels_details.rs` (`gpu_details_when_tall`), the golden files (regenerate, then
read every one).

Expected values (demo, `t = 119`): live speed `49.3`; `big_digits("49.3")` on rows 0–2; ctx `63.8K / 196.6K  32 %`;
idle: `56.7`, `t/s avg`, `9.2K / 196.6K  5 %`.

- `model_generating` (140 × 6): rows contain `Cyber-Tiel-Coder-35B-A3B  Q4_K_M`, `◉ GENERATING  task #9000`,
  the three rows of `big_digits("49.3")`, `t/s now`, `prompt 591 t/s   cache 98 %   draft 64 %`,
  `63.8K / 196.6K  32 %`, `req  1 running · 0 queued`, `draft-mtp · n-cpu-moe 21 · ngl 99`.
- `model_idle` (140 × 6): the rows of `big_digits("56.7")`, `t/s avg`, `9.2K / 196.6K  5 %`.
- `model_medium_and_small` (new): 90 × 6 → a row contains `49.3 t/s now`; 50 × 6 → `49.3 t/s now` and `ctx`.
- `cpu_generating`: 48 × 9 → title contains `CPU  Intel Core i7-11700K`, a row contains `cores ` followed by
  `big::level_bars` of the snapshot's `per_core_pct` (build the expected string in the test), `16 thr`, and
  `temp  60 °C   freq 3.5 GHz`.
- `gpu_generating`: rows contain `temp  73 °C junction  62 °C edge  76 °C mem` and
  `power 117 W / 212 W   fan 34 %   clk 2204 MHz`.
- `gpu_details_when_tall`: `volt  917 mV   GTT 175 M` and `link  16.0 GT/s PCIe x16`.
- `memory_generating`: 60 × 9 → `model 19.2 G rss   router 287 M`, `avail 9.5 G   cached 18.2 G`,
  `disk  / 61 %   /srv/models 61 %   /srv/data 11 %`.

## Pitfalls

- The exact cores string depends on rounding: build the expected value with `big::level_bars` in the test.
- `big_digits` rows are plain text: write them with `set_line` like any other text, `Server` bold.
- Keep each file under 400 lines: `model.rs` may split the three layouts into private functions or into a
  `src/ui/panels/model_band.rs`.

## Acceptance criteria

- [ ] The contract above is implemented; the Overview looks like the validated mockup at 140 × 40.
- [ ] The updated and new tests pass; golden files regenerated and read.
- [ ] `bash .devbox/check` is green.
