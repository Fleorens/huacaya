//! The `llamatop agent` server over HTTP: it samples the fixture machine, follows the fixture log, and
//! answers T26's three endpoints. The test binds `127.0.0.1:0` and reads the real port from `start`.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use llamatop::agent::proto::LogPage;
use llamatop::agent::server::{self, AgentConfig};
use llamatop::collect::sampler::SystemSnapshot;

/// The agent config pointed at the fixture machine and log, listening on an OS-assigned port.
fn config() -> AgentConfig {
    AgentConfig {
        listen: "127.0.0.1:0".parse().expect("a loopback address with an OS-assigned port"),
        token: Some("t".to_string()),
        sysroot: PathBuf::from(format!("{}/tests/fixtures/sysroot", env!("CARGO_MANIFEST_DIR"))),
        mounts: vec![PathBuf::from("/")],
        log_cmd: format!("cat {}/tests/fixtures/journal/llama-server.txt", env!("CARGO_MANIFEST_DIR")),
        interval: Duration::from_millis(200),
    }
}

/// A blocking HTTP client that does not turn 4xx/5xx into errors, so the 401 / 503 answers are readable.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into()
}

/// GET `path` on the agent, with an optional bearer token, returning `(status, body)`.
fn get(agent: &ureq::Agent, port: u16, path: &str, bearer: Option<&str>) -> (u16, String) {
    let mut request = agent.get(format!("http://127.0.0.1:{port}{path}"));
    if let Some(token) = bearer {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = request.call().expect("the request completes");
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().expect("read the body");
    (status, body)
}

/// GET `/v1/system` until it is `200` (the first sample arrives after at most one interval) or 3 s pass.
fn system_ready(agent: &ureq::Agent, port: u16) {
    let start = Instant::now();
    loop {
        let (status, _body) = get(agent, port, "/v1/system", Some("t"));
        if status == 200 {
            return;
        }
        if start.elapsed() > Duration::from_secs(3) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// GET `/v1/log?since=0` until the ring holds the whole fixture (`next == 37`) or 3 s pass: the log
/// command may still be starting when the first sample arrives.
fn log_ready(agent: &ureq::Agent, port: u16) -> LogPage {
    let start = Instant::now();
    loop {
        let (_status, body) = get(agent, port, "/v1/log?since=0", Some("t"));
        let page: LogPage = serde_json::from_str(&body).expect("a valid log page");
        if page.next == 37 {
            return page;
        }
        if start.elapsed() > Duration::from_secs(3) {
            return page;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn serves_system_and_log() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = server::start(config(), stop).expect("the agent starts");
    let port = addr.port();
    let agent = agent();

    system_ready(&agent, port);
    let (status, body) = get(&agent, port, "/v1/system", Some("t"));
    assert_eq!(status, 200);
    let snapshot: SystemSnapshot = serde_json::from_str(&body).expect("a valid snapshot");
    assert_eq!(snapshot.hostname.as_deref(), Some("aibox-flo"));

    let page = log_ready(&agent, port);
    assert_eq!(page.next, 37);
    assert_eq!(page.lines.len(), 36, "35 fixture lines plus the source-ended line");
    assert_eq!(page.lines.last().unwrap().text.as_str(), "log source ended: cat");

    let (status, body) = get(&agent, port, "/v1/log?since=37", Some("t"));
    assert_eq!(status, 200);
    let page: LogPage = serde_json::from_str(&body).expect("a valid log page");
    assert!(page.lines.is_empty(), "no line past the end of the ring");
}

#[test]
fn requires_the_token() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = server::start(config(), stop).expect("the agent starts");
    let port = addr.port();
    let agent = agent();

    let (status, _) = get(&agent, port, "/v1/system", None);
    assert_eq!(status, 401, "no token is 401");
    let (status, _) = get(&agent, port, "/v1/system", Some("x"));
    assert_eq!(status, 401, "a wrong token is 401");
    let (status, _) = get(&agent, port, "/v1/health", None);
    assert_eq!(status, 200, "health needs no token");
}

#[test]
fn bad_request() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = server::start(config(), stop).expect("the agent starts");
    let port = addr.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    stream.write_all(b"HELLO\r\n\r\n").expect("write the head");
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_to_string(&mut response).expect("read the response");
    assert!(response.starts_with("HTTP/1.1 400 Bad Request"), "got: {response:?}");
}

#[test]
fn stops() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, handle) = server::start(config(), stop.clone()).expect("the agent starts");
    let port = addr.port();
    let client = agent();

    // Wait for the first sample so we know the server is answering.
    let (status, _) = get(&client, port, "/v1/system", Some("t"));
    assert_eq!(status, 200);

    stop.store(true, Ordering::Relaxed);
    let start = Instant::now();
    handle.join().expect("the accept thread joins");
    assert!(start.elapsed() < Duration::from_secs(1), "joined within 1 s");

    // After the listener closes, a new connection is refused (or gets no answer).
    let dead = agent();
    let result = dead.get(format!("http://127.0.0.1:{port}/v1/health")).call();
    assert!(result.is_err(), "a connection after stop fails");
}

#[test]
fn no_token_beyond_loopback_is_refused() {
    let public: SocketAddr = "0.0.0.0:9099".parse().expect("a socket address");
    let loopback: SocketAddr = "127.0.0.1:9099".parse().expect("a socket address");

    // Beyond loopback, without a token and without --no-auth: an error naming the address.
    let error = server::check_token_policy(&public, None, false).expect_err("refused beyond loopback");
    assert!(error.to_string().contains("0.0.0.0:9099"), "names the address: {error}");

    // Loopback without a token is allowed; --no-auth or a token allows beyond loopback too.
    assert!(server::check_token_policy(&loopback, None, false).is_ok());
    assert!(server::check_token_policy(&public, None, true).is_ok());
    assert!(server::check_token_policy(&public, Some("t"), false).is_ok());
}
