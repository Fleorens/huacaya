//! The OpenCode server (`opencode serve`) client: its sessions and, for the followed session, its
//! timeline of steps (`tasks/T34`), read through any `llama_poll::Fetch` so tests need no network.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::collect::llama_poll::Fetch;

/// One OpenCode session.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub job: Option<u32>,
    pub model: Option<String>,
    pub agent: Option<String>,
    /// `succeeded`, `failed`, …; `None` while it runs.
    pub outcome: Option<String>,
    pub running: bool,
    pub created_ms: i64,
    pub idle_ms: Option<i64>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub directory: Option<String>,
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StepKind {
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
    pub time_ms: i64,
    pub kind: StepKind,
    pub summary: String,
    /// `Some(false)` for a failed tool, `Some(true)` for a completed one, `None` for reasoning/text.
    pub ok: Option<bool>,
    pub detail: Option<String>,
}

/// What the Agents screen gets every round.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenCodeSnapshot {
    pub sessions: Vec<Session>,
    pub focus: Option<String>,
    pub steps: Vec<Step>,
    /// The last error (`unreachable: …`, `HTTP 401` …), never containing the password.
    pub error: Option<String>,
}

pub fn job_number(title: &str) -> Option<u32> {
    let digits = title.trim().strip_prefix('#')?;
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    digits[..end].parse().ok()
}
/// Shortens `path` by removing a leading `directory` (and the following `/`), when it matches.
fn shorten(path: &str, directory: Option<&str>) -> String {
    match directory {
        Some(dir) if path.starts_with(dir) && path.len() > dir.len() => {
            path[dir.len()..].trim_start_matches('/').to_string()
        }
        _ => path.to_string(),
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
/// The `+<added> −<removed>` of a unified diff patch: lines starting with `+` / `-`, not `+++` / `---`.
fn patch_delta(patch: &str) -> String {
    let added = patch.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
    let removed = patch.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
    format!("+{added} −{removed}")
}
#[derive(Deserialize)]
struct RawSessions {
    data: Vec<RawSession>,
}

#[derive(Deserialize)]
struct RawSession {
    id: String,
    title: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    model: Option<RawModel>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    tokens: Option<RawTokens>,
    #[serde(default)]
    time: Option<RawTime>,
    #[serde(default)]
    location: Option<RawLocation>,
}
#[derive(Deserialize)]
struct RawModel {
    #[serde(default)]
    id: Option<String>,
}
#[derive(Deserialize, Default)]
struct RawTokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache: RawCache,
}
#[derive(Deserialize, Default)]
struct RawCache {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    #[allow(dead_code)]
    write: u64,
}
#[derive(Deserialize)]
struct RawTime {
    #[serde(default)]
    created: i64,
    #[serde(default)]
    idle: Option<i64>,
}
#[derive(Deserialize)]
struct RawLocation {
    #[serde(default)]
    directory: Option<String>,
}
#[derive(Deserialize)]
struct RawMessages {
    data: Vec<RawMessage>,
}
#[derive(Deserialize)]
struct RawMessage {
    #[serde(rename = "type", default)]
    r#type: Option<String>,
    #[serde(default)]
    time: Option<RawTime>,
    #[serde(default)]
    content: Vec<RawPart>,
}
#[derive(Deserialize)]
struct RawPart {
    #[serde(rename = "type", default)]
    r#type: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    time: Option<RawTime>,
    #[serde(default)]
    state: Option<RawPartState>,
}
#[derive(Deserialize)]
struct RawPartState {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    input: Option<RawToolInput>,
    #[serde(default)]
    content: Option<Vec<RawPartContent>>,
    #[serde(default)]
    metadata: Option<RawMetadata>,
}
#[derive(Deserialize)]
struct RawToolInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    content: Option<String>,
}
#[derive(Deserialize)]
struct RawPartContent {
    #[serde(default)]
    text: Option<String>,
}
#[derive(Deserialize)]
struct RawMetadata {
    #[serde(default)]
    exit: Option<i64>,
    #[serde(default)]
    files: Option<Vec<RawFile>>,
}
#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    patch: Option<String>,
}

/// Parses `GET /api/session`, newest first as the server returns it.
pub fn parse_sessions(json: &str) -> Result<Vec<Session>> {
    let raw: RawSessions = serde_json::from_str(json).context("parse sessions")?;
    Ok(raw.data.into_iter().map(session_from).collect())
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

/// Parses `GET /api/session/<id>/message` into steps, oldest first (the API is newest first). Only
/// `assistant` messages give steps, their parts in order; `directory` shortens paths.
pub fn parse_steps(json: &str, directory: Option<&str>) -> Result<Vec<Step>> {
    let messages: RawMessages = serde_json::from_str(json).context("parse messages")?;
    let mut steps: Vec<Step> = Vec::new();
    for msg in messages.data.iter().rev() {
        if msg.r#type.as_deref() != Some("assistant") {
            continue;
        }
        let message_time = msg.time.as_ref().map(|t| t.created).unwrap_or(0);
        for part in &msg.content {
            if let Some(step) = part_step(part, message_time, directory) {
                steps.push(step);
            }
        }
    }
    Ok(steps)
}

