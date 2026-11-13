# T37 — Logs screen v2: levels, filter and tasks beside the log

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T36 |
| **Read first** | `src/app/log_view.rs`; `src/app/state.rs` (`LogView` construction, `line_visible`, `visible_logs`); `src/app/keys.rs` (step 5, the Logs screen); `src/collect/logs.rs` (`LogLine`, `Level`, `LogEvent`); `src/ui/screens/logs.rs`; `src/ui/panels/log.rs`; `src/ui/text.rs` (`fit_groups`); `src/ui/help.rs`; `tests/logs_help.rs` |
| **Deliverable** | `src/app/log_tasks.rs` (new, `pub mod log_tasks;` in `src/app/mod.rs`); changes to `src/app/log_view.rs`, `src/app/state.rs`, `src/app/keys.rs`, `src/ui/screens/logs.rs` (split into `src/ui/screens/logs_side.rs` if needed), `src/ui/panels/log.rs`, `src/ui/help.rs`; `tests/log_tasks.rs` (new); `tests/logs_help.rs`; golden files regenerated |

## Why

The validated mockup turns the Logs screen into a workbench: on the left, how many lines of each level there
are, what the view filters, and the list of recent tasks (one per request); on the right, the log in columns
(time, task, level, message) with the filter text highlighted. Choosing a task shows only its lines.

## Contract

### `src/app/log_tasks.rs` (pure)

```rust
/// How a task seen in the log ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome { Running, Done, Failed }

/// One task (request) summarised from its log lines.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSummary {
    pub task: i64,
    pub outcome: TaskOutcome,
    /// Seconds between its first and last line, from their `time` (`HH:MM:SS`); `None` without both times.
    pub seconds: Option<u64>,
    /// `Released { n_tokens }` when seen, else the last `GenProgress { n_gen }`, else `None`.
    pub tokens: Option<u64>,
}

/// The tasks of `lines` (only `task >= 0`), most recently seen first (by their last line), at most `max`.
/// `Failed` when one of its lines is `Level::Error`, else `Done` when a `Released` event was seen, else
/// `Running`. A last time earlier than the first (midnight passed) adds 86 400 s. Never panics.
pub fn log_tasks<'a>(lines: impl IntoIterator<Item = &'a LogLine>, max: usize) -> Vec<TaskSummary>;
```

### State and keys

- `LogView` gains `pub task: Option<i64>` (only the lines of this task are visible), `pub tasks_focus: bool`
  (the Tasks list has the keys), `pub task_selected: usize` (index into `log_tasks(&app.logs, 50)`). `App::new`
  sets `None`, `false`, `0`.
- `line_visible` also hides a line whose `task` is not `log_view.task` when that is set. `visible_logs` keeps its
  signature and documentation, extended by this rule.
- On the Logs screen, before today's keys: `t` toggles `tasks_focus`. While `tasks_focus`: `Up` / `Down` move
  `task_selected` within the tasks (no wrap), `Home` / `End` likewise; `Enter` sets `task = Some(<selected
  task>)`, or `None` when it already is that task, then `scroll = 0`, `follow = true`; every other key falls
  through to today's handling. `Esc` (focused or not) also clears `task`.
- `src/ui/help.rs`: a row `t` → `Logs: tasks list, Enter shows one task` (box height + 1).

### `src/ui/screens/logs.rs`

`render` and `level_name` keep their signatures. **Below 90 columns or 12 rows**, the screen is exactly as
today. Otherwise:

- A left column of 32 columns, from the top: **Levels** (6 rows), **Filter** (7 rows), **Tasks** (the rest).
  The right part is the **Log** panel. The last row of the area is the hint row.
  A body too short for all three (12 to 16 rows) draws only the sections that fit whole: Levels always,
  Filter from 13 rows, Tasks from 16; the keys work as usual (decided by Florian on 2026-11-13).
- **Levels** — `frame::panel("Levels", Role::Server, false, …)`; counts over **all** of `app.logs`:
  `● info   <n>` (`Text`), `▲ warn   <n>` (`Warning`), `✖ error  <n>` (`Critical`), each `n` written with
  `text::group_thousands` right-aligned to 7 columns; then `  proxy  <n>  hidden` (or `shown`, by
  `show_proxy`), `Dim`, where `n` counts the lines whose event is `Proxy`.
- **Filter** — rows (labels `Dim`, padded to 7): `text   /<filter>` (`Warning`; `none` in `Dim` when empty;
  while `editing_filter`, `/<filter>▏`); `task   #<task>` (`Warning`) or `all`; `level  <level_name>`;
  `lines  compact` or `raw`; `shown  <group_thousands(visible)> of <group_thousands(logs.len())>`.
