//! The OpenCode server (`opencode serve`) client: its sessions and, for the followed session, its
//! timeline of steps (`tasks/T34`), read through any `llama_poll::Fetch` so tests need no network.

use anyhow::{Context, Result};

use crate::collect::activity::Activity;

mod poller;
mod raw;
pub use poller::OpenCodePoller;
use raw::{RawMessages, RawPart, RawSession, RawSessions};

/// One OpenCode session.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    /// `job_number(title)`.
    pub job: Option<u32>,
    /// `model.id`.
    pub model: Option<String>,
    pub agent: Option<String>,
    /// `outcome` (`succeeded`, `failed`, …); `None` while it runs.
    pub outcome: Option<String>,
    /// No `outcome` and no `time.idle`.
    pub running: bool,
    pub created_ms: i64,
    pub idle_ms: Option<i64>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    /// `location.directory`, used to shorten paths.
    pub directory: Option<String>,
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StepKind {
    User,
    Think,
    Say,
    Read,
    Edit,
    Write,
    Shell,
    Search,
    Other(String),
}
/// One thing the model did, in a session's timeline.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    /// The tool part's `time.created`, else its message's `time.created`.
    pub time_ms: i64,
    pub kind: StepKind,
    /// Stable across poll rounds: the tool part's `id`; for other parts `<message id>#<part index>`
    /// (index in the message's `content`, from 0); `#<part index>` alone when the message has no `id`.
    pub id: String,
    /// When the step ended, Unix ms: the part's `time.completed` for tools and reasoning; for text and
    /// user parts the message's `time.completed`, else `None`. `None` means the step still runs.
    pub end_ms: Option<i64>,
    /// The step's summary (see `StepKind`).
    pub summary: String,
    /// `Some(false)` for a tool whose `state.status` is `error`, or a shell whose `metadata.exit` is not 0;
    /// `Some(true)` for other completed tools; `None` for reasoning and text, and for a tool still running.
    pub ok: Option<bool>,
    pub detail: Option<String>,
}

/// What the Agents screen gets every round.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenCodeSnapshot {
    pub sessions: Vec<Session>,
    /// The session whose `steps` these are.
    pub focus: Option<String>,
    pub steps: Vec<Step>,
    /// The map's state for the followed session (tasks/T42), folded from every round.
    #[serde(skip)]
    pub activity: Activity,
    /// The last error (`unreachable: …`, `HTTP 401` …), never containing the password.
    pub error: Option<String>,
    /// Wall-clock time (Unix ms) this round was fetched; `0` when unread.
    #[serde(default)]
    pub fetched_ms: i64,
}

/// `#136 Execute …` → `Some(136)`: the digits right after a leading `#`.
pub fn job_number(title: &str) -> Option<u32> {
    let digits = title.trim().strip_prefix('#')?;
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    digits[..end].parse().ok()
}
/// Shortens `path` by removing a leading `directory` (and the following `/`), when it matches.
fn shorten(path: &str, directory: Option<&str>) -> String {
    match directory {
        Some(dir) => path
            .strip_prefix(dir.trim_end_matches('/'))
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|rest| !rest.is_empty())
            .unwrap_or(path)
            .to_string(),
        None => path.to_string(),
    }
}
/// Whitespace collapsed to single spaces, at most 200 chars.
fn summary_text(text: Option<&str>) -> String {
    let text = text.unwrap_or("");
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect()
}
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}
/// The `+<added> −<removed>` of unified diff patches: lines starting with `+` / `-`, not `+++` / `---`.
fn patch_delta<'a>(patches: impl Iterator<Item = &'a str>) -> String {
    let (mut added, mut removed) = (0, 0);
    for line in patches.flat_map(str::lines) {
        added += usize::from(line.starts_with('+') && !line.starts_with("+++"));
        removed += usize::from(line.starts_with('-') && !line.starts_with("---"));
    }
    format!("+{added} −{removed}")
}
/// Parses `GET /api/session`, newest first as the server returns it.
pub fn parse_sessions(json: &str) -> Result<Vec<Session>> {
    let raw: RawSessions = serde_json::from_str(json).context("parse sessions")?;
    Ok(raw.data.into_iter().map(session_from).collect())
}

/// Messages per page when loading a session's history (the server's maximum, 200 is `HTTP 400` above).
pub const HISTORY_PAGE: usize = 200;
/// Pages loaded at most for one history (2000 messages).
pub const HISTORY_PAGES: usize = 10;
/// Messages fetched by a live round.
pub const LIVE_LIMIT: usize = 12;
/// Steps kept in `OpenCodeSnapshot::steps` (the newest).
pub const SNAPSHOT_STEPS: usize = 200;

