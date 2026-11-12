//! Following the server log and classifying its lines.
//!
//! The log panel does not read the journal itself: it runs a command (by default
//! `journalctl -u llama-server -f …`) and reads its standard output line by line, see
//! `docs/01-architecture.md` § 4 and `tasks/T07-logs.md`. `parse_line` is a pure classifier; `follow`
//! runs the command on a thread. A line that cannot be understood is still returned, as `Info` with no
//! event: the parser never panics and never drops a line.

use std::io::{self, BufRead};
use std::process::{Child, Command, Stdio};
use std::thread;

use serde::Serialize;

/// Severity of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// A structured fact extracted from a line, so the UI can highlight a request without re-parsing.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub enum LogEvent {
    /// `prompt eval time = … / N tokens ( … , X tokens per second)`.
    PromptTiming { tokens: u64, tps: f64 },
    /// `eval time = … / N tokens ( … , X tokens per second)` on a line that is not a prompt line.
    GenTiming { tokens: u64, tps: f64 },
    /// `n_gen = N, tg = X t/s, tg_3s = Y t/s` — printed every ~3 s while generating.
    GenProgress { n_gen: u64, tps: f64, tps_3s: f64 },
    /// `prompt processing, n_tokens = N, progress = P, t = … s / X tokens per second`.
    PromptProgress { n_tokens: u64, progress: f32, tps: f64 },
    /// `draft acceptance = R ( A accepted / G generated), …`.
    DraftAcceptance { accepted: u64, generated: u64 },
    /// `stop processing: n_tokens = N, …` — the context size at the end of the request.
    Released { n_tokens: u64 },
    /// `proxying request to model <id> on port <port>`.
    Proxy { model: String, port: u16 },
    /// `request: <METHOD> <path> <client address> <status>`.
    Request { method: String, path: String, status: u16 },
}

/// One classified line of the log.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct LogLine {
    /// `HH:MM:SS` cut out of the ISO timestamp; `None` when the line has no journal prefix.
    pub time: Option<String>,
    /// The unit name without `[pid]` (`llama-server`, `systemd`); `None` without prefix.
    pub source: Option<String>,
    /// The `[<port>]` of the model instance that printed the line.
    pub instance: Option<u16>,
    /// The number after `| task ` (may be −1).
    pub task: Option<i64>,
    pub level: Level,
    /// The message: without journal prefix, `[port]`, elapsed time and level letter; leading whitespace
    /// removed. Without journal prefix: the whole line.
    pub text: String,
    pub event: Option<LogEvent>,
}

/// Classifies one raw line. Total function: never panics, whatever the input (empty, binary, huge).
pub fn parse_line(raw: &str) -> LogLine {
    // Locate the first three whitespace-separated tokens and the offset right after the third, all by
    // bytes so a non-ASCII or huge first token cannot panic.
    let bytes = raw.as_bytes();
    let mut i = 0usize;

    let mut token = |bytes: &[u8]| -> (usize, usize) {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        (start, i)
    };

    let (s1, e1) = token(bytes);
    let _ = token(bytes);
    let (s3, e3) = token(bytes);
    let has_third = e3 > s3;
    let first = &raw[s1..e1];
    let third = &raw[s3..e3];

    // 1. Journal prefix: a long ISO token, shaped like a time, followed by a unit token ending in `:`.
    let has_prefix = has_third
        && first.len() >= 19
        && first.as_bytes().get(10) == Some(&b'T')
        && first.as_bytes().get(13) == Some(&b':')
        && first.as_bytes().get(16) == Some(&b':')
        && third.ends_with(':');

    // 2. llama.cpp prefix: optional `[port]`, elapsed time and level letter, in that order. Skipped
    // without a journal prefix, where `text` is the whole line.
    let (time, source, instance, level_letter, text) = if has_prefix {
        let time = raw.get(s1 + 11..s1 + 19).map(str::to_string);
        let source = strip_pid(third.strip_suffix(':').unwrap_or(third)).to_string();
        let (instance, level_letter, text) = parse_llama_prefix(&raw[e3..]);
        (time, Some(source), instance, level_letter, text)
    } else {
        (None, None, None, None, raw.to_string())
    };

    // 3. Task.
    let task = token_after(&text, "| task ")
        .map(|t| t.strip_suffix(',').unwrap_or(t))
        .and_then(|t| t.parse::<i64>().ok());

    // 4. Event (first match wins).
    let event = detect_event(&text);

    // 5. Level.
    let level = level_for(&text, event.as_ref(), level_letter);

    LogLine { time, source, instance, task, level, text, event }
}

