# T28 — Dashboard side: read the server's panels from its agent

| | |
|---|---|
| **Batch** | 5 — the agent: the server's machine panels on any machine |
| **Depends on** | T27 |
| **Read first** | `src/agent/proto.rs`; `src/app/run.rs`; `src/dump.rs`; `src/settings.rs`; `src/setup.rs`; `grep -n "pub " src/config.rs src/http.rs` |
| **Deliverable** | `src/agent/client.rs`, `tests/agent_client.rs`; `pub mod client;` in `src/agent/mod.rs`; changes to `src/config.rs`, `src/settings.rs`, `src/app/run.rs`, `src/dump.rs`, `src/setup.rs`; README |

## Why

With an agent on the server (T27), the dashboard on another machine can show the server's GPU, CPU, memory,
disks, processes and log instead of its own. The llama.cpp panels keep talking to the server's HTTP API as
today. When no agent is configured, nothing changes.

## Contract

### `src/agent/client.rs`

```rust
/// Talks to one agent; the token goes in `Authorization: Bearer` (`http::Client` already does that).
pub struct AgentClient { /* private: an http::Client */ }

impl AgentClient {
    /// `url` like `http://192.168.1.203:9099`; 3 s timeout.
    pub fn new(url: &str, token: Option<String>) -> Self;
    /// `GET /v1/system`, parsed.
    pub fn system(&self) -> anyhow::Result<SystemSnapshot>;
    /// `GET /v1/log?since=<since>`, parsed.
    pub fn log(&self, since: u64) -> anyhow::Result<LogPage>;
    /// `GET /v1/health`: the agent's version.
    pub fn version(&self) -> anyhow::Result<String>;
}
```

### Configuration

- `src/config.rs`: `#[arg(long, env = "LLAMATOP_AGENT")] pub agent: Option<String>` ("URL of a llamatop agent
  on the server: its machine panels and log replace this machine's"), `#[arg(long, env =
  "LLAMATOP_AGENT_TOKEN_FILE")] pub agent_token_file: Option<PathBuf>`, `#[arg(skip)] pub
  agent_token_inline: Option<String>`, and `pub fn agent_token(&self) -> Option<String>` (file first line, else
  inline; trimmed; empty is `None`). The agent subcommand of T27 keeps its own `--token-file`.
- `src/settings.rs`: `Settings` gains `pub agent: AgentSettings` (`[agent]` table, same attributes as
  `LlamaSettings`) with `url: Option<String>`, `token: Option<String>`, `token_file: Option<PathBuf>`;
  `apply_settings` fills `agent`, `agent_token_file`, `agent_token_inline` when not given by flag / environment.

### `src/app/run.rs`

When `cli.agent` is `Some(url)` (and not `--demo`): instead of the sampler thread and the log follower, **one
agent thread**: every interval (the shared one, so `+` / `-` apply), `system()` → `Event::System`, then
`log(since)` → one `Event::Log` per line, `since = page.next`. The first call uses `since = 0`, so the history
the agent holds arrives at once. On an error, send **one** `Event::Log` with level `Warn` and text
`agent unreachable: <error>` when the agent goes from reachable (or unknown) to unreachable, and one with level
`Info` and `agent back` when it answers again; nothing in between. A page with `dropped` adds one `Warn` line
`agent log: some lines were missed`.

The poller is unchanged (direct-instance mode needs a loopback URL, which a remote dashboard does not have).

### `src/dump.rs`

With `cli.agent`: `system` comes from `AgentClient::system()` (an error is the dump's error, with context
`agent <url>`), and `log_tail` from `log(0)`, its last 20 lines; `log_error` is `None`.

### `src/setup.rs`

After the log command question: `llamatop agent URL (empty: none) [<current or empty>]: `. Empty → no agent
(and the log command question keeps its meaning). Otherwise `normalize_url`, then
`secret("agent token (empty: <keep the current one | none>): ")`, then `checking agent … ` and
`ok, <hostname from /v1/system>\n`, or `HTTP 401` → `the agent refused the token\n` and the token is asked again
(three refusals → `Err("setup aborted")`), or another error → `unreachable: <error>\n` and the dialogue goes on.
The returned `Settings.agent` holds the answers. The `Probe` trait gains
`fn agent(&self, url: &str, token: Option<&str>) -> anyhow::Result<String>` (the hostname); the existing tests'
fake probe implements it (adapt them: they answer `\n` to this new question by adding one more `\n` to their
input).

### README

"Agent" section: on the dashboard's machine, `llamatop setup` (agent question) or `--agent` / `[agent]`.

## Pitfalls

- `http::Client::get` returns the body of a 2xx answer and an error carrying `HTTP <status>` otherwise: reuse it,
  do not write another HTTP client.
- The agent thread must not flood the log with one error line per second: keep a `reachable: Option<bool>`.
- The header's hostname comes from the snapshot, so it shows the server's name with no extra work.
- Tests start a real agent with `agent::server::start` on `127.0.0.1:0` (fixture sysroot, `cat` log command,
  token `t`), as in `tests/agent_server.rs`.

## Tests

`tests/agent_client.rs`, exactly these names.

- `client_reads_system_and_log`: against a started agent, `system()` gives hostname `aibox-flo` (retry up to
  3 s for the first sample); `log(0)` gives 36 lines; `version()` is `env!("CARGO_PKG_VERSION")`.
- `client_wrong_token`: `AgentClient::new(url, Some("x")).system()` → an error containing `401`.
- `agent_token_order`: a `Cli` with `agent_token_file` pointing to a temp file `  tok \n` → `agent_token()` is
  `Some("tok")`; with no file and `agent_token_inline = Some("inline")` → `Some("inline")`.
- `settings_agent_round_trip`: `Settings` with an `[agent]` table saved then loaded → equal; `apply_settings`
  from matches of `["llamatop"]` fills `cli.agent`.
- `setup_asks_for_the_agent`: the wizard with input `"\n\nhttp://a:9099\n"`, secrets `""` (llama key) then
  `"tok-SECRET"` (agent token), a fake probe whose `agent` answers `Ok("aibox-flo")` → `Settings.agent.url`
  `Some("http://a:9099")`, token `Some("tok-SECRET")`; output contains `ok, aibox-flo` and not `SECRET`.

## Acceptance criteria

- [x] The public API above exists; with an agent, the dashboard and `--dump` show the agent's machine.
- [x] Without an agent, nothing changes: every earlier test passes (the setup tests with one more `\n`).
- [x] The 5 tests above exist under these names and pass.
- [x] `bash .devbox/check` is green.
- [ ] (Orchestrator, devbox → aibox) `llamatop --agent http://192.168.1.203:9099 …` on the devbox shows the
      aibox's GPU.
