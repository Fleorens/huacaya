# T09 — App state, events and keys

| | |
|---|---|
| **Batch** | 2 — the dashboard |
| **Depends on** | T08 (validée) |
| **Read first** | `docs/02-ui.md` § 2 and § 5 |
| **Deliverable** | `src/app/mod.rs`, `src/app/state.rs`, `src/app/keys.rs`, `tests/app_state.rs`; `pub mod app;` in `src/lib.rs` |

## Why

The dashboard is a pure state machine fed by events: snapshots from the collector threads, log lines, key
presses, ticks. Everything the screens show and every key's effect lives in one `App` value that the UI only
reads. Writing it first, without any terminal, means the whole behaviour is tested before anything is drawn.

This sheet draws nothing and starts no thread: no terminal, no `ratatui` here. It touches nothing outside the
deliverable.

## Contract

`src/app/mod.rs`: one-line module doc, `pub mod keys; pub mod state;` and `pub use state::*;`.

### `src/app/state.rs`

```rust
use std::collections::VecDeque;
use crossterm::event::KeyEvent;
use crate::collect::llama_poll::LlamaSnapshot;
use crate::collect::logs::{Level, LogLine};
use crate::collect::sampler::SystemSnapshot;
use crate::util::History;

/// Log lines kept in memory; the oldest is dropped beyond.
pub const LOG_CAPACITY: usize = 2000;
/// Samples kept per graph (5 min at the default 1 s interval).
pub const HISTORY_CAPACITY: usize = 300;
/// Refresh periods offered by `+` / `-`, in milliseconds.
pub const INTERVAL_STEPS: [u64; 6] = [200, 500, 1000, 2000, 5000, 10_000];
/// Lines moved by `PgUp` / `PgDn`.
pub const PAGE: usize = 10;

/// What reaches the UI thread.
#[derive(Debug, Clone)]
pub enum Event {
    System(SystemSnapshot),
    Llama(LlamaSnapshot),
    Log(LogLine),
    Key(KeyEvent),
    /// The terminal changed size; the next frame is laid out again.
    Resize,
    /// The 100 ms animation tick.
    Tick,
}

/// The four screens, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen { Overview, Models, Logs, System }

impl Screen {
    pub const ALL: [Screen; 4] = [Screen::Overview, Screen::Models, Screen::Logs, Screen::System];
    /// Next in tab order, wrapping (`System` → `Overview`).
    pub fn next(self) -> Self;
    /// Previous in tab order, wrapping (`Overview` → `System`).
    pub fn prev(self) -> Self;
    /// `"Overview"`, `"Models"`, `"Logs"`, `"System"`.
    pub fn title(self) -> &'static str;
}

/// Which levels the log shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFilter { All, WarnAndErrors, Errors }

impl LevelFilter {
    /// `All` → `WarnAndErrors` → `Errors` → `All`.
    pub fn next(self) -> Self;
    /// `All`: every level; `WarnAndErrors`: `Warn` and `Error`; `Errors`: `Error` only.
    pub fn accepts(self, level: Level) -> bool;
}

/// What the log view shows and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct LogView {
    /// `true`: the view sticks to the newest line.
    pub follow: bool,
    /// Visible lines hidden below the view; 0 = the newest line is the last one shown.
    pub scroll: usize,
    /// Case-insensitive substring searched in `LogLine::text`; empty = no filter.
    pub filter: String,
    /// `true` while the user types the filter after `/`.
    pub editing_filter: bool,
    pub level: LevelFilter,
    /// `false`: lines whose event is `LogEvent::Proxy` are hidden. The router prints two of them for
    /// every polling round of llamatop itself, so they are noise by default.
    pub show_proxy: bool,
}

/// The last request, assembled from the timing lines of one task.
#[derive(Debug, Clone, PartialEq)]
pub struct LastRequest {
    pub task: Option<i64>,
    /// `HH:MM:SS` of the line that started this request.
    pub time: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub prompt_tps: Option<f64>,
    pub gen_tokens: Option<u64>,
    pub gen_tps: Option<f64>,
    /// `accepted / generated × 100` of the `DraftAcceptance` line.
    pub draft_pct: Option<f64>,
    /// `n_tokens` of the `Released` line: the context size at the end of the request.
    pub ctx_tokens: Option<u64>,
}

/// The whole state of the dashboard. The UI reads it and never changes it.
#[derive(Debug, Clone)]
pub struct App {
    pub system: Option<SystemSnapshot>,
    pub llama: Option<LlamaSnapshot>,
    pub logs: VecDeque<LogLine>,
    /// `busy_pct` of the first GPU.
    pub gpu_load: History,
    /// `cpu.usage_pct`.
    pub cpu_usage: History,
    /// `live_gen_tps` of the first active model (0 when it is not generating).
    pub gen_tps: History,
    pub last_request: Option<LastRequest>,
    pub screen: Screen,
    pub help: bool,
    pub paused: bool,
    pub interval_ms: u64,
    pub log_view: LogView,
    /// Index into `llama.models` on the Models screen.
    pub model_selected: usize,
    /// Number of `Tick`s received, drives the animations.
    pub frame: u64,
    pub quit: bool,
}

impl App {
    /// Empty state: no snapshot, no line, `Screen::Overview`, follow on, filter empty, level `All`,
    /// `show_proxy` false, histories of `HISTORY_CAPACITY`, `frame` 0, everything else false / 0.
    pub fn new(interval_ms: u64) -> Self;
    /// Applies one event (rules below) and returns `true` when the screen must be redrawn.
    pub fn apply(&mut self, event: Event) -> bool;
    /// The log lines the log view shows, oldest first: level accepted by `log_view.level`, not a hidden
    /// `Proxy` line, and containing `log_view.filter` (ASCII case-insensitive) when it is not empty.
    pub fn visible_logs(&self) -> Vec<&LogLine>;
    /// `true` when something on screen moves by itself: a slot of an active model `is_processing`, or a
    /// model in state `Loading`. `false` without a llama snapshot.
    pub fn animating(&self) -> bool;
}
```

