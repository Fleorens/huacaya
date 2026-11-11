# T07 — Following and reading the server log

| | |
|---|---|
| **Batch** | 1 — data layer |
| **Depends on** | T02 |
| **Read first** | `docs/01-architecture.md` § 4 |
| **Deliverable** | `src/collect/logs.rs`, `tests/logs.rs`; one `pub mod` line in `src/collect/mod.rs` |

## Why

The log panel shows the server's journal live. llamatop does not read the journal itself: it runs a command
(by default `journalctl -u llama-server -f -n 200 --no-pager -q -o short-iso-precise`) and reads its standard
output line by line. Any command works (`ssh aibox journalctl …`, `tail -F file`), which is also how the
tests run: with `cat` on a fixture.

## Fixture (do not edit) — `tests/fixtures/journal/llama-server.txt`

18 lines. ⚠ The llama.cpp messages in it are **plausible, not captured**: the real format will be checked on
the aibox before batch 2. So the parser must be **tolerant**: a line it does not understand is still shown,
as `Info`, with no event. Never drop a line, never fail on one.

Shape of a journal line: `<ISO timestamp> <host> <unit>[<pid>]: <message>`, for example
`2026-11-11T10:21:03.900500+01:00 aibox-flo llama-server[1201]: prompt eval time =     169.37 ms /   109 tokens (    1.55 ms per token,   643.56 tokens per second)`.

## Contract — `src/collect/logs.rs`

Data types derive `Debug, Clone, PartialEq, serde::Serialize`.

```rust
pub enum Level { Info, Warn, Error }

pub enum LogEvent {
    /// From a `prompt eval time = … / N tokens ( … , X tokens per second)` line.
    PromptTiming { tokens: u64, tps: f64 },
    /// From an `eval time = …` line that is not a `prompt eval time` line.
    GenTiming { tokens: u64, tps: f64 },
    /// From `request: <METHOD> <path> <client address> <status>`.
    Request { method: String, path: String, status: u16 },
}

pub struct LogLine {
    /// `HH:MM:SS` cut out of the ISO timestamp; `None` when the line has no journal prefix.
    pub time: Option<String>,
    /// The unit name without `[pid]` (`llama-server`, `systemd`); `None` without prefix.
    pub source: Option<String>,
    pub level: Level,
    /// The message, without the journal prefix, leading whitespace removed. Without prefix: the whole line.
    pub text: String,
    pub event: Option<LogEvent>,
}

/// Classifies one raw line. Total function: never panics, whatever the input (empty, binary, huge).
pub fn parse_line(raw: &str) -> LogLine;

/// A running log command. Dropping it stops it.
pub struct Follower { /* private: the child process, the reader thread */ }

/// Starts `command` (split on ASCII whitespace; first item = program) with stdin and stderr closed
/// (`Stdio::null()`), and calls `sink` with every line of its standard output, from a new thread.
/// When the output ends, `sink` receives one last line: level `Warn`, text `log source ended: <program>`,
/// no time, no source. The thread ends then, or as soon as `sink` returns `false`.
/// `Err` when the command is empty or the program cannot be started (e.g. `journalctl` not installed).
pub fn follow(
    command: &str,
    sink: impl FnMut(LogLine) -> bool + Send + 'static,
) -> std::io::Result<Follower>;

impl Follower {
    /// Kills the command, waits for it, joins the thread. Safe to call twice.
    pub fn stop(&mut self);
}
impl Drop for Follower { /* calls stop */ }
```

### `parse_line` rules

**Prefix.** The line has a journal prefix when its first whitespace-separated token is at least 19 bytes
long, has `T` at byte 10 and `:` at bytes 13 and 16, and the third token ends with `:`. Then: `time` =
bytes 11..19 of the first token; `source` = third token without the trailing `:` and without a trailing
`[digits]`; `text` = everything after the third token, with leading whitespace removed. Otherwise there is
no prefix.

