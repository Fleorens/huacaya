# T27 — `llamatop agent`: the server

| | |
|---|---|
| **Batch** | 5 — the agent: the server's machine panels on any machine |
| **Depends on** | T26 |
| **Read first** | `src/agent/proto.rs`; `grep -n "pub " src/collect/sampler.rs src/collect/logs.rs src/config.rs src/settings.rs`; `src/main.rs` |
| **Deliverable** | `src/agent/server.rs`, `tests/agent_server.rs`; `pub mod server;` in `src/agent/mod.rs`; an `agent` subcommand in `src/config.rs`; its branch in `src/main.rs`; README section |

## Why

T26 wrote what the agent answers; this sheet makes it a program. `llamatop agent`, run on the server (by hand
or by a systemd unit), samples the machine every interval and follows the log, exactly as the dashboard does
locally, and answers T26's three endpoints over HTTP. No async runtime: one thread per job, like the rest of
llamatop.

## Contract

### `src/agent/server.rs`

```rust
/// What a running agent needs.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub listen: SocketAddr,
    /// `None` only when the user explicitly allowed it (see the subcommand).
    pub token: Option<String>,
    pub sysroot: PathBuf,
    pub mounts: Vec<PathBuf>,
    pub log_cmd: String,
    pub interval: Duration,
}

/// Binds `config.listen`, starts the threads, and returns the bound address (useful with port 0) and the
/// handle of the accept thread. Everything stops within ~200 ms once `stop` is set; the handle then ends.
pub fn start(config: AgentConfig, stop: Arc<AtomicBool>) -> anyhow::Result<(SocketAddr, JoinHandle<()>)>;

/// `start`, then prints `llamatop agent listening on <addr>` and waits for the accept thread (forever, until the
/// process is stopped).
pub fn run(config: AgentConfig) -> anyhow::Result<()>;
```

Threads started by `start`:

- **sampler**: `Sampler::new(sysroot, mounts)`, one `sample` per `interval` (sleeping in slices of at most
  100 ms, stopping on `stop`), each result stored in an `Arc<Mutex<Option<SystemSnapshot>>>`;
- **log**: `logs::follow(&log_cmd, …)` pushing every line into an `Arc<Mutex<LogRing>>`. When `follow` fails,
  push one `LogLine` with level `Warn` and text `log command failed: <error>`. Keep the `Follower` alive until
  `stop` (it kills the command when dropped);
- **accept**: the listener in non-blocking mode; on `WouldBlock` sleep 50 ms and check `stop`. Each accepted
  connection gets its own thread: 2 s read and write timeouts, read the head until `\r\n\r\n` (or `\n\n`) or
  `MAX_HEAD` bytes, `proto::parse_request` (`None` → `400`, `{"error":"bad request"}`), `proto::route` with the
  current snapshot and ring, write `proto::response`, close. Never hold a lock while writing to the socket:
  build the body, release, then write.

### `src/config.rs` — the subcommand

```rust
/// Publish this machine's panels (system and log) for remote llamatop dashboards.
Agent {
    /// Address and port to listen on.
    #[arg(long, default_value = "0.0.0.0:9099")]
    listen: SocketAddr,
    /// File holding the token clients must send (first line).
    #[arg(long, env = "LLAMATOP_AGENT_TOKEN_FILE")]
    token_file: Option<PathBuf>,
    /// Accept clients without a token (only sensible on a trusted network).
    #[arg(long)]
    no_auth: bool,
}
```

### `src/main.rs`

`Some(Command::Agent { … })` → build `AgentConfig` from it and the global flags (`--sysroot`, `--mount`,
`--log-cmd`, `--interval-ms`, after the configuration file is applied), then `server::run`. The token: the first
line of `token_file`, trimmed; an unreadable or empty file is an error. Without a token: allowed when
`no_auth` is set or the listen address is loopback; otherwise exit with the error
`a token is required to listen on <addr>: --token-file <file>, or --no-auth`.

### README

A section "Agent": `llamatop agent --token-file /etc/llamatop/agent-token` on the server, the three endpoints,
and that the token is required as soon as the agent listens beyond loopback.

## Pitfalls

- The integration test binds `127.0.0.1:0` and reads the real port from `start`'s result; it must not use 9099
  (another agent may run on the devbox one day).
- Use `ureq` (already a dependency) in the test as the HTTP client, with `http_status_as_error(false)` to read
  401 / 503 answers; or write the request by hand on a `TcpStream` — both are fine.
- The first sample arrives after one interval at most: in the test, poll `/v1/system` until 200, at most 3 s.
- A slow or silent client must not block others: the per-connection thread and the 2 s timeouts handle it.
- Clap: global flags go before the subcommand (`llamatop --interval-ms 500 agent --listen …`).

## Tests

`tests/agent_server.rs`, exactly these names. Agent config: `sysroot` = `tests/fixtures/sysroot`, mounts `/`,
`log_cmd` = `cat <manifest dir>/tests/fixtures/journal/llama-server.txt`, interval 200 ms, token `Some("t")`,
listen `127.0.0.1:0`.

- `serves_system_and_log`: `/v1/system` with bearer `t` → 200 within 3 s, parses to a `SystemSnapshot` with
  hostname `aibox-flo`; `/v1/log?since=0` → a `LogPage` of 36 lines (35 fixture lines and the
  `log source ended: cat` line) with `next` 37; `/v1/log?since=37` → no line.
- `requires_the_token`: `/v1/system` without bearer → 401; with bearer `x` → 401; `/v1/health` without → 200.
- `bad_request`: a raw `TcpStream` writing `"HELLO\r\n\r\n"` reads back a response starting with
  `HTTP/1.1 400 Bad Request`.
- `stops`: set `stop` → the accept thread's handle joins within 1 s, and a new connection is then refused or
  gets no answer.
- `no_token_beyond_loopback_is_refused`: the main-side check, as a pure function you write in
  `src/agent/server.rs`: `pub fn check_token_policy(listen: &SocketAddr, token: Option<&str>, no_auth: bool)
  -> anyhow::Result<()>` — `0.0.0.0:9099` without token or `no_auth` → `Err` naming the address;
  `127.0.0.1:9099` without token → `Ok`; `0.0.0.0:9099` with `no_auth` → `Ok`; with a token → `Ok`.

## Acceptance criteria

- [x] The public API above exists; `llamatop agent` runs; the token policy is enforced.
- [x] The 5 tests above exist under these names and pass.
- [x] README has the "Agent" section.
- [x] `bash .devbox/check` is green.
