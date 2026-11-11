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

35 lines. Lines 1–30 are **real**, captured on the aibox on 2026-11-11 (build b12151, router mode). Lines
31–35 are hand-written in the same shape, for the cases the capture did not contain (warning, error, another
unit, a `request:` line, a line with no prefix). The parser must be **tolerant**: a line it does not
understand is still shown, as `Info`, with no event. Never drop a line, never fail on one.

Shape of a line:

```
<ISO timestamp> <host> <unit>[<pid>]: [<port>] <elapsed> <L> <message>
2026-11-11T10:28:42.776572+01:00 aibox-flo llama-server[1194]: [35335] 60.30.873.549 I slot print_timing: id  0 | task 9137 | n_gen =    161, tg =  52.96 t/s, tg_3s =  53.29 t/s
2026-11-11T10:28:39.046742+01:00 aibox-flo llama-server[1194]: 90.25.429.894 I srv  proxy_reques: proxying request to model <id> on port 35335
```

- `[<port>]` is present on lines relayed from a model instance, absent on the router's own lines;
- `<elapsed>` is digits and dots only; `<L>` is one letter: `I`, `W`, `E` or `D`;
- all three are optional (other builds and other units do not print them).

Look at the fixture with `cut -c1-200 tests/fixtures/journal/llama-server.txt | sed -n '1,35p'`.

## Contract — `src/collect/logs.rs`

Data types derive `Debug, Clone, PartialEq, serde::Serialize`.