/// The `cursor.next` of a messages page: the token for the next older page, `None` when absent, null or
/// when the JSON does not parse.
pub fn next_cursor(json: &str) -> Option<String> {
    serde_json::from_str::<RawMessages>(json).ok().and_then(|m| m.cursor).and_then(|c| c.next)
}

fn session_from(raw: RawSession) -> Session {
    let has_outcome = raw.outcome.is_some();
    let has_idle = raw.time.as_ref().and_then(|t| t.idle).is_some();
    let tokens = raw.tokens.as_ref();
    let title = raw.title;
    let job = job_number(&title);
    Session {
        id: raw.id,
        title,
        job,
        model: raw.model.as_ref().and_then(|m| m.id.clone()).filter(|s| !s.is_empty()),
        agent: raw.agent.clone().filter(|s| !s.is_empty()),
        outcome: raw.outcome.clone().filter(|s| !s.is_empty()),
        running: !has_outcome && !has_idle,
        created_ms: raw.time.as_ref().map(|t| t.created).unwrap_or(0),
        idle_ms: raw.time.and_then(|t| t.idle),
        tokens_in: tokens.map(|t| t.input).unwrap_or(0),
        tokens_out: tokens.map(|t| t.output).unwrap_or(0),
        cache_read: tokens.map(|t| t.cache.read).unwrap_or(0),
        directory: raw.location.and_then(|l| l.directory.clone()).filter(|s| !s.is_empty()),
    }
}

