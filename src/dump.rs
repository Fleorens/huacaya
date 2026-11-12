//! The `--dump` report: every collector's view as one JSON object, then exit.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;

use crate::collect::llama_poll::{LlamaSnapshot, Poller};
use crate::collect::logs::{self, LogLine};
use crate::collect::sampler::SystemSnapshot;
use crate::config::Cli;
use crate::http::{Client, is_loopback_url};

/// The `--dump` report: one object with exactly these keys — `system`, `llama`, `log_tail`, `log_error`.
#[derive(Serialize)]
struct DumpReport {
    system: SystemSnapshot,
    llama: LlamaSnapshot,
    log_tail: Vec<LogLine>,
    log_error: Option<String>,
}

/// Builds the pretty-printed JSON report and returns it. Samples the system twice (300 ms apart, keeping
/// the second for its CPU usage and network rates), polls the server once, follows the log for 700 ms
/// keeping the last 20 lines, then serializes. Returns in about 1.5 s even when the server is down and
/// the log is silent.
pub fn run(cli: &Cli) -> Result<String> {
    // 1. Two system samples 300 ms apart; keep the second (it has CPU usage and network rates).
    let mut sampler = crate::collect::sampler::Sampler::new(cli.sysroot.clone(), cli.mounts.clone());
    let _first = sampler.sample(Instant::now());
    std::thread::sleep(Duration::from_millis(300));
    let system = sampler.sample(Instant::now());

    // 2. One polling round against the server. On a loopback URL, ask the loaded instance directly
    // (prints nothing in the journal); from another machine, only the router path with `model=`.
    let client = Client::new(&cli.url, cli.api_key(), Duration::from_secs(3));
    // Only on the real machine: a test sysroot's ports belong to nothing, and the key must not go there.
    let mut poller = if is_loopback_url(&cli.url) && cli.sysroot == std::path::Path::new("/") {
        Poller::new(client).with_local_instances(cli.sysroot.clone())
    } else {
        Poller::new(client)
    };
    let llama = poller.poll(Instant::now());

    // 3. Follow the log for 700 ms (or until it ends), keeping the last 20 lines. The deadline is
    // enforced here, not in the sink: a live but silent stream (`journalctl -f` on an idle server)
    // never calls the sink again, so a sink-side check would block forever. A command that ends in
    // time has been read to EOF before the "ended" line, so killing it afterwards loses nothing.
    let (tx, rx) = mpsc::channel::<LogLine>();
    let deadline = Instant::now() + Duration::from_millis(700);
    let mut log_tail = Vec::new();
    let log_error = match logs::follow(&cli.log_cmd, move |line: LogLine| tx.send(line).is_ok()) {
        Ok(mut follower) => {
            let mut all: Vec<LogLine> = Vec::new();
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                let Ok(line) = rx.recv_timeout(left) else { break };
                let ended = line.text.starts_with("log source ended:");
                all.push(line);
                if ended {
                    break;
                }
            }
            follower.stop(); // kills a still-running stream, which unblocks and ends the thread
            log_tail = all.split_off(all.len().saturating_sub(20));
            None
        }
        Err(e) => Some(format!("{e:#}")),
    };

    // 4. Serialize. The API key is never held by any snapshot, so it cannot appear here.
    let report = DumpReport { system, llama, log_tail, log_error };
    Ok(serde_json::to_string_pretty(&report)?)
}
