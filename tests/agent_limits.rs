//! The `huacaya agent` server's two limits: it answers at most `MAX_CONNECTIONS` at once (the rest get
//! 503 at once), and it drops a connection whose request head does not arrive within `HEAD_DEADLINE`.
//! The test binds `127.0.0.1:0` and reads the real port from `start`, talking to a real agent over TCP.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use huacaya::agent::server::{self, AgentConfig, HEAD_DEADLINE, MAX_CONNECTIONS};

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

/// Opens `MAX_CONNECTIONS` connections that send nothing, holding a slot each, and returns them so the
/// caller can keep them counted. The accept thread needs a moment to accept and count them.
fn hold_slots(port: u16) -> Vec<TcpStream> {
    let mut idle: Vec<TcpStream> = Vec::with_capacity(MAX_CONNECTIONS);
    for _ in 0..MAX_CONNECTIONS {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        idle.push(stream);
    }
    // Give the accept thread time to accept and count all of them before the next connection.
    std::thread::sleep(Duration::from_millis(300));
    idle
}

#[test]
fn busy_beyond_max_connections() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, handle) = server::start(config(), stop.clone()).expect("the agent starts");
    let port = addr.port();

    // MAX_CONNECTIONS idle connections each hold a slot.
    let idle = hold_slots(port);

    // One more is beyond the cap: it is answered at once on the accept thread with 503, no thread.
    let extra = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let _ = extra.set_read_timeout(Some(Duration::from_secs(3)));
    let mut reader = BufReader::new(extra);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("read the status line");
    assert!(
        status_line.starts_with("HTTP/1.1 503"),
        "beyond the cap the client gets 503 at once, got: {status_line:?}"
    );

    // Release the idle connections and stop.
    drop(idle);
    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
}

#[test]
fn slow_head_is_dropped() {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, handle) = server::start(config(), stop.clone()).expect("the agent starts");
    let port = addr.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    // A reader thread drains the connection until the server closes it (Ok(0) or error) and reports what
    // it read; the `closed` flag tells the write loop that the server closed, giving it a clean stop
    // condition (a FIN lets the client keep writing, so the write loop could otherwise never stop).
    let closed = Arc::new(AtomicBool::new(false));
    let closed_for_reader = closed.clone();
    let (tx, rx) = mpsc::channel::<String>();
    let reader_stream = stream.try_clone().expect("clone the stream");
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader_stream);
        let mut received = String::new();
        let _ = reader.read_to_string(&mut received);
        closed_for_reader.store(true, Ordering::Relaxed);
        let _ = tx.send(received);
    });

    // Send the request line, then one byte every 500 ms: the head never completes, but each read returns
    // before the 2 s per-read timeout, so only the request-head deadline ends the read on the server.
    let _ = stream.write_all(b"GET /v1/health HTTP/1.1\r\n");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(7);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        if stream.write_all(b"x").is_err() || closed.load(Ordering::Relaxed) {
            break;
        }
    }

    // The server must close within HEAD_DEADLINE + 3 s; the received bytes must not be a 200 answer.
    let received = rx.recv_timeout(Duration::from_secs(2)).expect("the reader reports the close");
    let elapsed = started.elapsed();
    assert!(
        elapsed <= HEAD_DEADLINE + Duration::from_secs(3),
        "the stalled head was not dropped in time: {elapsed:?}"
    );
    assert!(
        !received.starts_with("HTTP/1.1 200"),
        "a stalled head must not be answered with 200, got: {received:?}"
    );

    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
}
