# T36 — Models screen v2: table summary, four detail panels

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T35 |
| **Read first** | `src/ui/screens/models.rs` (whole file); `src/ui/panels/mod.rs` (`gauge_row`); `src/ui/text.rs` (`fit_groups`); `src/collect/llama.rs` (`Metrics` methods, `SlotInfo`); `src/collect/process.rs` (`ProcSample`); `tests/models_screen.rs` |
| **Deliverable** | changes to `src/ui/theme.rs`, `src/ui/screens/models.rs` (split into `src/ui/screens/models_details.rs` if it passes 400 lines); `tests/models_screen.rs`; `tests/snapshots/models-*.txt` regenerated |

## Why

The validated mockup keeps the Models table but replaces the single long "Details" panel by four small ones
side by side — what the model *is*, how it *runs*, how *fast* it is, and what its *slot* is doing — so the
screen reads at a glance instead of as a list of labels.

## Contract

`pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer)` and `pub fn params(n: u64) -> String` keep
their signatures. The "no model" cases (`waiting for the first poll`, `server down`, `no API key`, `the server
lists no model`) are unchanged.

### The table panel

- Same panel, same rows, same drop rules as today, plus a `train` column (`count(ctx_train)`, `-` when
  unknown, width 8) placed right after `ctx`; `train` is dropped first when narrow (new `DROP_ORDER`:
  `train`, `vision`, `spec`, `moe`, `quant`, `ctx`).
- Right title (`Dim`): `<n> known · <k> loaded · VRAM <bytes_pair(vram_used, vram_total)>` (`k` = models with
  state `Loaded`; the VRAM part only when `system.gpus[0]` has both values).
- The selected row: the `▶` marker in `Server`, the model name bold (as today), and across the table's inner
  width the background `Role::Selection` in true colour, the `REVERSED` modifier in 16 colours and without
  colour (a dark-grey band would hide the `Dim` cells there).
