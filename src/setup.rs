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
use crate::http::{self, Client};
use crate::settings::Settings;
use crate::util::short_model_name;

/// How the dialogue checks a server: `HttpProbe` for real, a fake in the tests.
pub trait Probe {
    /// `GET <url>/health` without key; `Ok` on a 2xx answer.
    fn health(&self, url: &str) -> Result<()>;
    /// `GET <url>/v1/models` with `key`, parsed by `collect::llama::parse_models`.
    fn models(&self, url: &str, key: Option<&str>) -> Result<Vec<ModelInfo>>;
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
    writeln!(out, "llamatop setup")?;
    writeln!(out)?;

    // 1. The URL, with a health check and up to three refusals.
    let default_url = current.llama.url.clone().unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let mut refused = 0usize;
    let url = loop {
        let mut line = String::new();
        writeln!(out, "llama.cpp server URL [<{}>]: ", default_url)?;
        read_line(input, &mut line)?;
        let entered = line.trim().to_string();
        let url = if entered.is_empty() { default_url.clone() } else { normalize_url(&entered) };

        write!(out, "checking {}/health … ", url)?;
        match probe.health(&url) {
            Ok(()) => {
                writeln!(out, "ok")?;
                break url;
            }
            Err(err) => {
                writeln!(out, "unreachable: {}", err)?;
                let mut answer = String::new();
                writeln!(out, "use it anyway? [y/N] ")?;
                read_line(input, &mut answer)?;
                if answer.trim().eq_ignore_ascii_case("yes") || answer.trim() == "y" {
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
    let prompt = match &default_key {
        Some(_) => "API key (empty: keep the current one): ".to_string(),
        None => "API key (empty: none): ".to_string(),
    };
    let mut key_refused = 0usize;
    let api_key = loop {
        let entered = secret(&prompt)?;
        let key = if entered.is_empty() { default_key.clone() } else { Some(entered) };
        write!(out, "checking models … ")?;
        match probe.models(&url, key.as_deref()) {
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
    let mut line = String::new();
    writeln!(out, "log command [<{}>]: ", default_log.as_deref().unwrap_or(""))?;
    let mut eof = false;
    line.clear();
    if input.read_line(&mut line)? == 0 {
        eof = true;
    }
    let log_cmd = if eof || line.trim().is_empty() { default_log } else { Some(line.trim().to_string()) };

    Ok(Settings {
        llama: crate::settings::LlamaSettings {
            url: Some(url),
            api_key,
            api_key_file: current.llama.api_key_file.clone(),
            log_cmd,
        },
    })
}

/// The default log command for a URL: the flag's `journalctl` on loopback, else `ssh <host> journalctl …`.
fn default_log_for(url: &str) -> Option<String> {
    const JOURNALCTL: &str = "journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise";
    if http::is_loopback_url(url) {
        return Some(JOURNALCTL.to_string());
    }
    host_of(url).map(|host| format!("ssh {} {}", host, JOURNALCTL))
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
                KeyCode::Enter => return Ok(result),
                KeyCode::Backspace => {
                    result.pop();
                }
                KeyCode::Char(c) => {
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
    let mut input = std::io::stdin().lock();
    let mut out = std::io::stdout().lock();
    let settings = wizard(&mut input, &mut out, &mut read_secret, &HttpProbe, current)?;
    crate::settings::save(path, &settings).context("save configuration file")?;
    writeln!(out, "saved {}", path.display())?;
    Ok(settings)
}

/// Reads one line into `line`. At end of input (`Ok(0)`) it is an error, so a test can never loop forever.
fn read_line(input: &mut dyn BufRead, line: &mut String) -> Result<()> {
    line.clear();
    let read = input.read_line(line).context("read input")?;
    if read == 0 {
        return Err(anyhow!("setup aborted"));
    }
    Ok(())
}

/// The host of a URL, for the remote log command.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    rest.split([':', '/']).next().map(str::to_string)
}
