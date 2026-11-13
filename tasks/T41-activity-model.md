# T41 — Activity model: stations, command classification, per-session activity (pure)

| | |
|---|---|
| **Batch** | 8 — the Agents map (design validated by Florian on 2026-11-13) |
| **Depends on** | T40 |
| **Read first** | `src/collect/opencode.rs` and `src/collect/opencode/raw.rs` (whole files); `tests/opencode.rs` (`steps_fixture`, `steps_real_fixture`); `tests/fixtures/opencode/messages.json` and `messages-real.json` |
| **Deliverable** | `src/collect/activity.rs`, `tests/activity.rs`; `pub mod activity;` in `src/collect/mod.rs`; changes to `src/collect/opencode.rs`, `src/collect/opencode/raw.rs`, `src/ui/screens/agents.rs` (one glyph), `tests/opencode.rs`, `tests/snapshots/agents-140x40.txt` |

## Why

The Agents screen gets a live map of what the OpenCode session does (T43, T44): a ring of eight **stations**
(You, Model, Files, Shell, Checks, Web, Agents, Tools) around the model, a pulse on a spoke each time a step
starts or ends. This sheet is the data side, pure and fully testable: which station a step belongs to, and an
`Activity` that folds the poller's rounds into per-station counts, running steps and pulses.

The poller (T42) will fetch only the last few messages each second, so steps must have a stable identity
across rounds and an end time that says whether they still run. Real OpenCode 2.2.7 answers
(`messages-real.json`) give: tool parts have an `id` and `time.created` / `time.completed`; reasoning parts
have `time.created` / `time.completed` but no `id`; text parts have neither; every message has an `id` and
`time.created`, plus `time.completed` once finished. A message still running has no `time.completed`, and a
part still running has no `time.completed`.

## Contract

### `src/collect/opencode/raw.rs` (private)

- `RawMessage` gains `#[serde(default)] id: Option<String>`.
- `RawPart` gains `#[serde(default)] id: Option<String>`.
- `RawTime` gains `#[serde(default)] completed: Option<i64>`.

### `src/collect/opencode.rs`

`Step` gains two fields (keep the existing ones and their doc comments):

```rust
/// Stable across poll rounds: the tool part's `id`; for other parts `<message id>#<part index>`
/// (index in the message's `content`, from 0); `#<part index>` alone when the message has no `id`.
pub id: String,
/// When the step ended, Unix ms: the part's `time.completed` for tools and reasoning; for text and user
/// parts the message's `time.completed`, else `None`. `None` means the step still runs.
pub end_ms: Option<i64>,
```

`StepKind` gains `User` (first variant). `parse_steps` now also turns `user` messages into steps: one
`StepKind::User` step per `text` part, `summary` = the first line of the text (as `summary_text` does),
`ok: None`, `detail: None`, `time_ms` = the message's `time.created`, `end_ms` = `Some(time_ms)`. Other message
types (`idle`, …) still give nothing. The doc comment of `parse_steps` says so.

Shell summaries drop a leading `cd <directory> && ` when `<directory>` is exactly the `directory` argument
(the model prefixes every command with it): `cd /home/dev/work/.wt/llamatop-145 && cargo fmt` with that
directory → `cargo fmt`. Any other `cd` stays.

### `src/collect/activity.rs`

```rust
/// A place on the Agents map. `ALL` is the drawing order, clockwise from the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Station { You, Model, Files, Shell, Checks, Web, Agents, Tools }

impl Station {
    pub const ALL: [Station; 8] = [/* You, Model, Files, Shell, Checks, Web, Agents, Tools */];
    /// Position in `ALL`.
    pub fn index(self) -> usize;
    /// `"You"`, `"Model"`, `"Files"`, `"Shell"`, `"Checks"`, `"Web"`, `"Agents"`, `"Tools"`.
    pub fn name(self) -> &'static str;
}

/// The station of a step (rules below).
pub fn station(step: &Step) -> Station;

/// The station of a shell command line (rules below).
pub fn command_station(command: &str) -> Station;

/// How long a pulse travels along its spoke, ms.
pub const PULSE_MS: i64 = 600;

/// One station's totals for the followed session.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StationStats {
    /// Steps seen at this station since the session was first folded in.
    pub calls: u32,
    /// Steps that ended with `ok == Some(false)`.
    pub errors: u32,
    /// Steps running now (`end_ms == None`) in the last round.
    pub running: u32,
    /// The earliest `time_ms` among the running ones.
    pub running_since: Option<i64>,
    /// `time_ms` of the newest step seen at this station.
    pub last_ms: Option<i64>,
}

/// A step seen starting (`outbound`) or ending by the poller.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulse {
    pub station: Station,
    /// When the pulse leaves, Unix ms (the round's time, see `update`).
    pub at_ms: i64,
    pub outbound: bool,
    /// An inbound pulse of a step that ended with `ok == Some(false)`.
    pub failed: bool,
}