- `src/ui/theme.rs` gains `Role::Selection`: `rgb` `(0x28, 0x34, 0x57)` (the mockup's selection band); in
  `Theme::color`, `Ansi16` → `Color::DarkGray` (only ever used as a background, by the rule above: never). Add
  it to every exhaustive `match` on `Role`.

### The details area (below the table)

Two columns of equal width (the left one takes the odd column) when the area is at least 80 wide, else one
column with the panels stacked in the order below. In two columns: left **Identity** (6 rows high) over
**Runtime** (the rest); right **Performance** (7 rows high) over **Slot** (the rest). In one column each panel
takes its own height (Identity 6, Runtime 6, Performance 7, Slot 6) and the ones that do not fit entirely are
not drawn. Labels are `Dim` and padded to 8 columns; values `Text`. Every row is cut at the panel width.

- **Identity** — `frame::panel("Identity", Role::Server, false, …)`, rows:
  `id      <id>`; `file    <model_path, text::fit to the width>` (`n/a`); `size    <bytes(size)>   params
  <params(n)>   type <ftype>` (the parts that exist; `loaded models only` in `Dim` without a size);
  `vision  yes` / `no`.
- **Runtime** — rows: `gauge_row("context ", ctx_size / ctx_train, Server, PCT_THRESHOLDS, "<count(ctx_size)>
  of <count(ctx_train)> trained")` (without `ctx_train`: `context <count(ctx_size)>` as text; neither: `n/a`);
  `offload ngl <n>   n-cpu-moe <n>   threads <n>` (`-` for a missing one); `spec    <spec_type>` (`none`);
  `port    <port>   pid <pid>   rss <bytes(rss)>` from the `ProcSample` with role `Instance` whose `model` equals
  the model `id` (row absent when there is none).
- **Performance** — right title `since load` (`Dim`). When the selected model is not in `llama.active`: one
  `Dim` row `not loaded`. Else rows, from its `Metrics`:
  `gen     <tps(avg_gen_tps)> t/s avg   <one-row BrailleGraph of app.gen_tps on the columns left, accent
  Server, max = max(gen_tps.max, 1); omitted when !ctx.graphs or fewer than 8 columns left>` (the number bold);
  `prompt  <tps(avg_prompt_tps)> t/s avg` (number bold);
  `gauge_row("cache   ", cache_reuse_pct / 100, Good, (101.0, 101.0), "<pct> reused")` (thresholds above 100:
  a high cache reuse is good, never red);
  `gauge_row("draft   ", draft_acceptance_pct / 100, Server, (101.0, 101.0), "<pct> accepted")` (row absent when
  `draft_acceptance_pct` is `None`);
  `tokens  <count(tokens_predicted_total)> generated   <count(prompt_tokens_total)> prompted`.
- **Slot** — title `Slot <id>` of the first slot of the active model, right title `◉ generating` (`Server`)
  when `is_processing && n_decoded > 0`, `◉ prompt` (`Server`) when `is_processing`, else `○ idle` (`Dim`).
  Not active, or no slot: title `Slot`, one `Dim` row `no slot`. Rows:
  `task    #<id_task>   running <util::duration(app.task_elapsed_s())>` (the `running` part only when
  processing and known; `task    -` when `id_task < 0`);
  `gauge_row("ctx     ", used / n_ctx, Server, PCT_THRESHOLDS, "<count(used)> / <count(n_ctx)>")` with
  `used = n_prompt_tokens + n_decoded`;
  `decoded <group_thousands(n_decoded)> tok   remain <group_thousands(n_remain)>` (`remain` part only when
  `n_remain >= 0`);
  `reqs    <requests_processing> processing   <requests_deferred> deferred` (integers).

### The hint row

When the area is at least 12 rows high, its last row (taken from the details area) is a `Dim` line
`↑↓ select · Home/End · read-only: llamatop never loads or unloads a model`, groups dropped whole from the end
with `text::fit_groups` (split at ` · `).

## Tests

Update in `tests/models_screen.rs`, never weaken: `table_lists_all_models` (adds the `train` header and
`262.1K` on the loaded model's row), `selection_marker`, `narrow_drops_columns` (`train` disappears first),
`narrow_drops_quant_before_ctx`, `details_of_loaded_model`, `details_of_unloaded_model`, `server_down`,
`waiting_for_first_poll`, `models_golden` (regenerate with `UPDATE_GOLDEN=1`, then read every golden file).

Demo `Generating`, `t = 119`, 120 × 30, loaded model selected — the rows contain:
- `5 known · 1 loaded · VRAM 15.9/16.0 G` (right title of the table);
- `id      peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M`;
- `size    21.0 G   params 35.5B   type Q4_K - Medium`;
- `196.6K of 262.1K trained`; `offload ngl 99   n-cpu-moe 21   threads 8`; `spec    draft-mtp`;
- `port    35335   pid 6873   rss 19.2 G`;
- `98 % reused`, `64 % accepted`, `since load`;
- `Slot 0` and `◉ generating`; `63.8K / 196.6K`;
- `↑↓ select · Home/End`.

New tests:
- `details_four_panels` (120 × 30): the titles `Identity`, `Runtime`, `Performance`, `Slot 0` are all on screen,
  `Identity` and `Performance` on the same row.
- `details_unloaded_model` (demo `Generating`, select `Qwen3-8B`): `not loaded` and `no slot` are shown, no
  `port ` row.
- `details_one_column` (70 × 40): `Identity` is above `Runtime`, which is above `Performance`.
- `tokens_row`: build the expected `tokens  … generated   … prompted` string from the demo snapshot's
  `Metrics` with `util::count` in the test, and find it on screen.
- `selected_row_band`: in true colour every cell of the selected row inside the table has the background
  `Color::Rgb(0x28, 0x34, 0x57)`; with `ColorMode::NoColor`, every cell of the selected row inside the
  table has the `REVERSED` modifier.

## Pitfalls

- `models.rs` is 337 lines: move the four detail panels into `src/ui/screens/models_details.rs`
  (`pub(crate) fn render_details(app, llama, ctx, area, buf)`), declared in `src/ui/screens/mod.rs`.
- The selected row: write the cells first, then `buf.set_style(row_rect, style)` with
  `Style::default().bg(rgb Selection)` in true colour, else `Style::default().add_modifier(Modifier::REVERSED)`.
- `count` and `group_thousands` differ: `count(1043)` is `1.0K`, `group_thousands(1043)` is `1 043`. The sheet
  says which one each row uses.

## Acceptance criteria

- [ ] The contract above is implemented.
- [ ] The updated and new tests pass; golden files regenerated and read.
- [ ] `bash .devbox/check` is green.
- [ ] On screen (Florian): the Models screen looks like the validated mockup at 140 × 40.
