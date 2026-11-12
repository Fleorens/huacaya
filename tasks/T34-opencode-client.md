# T34 — OpenCode client: sessions and steps, `[opencode]` settings, setup question

| | |
|---|---|
| **Batch** | 7 — the TUI v2 (mockups validated by Florian on 2026-11-12) |
| **Depends on** | T33 |
| **Read first** | `src/http.rs`; `src/collect/llama_poll.rs` (`Fetch`); `src/settings.rs`; `src/config.rs` (`apply_settings`); `src/setup.rs` (`Probe`, the agent question); `src/app/run.rs` (the agent thread); `tests/fixtures/opencode/*.json` |
| **Deliverable** | `src/collect/opencode.rs`, `tests/opencode.rs`; `pub mod opencode;` in `src/collect/mod.rs`; changes to `src/http.rs`, `src/settings.rs`, `src/config.rs`, `src/setup.rs`, `src/app/state.rs`, `src/app/run.rs`; README |

## Why

The Agents screen (T35) shows what OpenCode is doing: its sessions (one per `dev` job, titled `#<job> …`) and,
for the session followed, every step — files read, edited, written, commands run and their results, the
model's reasoning. OpenCode's server (`opencode serve`) has an HTTP API protected by HTTP Basic auth. This sheet
reads it, configures it, and delivers it to the `App`; it draws nothing.

The API, as measured on the devbox (OpenCode 2.0.22): `GET /api/session` → `{"data": [session…], "cursor":
…}`, newest first; `GET /api/session/<id>/message` → `{"data": [message…], "cursor": …}`, newest first, 50 per
page. The fixtures `tests/fixtures/opencode/sessions.json` and `messages.json` have exactly that shape (their
content is invented; do not edit them).

## Contract

### `src/http.rs`

```rust
impl Client {
    /// Like `new`, but authenticates with HTTP Basic (`Authorization: Basic base64(user:password)`).
    pub fn new_basic(base_url: &str, user: &str, password: &str, timeout: Duration) -> Self;
}

/// Standard base64 (RFC 4648 alphabet, `=` padding).
pub fn base64(bytes: &[u8]) -> String;
```

The client stores the whole header value (`Bearer …` or `Basic …`) instead of the key; errors still never
contain it.

### `src/collect/opencode.rs`

```rust
/// One OpenCode session.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    /// `job_number(title)`.
    pub job: Option<u32>,
    /// `model.id`.
    pub model: Option<String>,
    pub agent: Option<String>,
    /// `outcome` (`succeeded`, `failed`, …); `None` while it runs.
    pub outcome: Option<String>,
    /// No `outcome` and no `time.idle`.
    pub running: bool,
    pub created_ms: i64,
    pub idle_ms: Option<i64>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    /// `location.directory`, used to shorten paths.
    pub directory: Option<String>,
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StepKind { Read, Edit, Write, Shell, Search, Think, Say, Other(String) }

/// One thing the model did, in a session's timeline.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    /// The tool part's `time.created`, else its message's `time.created`.
    pub time_ms: i64,
    pub kind: StepKind,
    pub summary: String,
    /// `Some(false)` for a tool whose `state.status` is `error`, or a shell whose `metadata.exit` is not 0;
    /// `Some(true)` for other completed tools; `None` for reasoning and text.
    pub ok: Option<bool>,
    pub detail: Option<String>,
}

/// What the Agents screen gets every round.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenCodeSnapshot {
    pub sessions: Vec<Session>,
    /// The session whose `steps` these are.
    pub focus: Option<String>,
    pub steps: Vec<Step>,
    /// The last error (`unreachable: …`, `HTTP 401` …), never containing the password.
    pub error: Option<String>,
}

/// `#136 Execute …` → `Some(136)`: the digits right after a leading `#`.
pub fn job_number(title: &str) -> Option<u32>;
/// Parses `GET /api/session`, keeping the order.
pub fn parse_sessions(json: &str) -> anyhow::Result<Vec<Session>>;
/// Parses `GET /api/session/<id>/message` into steps, oldest first (the API is newest first). Only `assistant`
/// messages give steps, their parts in order; `directory` shortens paths.
pub fn parse_steps(json: &str, directory: Option<&str>) -> anyhow::Result<Vec<Step>>;