fn part_step(part: &RawPart, message_time: i64, directory: Option<&str>) -> Option<Step> {
    let time = part.time.as_ref().map(|t| t.created).unwrap_or(message_time);
    match part.r#type.as_deref() {
        Some("reasoning") => Some(Step {
            time_ms: time,
            kind: StepKind::Think,
            summary: summary_text(part.text.as_deref()),
            ok: None,
            detail: None,
        }),
        Some("text") => Some(Step {
            time_ms: time,
            kind: StepKind::Say,
            summary: summary_text(part.text.as_deref()),
            ok: None,
            detail: None,
        }),
        Some("tool") => tool_step(part, time, directory),
        _ => None,
    }
}

/// `status == "error"` is `ok: Some(false)`; a shell with `exit != 0` too.
fn tool_step(part: &RawPart, time: i64, directory: Option<&str>) -> Option<Step> {
    let name = part.name.as_deref().unwrap_or("");
    let state = part.state.as_ref();
    let is_error = state.and_then(|s| s.status.as_deref()) == Some("error");
    let ok = match (name, is_error) {
        (_, true) => Some(false),
        ("shell", false) => match state.and_then(|s| s.metadata.as_ref()).and_then(|m| m.exit) {
            Some(0) => Some(true),
            _ => Some(false),
        },
        _ => Some(true),
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
            Some(step(time, StepKind::Read, path.unwrap_or_default(), ok, detail))
        }
        "edit" => {
            let path =
                input.and_then(|i| i.path.as_deref()).map(|p| shorten(p, directory)).unwrap_or_default();
            let detail = state
                .and_then(|s| s.metadata.as_ref())
                .and_then(|m| m.files.as_ref())
                .and_then(|files| files.first())
                .and_then(|f| f.patch.as_deref())
                .map(patch_delta);
            Some(step(time, StepKind::Edit, path, ok, detail))
        }
        "write" => {
            let path =
                input.and_then(|i| i.path.as_deref()).map(|p| shorten(p, directory)).unwrap_or_default();
            let lines =
                input.and_then(|i| i.content.as_deref()).map(str::lines).map(|l| l.count()).unwrap_or(0);
            Some(step(time, StepKind::Write, path, ok, Some(format!("{lines} lines"))))
        }
        "shell" => {
            let command = input.and_then(|i| i.command.as_deref()).unwrap_or("");
            let detail = content
                .and_then(|c| c.first())
                .and_then(|c| c.text.as_deref())
                .map(|s| first_line(s).to_string());
            Some(step(time, StepKind::Shell, first_line(command).to_string(), ok, detail))
        }
        "grep" => {
            let path = input.and_then(|i| i.path.as_deref()).unwrap_or("");
            let summary = format!(
                "{} in {}",
                input.and_then(|i| i.pattern.as_deref()).unwrap_or(""),
                shorten(path, directory)
            );
            Some(step(time, StepKind::Search, summary, ok, None))
        }
        other => Some(step(time, StepKind::Other(other.to_string()), name.to_string(), ok, None)),
    }
}

fn step(time_ms: i64, kind: StepKind, summary: String, ok: Option<bool>, detail: Option<String>) -> Step {
    Step { time_ms, kind, summary, ok, detail }
}

pub struct OpenCodePoller<F: Fetch> {
    fetch: F,
}

impl<F: Fetch> OpenCodePoller<F> {
    pub fn new(fetch: F) -> Self {
        Self { fetch }
    }

    pub fn poll(&mut self, focus: Option<&str>) -> OpenCodeSnapshot {
        let empty = |error: String| OpenCodeSnapshot { error: Some(error), ..Default::default() };

        let sessions = match self.fetch.get("/api/session", &[]) {
            Ok(body) => match parse_sessions(&body) {
                Ok(sessions) => sessions,
                Err(e) => return empty(format!("unreachable: {e:#}")),
            },
            Err(e) => return empty(format!("unreachable: {e:#}")),
        };

        let focus = match focus {
            Some(id) if sessions.iter().any(|s| s.id == id) => Some(id.to_string()),
            _ => sessions.iter().find(|s| s.running).or_else(|| sessions.first()).map(|s| s.id.clone()),
        };

        let steps = match focus.as_deref() {
            Some(id) => {
                let directory = sessions.iter().find(|s| s.id == id).and_then(|s| s.directory.clone());
                match self.fetch.get(&format!("/api/session/{id}/message"), &[]) {
                    Ok(body) => {
                        parse_steps(&body, directory.as_deref()).map_err(|e| format!("unreachable: {e:#}"))
                    }
                    Err(e) => return empty(format!("unreachable: {e:#}")),
                }
            }
            None => Ok(Vec::new()),
        };
        let steps = match steps {
            Ok(steps) => steps,
            Err(e) => return empty(e),
        };

        OpenCodeSnapshot { sessions, focus, steps, error: None }
    }
}
