//! The pure part of the agent protocol: parse a request head, build a response, check the token,
//! page the log ring, and route a request to `(status, JSON body)`. No I/O here.

use std::collections::VecDeque;

use crate::collect::logs::LogLine;
use crate::collect::sampler::SystemSnapshot;

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
pub fn parse_request(head: &str) -> Option<Request> {
    if head.len() > MAX_HEAD {
        return None;
    }
    let mut lines = head.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    let request_line = lines.next()?;
    if request_line.is_empty() {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let proto = parts.next()?;
    if proto != "HTTP/1.0" && proto != "HTTP/1.1" {
        return None;
    }
    if parts.next().is_some() {
        return None;
    }
    let (path, query) = match target.split_once('?') {
        Some((path, q)) => (path.to_string(), parse_query(q)),
        None => (target.to_string(), Vec::new()),
    };
    let mut bearer = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("authorization")
        {
            bearer = bearer_from(value);
        }
    }
    Some(Request { method, path, query, bearer })
}

/// Splits a query string into `name=value` pairs in order, not percent-decoded.
fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (pair.to_string(), String::new()),
        })
        .collect()
}

/// Extracts the token from an `Authorization: Bearer <token>` header value (scheme case-insensitive).
fn bearer_from(value: &str) -> Option<String> {
    // `get` rather than slicing: the header comes from the network and may cut a UTF-8 character at byte 6.
    let value = value.trim();
    let scheme = value.get(..6)?;
    let rest = value.get(6..)?;
    // `Bearer` must be followed by whitespace: `Bearerabc` is not the token `abc`.
    if !scheme.eq_ignore_ascii_case("bearer") || !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
        return None;
    }
    let token = rest.trim();
    if token.is_empty() { None } else { Some(token.to_string()) }
}

/// A whole HTTP/1.1 response: status line with its reason phrase, then `Content-Type`, `Content-Length`
/// (bytes), `Connection: close`, an empty line, and the body. Line ends `\r\n`.
pub fn response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        len = body.len()
    )
}

/// `true` when `given` equals `expected`, compared in constant time over the bytes (no early exit on the
/// first difference; different lengths are `false`). `given` `None` is `false`.
pub fn token_matches(expected: &str, given: Option<&str>) -> bool {
    let Some(given) = given else {
        return false;
    };
    let a = expected.as_bytes();
    let b = given.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

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
pub struct LogRing {
    lines: VecDeque<(u64, LogLine)>,
    next_seq: u64,
}

impl LogRing {
    pub fn new() -> Self {
        Self { lines: VecDeque::new(), next_seq: 1 }
    }
    /// Appends a line, dropping the oldest beyond `LOG_RING_CAPACITY`.
    pub fn push(&mut self, line: LogLine) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.lines.push_back((seq, line));
        while self.lines.len() > LOG_RING_CAPACITY {
            self.lines.pop_front();
        }
    }
    /// The lines whose sequence number is `>= since`, oldest first; `since = 0` means everything kept.
    /// `next` is the last sequence number + 1 (1 when empty). `dropped` is `true` when `since > 0` and
    /// `since` is below the oldest sequence number kept.
    pub fn since(&self, since: u64) -> LogPage {
        let lines: Vec<LogLine> =
            self.lines.iter().filter(|(seq, _)| *seq >= since).map(|(_, line)| line.clone()).collect();
        let oldest = self.lines.front().map(|(seq, _)| *seq).unwrap_or(self.next_seq);
        let dropped = since > 0 && since < oldest;
        LogPage { next: self.next_seq, dropped, lines }
    }
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
pub fn route(
    req: &Request,
    token: Option<&str>,
    system: Option<&SystemSnapshot>,
    logs: &LogRing,
) -> (u16, String) {
    if req.method != "GET" {
        return (405, error_body("method not allowed"));
    }
    if req.path == "/v1/health" {
        return (200, format!("{{\"version\":\"{}\"}}", env!("CARGO_PKG_VERSION")));
    }
    if let Some(expected) = token
        && !token_matches(expected, req.bearer.as_deref())
    {
        return (401, error_body("unauthorized"));
    }
    match req.path.as_str() {
        "/v1/system" => match system {
            Some(system) => (200, to_json(system)),
            None => (503, error_body("no sample yet")),
        },
        "/v1/log" => match query_u64(req, "since") {
            Ok(since) => (200, to_json(&logs.since(since))),
            Err(()) => (400, error_body("bad since")),
        },
        _ => (404, error_body("not found")),
    }
}

/// `{"error":"<msg>"}` — never carries the token.
fn error_body(msg: &str) -> String {
    format!("{{\"error\":\"{}\"}}", msg)
}

/// Reads a `since` query parameter: missing → 0, present but not a number → `Err`.
fn query_u64(req: &Request, key: &str) -> Result<u64, ()> {
    match req.query.iter().find(|(k, _)| k == key) {
        None => Ok(0),
        Some((_, v)) => v.parse::<u64>().map_err(|_| ()),
    }
}

/// Serializes to JSON; a value of this shape never fails to serialize, so a failure yields an empty body.
fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