/// Removes a trailing `[pid]` (`llama-server[1194]` → `llama-server`); leaves other names untouched.
fn strip_pid(name: &str) -> &str {
    let Some(idx) = name.rfind('[') else {
        return name;
    };
    let after = &name[idx + 1..];
    let Some(digits) = after.strip_suffix(']') else {
        return name;
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return name;
    }
    &name[..idx]
}

/// Consumes the optional `[port]`, elapsed time and level letter at the start of `rest`, returning the
/// port, the level letter (if any) and the remaining message with leading whitespace removed.
fn parse_llama_prefix(rest: &str) -> (Option<u16>, Option<char>, String) {
    let mut rest = rest.trim_start();

    // Each step consumes at most one token, and only in the sheet's order.
    let mut instance = None;
    if let Some(inner) = next_token(rest).and_then(|t| t.strip_prefix('[')?.strip_suffix(']'))
        && !inner.is_empty()
        && inner.bytes().all(|b| b.is_ascii_digit())
    {
        instance = inner.parse::<u16>().ok();
        rest = after_token(rest);
    }

    if next_token(rest)
        .is_some_and(|t| t.bytes().any(|b| b == b'.') && t.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
    {
        rest = after_token(rest);
    }

    let mut level = None;
    if let Some(t) = next_token(rest)
        && t.len() == 1
        && matches!(t.as_bytes()[0], b'I' | b'W' | b'E' | b'D')
    {
        level = Some(t.as_bytes()[0] as char);
        rest = after_token(rest);
    }

    (instance, level, rest.to_string())
}

/// The first whitespace-separated token of `s`, which starts without whitespace.
fn next_token(s: &str) -> Option<&str> {
    s.split_ascii_whitespace().next()
}

/// What follows the first token of `s`, leading whitespace removed.
fn after_token(s: &str) -> &str {
    let end = s.find(|c: char| c.is_ascii_whitespace()).unwrap_or(s.len());
    s[end..].trim_start()
}

/// First whitespace-separated token after `key`, or `None`.
fn token_after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pos = text.find(key)?;
    text[pos + key.len()..].split_whitespace().next()
}

/// Last whitespace-separated token before `key`, or `None`.
fn token_before<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pos = text.find(key)?;
    text[..pos].split_whitespace().last()
}

/// `token_after`, with a trailing `,` removed, parsed. A number that cannot be read is `None`.
fn number_after<T: std::str::FromStr>(text: &str, key: &str) -> Option<T> {
    token_after(text, key).map(|t| t.strip_suffix(',').unwrap_or(t)).and_then(|t| t.parse().ok())
}

/// `token_before`, with a leading `(` removed, parsed. A number that cannot be read is `None`.
fn number_before<T: std::str::FromStr>(text: &str, key: &str) -> Option<T> {
    token_before(text, key).map(|t| t.strip_prefix('(').unwrap_or(t)).and_then(|t| t.parse().ok())
}

