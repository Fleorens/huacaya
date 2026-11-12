# T20 — Compact log lines

| | |
|---|---|
| **Batch** | 3 — after the first look at the real screen |
| **Depends on** | T18 |
| **Read first** | `docs/02-ui.md` § 5; `grep -n "pub \|fn render_line" src/ui/panels/log.rs src/ui/compact.rs src/ui/screens/logs.rs src/ui/help.rs src/app/state.rs src/app/keys.rs src/collect/logs.rs src/ui/text.rs` |
| **Deliverable** | `src/ui/logtext.rs`, `tests/logtext.rs`; `pub mod logtext;` in `src/ui/mod.rs`; the changes listed under "Wiring" |

## Why

On the real screen, almost every log line starts with `slot print_timing: id  0 | task 565380 | n_gen =`, so
a 70-column panel shows the same prefix 35 times and cuts what matters. Florian validated a compact form:
drop the `slot <function>: id N |` prefix, show the task as a short dim column, and rewrite the known events
in a few words. The raw line stays one key away (`r`).

## Contract — `src/ui/logtext.rs`

```rust
/// The message of `line` in compact form (rules below). Never empty for a non-empty `line.text`.
pub fn compact(line: &LogLine) -> String;

/// `line.text` without its llama.cpp source prefix (rules below), runs of whitespace collapsed to one space,
/// trimmed.
pub fn strip_prefix(text: &str) -> String;
```

`strip_prefix`: split on whitespace. When the first token is `slot` or `srv` and the next one ends with `:`
(the function name, e.g. `print_timing:`, `release:`, `load_model:`), drop both. Then, after `slot`, drop
`id <n> |` if it follows, then `task <t> |` if it follows. Every other text is kept whole (only the whitespace
is collapsed). Examples:

| `text` | `strip_prefix` |
|---|---|
| `slot get_availabl: id  0 \| task -1 \| selected slot by LCP similarity, f_keep = 1.000` | `selected slot by LCP similarity, f_keep = 1.000` |
| `slot print_timing: id  0 \| task 9137 \|       total time =   13987.46 ms /   675 tokens` | `total time = 13987.46 ms / 675 tokens` |
| `srv  update_slots: all slots are idle` | `all slots are idle` |
| `srv    load_model: the slot context (196608) exceeds the training context of the model` | `the slot context (196608) exceeds the training context of the model` |
| `load_tensors: failed to allocate 2048.00 MiB on device 0: out of memory` | unchanged |
| `this line has no journald prefix at all` | unchanged |

(`\|` is a plain `|`; the table escapes it.)

`compact`: by `line.event` (formatters from `ui::text` and `util`):

| Event | Compact text |
|---|---|
| `GenProgress { n_gen, tps, tps_3s }` | `n_gen <n_gen>  tg <tps(tps)> t/s  3s <tps(tps_3s)> t/s` |
| `PromptProgress { n_tokens, progress, tps }` | `prompt <n_tokens> tok  <pct(progress × 100)>  <tps(tps)> t/s` |
| `PromptTiming { tokens, tps }` | `prompt <tokens> tok  <tps(tps)> t/s` |
| `GenTiming { tokens, tps }` | `gen <tokens> tok  <tps(tps)> t/s` |
| `DraftAcceptance { accepted, generated }` | `draft <accepted>/<generated> accepted  <pct(accepted / generated × 100)>` (`n/a` when generated is 0) |
| `Released { n_tokens }` | `released  ctx <count(n_tokens)>` |
| `Request { method, path, status }` | `<method> <path> <status>` |
| `Proxy { model, port }` | `→ <short_model_name(model).0> :<port>` |
| none | `strip_prefix(&line.text)` |

## Wiring

