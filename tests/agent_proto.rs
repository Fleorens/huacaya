//! The pure part of the agent protocol: request parsing, response formatting, the token check, the
//! log ring, and routing — exercised without a network.

use llamatop::agent::proto::{self, LOG_RING_CAPACITY, LogPage, LogRing, MAX_HEAD};
use llamatop::app::demo::{self, DemoState};
use llamatop::collect::logs::LogLine;
use llamatop::collect::sampler::SystemSnapshot;

#[test]
fn parse_get_with_query_and_token() {
    let req = proto::parse_request(
        "GET /v1/log?since=42&x=1 HTTP/1.1\r\nHost: a\r\nauthorization: Bearer  t0k \r\n",
    )
    .unwrap();
    assert_eq!(req.method, "GET");
    assert_eq!(req.path, "/v1/log");
    assert_eq!(req.query, [("since".to_string(), "42".to_string()), ("x".to_string(), "1".to_string())]);
    assert_eq!(req.bearer.as_deref(), Some("t0k"));
}

#[test]
fn parse_rejects() {
    assert!(proto::parse_request("").is_none());
    assert!(proto::parse_request("GET /v1/log").is_none());
    assert!(proto::parse_request("GET /v1/log FTP/1.0").is_none());
    assert!(proto::parse_request(&"a".repeat(MAX_HEAD + 1)).is_none());
    let req = proto::parse_request("GET / HTTP/1.0\n").unwrap();
    assert_eq!(req.method, "GET");
    assert_eq!(req.path, "/");
    assert!(req.query.is_empty());
    assert!(req.bearer.is_none());
}

#[test]
fn response_format() {
    let body = proto::response(200, "{\"a\":\"é\"}");
    assert!(body.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(body.contains("Content-Length: 10\r\n"));
    assert!(body.contains("Connection: close\r\n"));
    assert!(body.ends_with("\r\n\r\n{\"a\":\"é\"}"));
}

#[test]
fn tokens() {
    assert!(proto::token_matches("abc", Some("abc")));
    assert!(!proto::token_matches("abc", Some("abd")));
    assert!(!proto::token_matches("abc", Some("ab")));
    assert!(!proto::token_matches("abc", Some("abcd")));
    assert!(!proto::token_matches("abc", None));
}

#[test]
fn ring_pages() {
    let mut ring = LogRing::new();
    ring.push(proto_line("line 1"));
    ring.push(proto_line("line 2"));
    ring.push(proto_line("line 3"));
    let page = ring.since(0);
    assert_eq!(page.lines.len(), 3);
    assert_eq!(page.next, 4);
    assert_eq!(ring.since(3).lines.len(), 1);
    let page = ring.since(4);
    assert!(page.lines.is_empty());
    assert_eq!(page.next, 4);
    assert!(!page.dropped);
}

#[test]
fn ring_drops() {
    let mut ring = LogRing::new();
    for i in 0..(LOG_RING_CAPACITY as u64 + 5) {
        ring.push(proto_line(&format!("line {i}")));
    }
    let page = ring.since(0);
    assert_eq!(page.lines.len(), LOG_RING_CAPACITY);
    assert!(!page.dropped);
    let page = ring.since(1);
    assert!(page.dropped);
    assert_eq!(page.lines.len(), LOG_RING_CAPACITY);
    assert!(!ring.since(6).dropped);
}

#[test]
fn route_auth() {
    let system = demo::system(17.3, DemoState::Generating);
    let logs = LogRing::new();

    // health works without a token even when one is configured.
    let (status, body) = proto::route(
        &proto::parse_request("GET /v1/health HTTP/1.1").unwrap(),
        Some("s3cret"),
        Some(&system),
        &logs,
    );
    assert_eq!(status, 200);
    assert!(body.contains("\"version\""));

    // /v1/system without the bearer is 401; with it, the snapshot round-trips.
    let (status, _) = proto::route(
        &proto::parse_request("GET /v1/system HTTP/1.1").unwrap(),
        Some("s3cret"),
        Some(&system),
        &logs,
    );
    assert_eq!(status, 401);

    let req = proto::parse_request("GET /v1/system HTTP/1.1\r\nauthorization: Bearer s3cret\r\n").unwrap();
    let (status, body) = proto::route(&req, Some("s3cret"), Some(&system), &logs);
    assert_eq!(status, 200);
    let back: SystemSnapshot = serde_json::from_str(&body).unwrap();
    assert_eq!(back, system);

    // unknown path: 401 without the token, 404 with it.
    let req = proto::parse_request("GET /nope HTTP/1.1").unwrap();
    assert_eq!(proto::route(&req, Some("s3cret"), Some(&system), &logs).0, 401);
    let req = proto::parse_request("GET /nope HTTP/1.1\r\nauthorization: Bearer s3cret\r\n").unwrap();
    assert_eq!(proto::route(&req, Some("s3cret"), Some(&system), &logs).0, 404);

    // non-GET is 405.
    let req = proto::parse_request("POST /v1/system HTTP/1.1").unwrap();
    assert_eq!(proto::route(&req, Some("s3cret"), Some(&system), &logs).0, 405);

    // no token configured: /v1/system is reachable without a bearer.
    let req = proto::parse_request("GET /v1/system HTTP/1.1").unwrap();
    let (status, _) = proto::route(&req, None, Some(&system), &logs);
    assert_eq!(status, 200);

    // the token never leaks into a body.
    let req = proto::parse_request("GET /v1/system HTTP/1.1\r\nauthorization: Bearer s3cret\r\n").unwrap();
    let (_, body) = proto::route(&req, Some("s3cret"), Some(&system), &logs);
    assert!(!body.contains("s3cret"));
}

#[test]
fn route_system_and_log() {
    let mut logs = LogRing::new();
    for i in 0..3 {
        logs.push(proto_line(&format!("line {i}")));
    }

    // no snapshot yet.
    let req = proto::parse_request("GET /v1/system HTTP/1.1").unwrap();
    assert_eq!(proto::route(&req, None, None, &logs).0, 503);

    // a since that is not a number is a 400.
    let req = proto::parse_request("GET /v1/log?since=abc HTTP/1.1").unwrap();
    assert_eq!(proto::route(&req, None, None, &logs).0, 400);

    // since=2 on a ring of 3 lines: the last two, next 4.
    let req = proto::parse_request("GET /v1/log?since=2 HTTP/1.1").unwrap();
    let (status, body) = proto::route(&req, None, None, &logs);
    assert_eq!(status, 200);
    let page: LogPage = serde_json::from_str(&body).unwrap();
    assert_eq!(page.lines.len(), 2);
    assert_eq!(page.next, 4);
}

/// One log line for the ring tests, built the same way the agent's clients build theirs.
fn proto_line(text: &str) -> LogLine {
    llamatop::collect::logs::parse_line(text)
}