**Event** (first match wins, on `text`):
1. contains `prompt eval time =` → `PromptTiming`;
2. contains `eval time =` → `GenTiming`;
   for both: `tokens` = the integer just before the first ` tokens`; `tps` = the number just before
   ` tokens per second`. If either number cannot be read: no event;
3. contains `request: ` followed by at least 4 whitespace-separated items → `Request` with method = 1st,
   path = 2nd, status = 4th parsed as `u16`. If the status does not parse: no event.

**Level** (first match wins):
1. `Request` event: status ≥ 500 → `Error`; 400–499 → `Warn`; else `Info`;
2. `text` starts with `E ` or, lower-cased, contains `error`, `failed`, `failure` or `out of memory` → `Error`;
3. `text` starts with `W ` or, lower-cased, contains `warn` → `Warn`;
4. else `Info`.

## Pitfalls

- Slice by **bytes** only after checking `is_char_boundary` (or use `get(11..19)`): a non-ASCII first token
  must not panic.
- Lines may not be valid UTF-8: read with `BufRead::read_until(b'\n', …)` and `String::from_utf8_lossy`,
  not with `lines()`. Strip the trailing `\n` and `\r`.
- `stop` must work while the reader thread is **blocked** waiting for a line (a quiet journal): keep the
  `Child` where `stop` can reach it (take `child.stdout` out for the thread first), `kill()` it, `wait()`
  for it, then join the thread. Killing the child ends the pipe, which unblocks the reader.
- Never let the child outlive llamatop: `Drop` must stop it.
- `"       eval time = …"` (the generation line) contains `eval time =` but not `prompt eval time =`; the
  prompt line contains both. Order of the rules matters.

## Tests — `tests/logs.rs`

`parse_line` on the fixture lines (1-based numbers; read the file and index its lines):

- `journal_prefix_is_split`: line 1 → time `10:20:58`, source `llama-server`, text starts with
  `srv  log_server_r: request: GET /health`, event `Request { GET, /health, 200 }`, level `Info`.
- `prompt_timing`: line 7 → `PromptTiming { tokens: 109, tps: 643.56 }`, level `Info`.
- `gen_timing`: line 8 → `GenTiming { tokens: 200, tps: 62.48 }`; `text` starts with `eval time =`.
- `total_time_has_no_event`: line 9 → event `None`.
- `warning_line`: line 13 → `Warn`.
- `unauthorized_request_is_a_warning`: line 14 → `Request` with status 401, level `Warn`.
- `error_line`: line 15 → `Error`.
- `other_unit`: line 17 → source `systemd`, level `Error`.
- `line_without_prefix`: line 18 → time `None`, source `None`, text = the whole line, `Info`, no event.
- `weird_input_never_panics`: `""`, `"   "`, `"é"`, `"éééééééééééééééééééééé x y: z"`, a 100 000-byte line,
  `"request: GET"`, `"eval time = x tokens per second"` → each returns a `LogLine` (event `None` for the
  last two).
- `server_error_request`: `"srv  log_server_r: request: POST /v1/chat/completions 10.0.0.1 500"` → `Error`.

`follow` (collect lines into an `Arc<Mutex<Vec<LogLine>>>`; wait by polling up to 3 s, never a fixed sleep
longer than 50 ms):

- `follows_a_command_to_its_end`: `cat <absolute fixture path>` → 19 lines; the last one is the `Warn`
  `log source ended: cat`; the first has time `10:20:58`.
- `stop_kills_a_silent_command`: `sleep 30` → `stop()` returns within 2 s (measure with `Instant`).
- `drop_stops_the_command`: `sleep 30`, drop the `Follower` → returns within 2 s.
- `missing_program_is_an_error`: `llamatop-no-such-program --x` → `Err`; empty command `"  "` → `Err`.
- `sink_can_refuse`: `cat <fixture>` with a sink returning `false` → exactly 1 line collected.

## Acceptance criteria

- [ ] Exact public API above.
- [ ] The 16 tests exist under these names and pass.
- [ ] After `cargo test`, no `sleep 30` process is left running (`pgrep -x sleep` shows none of ours).
- [ ] No fixture modified.
- [ ] `bash .devbox/check` is green.
