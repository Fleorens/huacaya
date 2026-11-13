# T40 — Logs screen: the Tasks selection follows its task, not its row

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (follow-up of the T37 review, asked by Florian on 2026-11-13) |
| **Depends on** | T39 |
| **Read first** | `src/app/log_view.rs` (`LogView`); `src/app/state.rs` (`LogView` construction); `src/app/keys.rs` (step 5, the Logs screen, the `tasks_focus` block); `src/app/log_tasks.rs` (`log_tasks`, `TaskSummary`); `src/ui/screens/logs_side.rs` (`render_tasks`); `tests/logs_help.rs` (`logs_task_filter`) |
| **Deliverable** | changes to `src/app/log_view.rs`, `src/app/state.rs`, `src/app/keys.rs`, `src/ui/screens/logs_side.rs`; `tests/logs_help.rs` |

## Why

`LogView::task_selected` is an index into `log_tasks(&app.logs, 50)`, a list sorted newest first. When a new
request arrives, every task moves down one row, so the same index now points at another task: the `▶` jumps
and `Enter` filters on a task the user did not choose. The selection must remember the task itself.

## Contract

- `LogView::task_selected` becomes `pub task_selected: Option<i64>`, doc comment: `/// The task under the ▶ in
  the Tasks list; None: the first (newest) task.` `App::new` sets `None`.
- A new pure function in `src/app/log_tasks.rs`:

  ```rust
  /// The row of `selected` in `tasks`: its index when it is there, else 0 (the newest task, also when
  /// `selected` is `None` or the task has left the buffer). 0 for an empty list. Never panics.
  pub fn selected_row(tasks: &[TaskSummary], selected: Option<i64>) -> usize;
  ```
- Keys, while `tasks_focus` (same keys as today, same "no wrap"): compute `row = selected_row(&tasks,
  task_selected)`; `Up` / `Down` / `Home` / `End` move from `row` and store the **task** of the new row
  (`Some(tasks[new_row].task)`), nothing when the list is empty; `Enter` uses `tasks[row]`, otherwise exactly
  as today (toggle `task`, `scroll = 0`, `follow = true`). Every other key falls through as today.
- `render_tasks` uses `selected_row(&tasks, app.log_view.task_selected)` wherever it uses the index today
  (the `▶` and the scrolling). Nothing else on screen changes: the golden files must not change.

## Tests

`tests/log_tasks.rs`:
- `selected_row_cases`: tasks `[8, 7]` → `selected_row(.., Some(7)) == 1`, `Some(8) == 0`, `None == 0`,
  `Some(99) == 0`; empty list → `0`.

`tests/logs_help.rs` — update, never weaken; `logs_task_filter` keeps passing unchanged. New:
- `logs_task_selection_follows_task`: demo `Generating`, Logs screen, `t`, then append to `app.logs` the
  lines of a newer task (task `9001`, built in the test) so it becomes the first row; `Enter` → `log_view.task ==
  Some(9000)` (the `▶` stayed on 9000, not on the new row 0).

## Pitfalls

- `logs_side.rs` and `keys.rs` are close to 300 lines: keep the change local, no new helper beyond
  `selected_row`.
- `tasks[row]` on an empty list panics: check `tasks.is_empty()` first, as today.

## Acceptance criteria

- [ ] The contract above is implemented.
- [ ] The new and updated tests pass; golden files unchanged.
- [ ] `bash .devbox/check` is green.
