//! The `llamatop agent` server: sample the machine, follow the log, and answer T26's three endpoints
//! over HTTP. No async runtime — one thread per job (sampler, log, accept), one per connection, like the
//! rest of llamatop.

use std::io::{self, BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::agent::proto::{self, LogRing};
use crate::collect::logs::{self as logs, Level, LogLine};
use crate::collect::sampler::{Sampler, SystemSnapshot};

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
pub fn start(config: AgentConfig, stop: Arc<AtomicBool>) -> Result<(SocketAddr, JoinHandle<()>)> {
    // Current snapshot and log ring, shared with the accept thread.
    let snapshot: Arc<Mutex<Option<SystemSnapshot>>> = Arc::new(Mutex::new(None));
    let ring: Arc<Mutex<LogRing>> = Arc::new(Mutex::new(LogRing::new()));

    // Sampler: one sample per interval, sleeping in slices of at most 100 ms.
    {
        let snapshot = snapshot.clone();
        let sampler = Sampler::new(config.sysroot.clone(), config.mounts.clone());
        let interval = config.interval;
        let stop = stop.clone();
        thread::spawn(move || {
            let mut sampler = sampler;
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                *snapshot.lock().unwrap() = Some(sampler.sample(Instant::now()));
                sleep_in_slices(interval, &stop);
            }
        });
    }

    // Log: follow the command, pushing every line into the ring, and keep the follower alive until stop.
    {
        let ring = ring.clone();
        let sink_ring = ring.clone();
        let log_cmd = config.log_cmd.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            match logs::follow(&log_cmd, move |line| {
                sink_ring.lock().unwrap().push(line);
                true
            }) {
                Ok(mut follower) => {
                    sleep_in_slices(Duration::from_millis(100), &stop);
                    follower.stop();
                }
                Err(err) => {
                    ring.lock().unwrap().push(LogLine {
                        time: None,
                        source: None,
                        instance: None,
                        task: None,
                        level: Level::Warn,
                        text: format!("log command failed: {err}"),
                        event: None,
                    });
                }
            }
        });
    }

    // Accept: the listener in non-blocking mode, one thread per connection.
    let listener = TcpListener::bind(config.listen)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let accept_handle =
        thread::spawn(move || accept_loop(listener, snapshot, ring, config.token.clone(), stop));

    Ok((addr, accept_handle))
}

/// `start`, then prints `llamatop agent listening on <addr>` and waits for the accept thread (forever,
/// until the process is stopped).
pub fn run(config: AgentConfig) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _handle) = start(config, stop)?;
    println!("llamatop agent listening on {addr}");
    std::thread::park();
    Ok(())
}

/// Accepts connections until `stop` is set, answering each on its own thread, then closes the listener so
/// a later connection is refused.
fn accept_loop(
    listener: TcpListener,
    snapshot: Arc<Mutex<Option<SystemSnapshot>>>,
    ring: Arc<Mutex<LogRing>>,
    token: Option<String>,
    stop: Arc<AtomicBool>,
) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let token = token.clone();
                let snapshot = snapshot.clone();
                let ring = ring.clone();
                thread::spawn(move || handle_connection(stream, token, snapshot, ring));
            }
            Err(_) => {
                // Non-blocking: `WouldBlock` (nothing to accept yet) or a transient error. Wait a beat
                // and check stop, so a stop request is honoured within ~200 ms.
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    // The listener closes when `listener` drops at the end of this function, so a later connection is
    // refused once the agent has stopped.
}

/// Answers one connection: read the request head, route it against the current snapshot and ring, write
/// the response, close. Two-second read and write timeouts keep a slow client from blocking others.
fn handle_connection(
    mut stream: TcpStream,
    token: Option<String>,
    snapshot: Arc<Mutex<Option<SystemSnapshot>>>,
    ring: Arc<Mutex<LogRing>>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    // Read the head on a clone, write the response on the original socket.
    let mut reader = match stream.try_clone() {
        Ok(clone) => io::BufReader::new(clone),
        Err(_) => return,
    };
    let Some(head) = read_head(&mut reader) else {
        return;
    };
    let Some(req) = proto::parse_request(&String::from_utf8_lossy(&head)) else {
        let resp = proto::response(400, r#"{"error":"bad request"}"#);
        let _ = stream.write_all(resp.as_bytes());
        return;
    };

    // Route with the current snapshot and ring, building the whole body before releasing the locks so a
    // socket write never happens while one is held.
    let (status, body) = {
        let snapshot = snapshot.lock().unwrap();
        let ring = ring.lock().unwrap();
        proto::route(&req, token.as_deref(), snapshot.as_ref(), &ring)
    };
    let resp = proto::response(status, &body);
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

/// Reads the request head: everything up to the empty line (`\r\n\r\n` or `\n\n`), or up to `MAX_HEAD`
/// bytes, or until EOF. `None` when the client sent nothing.
fn read_head(reader: &mut impl BufRead) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    loop {
        if buf.len() >= proto::MAX_HEAD {
            return Some(buf);
        }
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => return if buf.is_empty() { None } else { Some(buf) },
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") || buf.ends_with(b"\n\n") {
                    return Some(buf);
                }
            }
            Err(_) => return Some(buf), // timeout or error: answer with what we have.
        }
    }
}

/// Sleeps `duration` in slices of at most 100 ms, waking on `stop` so a stop request is honoured within a
/// fraction of a slice.
fn sleep_in_slices(duration: Duration, stop: &AtomicBool) {
    let mut slept = Duration::ZERO;
    while slept < duration {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let slice = (duration - slept).min(Duration::from_millis(100));
        thread::sleep(slice);
        slept += slice;
    }
}

/// Enforces the token policy: a token is required as soon as the agent listens beyond loopback, unless the
/// user explicitly allowed no token with `no_auth`. `Ok` when there is a token, `no_auth` is set, or the
/// address is loopback; otherwise an error naming the address.
pub fn check_token_policy(listen: &SocketAddr, token: Option<&str>, no_auth: bool) -> Result<()> {
    if token.is_some() || no_auth || listen.ip().is_loopback() {
        return Ok(());
    }
    Err(anyhow!("a token is required to listen on {listen}: --token-file <file>, or --no-auth"))
}
