//! The first-run setup and `llamatop setup`: a plain question-and-answer dialogue on the terminal,
//! before the dashboard starts. It asks for the server URL and API key, checks them against the server,
//! and returns the `Settings` to save (tasks/T23, tasks/T24).
//!
//! The dialogue is `wizard`, written over any input and output, so the tests exercise it with a cursor
//! and a fake `Probe`. `read_secret` reads a line without echo; `run` wires them to the terminal, the
//! real probe and the configuration file.

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

use crate::collect::llama::{ModelInfo, ModelState, parse_models};
use crate::collect::sampler::SystemSnapshot;
use crate::http::{self, Client};
use crate::settings::Settings;
use crate::util::short_model_name;

use serde_json;

/// How the dialogue checks a server: `HttpProbe` for real, a fake in the tests.
pub trait Probe {
    /// `GET <url>/health` without key; `Ok` on a 2xx answer.
    fn health(&self, url: &str) -> Result<()>;
    /// `GET <url>/v1/models` with `key`, parsed by `collect::llama::parse_models`.
    fn models(&self, url: &str, key: Option<&str>) -> Result<Vec<ModelInfo>>;
    /// `GET <url>/v1/system` with `key`: the agent's hostname (or an error carrying `HTTP 401` for a
    /// refused token).
    fn agent(&self, url: &str, token: Option<&str>) -> Result<String>;
}

/// `Probe` over `http::Client`, 3 s timeout.
pub struct HttpProbe;

impl Probe for HttpProbe {
    fn health(&self, url: &str) -> Result<()> {
        let client = Client::new(url, None, Duration::from_secs(3));
        client.get("/health", &[]).map(|_| ())
    }

    fn models(&self, url: &str, key: Option<&str>) -> Result<Vec<ModelInfo>> {
        let client = Client::new(url, key.map(str::to_string), Duration::from_secs(3));
        let body = client.get("/v1/models", &[])?;
        parse_models(&body)
    }

    fn agent(&self, url: &str, token: Option<&str>) -> Result<String> {
        let client = Client::new(url, token.map(str::to_string), Duration::from_secs(3));
        let body = client.get("/v1/system", &[])?;
        let snapshot: SystemSnapshot = serde_json::from_str(&body).context("parse /v1/system")?;
        snapshot.hostname.ok_or_else(|| anyhow!("no hostname in /v1/system"))
    }
}

/// The dialogue (below) over any input and output. `secret` reads the API key without echo.
/// Returns the settings to save; `current` gives the defaults shown in brackets.
pub fn wizard(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    secret: &mut dyn FnMut(&str) -> Result<String>,
    probe: &dyn Probe,
    current: &Settings,
) -> Result<Settings> {
    dialogue(input, out, secret, probe, current, None)
}