/// Polls OpenCode through any `llama_poll::Fetch`.
pub struct OpenCodePoller<F: Fetch> { /* private */ }
impl<F: Fetch> OpenCodePoller<F> {
    pub fn new(fetch: F) -> Self;
    /// One round: the sessions, then the steps of `focus` when it is one of them, else of the newest running
    /// session, else of the newest session. An error gives a snapshot with `error` set and empty lists.
    pub fn poll(&mut self, focus: Option<&str>) -> OpenCodeSnapshot;
}
```

Steps, part by part (paths: `directory` + `/` removed from the front when it matches):

| Part | `kind` | `summary` | `detail` |
|---|---|---|---|
| `reasoning` | `Think` | its text, whitespace collapsed, at most 200 chars | — |
| `text` | `Say` | idem | — |
| tool `read` | `Read` | the path | on error, the first line of its content |
| tool `edit` | `Edit` | the path | `+<added> −<removed>` counted in `metadata.files[*].patch` (lines starting with `+` / `-`, not `+++` / `---`) |
| tool `write` | `Write` | the path | `<n> lines` of `input.content` |
| tool `shell` | `Shell` | the command's first line | the first line of its content |
| tool `grep` | `Search` | `<pattern> in <path>` | — |
| other tool | `Other(name)` | the tool's name | — |

### Settings, flags, setup

- `src/settings.rs`: `Settings` gains `pub opencode: OpenCodeSettings` (`[opencode]`, same attributes as the
  other tables): `url`, `user`, `password`, `password_file`.
- `src/config.rs`: `--opencode <URL>` (`LLAMATOP_OPENCODE`), `--opencode-user <USER>`,
  `--opencode-password-file <FILE>`, `#[arg(skip)] opencode_password_inline`, and
  `pub fn opencode_password(&self) -> Option<String>` (file's first line, else inline; trimmed; empty is
  `None`); `apply_settings` fills them like the agent's.
- `src/setup.rs`: after the agent question, `OpenCode server URL (empty: keep, -: none) [<current>]: `; then
  `OpenCode user [<current or opencode>]: `; then `secret("OpenCode password (empty: keep the current one |
  none): ")`; then `checking OpenCode … ` → `ok, <n> sessions` / `the server refused the password` (asked
  again, three refusals abort) / `unreachable: <error>` (goes on). `Probe` gains
  `fn opencode(&self, url: &str, user: &str, password: Option<&str>) -> anyhow::Result<usize>`. Existing setup
  tests get one more `\n` for the new question.

### App and run loop

- `src/app/state.rs`: `Event::OpenCode(OpenCodeSnapshot)`; `App` gains `pub opencode: Option<OpenCodeSnapshot>`
  (stored on that event; `apply` returns `!paused`) and `pub agent_focus: Option<String>` (`None` for now; T35
  sets it).
- `src/app/run.rs`: when `cli.opencode` is set (and not `--demo`), a thread polls every 2 s
  (`OpenCodePoller` over `Client::new_basic(url, user or "opencode", password, 3 s)`), with the focus read from
  an `Arc<Mutex<Option<String>>>` that the main loop updates from `app.agent_focus` after each event batch.

## Pitfalls

- Never put the password, the `Authorization` value or the base64 of it in an error, a log line or a test
  assertion message.
- The messages endpoint returns 50 messages per page: that is enough for the timeline; do not page.
- `Fetch::get` returns the body of a 2xx answer and an error with `HTTP <code>` otherwise (from `http::Client`).
- No new dependency (base64 is a dozen lines).

## Tests

`tests/opencode.rs`, exactly these names.

- `base64_vectors`: `""`, `"f"` → `Zg==`, `"fo"` → `Zm8=`, `"foo"` → `Zm9v`, `"foobar"` → `Zm9vYmFy`,
  `"user:pass"` → `dXNlcjpwYXNz`.
- `job_numbers`: `#136 Execute tasks/T30-nvidia.md` → 136; `Refactor the parser` → `None`; `#x` → `None`.
- `sessions_fixture`: 3 sessions; the first running, job 136, tokens 49133 / 35718; the second `succeeded`,
  not running, job 131; the third `failed`, job `None`.
- `steps_fixture`: `parse_steps(messages.json, Some("/work/.wt/demo-131"))` → 10 steps, in this order: Think
  `Read the sheet first, then the cited sections.`; Read `tasks/T24-setup-wizard.md` ok; Search
  `pub fn in src`; Read `src/missing.rs` `ok: Some(false)` detail `File not found`; Write `tests/setup.rs` detail
  `3 lines`; Edit `src/setup.rs` detail `+3 −1`; Shell `cargo test --test setup` `ok: Some(false)` detail
  `test result: FAILED. 8 passed; 1 failed`; Shell `bash .devbox/check` ok detail `test result: ok. 9 passed`;
  Think `All tests pass…`; Say `Rapport : …`.
- `poller_focus`: a fake `Fetch` serving the two fixtures (any session id gets `messages.json`) → with no focus,
  `focus` is the running session's id; with focus `ses_done000000000000000000`, that one.
- `poller_error_hides_the_password`: a `Client::new_basic("http://127.0.0.1:9", "u", "pw-SECRET", 1 s)` (nothing
  listens on port 9) → `error` is `Some`, contains `unreachable` or a connection error, not `SECRET`.
- `settings_and_password`: an `[opencode]` table round-trips through `save` / `load`; `opencode_password()`
  reads a temp file `  pw \n` as `pw`.
- `setup_asks_for_opencode`: the wizard with the extra answers → `Settings.opencode` filled; output contains
  `ok, 3 sessions` and not the password.

## Acceptance criteria

- [ ] The public API above exists; with `--opencode`, the `App` receives `Event::OpenCode` every 2 s.
- [ ] Every earlier test passes (setup tests with their extra `\n`); the 8 tests above pass.
- [ ] README: the `[opencode]` table and the `--opencode*` flags.
- [ ] `bash .devbox/check` is green.
