//! The `--dump` report: every collector's view as one JSON object, then exit.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Serialize;

use crate::collect::llama_poll::{LlamaSnapshot, Poller};
use crate::collect::logs::{self, LogLine};
use crate::collect::sampler::SystemSnapshot;
use crate::config::Cli;
use crate::http::Client;

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

    // 2. One polling round against the server.
    let client = Client::new(&cli.url, cli.api_key(), Duration::from_secs(3));
    let llama = Poller::new(client).poll(Instant::now());

    // 3. Follow the log for 700 ms (or until it ends), keeping the last 20 lines. Do not stop the
    // command before the thread has ended: SIGKILL would skip the program's flush and drop buffered
    // lines. `rx.iter()` returns once the thread drops `tx`, i.e. after the "ended" line or 700 ms.
    let (tx, rx) = mpsc::channel::<LogLine>();
    let start = Instant::now();
    let mut log_tail = Vec::new();
    let log_error = match logs::follow(&cli.log_cmd, move |line: LogLine| {
        let source_ended = line.text.starts_with("log source ended:");
        let too_late = start.elapsed() >= Duration::from_millis(700);
        let _ = tx.send(line); // keep every line, including the "ended" one
        !source_ended && !too_late
    }) {
        Ok(mut follower) => {
            let mut all: Vec<LogLine> = rx.iter().collect();
            follower.stop(); // no-op if the thread already ended; kills a still-running live stream
            log_tail = all.split_off(all.len().saturating_sub(20));
            None
        }
        Err(e) => Some(format!("{e:#}")),
    };

    // 4. Serialize. The API key is never held by any snapshot, so it cannot appear here.
    let report = DumpReport { system, llama, log_tail, log_error };
    Ok(serde_json::to_string_pretty(&report)?)
}