/// Extracts the first structured fact from `text`, in the order the table of the sheet lists it.
fn detect_event(text: &str) -> Option<LogEvent> {
    if text.contains("prompt processing, n_tokens =") {
        return Some(LogEvent::PromptProgress {
            n_tokens: number_after(text, "n_tokens =")?,
            progress: number_after(text, "progress =")?,
            tps: number_before(text, " tokens per second")?,
        });
    }
    if text.contains("prompt eval time =") {
        return Some(LogEvent::PromptTiming {
            tokens: number_before(text, " tokens (")?,
            tps: number_before(text, " tokens per second")?,
        });
    }
    if text.contains("eval time =") {
        return Some(LogEvent::GenTiming {
            tokens: number_before(text, " tokens (")?,
            tps: number_before(text, " tokens per second")?,
        });
    }
    if text.contains("n_gen =") {
        return Some(LogEvent::GenProgress {
            n_gen: number_after(text, "n_gen =")?,
            tps: number_after(text, "tg =")?,
            tps_3s: number_after(text, "tg_3s =")?,
        });
    }
    if text.contains("draft acceptance =") {
        return Some(LogEvent::DraftAcceptance {
            accepted: number_before(text, " accepted")?,
            generated: number_before(text, " generated")?,
        });
    }
    if text.contains("stop processing: n_tokens =") {
        return Some(LogEvent::Released { n_tokens: number_after(text, "n_tokens =")? });
    }
    if let Some(after) = text.find("proxying request to model ") {
        let model = text[after + "proxying request to model ".len()..].split_whitespace().next()?;
        let port = number_after(text, " on port ")?;
        return Some(LogEvent::Proxy { model: model.to_string(), port });
    }
    if let Some(after) = text.find("request: ") {
        let items: Vec<&str> = text[after + "request: ".len()..].split_whitespace().collect();
        if items.len() >= 4 {
            let status = items[3].strip_suffix(',').unwrap_or(items[3]).parse::<u16>().ok()?;
            return Some(LogEvent::Request {
                method: items[0].to_string(),
                path: items[1].to_string(),
                status,
            });
        }
    }
    None
}

/// Resolves the level, honouring the sheet's precedence: a `Request` status, then a level letter, then
/// the error/warn keywords.
fn level_for(text: &str, event: Option<&LogEvent>, level_letter: Option<char>) -> Level {
    if let Some(LogEvent::Request { status, .. }) = event {
        return if *status >= 500 {
            Level::Error
        } else if *status >= 400 {
            Level::Warn
        } else {
            Level::Info
        };
    }
    if let Some(letter) = level_letter {
        return match letter {
            'E' => Level::Error,
            'W' => Level::Warn,
            _ => Level::Info, // 'I' and 'D'
        };
    }
    let lower = text.to_lowercase();
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
        || lower.contains("out of memory")
    {
        Level::Error
    } else if lower.contains("warn") {
        Level::Warn
    } else {
        Level::Info
    }
}

/// A running log command. Dropping it stops it.
pub struct Follower {
    child: Option<Child>,
    thread: Option<thread::JoinHandle<()>>,
}

/// Starts `command` (split on ASCII whitespace; first item = program) with stdin and stderr closed
/// (`Stdio::null()`), and calls `sink` with every line of its standard output, from a new thread. When
/// the output ends, `sink` receives one last line: level `Warn`, text `log source ended: <program>`, no
/// time, no source. The thread ends then, or as soon as `sink` returns `false`. `Err` when the command is
/// empty or the program cannot be started (e.g. `journalctl` not installed).
pub fn follow(command: &str, mut sink: impl FnMut(LogLine) -> bool + Send + 'static) -> io::Result<Follower> {
    let mut parts: Vec<String> = command.split_ascii_whitespace().map(String::from).collect();
    if parts.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty log command"));
    }
    let program = parts.remove(0);

    let mut child = Command::new(&program)
        .args(&parts)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    // Take the stdout out for the thread so `stop` can still reach the child and unblock it by killing.
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::other("log command has no stdout"));
    };
    let thread = thread::spawn(move || {
        let mut reader = io::BufReader::new(stdout);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break, // EOF: fall through to the final line.
                Ok(_) => {
                    while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
                        buf.pop();
                    }
                    let line = String::from_utf8_lossy(&buf);
                    if !sink(parse_line(&line)) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        sink(LogLine {
            time: None,
            source: None,
            instance: None,
            task: None,
            level: Level::Warn,
            text: format!("log source ended: {program}"),
            event: None,
        });
    });

    Ok(Follower { child: Some(child), thread: Some(thread) })
}

impl Follower {
    /// Kills the command, waits for it, joins the thread. Safe to call twice.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Follower {
    fn drop(&mut self) {
        self.stop();
    }
}