```rust
pub enum Level { Info, Warn, Error }

pub enum LogEvent {
    /// `prompt eval time = … / N tokens ( … , X tokens per second)`
    PromptTiming { tokens: u64, tps: f64 },
    /// `eval time = … / N tokens ( … , X tokens per second)` on a line that is not a prompt line
    GenTiming { tokens: u64, tps: f64 },
    /// `n_gen = N, tg = X t/s, tg_3s = Y t/s` — printed every ~3 s while generating
    GenProgress { n_gen: u64, tps: f64, tps_3s: f64 },
    /// `prompt processing, n_tokens = N, progress = P, t = … s / X tokens per second`
    PromptProgress { n_tokens: u64, progress: f32, tps: f64 },
    /// `draft acceptance = R ( A accepted / G generated), …`
    DraftAcceptance { accepted: u64, generated: u64 },
    /// `stop processing: n_tokens = N, …` — the context size at the end of the request
    Released { n_tokens: u64 },
    /// `proxying request to model <id> on port <port>`
    Proxy { model: String, port: u16 },
    /// `request: <METHOD> <path> <client address> <status>`
    Request { method: String, path: String, status: u16 },
}

pub struct LogLine {
    /// `HH:MM:SS` cut out of the ISO timestamp; `None` when the line has no journal prefix.
    pub time: Option<String>,
    /// The unit name without `[pid]` (`llama-server`, `systemd`); `None` without prefix.
    pub source: Option<String>,
    /// The `[<port>]` of the model instance that printed the line.
    pub instance: Option<u16>,
    /// The number after `| task ` (may be −1).
    pub task: Option<i64>,
    pub level: Level,
    /// The message: without journal prefix, `[port]`, elapsed time and level letter; leading whitespace
    /// removed. Without journal prefix: the whole line.
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

**1. Journal prefix.** The line has one when its first whitespace-separated token is at least 19 bytes
long, has `T` at byte 10 and `:` at bytes 13 and 16, and the third token ends with `:`. Then: `time` =
bytes 11..19 of the first token; `source` = third token without the trailing `:` and without a trailing
`[digits]`; the **rest** is what follows the third token. Otherwise there is no prefix: `text` is the whole
line and step 2 is skipped.

**2. llama.cpp prefix**, on the rest, each part optional, in this order:
- a token `[digits]` → `instance`;
- a token made of ASCII digits and dots only, with at least one dot → dropped (elapsed time);
- a token that is exactly `I`, `W`, `E` or `D` → the **level letter**.
`text` = what remains, leading whitespace removed.

**3. Task.** `task` = the integer after the first `| task ` in `text`, if any.

**4. Event** (first match wins, on `text`). Two private helpers make this short: `number_after(text, key)`
= the first whitespace-separated token after `key`, trailing `,` removed, parsed; `number_before(text, key)`
= the last whitespace-separated token before `key`, leading `(` removed, parsed. A number that cannot be
read means **no event**.

| `text` contains | Event | Fields |
|---|---|---|
| `prompt processing, n_tokens =` | `PromptProgress` | `number_after("n_tokens =")`, `number_after("progress =")`, `number_before(" tokens per second")` |
| `prompt eval time =` | `PromptTiming` | tokens = `number_before(" tokens (")`, tps = `number_before(" tokens per second")` |
| `eval time =` | `GenTiming` | same two |
| `n_gen =` | `GenProgress` | `number_after("n_gen =")`, `number_after("tg =")`, `number_after("tg_3s =")` |
| `draft acceptance =` | `DraftAcceptance` | `number_before(" accepted")`, `number_before(" generated")` |
| `stop processing: n_tokens =` | `Released` | `number_after("n_tokens =")` |
| `proxying request to model ` | `Proxy` | model = the token after `model `, port = `number_after(" on port ")` |
| `request: ` + at least 4 items | `Request` | method = 1st item, path = 2nd, status = 4th |

**5. Level** (first match wins):
1. `Request` event: status ≥ 500 → `Error`; 400–499 → `Warn`; else `Info`;
2. a level letter was found: `E` → `Error`, `W` → `Warn`, `I` and `D` → `Info` — and the keywords below are
   **not** looked at;
3. `text`, lower-cased, contains `error`, `failed`, `failure` or `out of memory` → `Error`;
4. `text`, lower-cased, contains `warn` → `Warn`;
5. else `Info`.

## Pitfalls

- Slice by **bytes** only after checking `is_char_boundary` (or use `get(11..19)`): a non-ASCII first token
  must not panic.
- Lines may not be valid UTF-8: read with `BufRead::read_until(b'\n', …)` and `String::from_utf8_lossy`,
  not with `lines()`. Strip the trailing `\n` and `\r`.
- `stop` must work while the reader thread is **blocked** waiting for a line (a quiet journal): keep the
  `Child` where `stop` can reach it (take `child.stdout` out for the thread first), `kill()` it, `wait()`
  for it, then join the thread. Killing the child ends the pipe, which unblocks the reader.
- Never let the child outlive llamatop: `Drop` must stop it.
- The generation line contains `eval time =` but not `prompt eval time =`; the prompt line contains both;
  the `prompt processing` line also ends with `tokens per second`. The order of the table matters.
- Search for the keys with their ` =` (`tg =`, `tg_3s =`, `n_tokens =`): `tg` alone would also match `tg_3s`.
- The model id of a `Proxy` line contains `/` and `:`; it never contains a space.

## Tests — `tests/logs.rs`

`parse_line` on the fixture lines (1-based numbers; read the file and index its lines). Float tolerance 0.01.

- `router_line`: line 1 → time `10:28:39`, source `llama-server`, instance `None`, task `None`, level `Info`,
  text starts with `srv  proxy_reques: proxying request`, event
  `Proxy { model: "peculiar-ragdoll/Cyber-Tiel-Coder-35B-A3B-GGUF:Q4_K_M", port: 35335 }`.
- `instance_line`: line 3 → instance `Some(35335)`, task `Some(9137)`, text starts with `slot launch_slot_:`,
  event `None`. Line 2 → task `Some(-1)`.
- `gen_progress`: line 4 → `GenProgress { n_gen: 161, tps: 52.96, tps_3s: 53.29 }`.
- `prompt_timing`: line 8 → `PromptTiming { tokens: 48, tps: 85.26 }`; line 24 → tokens 10229, tps 891.00.
- `gen_timing`: line 9 → `GenTiming { tokens: 627, tps: 46.63 }`.
- `lines_without_event`: lines 10, 11 and 12 → event `None`, level `Info`.
- `draft_acceptance`: line 13 → `DraftAcceptance { accepted: 369, generated: 771 }`.
- `released`: line 14 → `Released { n_tokens: 57724 }`.
- `prompt_progress`: line 18 → `PromptProgress { n_tokens: 4096, progress: 0.40, tps: 1004.65 }`.
- `warning_letter`: line 31 → `Warn`, text starts with `srv    load_model:`.
- `error_letter`: line 32 → `Error`.
- `other_unit_uses_keywords`: line 33 → source `systemd`, instance `None`, level `Error`.
- `request_line`: line 34 → `Request { method: "POST", path: "/v1/chat/completions", status: 401 }`, `Warn`.
- `line_without_prefix`: line 35 → time `None`, source `None`, text = the whole line, `Info`, no event.
- `letter_wins_over_keywords`: `"2026-11-11T10:00:00.000000+01:00 h llama-server[1]: [1] 1.2 I srv x: no error here"`
  → `Info`.
- `weird_input_never_panics`: `""`, `"   "`, `"é"`, `"éééééééééééééééééééééé x y: z"`, a 100 000-byte line,
  `"request: GET"`, `"eval time = x tokens per second"`, `"n_gen = a, tg = b"`, `"[99999999] I"` → each returns
  a `LogLine` with event `None`.

`follow` (collect lines into an `Arc<Mutex<Vec<LogLine>>>`; wait by polling up to 3 s, never a fixed sleep
longer than 50 ms):

- `follows_a_command_to_its_end`: `cat <absolute fixture path>` → 36 lines; the last one is the `Warn`
  `log source ended: cat`; the first has time `10:28:39`.
- `stop_kills_a_silent_command`: `sleep 30` → `stop()` returns within 2 s (measure with `Instant`).
- `drop_stops_the_command`: `sleep 30`, drop the `Follower` → returns within 2 s.
- `missing_program_is_an_error`: `llamatop-no-such-program --x` → `Err`; empty command `"  "` → `Err`.
- `sink_can_refuse`: `cat <fixture>` with a sink returning `false` → exactly 1 line collected.

## Acceptance criteria

- [x] Exact public API above.
- [x] The 21 tests exist under these names and pass.
- [x] After `cargo test`, no `sleep 30` process is left running (`pgrep -x sleep` shows none of ours).
- [x] No fixture modified.
- [x] `bash .devbox/check` is green.