/// The map's state for the followed session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Activity {
    /// The session folded in, `None` before the first round.
    pub session: Option<String>,
    /// Indexed by `Station::index`.
    pub stations: [StationStats; 8],
    /// Pulses not older than `2 * PULSE_MS`, oldest first.
    pub pulses: Vec<Pulse>,
    /// The newest running step: its station, summary and `time_ms`.
    pub now: Option<(Station, String, i64)>,
    // private: step id → its `end_ms` as last seen (a HashMap), to tell new and ended steps apart.
}

impl Activity {
    /// Folds one round in. `steps` is any window of the session's timeline (a full history or only the
    /// newest steps); `now_ms` is the round's wall-clock time.
    pub fn update(&mut self, session: &str, steps: &[Step], now_ms: i64);
}
```

#### `station`

- `User` → `You`; `Think`, `Say` → `Model`; `Read`, `Search`, `Edit`, `Write` → `Files`;
  `Shell` → `command_station(&step.summary)`.
- `Other(name)`: `webfetch`, `websearch`, `codesearch` → `Web`; `task` → `Agents`; `glob`, `list`, `grep`,
  `patch`, `multiedit` → `Files`; anything else (`todowrite`, `skill`, `compress`, MCP tools…) → `Tools`.

#### `command_station`

1. Split the line into segments on `&&`, `||`, `;` and `|` (plain text split, no quote handling).
2. In each segment, drop redirections: a word starting with `>`, `<`, `2>`, `&>`, or `1>`, and when such a
   word is exactly the operator (`>`, `>>`, `<`, `2>`), the next word too. Then skip leading `NAME=value`
   words and the wrappers `sudo`, `env`, `nohup`, `time`, `timeout` (and `timeout`'s next word, the
   duration). The next word is the **program**: its file name (`/usr/bin/cargo` → `cargo`); the words after it
   are its arguments.
3. Segments whose program is `cd`, `echo`, `printf`, `export`, `true`, `false`, `set`, `pwd`, `sleep`, `which`,
   `test`, `[` or `exit`, and empty segments, are **neutral**: they are ignored below.
4. **Checks** if any segment is a check:
   - `cargo` with first argument `test`, `nextest`, `clippy` or `check`, or `fmt` with `--check` among its
     arguments;
   - `bash` or `sh` whose first argument ends with `check` (`.devbox/check`), or a program named `check`;
   - `make` with first argument `test`, `check` or `lint`;
   - `pytest`, `ruff`, `mypy`, `eslint`, `tsc`; `go test`; `npm test`; `npm run` with `test`, `lint` or
     `check`.
5. Else **Web** if any segment reaches the network: `curl`, `wget`, `gh`, `ssh`, `scp`, `rsync`, `ping`;
   `git` with first argument `push`, `pull`, `fetch`, `clone` or `ls-remote`; `cargo` with `add`, `install`,
   `update`, `fetch` or `publish`; `npm`, `pnpm`, `yarn` with `install`, `i`, `ci`, `add` or `update`; `pip`,
   `pip3` with `install` or `download`.
6. Else **Files** if there is at least one non-neutral segment and every non-neutral one is a file command:
   `cat`, `head`, `tail`, `sed`, `grep`, `rg`, `find`, `ls`, `wc`, `cp`, `mv`, `mkdir`, `touch`, `diff`,
   `cmp`, `sort`, `uniq`, `cut`, `tr`, `awk`, `tee`, `stat`, `file`, `du`, `tree`, `ln`, `rm`, `rmdir`,
   `realpath`, `basename`, `dirname`, `jq`.
7. Else **Shell**.

#### `Activity::update`

1. When `session` differs from `self.session`: reset everything (`*self = Activity::default()`), set
   `session`, and treat this round as **history**: no pulses are made in it.
2. For each step (in the given order), at `s = station(step)`:
   - id never seen → `calls += 1`; `last_ms = max(last_ms, time_ms)`; if `end_ms` is `Some` and
     `ok == Some(false)`, `errors += 1`. Outside history: an outbound pulse at `now_ms`; if the step has
     already ended, also an inbound pulse at `now_ms + PULSE_MS` (`failed` when `ok == Some(false)`).
   - id seen running (`None`) and now ended → if `ok == Some(false)`, `errors += 1`; outside history an
     inbound pulse at `now_ms` (`failed` likewise).
   - Record the id's current `end_ms`.
3. `running` / `running_since` are recomputed from this round's steps only (zero for stations with no
   running step in it). `now` = the running step with the greatest `time_ms` in this round (`None` if
   none).
4. Drop pulses with `at_ms < now_ms - 2 * PULSE_MS`; keep the rest in `at_ms` order.

Never panics, whatever the steps (empty, duplicated ids, ids going from ended back to running: treat the
latter as a plain update of the recorded `end_ms`, no pulse).

### `src/ui/screens/agents.rs`

The timeline glyph for `StepKind::User` is `❯ you` in `Role::Text` (padded to 8 like the others). Nothing else
changes on the screen.

## Pitfalls

- `steps_fixture` (T34) breaks on purpose: `messages.json` starts with a user message, so its first step is now
  `User` (`Execute tasks/T24-setup-wizard.md`, `time_ms` 1794500600000). Update it **mechanically**: length
  `11`, the closure becomes `|i: usize| &steps[i + 1]`, and add asserts on `steps[0]` (kind, summary,
  `ok == None`, `end_ms == Some(1794500600000)`). Change nothing else in it.
- `poller_focus` (T34) counts the same fixture's steps: `focused.steps.len()` becomes `11`.
- `steps_real_fixture` (T40) must still pass unchanged: `messages-real.json` has no user message.
- The Agents golden (`tests/snapshots/agents-140x40.txt`) gains the user row: regenerate it with
  `UPDATE_GOLDEN=1 cargo test --test agents_screen`, then read the diff — only that row and the rows it pushes
  may change.
- `command_station` gets the real wrapped commands: `cd /w && cargo fmt && cargo clippy --all-targets -- -D
  warnings; echo EXIT: $?` is `Checks` (clippy), not Shell.
- `HashMap` is in `std`; add no dependency.

## Tests

`tests/activity.rs`, exactly these names. Build `Step`s with a small helper in the test file
(`fn step(id, kind, summary, time_ms, end_ms, ok) -> Step`).

- `station_order`: `Station::ALL.map(Station::name)` is `["You", "Model", "Files", "Shell", "Checks", "Web",
  "Agents", "Tools"]`; `Station::Checks.index() == 4`.
- `command_stations`: each line gives its station:
  - `cargo test --test setup` → Checks; `cargo fmt` → Shell; `cargo fmt --check` → Checks;
  - `cd /w && cargo fmt && cargo clippy --all-targets -- -D warnings; echo EXIT: $?` → Checks;
  - `bash .devbox/check > /tmp/check4.out 2>&1; echo EXIT: $?` → Checks;
  - `FOO=1 timeout 30 cargo test` → Checks; `/usr/bin/cargo test` → Checks;
  - `cd /w && tail -8 /tmp/check4.out` → Files; `sed -n 1,40p src/a.rs | grep fn` → Files;
  - `git push origin main` → Web; `curl -s http://example.test` → Web; `git status --short` → Shell;
  - `python3 script.py` → Shell; `ls src && cargo build` → Shell; `cd /w` → Shell; `` (empty) → Shell.
- `step_stations`: `station` of a `User` step → You; `Think` → Model; `Edit` → Files;
  `Shell` with summary `cargo test` → Checks; `Other("webfetch")` → Web; `Other("task")` → Agents;
  `Other("compress")` → Tools; `Other("glob")` → Files.
- `real_ids_and_ends`: on `messages-real.json` (directory `/home/dev/work/.wt/llamatop-145`): the last step
  (the running think) has id `msg_1d5b4257d001SGHohrO5bXFuks#0` and `end_ms == None`; the edit step
  (`steps[8]`) has id `XpttFmriPBEmPIK9GrDtFqFUFJGA2O7X` and `end_ms == Some(1794586230621)`; the compress step
  (`steps[2]`) has id `G8VMZWonRg17YrfMtuJiimpVoT3ZfIY1` and `end_ms == Some(1794585602850)`.
- `cd_prefix`: on `messages-real.json`, `steps[13].summary` starts with `cargo fmt && cargo clippy` (the
  `cd /home/dev/work/.wt/llamatop-145 && ` prefix is gone).
- `user_step`: on `messages.json`, `steps[0]` is `User`, summary `Execute tasks/T24-setup-wizard.md`.
- `history_round`: `update("s1", <the 21 real steps>, 1794586720000)` → no pulses; `now` is
  `Some((Model, "", 1794586716911))`; calls per station: Model 14 (8 think + 6 say), Files 5 (2 reads, the
  edit, the write, the `tail -8` shell), Checks 1 (the clippy shell), Tools 1 (compress), the others 0;
  `stations[Model].running == 1`, every other `running == 0`; no errors.
- `live_round`: after the history round, `update("s1", [one new step id "x1", Shell, "cargo test", time
  1794586721000, end None, ok None], 1794586722000)` → one outbound pulse at Checks, `at_ms` 1794586722000,
  `stations[Checks].running == 1`, `running_since == Some(1794586721000)`; then `update` with the same id ended
  (`end Some(1794586723000)`, `ok Some(false)`) at 1794586724000 → an inbound pulse with `failed == true`,
  `stations[Checks].errors` up by one, `running == 0`, and the outbound pulse is gone (older than
  `2 * PULSE_MS`).
- `fast_step`: a new step already ended at its first round makes two pulses: outbound at `now_ms`, inbound at
  `now_ms + PULSE_MS`.
- `session_change_resets`: an update for `"s2"` after `"s1"` leaves only `"s2"`'s counts and no pulses.
- Plus the updated `steps_fixture` and the regenerated Agents golden.

## Acceptance criteria

- [ ] The contract above is implemented; `parse_steps` gives ids, end times, user steps and short shell
      summaries.
- [ ] Every earlier test passes (`steps_fixture` updated as described, Agents golden regenerated); the tests
      above pass.
- [ ] `bash .devbox/check` is green.
