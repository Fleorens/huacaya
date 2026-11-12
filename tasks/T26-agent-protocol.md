# T26 — Agent protocol (pure)

| | |
|---|---|
| **Batch** | 5 — the agent: the server's machine panels on any machine |
| **Depends on** | T24 |
| **Read first** | `grep -n "pub " src/collect/sampler.rs src/collect/logs.rs`; `tests/round_trip.rs` |
| **Deliverable** | `src/agent/mod.rs`, `src/agent/proto.rs`, `tests/agent_proto.rs`; `pub mod agent;` in `src/lib.rs` |

## Why

llamatop now runs from any Linux machine, but its GPU, CPU, memory and log panels describe the machine it
runs on. A small agent on the server (`llamatop agent`, T27) will publish the server's `SystemSnapshot` and log
lines over HTTP, and the dashboard (T28) will read them instead of the local machine. This sheet writes the
pure part: parsing a request, routing it, the log buffer the clients page through, and the token check. No
socket here, no thread: everything is a function of strings and values, tested without a network.

`SystemSnapshot` and `LogLine` already derive `serde::Deserialize` (see `tests/round_trip.rs`).

## Contract

`src/agent/mod.rs`: a one-line module doc and `pub mod proto;`.

### `src/agent/proto.rs`

```rust
/// Port the agent listens on by default.
pub const DEFAULT_PORT: u16 = 9099;
/// Log lines the agent keeps for its clients.
pub const LOG_RING_CAPACITY: usize = 2000;
/// Largest request head the agent reads (request line + headers), in bytes.
pub const MAX_HEAD: usize = 8192;

/// The parts of an HTTP/1.1 request head the agent uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    /// The path without the query string, e.g. `/v1/log`.
    pub path: String,
    /// `name=value` pairs of the query string, in order, not percent-decoded (the agent's own parameters
    /// are plain numbers).
    pub query: Vec<(String, String)>,
    /// The token of an `Authorization: Bearer <token>` header (header name case-insensitive), trimmed.
    pub bearer: Option<String>,
}

/// Parses a request head (everything before the empty line; `\r\n` or `\n` line ends). `None` when the
/// request line is not `<METHOD> <target> HTTP/1.x` or the head is longer than `MAX_HEAD`.
pub fn parse_request(head: &str) -> Option<Request>;

/// A whole HTTP/1.1 response: status line with its reason phrase (200 OK, 400 Bad Request,
/// 401 Unauthorized, 404 Not Found, 405 Method Not Allowed, 503 Service Unavailable), then
/// `Content-Type: application/json`, `Content-Length: <bytes of body>`, `Connection: close`, an empty line,
/// the body. Line ends `\r\n`.
pub fn response(status: u16, body: &str) -> String;

/// `true` when `given` equals `expected`, compared in constant time over the bytes (no early exit on the
/// first difference; different lengths are `false`). `given` `None` is `false`.
pub fn token_matches(expected: &str, given: Option<&str>) -> bool;

/// One page of log lines for a client.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LogPage {
    /// The `since` to ask next time: the sequence number after the last line of the ring.
    pub next: u64,
    /// `true` when lines the client asked for had already left the ring.
    pub dropped: bool,
    pub lines: Vec<LogLine>,
}

/// The last `LOG_RING_CAPACITY` log lines, each with a sequence number: 1 for the first line ever pushed,
/// then +1.
#[derive(Debug, Clone, Default)]
pub struct LogRing { /* private */ }

impl LogRing {
    pub fn new() -> Self;
    /// Appends a line, dropping the oldest beyond `LOG_RING_CAPACITY`.
    pub fn push(&mut self, line: LogLine);
    /// The lines whose sequence number is `>= since`, oldest first; `since = 0` means everything kept.
    /// `next` is the last sequence number + 1 (1 when empty). `dropped` is `true` when `since > 0` and
    /// `since` is below the oldest sequence number kept.
    pub fn since(&self, since: u64) -> LogPage;
}

/// The agent's answer to one request, as `(status, JSON body)`:
/// - a method other than `GET` → `405`, `{"error":"method not allowed"}`;
/// - `GET /v1/health` → `200`, `{"version":"<CARGO_PKG_VERSION>"}`, **without** token;
/// - any other path when `token` is `Some(t)` and the request's bearer does not match → `401`,
///   `{"error":"unauthorized"}` (checked before the path is looked at, so unknown paths are 401 too);
/// - `GET /v1/system` → `200` and the snapshot as JSON, or `503`, `{"error":"no sample yet"}` when `None`;
/// - `GET /v1/log` with `since=<u64>` (missing → 0) → `200` and `logs.since(since)` as JSON; a `since` that is
///   not a number → `400`, `{"error":"bad since"}`;
/// - anything else → `404`, `{"error":"not found"}`.
pub fn route(req: &Request, token: Option<&str>, system: Option<&SystemSnapshot>, logs: &LogRing) -> (u16, String);
```