/// The dialogue. `effective_key` is the key the dashboard would use (environment, key file): an empty answer
/// with no `current.llama.api_key` checks the server with it, without saving it in `api_key`.
fn dialogue(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    secret: &mut dyn FnMut(&str) -> Result<String>,
    probe: &dyn Probe,
    current: &Settings,
    effective_key: Option<String>,
) -> Result<Settings> {
    let mut input = Input { inner: input, ended: false };
    writeln!(out, "llamatop setup")?;
    writeln!(out)?;

    // 1. The URL, with a health check and up to three refusals.
    let default_url = current.llama.url.clone().unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let mut refused = 0usize;
    let url = loop {
        write!(out, "llama.cpp server URL [{}]: ", default_url)?;
        out.flush()?;
        let entered = input.answer()?;
        let url = if entered.is_empty() { default_url.clone() } else { normalize_url(&entered) };

        write!(out, "checking {}/health … ", url)?;
        out.flush()?;
        match probe.health(&url) {
            Ok(()) => {
                writeln!(out, "ok")?;
                break url;
            }
            Err(err) => {
                writeln!(out, "unreachable: {}", err)?;
                write!(out, "use it anyway? [y/N] ")?;
                out.flush()?;
                let answer = input.answer()?;
                if answer.eq_ignore_ascii_case("yes") || answer.eq_ignore_ascii_case("y") {
                    break url;
                }
                refused += 1;
                if refused >= 3 {
                    return Err(anyhow!("setup aborted"));
                }
            }
        }
    };

    // 2. The API key, checked against `/v1/models`, up to three refusals.
    let default_key = current.llama.api_key.clone();
    // Only for the check: a key from the environment or a key file is never copied into `api_key`.
    let probe_default = default_key.clone().or(effective_key).or_else(|| {
        let path = current.llama.api_key_file.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        let key = text.lines().next()?.trim().to_string();
        (!key.is_empty()).then_some(key)
    });
    let prompt = match &default_key {
        Some(_) => "API key (empty: keep the current one): ".to_string(),
        None => "API key (empty: none): ".to_string(),
    };
    let mut key_refused = 0usize;
    let api_key = loop {
        let entered = secret(&prompt)?;
        let (key, probe_key) = if entered.is_empty() {
            (default_key.clone(), probe_default.clone())
        } else {
            (Some(entered.clone()), Some(entered))
        };
        write!(out, "checking models … ")?;
        out.flush()?;
        match probe.models(&url, probe_key.as_deref()) {
            Ok(models) => {
                let loaded: Vec<&ModelInfo> =
                    models.iter().filter(|m| m.state == ModelState::Loaded).collect();
                write!(out, "{} models, {} loaded", models.len(), loaded.len())?;
                if !loaded.is_empty() {
                    let names: Vec<String> = loaded.iter().map(|m| short_model_name(&m.id).0).collect();
                    write!(out, ": {}", names.join(", "))?;
                }
                writeln!(out)?;
                break key;
            }
            Err(err) => {
                if format!("{}", err).contains("HTTP 401") {
                    writeln!(out, "the server refused the key")?;
                    key_refused += 1;
                    if key_refused >= 3 {
                        return Err(anyhow!("setup aborted"));
                    }
                    continue;
                }
                writeln!(out, "could not list the models: {}", err)?;
                break key;
            }
        }
    };

    // 3. The log command: the value from the file, else a local `journalctl` or an `ssh` to the host.
    let default_log = current.llama.log_cmd.clone().or_else(|| default_log_for(&url));
    write!(out, "log command [{}]: ", default_log.as_deref().unwrap_or(""))?;
    out.flush()?;
    let entered = input.answer()?;
    let log_cmd = if entered.is_empty() { default_log } else { Some(entered) };

    // 4. The agent: a URL on the server, whose machine panels and log replace this machine's. Empty is
    // none (the log command above keeps its meaning); the token is checked against `/v1/system`.
    let default_agent_url = current.agent.url.clone().unwrap_or_default();
    write!(out, "llamatop agent URL (empty: none) [{}]: ", default_agent_url)?;
    out.flush()?;
    let entered_agent = input.answer()?;
    let (agent_url, agent_token) = if entered_agent.is_empty() {
        (None, None)
    } else {
        let url = normalize_url(&entered_agent);
        let token_prompt = match &current.agent.token {
            Some(_) => "agent token (empty: keep the current one): ".to_string(),
            None => "agent token (empty: none): ".to_string(),
        };
        let mut refused = 0usize;
        let agent_token = loop {
            let entered_token = secret(&token_prompt)?;
            let token =
                if entered_token.is_empty() { current.agent.token.clone() } else { Some(entered_token) };
            write!(out, "checking agent … ")?;
            out.flush()?;
            match probe.agent(&url, token.as_deref()) {
                Ok(hostname) => {
                    writeln!(out, "ok, {hostname}")?;
                    break token;
                }
                Err(err) => {
                    if format!("{err}").contains("HTTP 401") {
                        writeln!(out, "the agent refused the token")?;
                        refused += 1;
                        if refused >= 3 {
                            return Err(anyhow!("setup aborted"));
                        }
                        continue;
                    }
                    writeln!(out, "unreachable: {}", err)?;
                    break token;
                }
            }
        };
        (Some(url), agent_token)
    };

    Ok(Settings {
        llama: crate::settings::LlamaSettings {
            url: Some(url),
            api_key,
            api_key_file: current.llama.api_key_file.clone(),
            log_cmd,
        },
        agent: crate::settings::AgentSettings {
            url: agent_url,
            token: agent_token,
            token_file: current.agent.token_file.clone(),
        },
    })
}