Rules of `apply`:

| Event | Effect | Returns |
|---|---|---|
| `System(s)` | store it; push `gpus[0].busy_pct` into `gpu_load` and `cpu.usage_pct` into `cpu_usage`, each only when `Some` | `!paused` |
| `Llama(s)` | store it; when `active` is not empty, push `active[0].live_gen_tps.unwrap_or(0.0)` into `gen_tps`; clamp `model_selected` to `models.len() - 1` (0 when empty) | `!paused` |
| `Log(l)` | update `last_request` (below); push at the back, dropping the front beyond `LOG_CAPACITY`; when `!follow` and the line would be visible, `scroll += 1` so the view stays put | `!paused` |
| `Key(k)` | `keys::handle(self, k)` | `true` |
| `Resize` | — | `true` |
| `Tick` | `frame += 1` | `!paused && animating()` |

Data keeps flowing while paused: `paused` only stops the redraws that data and ticks ask for.

`last_request`, from `LogLine::event` and `LogLine::task`:
- `PromptTiming { tokens, tps }` starts a **new** `LastRequest` with that `task` and `time`, `prompt_tokens`,
  `prompt_tps` set and everything else `None`;
- `GenTiming`, `DraftAcceptance`, `Released` fill `gen_tokens` + `gen_tps`, `draft_pct`, `ctx_tokens` of the
  current one when its `task` equals the line's `task`; otherwise they start a new one (same `task` and
  `time` rule) holding only that field;
- `DraftAcceptance` with `generated == 0` gives `draft_pct: None`; every other event changes nothing.

### `src/app/keys.rs`

```rust
/// Applies one key press to the state. Key releases (`KeyEventKind::Release`) are ignored.
pub fn handle(app: &mut App, key: KeyEvent);
```

In this order, the first matching block wins:

1. `Ctrl-C` (char `c` with `KeyModifiers::CONTROL`) → `quit = true`, from anywhere.
2. **Filter being typed** (`log_view.editing_filter`): a `Char` without `CONTROL` is appended; `Backspace`
   removes the last char; `Enter` ends editing and keeps the filter; `Esc` ends editing and clears the
   filter. Any change of the filter sets `scroll = 0` and `follow = true`. Other keys are ignored.
3. **Help shown** (`help`): `?` or `Esc` → `help = false`; `q` → quit; other keys ignored.
4. Everywhere: `q` → quit; `1`–`4` → `Screen::ALL[n - 1]`; `Tab` → `next()`; `BackTab` → `prev()`;
   `?` → `help = true`; `p` → toggle `paused`; `+` → the largest `INTERVAL_STEPS` value strictly below
   `interval_ms` (unchanged when none); `-` → the smallest one strictly above (unchanged when none).
5. **Logs screen** only — with `max = visible_logs().len().saturating_sub(1)`:
   `Up` / `PgUp` → `follow = false`, `scroll` + 1 / + `PAGE`, capped at `max`;
   `Down` / `PgDn` → `scroll` − 1 / − `PAGE`, saturating at 0, then `follow = (scroll == 0)`;
   `Home` → `scroll = max`, `follow = false`; `End` → `scroll = 0`, `follow = true`;
   `f` → toggle `follow`, and `scroll = 0` when it becomes `true`; `/` → `editing_filter = true`;
   `e` → `level = level.next()`; `x` → toggle `show_proxy`; `Esc` → clear the filter.
   `e`, `x` and `Esc` also set `scroll = 0` and `follow = true`.