/// Parses `GET /api/session/<id>/message` into steps, oldest first (the API is newest first). `assistant`
/// messages give steps, their parts in order; a `user` message gives one `User` step for its top-level
/// `text` (the real server's shape, id `<message id>#text`) and one per text part; other
/// types (`idle`, …) give nothing. `directory` shortens paths and prefixes `cd <directory> && ` in shell
/// summaries.
pub fn parse_steps(json: &str, directory: Option<&str>) -> Result<Vec<Step>> {
    let messages: RawMessages = serde_json::from_str(json).context("parse messages")?;
    let mut steps: Vec<Step> = Vec::new();
    for msg in messages.data.iter().rev() {
        let message_id = msg.id.as_deref();
        let message_time = msg.time.as_ref().map(|t| t.created).unwrap_or(0);
        let message_completed = msg.time.as_ref().and_then(|t| t.completed);
        match msg.r#type.as_deref() {
            Some("user") => {
                if let Some(text) = msg.text.as_deref() {
                    steps.push(Step {
                        time_ms: message_time,
                        kind: StepKind::User,
                        id: format!("{}#text", message_id.unwrap_or("")),
                        end_ms: Some(message_time),
                        summary: summary_text(Some(text)),
                        ok: None,
                        detail: None,
                    });
                }
                for (pi, part) in msg.content.iter().enumerate() {
                    if part.r#type.as_deref() == Some("text") {
                        steps.push(Step {
                            time_ms: message_time,
                            kind: StepKind::User,
                            id: step_id(message_id, pi),
                            end_ms: Some(message_time),
                            summary: summary_text(part.text.as_deref()),
                            ok: None,
                            detail: None,
                        });
                    }
                }
            }
            Some("assistant") => {
                for (pi, part) in msg.content.iter().enumerate() {
                    if let Some(step) =
                        part_step(part, message_id, pi, message_time, message_completed, directory)
                    {
                        steps.push(step);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(steps)
}

fn step_id(message_id: Option<&str>, pi: usize) -> String {
    format!("{}#{pi}", message_id.unwrap_or(""))
}

fn part_step(
    part: &RawPart,
    message_id: Option<&str>,
    pi: usize,
    message_time: i64,
    message_completed: Option<i64>,
    directory: Option<&str>,
) -> Option<Step> {
    let time = part.time.as_ref().map(|t| t.created).unwrap_or(message_time);
    let end_ms = match part.r#type.as_deref() {
        // Tools and reasoning end with their part; text with its message.
        Some("tool") | Some("reasoning") => part.time.as_ref().and_then(|t| t.completed),
        _ => message_completed,
    };
    match part.r#type.as_deref() {
        Some("reasoning") => Some(Step {
            time_ms: time,
            kind: StepKind::Think,
            id: step_id(message_id, pi),
            end_ms,
            summary: summary_text(part.text.as_deref()),
            ok: None,
            detail: None,
        }),
        Some("text") => Some(Step {
            time_ms: time,
            kind: StepKind::Say,
            id: step_id(message_id, pi),
            end_ms,
            summary: summary_text(part.text.as_deref()),
            ok: None,
            detail: None,
        }),
        Some("tool") => tool_step(
            part,
            time,
            part.id.clone().unwrap_or_else(|| step_id(message_id, pi)),
            end_ms,
            directory,
        ),
        _ => None,
    }
}

/// `status == "error"` is `ok: Some(false)`, a completed shell with `exit != 0` too; a tool that is
/// neither completed nor in error (pending, running) is `None`, so it is not shown as a failure.
fn tool_step(
    part: &RawPart,
    time: i64,
    id: String,
    end_ms: Option<i64>,
    directory: Option<&str>,
) -> Option<Step> {
    let name = part.name.as_deref().unwrap_or("");
    let state = part.state.as_ref();
    let status = state.and_then(|s| s.status.as_deref());
    let is_error = status == Some("error");
    let exit = state.and_then(|s| s.metadata.as_ref()).and_then(|m| m.exit);
    let ok = match (status, name) {
        (Some("error"), _) => Some(false),
        (Some("completed"), "shell") => Some(exit == Some(0)),
        (Some("completed"), _) => Some(true),
        _ => None,
    };

    let input = state.and_then(|s| s.input.as_ref());
    let content = state.and_then(|s| s.content.as_ref());
    match name {
        "read" => {
            let path = input.and_then(|i| i.path.as_deref()).map(|p| shorten(p, directory));
            let detail = if is_error {
                content
                    .and_then(|c| c.first())
                    .and_then(|c| c.text.as_deref())
                    .map(|s| first_line(s).to_string())
            } else {
                None
            };
            Some(step(time, StepKind::Read, path.unwrap_or_default(), ok, detail, id, end_ms))
        }
        "edit" => {
            let path =
                input.and_then(|i| i.path.as_deref()).map(|p| shorten(p, directory)).unwrap_or_default();
            let detail = state
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.files.as_ref())
                .filter(|files| files.iter().any(|f| f.patch.is_some()))
                .map(|files| patch_delta(files.iter().filter_map(|f| f.patch.as_deref())));
            Some(step(time, StepKind::Edit, path, ok, detail, id, end_ms))
        }
        "write" => {
            let path =
                input.and_then(|i| i.path.as_deref()).map(|p| shorten(p, directory)).unwrap_or_default();
            let lines = input
                .and_then(|i| i.content.as_ref())
                .and_then(|v| v.as_str())
                .map(str::lines)
                .map(|l| l.count())
                .unwrap_or(0);
            Some(step(time, StepKind::Write, path, ok, Some(format!("{lines} lines")), id, end_ms))
        }
        "shell" => {
            let command = strip_cd_prefix(
                first_line(input.and_then(|i| i.command.as_deref()).unwrap_or("")),
                directory,
            );
            let detail = content
                .and_then(|c| c.first())
                .and_then(|c| c.text.as_deref())
                .map(|s| first_line(s).to_string());
            Some(step(time, StepKind::Shell, command, ok, detail, id, end_ms))
        }
        "grep" => {
            let path = input.and_then(|i| i.path.as_deref()).unwrap_or("");
            let summary = format!(
                "{} in {}",
                input.and_then(|i| i.pattern.as_deref()).unwrap_or(""),
                shorten(path, directory)
            );
            Some(step(time, StepKind::Search, summary, ok, None, id, end_ms))
        }
        other => Some(step(time, StepKind::Other(other.to_string()), name.to_string(), ok, None, id, end_ms)),
    }
}

/// Drops a leading `cd <directory> && ` when `<directory>` is exactly `directory` (the model prefixes
/// every command with it); any other `cd` stays.
fn strip_cd_prefix(command: &str, directory: Option<&str>) -> String {
    match directory {
        Some(dir) => command
            .strip_prefix(&format!("cd {} && ", dir.trim_end_matches('/')))
            .unwrap_or(command)
            .to_string(),
        None => command.to_string(),
    }
}

fn step(
    time_ms: i64,
    kind: StepKind,
    summary: String,
    ok: Option<bool>,
    detail: Option<String>,
    id: String,
    end_ms: Option<i64>,
) -> Step {
    Step { time_ms, kind, id, end_ms, summary, ok, detail }
}