## Pitfalls

- No new dependency: the request head is a few lines of text; split on `\n`, trim a trailing `\r`.
- `Content-Length` counts **bytes** (`body.len()`), not characters: log lines may hold `…` or `°`.
- Constant-time compare: fold every byte pair with `|=` of their XOR, compare lengths without returning early
  on the bytes. It matters little on a LAN, but it is the habit for a token.
- `route` never puts the token, nor the request's `Authorization` value, in a body.
- Build test lines with `collect::logs::parse_line("line N")` and test snapshots with
  `app::demo::system(…)`.

## Tests

`tests/agent_proto.rs`, exactly these names.

- `parse_get_with_query_and_token`: `"GET /v1/log?since=42&x=1 HTTP/1.1\r\nHost: a\r\nauthorization: Bearer  t0k \r\n"`
  → method `GET`, path `/v1/log`, query `[("since","42"),("x","1")]`, bearer `Some("t0k")`.
- `parse_rejects`: `""`, `"GET /v1/log"`, `"GET /v1/log FTP/1.0"`, a head of `MAX_HEAD + 1` bytes → `None`;
  `"GET / HTTP/1.0\n"` → `Some` (path `/`, no query, no bearer).
- `response_format`: `response(200, "{\"a\":\"é\"}")` starts with `HTTP/1.1 200 OK\r\n`, contains
  `Content-Length: 10\r\n` (é is two bytes) and `Connection: close\r\n`, and ends with `\r\n\r\n{"a":"é"}`.
- `tokens`: `token_matches("abc", Some("abc"))` true; `Some("abd")`, `Some("ab")`, `Some("abcd")`, `None` false.
- `ring_pages`: push 3 lines → `since(0)` has 3 lines and `next` 4; `since(3)` has the third only; `since(4)`
  none, `next` 4, `dropped` false.
- `ring_drops`: push `LOG_RING_CAPACITY + 5` lines → `since(0).lines.len()` is the capacity, `dropped` false;
  `since(1)` → `dropped` true and the capacity's worth of lines; `since(6)` → `dropped` false.
- `route_auth`: with `token = Some("s3cret")`: `/v1/health` without bearer → 200 and the body contains
  `"version"`; `/v1/system` without bearer → 401; with bearer `s3cret` and a demo snapshot → 200 and the body
  parses back (`serde_json::from_str::<SystemSnapshot>`) equal to it; `/nope` without bearer → 401, with it → 404;
  `POST /v1/system` → 405. With `token = None`, `/v1/system` without bearer → 200. No body contains `s3cret`.
- `route_system_and_log`: no snapshot → 503; `/v1/log?since=abc` → 400; `/v1/log?since=2` on a ring of 3 lines →
  200 and the body parses to a `LogPage` of 2 lines with `next` 4.

## Acceptance criteria

- [x] The files exist with the exact public API above; no new dependency.
- [x] The 8 tests above exist under these names and pass.
- [x] `bash .devbox/check` is green.