6. **Models screen** only: `Up` / `Down` move `model_selected` by one within `0..models.len()`
   (no wrap; nothing without models); `Home` → 0; `End` → last.

## Pitfalls

- Build keys in tests with `KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)`; its kind is `Press`.
- `Shift-Tab` arrives as `KeyCode::BackTab`; `+` may arrive with `KeyModifiers::SHIFT`: match the char,
  not the modifiers (except `CONTROL` for Ctrl-C and during filter typing).
- The borrow checker: compute `visible_logs().len()` into a local before mutating `log_view`.
- Building a `SystemSnapshot` / `LlamaSnapshot` by hand in tests is long: write small helpers in the test
  file (`fn system(busy: Option<f32>, cpu: Option<f32>) -> SystemSnapshot`, `fn llama(…)`), not in `src/`.
- 400-line limit: `state.rs` holds types and `apply`, `keys.rs` the key table.

## Tests

`tests/app_state.rs`, exactly these names. `key(c)` and `code(k)` are helpers of the test file building
`Event::Key`. The fixture lines come from `tests/fixtures/journal/llama-server.txt` through
`collect::logs::parse_line`.

- `new_state`: `App::new(1000)` → `screen` Overview, `log_view.follow` true, `show_proxy` false, `frame` 0,
  `!quit`, `gpu_load.capacity()` = 300.
- `screen_keys`: `2` → Models; `Tab` → Logs; `BackTab` twice → Overview; `BackTab` → System; `4` then
  `Tab` → Overview.
- `quit_keys`: `q` quits; on a fresh app, `Ctrl-C` quits; on a fresh app on the Logs screen, `/` then `q`
  → the filter is `"q"` and the app does not quit, then `Ctrl-C` quits.
- `help_overlay`: `?` → help; `1` ignored while help (screen unchanged); `Esc` closes; `?` then `q` quits.
- `interval_steps`: from 1000: `+` → 500, `+` → 200, `+` → 200; `-` → 500. From 750: `+` → 500;
  from 750: `-` → 1000. From 10000: `-` → 10000.
- `pause_stops_data_redraws`: `p` → paused; `apply(System(..))` returns false but the snapshot is stored
  and `cpu_usage.len()` grew; `p` again → `apply(System(..))` returns true.
- `histories`: a system snapshot with `busy_pct` 97 and `usage_pct` 38 → `gpu_load.last()` 97,
  `cpu_usage.last()` 38; with both `None` → lengths unchanged. A llama snapshot with one active model
  whose `live_gen_tps` is `Some(62.5)` → `gen_tps.last()` 62.5; `None` → 0.0; no active model → length
  unchanged.
- `tick_redraws_only_when_animating`: no llama → `apply(Tick)` false, `frame` 1; with an active model whose
  slot `is_processing` → true; same but paused → false.
- `last_request_from_fixture`: apply all fixture lines as `Event::Log` → `last_request` = task `Some(9397)`,
  time `"10:38:21"`, prompt `10229` tokens at `891.0`, gen `86` at `75.83`, `ctx_tokens` `10314`, and
  `draft_pct` within 1e-6 of `61.0 / 72.0 * 100.0`.
- `log_capacity`: 2005 lines `parse_line("line N")` → `logs.len()` 2000, the front is `"line 5"`.
- `proxy_lines_hidden`: fixture lines → no `visible_logs()` entry has a `Proxy` event; on the Logs screen
  `x` → at least one has; `x` again → none.
- `level_and_text_filter`: fixture lines, Logs screen: `e` → every visible line is `Warn` or `Error` and
  there is at least one; `e` → only `Error`; `e` → back to all. `/`, type `TASK 9397`, `Enter` → every
  visible line contains `task 9397` (case-insensitive) and `editing_filter` is false; `Esc` → filter empty.
- `scrolling`: fixture lines, Logs screen: `Up` → follow false, scroll 1; `PgUp` → 11 (capped at
  `visible_logs().len() - 1` if lower); `End` → scroll 0, follow true; `Up`, then one more `Event::Log`
  that is visible → scroll 2; `Down` twice → scroll 0, follow true; `Home` → scroll = `len - 1`.
- `model_selection`: a llama snapshot with 5 models, Models screen: `Down` ×6 → 4; `Up` → 3; `Home` → 0;
  `End` → 4; a new snapshot with 2 models → `model_selected` 1. On the Overview screen `Down` changes
  nothing.

## Acceptance criteria

- [ ] The files exist with the exact public API above; `src/lib.rs` declares `pub mod app;`.
- [ ] Nothing outside the deliverable changed (except `tasks/`).
- [ ] The 14 tests above exist under these names and pass.
- [ ] `bash .devbox/check` is green.
