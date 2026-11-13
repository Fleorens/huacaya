# T39 — 80 × 24 v2: the dense Overview of the Narrow tier

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T38 |
| **Read first** | `src/ui/draw.rs`; `src/ui/layout.rs` (`tier`); `src/ui/compact.rs` (the frameless style); `src/ui/panels/model.rs` (`badge_state`, the no-model message); `src/ui/panels/log.rs` (`render_line`); `src/ui/screens/agents.rs` (`step_glyph`); `src/ui/text.rs` (`fit_groups`); `src/ui/gauge.rs`; `tests/layout.rs` |
| **Deliverable** | `src/ui/dense.rs` (new, `pub mod dense;` in `src/ui/mod.rs`); changes to `src/ui/draw.rs`, `src/ui/panels/model.rs`, `src/ui/screens/agents.rs`, `docs/02-ui.md` (§ 3, the Narrow tier); `tests/dense.rs` (new); golden files regenerated |

## Why

At 80 × 24 the framed Overview spends a third of its rows on borders and cuts its lines. The validated mockup
gives the Narrow tier a frameless, dense Overview: one line per subsystem separated by thin rules, then the
log, with the coding agent's latest step mixed into it.

## Contract

### Where it applies

`draw`: on the **Narrow** tier (`60 <= w < 100`, `h >= 20`) and the **Overview** screen only, the header is drawn
as today on row 0 and `dense::render(app, &ctx, body, buf)` draws everything below it (key row included). The
other screens and tiers are unchanged. The help overlay still draws on top.

### `src/ui/dense.rs`

```rust
/// The Narrow tier's Overview in `area` (the body below the header): frameless rows, then the log.
pub fn render(app: &App, ctx: &Ctx, area: Rect, buf: &mut Buffer);

/// The badge's short form: `GENERATING` → `◉ GEN`, `PROMPT` → `◉ PROMPT`, `IDLE` → `○ IDLE`, `LOADING` →
/// `◐ LOAD`, `NO MODEL` → `○ NO MODEL`, `SERVER DOWN` → `✖ DOWN`, `UNAUTHORIZED` → `✖ AUTH`, `WAITING` →
/// `○ WAIT`; any other text unchanged.
pub fn short_badge(text: &str) -> String;
```

Rows, from the top of `area` (`W` = its width; a rule is a full row of `─` in `Border`):

0. a rule.
1. **Model** — `short_badge` (bold, in the badge's role), a space, the short name, a space, the quant
   (`Dim`); then three spaces, the speed `S` (as the model band: live while generating, else the average,
   `text::tps`) and ` t/s` in `Server` bold, a space, and a one-row `BrailleGraph` of `app.gen_tps` on 8 to 12
   columns (as many as are left, none below 8 or when `!ctx.graphs`). Without an active model: the badge, then
   the model band's message in `Dim` (make that message a `pub(crate) fn no_model_message(app: &App) -> String`
   in `model.rs` and use it in both places).
2. **Context** — `text::fit_groups` of: `ctx ` (`Dim`) + a 20-cell `Gauge` (`used / n_ctx`, accent `Server`,
   `PCT_THRESHOLDS`) + ` <count(used)>/<count(n_ctx)>`; `  cache <pct>`; `  draft <pct>` (labels `Dim`). Row
   blank without an active slot.
3. a rule.
4. **GPU** of `gpus[0]` — groups: `GPU ` (`Gpu`) + a 10-cell `Gauge` (busy, accent `Gpu`) + ` <pct>` + a
   space and a 5-column `BrailleGraph` of `app.gpu_load` (when `ctx.graphs`); `  VRAM <bytes_pair>`;
   `  <temp>` (junction, else edge, `text::temp`, coloured by `GPU_JUNCTION_THRESHOLDS`); `  <power> W`
   (labels and units `Dim`). No GPU: `GPU ` then `no GPU` (`Dim`).
5. **CPU** — groups: `CPU ` (`Cpu`) + a 10-cell `Gauge` (usage, accent `Cpu`) + ` <pct>` + a space and a
   5-column `BrailleGraph` of `app.cpu_usage`; `  RAM  <bytes_pair(used, total)>`; `  <cpu_temp_c>`.
6. a rule.
7. … `h − 2`: **Log** — the end of `visible_logs()`, one `panels::log::render_line` each (compact or raw as
   the view says). When `app.opencode` has a focused session that is `running` and `steps` is not empty, the
   last of these rows is instead the newest step:
   `<clock::hms(time_ms / 1000, utc_offset_s)> agent #<job> <glyph> <summary>  <detail>` — time `Dim`, `agent`
   in `Gpu`, `#<job>` `Dim` (left out without a job), glyph and colours as on the Agents screen (make
   `agents::step_glyph` `pub(crate)`), cut at the width.
8. last row: **keys** — `Dim`, `text::fit_groups` of `q quit`, `  ? help`, `  1-5 screens`, `  / filter`,
   `  p pause`.

When `area` has fewer than 12 rows, rows 3 and 6 (the inner rules) are left out first, then row 2.

### `docs/02-ui.md`

§ 3: the Narrow tier's Overview is the dense layout above (one paragraph and the 80 × 24 example from the
golden file).

## Tests

`tests/dense.rs` (new), demo `Generating`, `t = 119`, through `ui::draw` at 80 × 24 unless stated:
- `short_badges`: the eight mappings of the contract, and `FOO` → `FOO`.
- `dense_rows`: row 2 (the first under the header and rule) contains `◉ GEN`, `Cyber-Tiel-Coder-35B-A3B Q4_K_M`
  and `49.3 t/s`; a row contains `63.8K/196.6K`, `cache 98 %` and `draft 64 %`; a row contains `GPU `, `95 %`,
  `VRAM 15.9/16.0 G`, `73 °C` and `117 W`; a row contains `CPU `, `50 %`, `RAM  21.6/31.1 G` and `60 °C`.
- `dense_rules`: rows 1, 4 and 7 are made of `─` only.
- `dense_log_and_keys`: the row above the last contains `all slots are idle`; the last row starts with
  `q quit  ? help  1-5 screens`.
- `dense_agent_step`: with `app.opencode` built from `tests/fixtures/opencode/*.json` (`parse_sessions`,
  `parse_steps`) and `focus` set to the running session's id, the row above the last contains `agent` and the
  summary of the snapshot's last step (read it from the snapshot in the test, never hard-code it).
- `dense_only_on_narrow_overview`: at 120 × 30 the Overview still has its `Model` frame; at 80 × 24 the Models
  screen still has its `Models` frame.
- `dense_short_height` (80 × 20): no row is cut mid-word at the right edge and the key row is the last one.

`tests/layout.rs`: update the 80 × 24 expectations to the dense layout (never weaken the others), regenerate
the golden files (`UPDATE_GOLDEN=1`), read every one.

## Pitfalls

- `render_line` is `pub(crate)` in `panels/log.rs`: call it, do not copy it.
- The Narrow tier is chosen by width alone above 20 rows: 99 × 50 is dense too. That is intended.
- `count` gives `63.8K`, `bytes_pair` gives `15.9/16.0 G`: use the helper the sheet names for each part.

## Acceptance criteria

- [ ] The contract above is implemented.
- [ ] The new and updated tests pass; golden files regenerated and read.
- [ ] `bash .devbox/check` is green.
- [ ] On screen (Florian): the Overview at 80 × 24 looks like the validated mockup.
