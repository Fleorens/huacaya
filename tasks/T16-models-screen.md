# T16 — Models screen

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T14 |
| **Read first** | `docs/02-ui.md` § 2 (Models); `grep -n "pub " src/ui/*.rs src/ui/panels/*.rs src/app/state.rs src/collect/llama.rs` |
| **Deliverable** | `src/ui/screens/mod.rs`, `src/ui/screens/models.rs`, `tests/models_screen.rs`, `tests/snapshots/models-*.txt`; `pub mod screens;` in `src/ui/mod.rs`; the Models branch of `src/ui/draw.rs` |

## Why

The Overview shows the loaded model; the Models screen shows **all** the models the router knows, their
state and settings, and the details of the one selected with `↑` / `↓` (the selection is already handled by
T09: `app.model_selected`). It is read-only: nothing here can load or unload a model.

## Contract

### `src/ui/screens/mod.rs`

`pub mod models;` and a one-line module doc. (T17 and T18 add their screens here.)

### `src/ui/screens/models.rs`

```rust
/// The Models screen in `area` (the body below the header): the table on top, the details below.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);

/// Parameters in billions with one decimal: `35_505_251_456` → `35.5B`; under one billion, in millions:
/// `8_190_000` → `8.2M`.
pub fn params(n: u64) -> String;
```

Layout of `area`: the **table** panel `frame::panel("Models", Role::Server, true, theme)` gets
`models.len() + 3` rows (borders + the column header row), capped at half of `area.height` but at least 4;
the **details** panel `frame::panel("Details", Role::Server, false, theme)` gets the rest. Without a llama
snapshot, with `Health::Down` / `Unauthorized`, or with no model: one table panel over the whole area with a
`Dim` row `no model: <reason>` (`waiting for the first poll`, `server down`, `no API key`, `the server
lists no model`).

Table: one header row (`Dim`, bold), then one row per model in `llama.models` order:

| Column | Width | Content |
|---|---|---|
| marker | 2 | `▶ ` on the selected row, else two spaces |
| state | 11 | `● loaded` (`Good`), `◐ loading` (`Warning`), `○ unloaded` (`Dim`), `○ <other>` (`Dim`) |
| name | rest | `short_model_name(id).0`, cut with `text::fit` |
| quant | 8 | the tag of `short_model_name`, or empty |
| ctx | 8 | `count(ctx_size)`, or `n/a` |
| moe | 5 | `n_cpu_moe`, or `-` |
| spec | 13 | `spec_type`, or `-` |
| vision | 4 | `img` when `vision`, else empty |

Header labels: `state`, `model`, `quant`, `ctx`, `moe`, `spec`, `img`. Columns are separated by one space.
When the panel is too narrow, drop columns from the right in this order: `vision`, `spec`, `moe`, `quant`,
`ctx`, until the name column has at least 12 columns. The selected row is bold. With more models than rows,
scroll so the selected row stays visible.

Details of `models[model_selected]`, one `label value` row each (labels `Dim`, padded to 12):

1. `id` — the full id
2. `state` — as in the table
3. `path` — `model_path` or `n/a`, cut with `text::fit`
4. `context` — `<count(ctx_size)> of <count(ctx_train)> trained`
5. `offload` — `ngl <n_gpu_layers>   n-cpu-moe <n>   threads <n>` (`-` for each missing)
6. `speculative` — `spec_type` or `none`
7. `size` — `<util::bytes(size_bytes)>   <params(n_params)> params   <ftype>`, or `loaded models only` (`Dim`)
   when `size_bytes` is `None`

Then, when the model is in `llama.active` (its `ActiveModel` by id), a blank row and:

8. `speed` — `gen <tps(avg_gen_tps)> t/s   prompt <tps(avg_prompt_tps)> t/s`
9. `cache` — `<pct(cache_reuse_pct)> reused   draft <pct(draft_acceptance_pct)> accepted`
10. `requests` — `<requests_processing> processing   <requests_deferred> deferred` (as integers)
11. `slot` — `<count(n_prompt_tokens + n_decoded)> / <count(n_ctx)> tokens` of the first slot, or `n/a`

Rows that do not fit the panel are dropped from the bottom.

### `src/ui/draw.rs`

On `Screen::Models` (and a tier other than `TooSmall` / `Compact`), draw the header as for the Overview,
then `screens::models::render` on the body (everything under the header). Nothing else changes.

## Pitfalls

- Use `app.model_selected` as given, but guard the index (`get`), in case a new snapshot shrank the list
  before T09's clamp ran.
- `−`, `▶`, `●`, `◐`, `○` are one column each.
- The demo values: model 4 (index 3) is loaded: `size_bytes` 22 512 939 520 → `21.0 G`, `n_params`
  35 505 251 456 → `35.5B`, `ctx_size` 196608 → `196.6K`, `ctx_train` 262144 → `262.1K`; its
  metrics give `591` prompt t/s, `56.7` gen t/s, `98 %` cache, `64 %` draft.

## Tests

`tests/models_screen.rs`, exactly these names; same `draw` helper and `T` / `ctx` as `tests/panels_model.rs`.

- `params_format`: `params(35_505_251_456)` → `35.5B`; `params(8_190_000)` → `8.2M`; `params(1_000_000_000)`
  → `1.0B`.
- `table_lists_all_models`: `demo::app(Idle)`, 120 × 26 → the header row contains `state`, `model`, `quant`,
  `ctx`, `moe`, `spec`, `img`; rows contain `Ornith-1.5-35B-A3B`, `gsq-qwen3.8-27b`, `muse-glimmer-30b`,
  `Cyber-Tiel-Coder-35B-A3B`, `Qwen3-8B`; the Cyber-Tiel row contains `● loaded` and `draft-mtp`; the
  Ornith row contains `○ unloaded` and `img`.
- `selection_marker`: same app with `model_selected = 3` → exactly one row contains `▶`, and it contains
  `Cyber-Tiel-Coder-35B-A3B`.
- `details_of_loaded_model`: `model_selected = 3` → rows contain
  `/srv/models/direct/Cyber-Tiel-Coder-35B-A3B-MTP-UD-Q4_K_M.gguf`, `196.6K of 262.1K trained`,
  `ngl 99   n-cpu-moe 21   threads 8`, `21.0 G   35.5B params   Q4_K - Medium`, `gen 56.7 t/s   prompt 591 t/s`,
  `98 % reused   draft 64 % accepted`.
- `details_of_unloaded_model`: `model_selected = 0` → `loaded models only`, no row contains `speed`.
- `narrow_drops_columns`: `demo::app(Idle)` at 50 × 20 → no row contains `draft-mtp` nor `img`, and a row
  still contains `Cyber-Tiel` (the name may be cut).
- `server_down`: `demo::app(Down)` → a row contains `no model: server down`.
- `models_golden`: golden files `tests/snapshots/models-120x30.txt` and `models-80x24.txt`: full-frame plain
  snapshots (`ui::snapshot::snapshot`) of `demo::app(Generating)` with `screen = Screen::Models` and
  `model_selected = 3`, same `UPDATE_GOLDEN=1` mechanism as T14. Read them and describe them in the report.

## Acceptance criteria

- [ ] The files exist with the exact public API above; `draw.rs` routes the Models screen.
- [ ] Nothing outside the deliverable changed (except `tasks/`).
- [ ] The 8 tests above exist under these names and pass; the Overview golden files are unchanged.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) `--demo`, key `2`, then `↑` / `↓` move the selection.