- `src/app/state.rs`: `LogView` gains `pub raw: bool` (doc: "`true`: log lines are shown as printed; `false`:
  in compact form"), `false` in `App::new`.
- `src/app/keys.rs`: in block 4 (everywhere), `r` toggles `log_view.raw`.
- `src/ui/panels/log.rs` and `src/ui/compact.rs`: one shared row renderer (make the one of `panels/log.rs`
  `pub(crate)` and delete the copy in `compact.rs`). A row is `<time> ` (`Dim`), then, when not `raw` and the
  line's `task` is `Some(t)` with `t >= 0`, `#<t> ` (`Dim`), then the text — `compact(line)` when not `raw`,
  `line.text` when `raw` — coloured by level as today, cut at the width.
- `src/ui/screens/logs.rs`: the status row gains a segment `lines compact` / `lines raw` after the proxy
  segment, and the key hints become `f follow  / filter  e level  x proxy  r raw  ↑↓ scroll`.
- `src/ui/help.rs`: a 12th row `r` → `raw / compact log lines`; the box height becomes `min(area.height, 14)`.
- `docs/02-ui.md` § 5: a row `| r | raw / compact log lines (compact by default: no "slot … id N |" prefix) |`.
- The text filter (`/`) keeps matching `line.text` (the raw text), unchanged.
- Golden files: regenerate with `UPDATE_GOLDEN=1` (the log rows and the help box change) and read them.

## Pitfalls

- `progress` is a fraction (0.25 for 25 %): multiply by 100 before `pct`.
- `tps(Some(52.96))` is `53.0` (one decimal below 100): use the helper, do not format by hand.
- The fixture's GenProgress line has `n_gen =    161,` — the event already holds 161; do not re-parse text.
- Tests that assert on log rows of the Overview, the Logs screen or compact mode may need the compact text
  now; update their expected strings, never weaken them to `contains("")`.

## Tests

`tests/logtext.rs`, exactly these names; lines come from `tests/fixtures/journal/llama-server.txt` (line
numbers 1-based) through `collect::logs::parse_line`.

- `strip_prefix_examples`: the six rows of the table above.
- `compact_events`: fixture line 4 → `n_gen 161  tg 53.0 t/s  3s 53.3 t/s`; line 8 → `prompt 48 tok  85.3 t/s`;
  line 9 → `gen 627 tok  46.6 t/s`; line 13 → `draft 369/771 accepted  48 %`; line 14 →
  `released  ctx 57.7K`; line 1 → `→ Cyber-Tiel-Coder-35B-A3B :35335`; line 34 →
  `POST /v1/chat/completions 401`; the demo's `PromptProgress` line (from `app::demo::log_lines(9.0, 10.0)`)
  → `prompt 2048 tok  25 %  890 t/s`.
- `compact_plain_lines`: line 3 → `processing task, is_child = 0`; line 10 →
  `total time = 13987.46 ms / 675 tokens`; line 33 (systemd) → its `text` with whitespace collapsed.
- `raw_key`: `App::new(1000)` → `log_view.raw` false; `r` → true; `r` → false; on every screen.
- `log_panel_rows`: `demo::app(Generating)` through `panels::log::render`, 70 × 8 → a row contains
  `10:00:32 #9000 released  ctx 9.2K`; the last inner row contains `10:00:32 all slots are idle` (that line
  has no task, so no `#`; no `srv`, no `update_slots`); with `raw = true` → the last inner row contains
  `srv  update_slots: all slots are idle` and no row contains `#9000`.
- `logs_status_raw`: Logs screen body 130 × 20 → the last row contains `lines compact` and `r raw`; with
  `raw = true` → `lines raw`.

## Acceptance criteria

- [ ] `src/ui/logtext.rs` exists with the exact public API above; the wiring is done as listed.
- [ ] One row renderer is shared by the Log panel and compact mode.
- [ ] The 6 tests above exist under these names and pass; the golden files are regenerated and read.
- [ ] `bash .devbox/check` is green.
- [ ] (On screen, not by the model) the Overview log reads at a glance on the aibox.