- **Tasks** — `frame::panel("Tasks", Role::Server, tasks_focus, …)`, right title `Enter: filter` (`Dim`). One
  row per `log_tasks(&app.logs, 50)` that fits, scrolled so `task_selected` stays visible:
  `▶ ` (`Server`) on `task_selected` (else two spaces), `#<task>` padded to 9, the glyph — `◉` `Server`
  (running), `✓` `Good` (done), `✖` `Critical` (failed) — then `<util::duration(seconds)>` right-aligned to
  7 (`-` when `None`), then `<util::count(tokens)> tok` right-aligned to 10 (nothing when `None`). The row of the
  task in `log_view.task` is bold. No task: one `Dim` row `no task yet`.
- **Log** — `frame::panel("Log", Role::Server, !tasks_focus, …)` with today's border indicators (make
  `panels::log::render_indicators` `pub(crate)` and pass `"Log"`). First row, `Dim`:
  `time      task      lvl  message`. Then the end of `visible_logs()` (scroll as today), one row each:
  the time padded to 10 (`Dim`), `#<task>` padded to 10 (`Dim`; blank without a task or with a negative one),
  the level glyph `●` / `▲` / `✖` padded to 5 in the level's role, then the message — `logtext::compact(line)`,
  or `line.text` when `raw` — in the level's role, cut at the width. When the filter is not empty, the first
  ASCII-case-insensitive occurrence of it in the shown message has the style `theme.fg(Role::Warning)` plus
  the `REVERSED` modifier. Empty states as today (`no log line yet`, `<n> lines hidden — …`).
- **Hint row** — `Dim`, `text::fit_groups` split at ` · `:
  `f follow · / filter · e level · r raw · x proxy · t tasks · Enter filter on task · ↑↓ scroll`. While
  `editing_filter`, the row is today's status row instead (it shows the cursor and the keep/clear hint).

## Tests

`tests/log_tasks.rs` (new), building `LogLine` values in the test:
- `tasks_newest_first`: lines of tasks 7, 8, 7 → `[7, 8]`.
- `task_outcomes`: a task with an `Error` line → `Failed`; one with `Released` → `Done`; one with neither →
  `Running`.
- `task_seconds_and_midnight`: first `10:00:08`, last `10:00:32` → `Some(24)`; `23:59:50` then `00:00:10` →
  `Some(20)`; a task with a single timed line (the others without `time`) → `Some(0)`; a task whose lines all
  lack `time` → `None`.
- `task_tokens`: `Released { n_tokens: 9232 }` → `Some(9232)`; only `GenProgress { n_gen: 780, .. }` → `Some(780)`.
- `negative_tasks_skipped` and `max_respected`.

`tests/logs_help.rs` — update, never weaken: `logs_status_row`, `logs_status_states`, `logs_editing_filter`
(now at 80 × 24, where the screen is as today), `logs_narrow`, `help_overlay` (the `t` row), `help_small`,
`logs_and_help_golden` (regenerate, read every golden file). New:
- `logs_v2_layout` (demo `Generating`, 120 × 30): `Levels`, `Filter`, `Tasks` and `Log` titles on screen; the
  header `time      task      lvl  message`; a row containing `#9000` and `n_gen 156`.
- `logs_levels_counts`: build the expected info / warn / error counts from `app.logs` in the test and find
  each `group_thousands` value on its row.
- `logs_task_filter`: `t`, `Enter` → `log_view.task == Some(9000)`, every visible line has task 9000, the
  Filter panel shows `task   #9000`; `Enter` again → `None`; `Esc` also clears it.
- `logs_filter_highlight`: filter `n_gen` → the cells of the first `n_gen` of a row have `REVERSED`.

## Pitfalls

- `logs.rs` is 135 lines and `panels/log.rs` around 150: put the three side panels in
  `src/ui/screens/logs_side.rs` if `logs.rs` would pass 400 lines.
- `log_tasks` takes any iterator of `&LogLine`: call it with `app.logs.iter()`.
- `tasks_focus` must not swallow the global keys (`q`, `?`, `1`–`5`, `Tab`): they are handled before step 5.

## Acceptance criteria

- [x] The contract above is implemented.
- [x] The new and updated tests pass; golden files regenerated and read.
- [x] `bash .devbox/check` is green.
- [ ] On screen (Florian): the Logs screen looks like the validated mockup at 140 × 40.