/// The default log command for a URL: the flag's `journalctl` on loopback, else `ssh <host> journalctl …`.
fn default_log_for(url: &str) -> Option<String> {
    const JOURNALCTL: &str = "journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise";
    if http::is_loopback_url(url) {
        return Some(JOURNALCTL.to_string());
    }
    http::host_of(url)
        .map(|host| host.trim_start_matches('[').trim_end_matches(']').to_string())
        .map(|host| format!("ssh {} {}", host, JOURNALCTL))
}

/// `url` with `http://` added when it has no scheme and without trailing `/`; surrounding spaces trimmed.
pub fn normalize_url(url: &str) -> String {
    let mut url = url.trim().to_string();
    if !url.contains("://") {
        url = format!("http://{}", url);
    }
    url = url.trim_end_matches('/').to_string();
    url
}

/// Reads one line from the terminal without echo: crossterm raw mode, `Enter` ends, `Backspace` erases,
/// `Ctrl-C` or `Esc` is an error `setup cancelled`. The prompt is written first; raw mode is disabled again on
/// every path (a guard whose `Drop` calls `disable_raw_mode`).
pub fn read_secret(prompt: &str) -> Result<String> {
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
    use std::io::{Write, stdout};

    let mut out = stdout().lock();
    write!(out, "{}", prompt)?;
    out.flush()?;

    // Raw mode is disabled on every return path.
    struct RawGuard;
    impl Drop for RawGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
        }
    }
    let _guard = RawGuard;
    enable_raw_mode().context("enter raw mode")?;

    let mut result = String::new();
    loop {
        if let Event::Key(key) = event::read().context("read key")? {
            match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(anyhow!("setup cancelled"));
                }
                KeyCode::Esc => return Err(anyhow!("setup cancelled")),
                KeyCode::Enter => {
                    // Raw mode echoes nothing, not even the line break.
                    write!(out, "\r\n")?;
                    out.flush()?;
                    return Ok(result);
                }
                KeyCode::Backspace => {
                    result.pop();
                }
                // Ctrl-U, Alt-x… are shortcuts, not characters of the key: an invisible stray letter would
                // make the key wrong with no way to see why.
                KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    result.push(c);
                }
                _ => {}
            }
        }
    }
}

/// `wizard` on stdin / stdout with `read_secret` and `HttpProbe`, then `settings::save(path, …)`, then prints
/// `saved <path>`. Returns the saved settings.
pub fn run(path: &Path, current: &Settings) -> Result<Settings> {
    run_with_key(path, current, None)
}

/// `run`, where an empty key answer checks the server with `effective_key` (the key `Cli::api_key` resolved
/// from the environment or a key file) without saving it.
pub fn run_with_key(path: &Path, current: &Settings, effective_key: Option<String>) -> Result<Settings> {
    let mut input = std::io::stdin().lock();
    let mut out = std::io::stdout().lock();
    let settings = dialogue(&mut input, &mut out, &mut read_secret, &HttpProbe, current, effective_key)?;
    crate::settings::save(path, &settings).context("save configuration file")?;
    writeln!(out, "saved {}", path.display())?;
    Ok(settings)
}

/// The answers. The first end of input reads as an empty answer; asking again after it is `setup aborted`,
/// so a test can never loop forever.
struct Input<'a> {
    inner: &'a mut dyn BufRead,
    ended: bool,
}

impl Input<'_> {
    /// The next answer, trimmed.
    fn answer(&mut self) -> Result<String> {
        if self.ended {
            return Err(anyhow!("setup aborted"));
        }
        let mut line = String::new();
        if self.inner.read_line(&mut line).context("read input")? == 0 {
            self.ended = true;
        }
        Ok(line.trim().to_string())
    }
}
