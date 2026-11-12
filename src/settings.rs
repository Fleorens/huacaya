//! The optional configuration file (`~/.config/llamatop/config.toml`, mode 600; tasks/T23).
//!
//! Flags and environment variables keep priority over the file. The file is written by the first-run
//! setup of T24 and read here so llamatop can run from any Linux machine, not only the server.

use std::fs::{self, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The content of the configuration file. Every field is optional: a missing one falls back to the flag's
/// default.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub llama: LlamaSettings,
    pub agent: AgentSettings,
}

/// The `[llama]` table.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlamaSettings {
    /// Base URL of the server, like `--url`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The API key itself (the file is created with mode 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// A file holding the key, like `--api-key-file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_file: Option<PathBuf>,
    /// Like `--log-cmd`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_cmd: Option<String>,
}

/// The `[agent]` table: the server's agent a remote dashboard reads.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentSettings {
    /// Base URL of the agent, like `--agent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The token itself (the file is created with mode 600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// A file holding the token, like `--agent-token-file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_file: Option<PathBuf>,
}

/// `$XDG_CONFIG_HOME/llamatop/config.toml` when `XDG_CONFIG_HOME` is set and not empty, else
/// `$HOME/.config/llamatop/config.toml`; `None` without either variable.
pub fn default_path() -> Option<PathBuf> {
    path_from(
        std::env::var("XDG_CONFIG_HOME").ok().filter(|value| !value.is_empty()).as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Same rule from given values, so it can be tested without touching the environment.
pub fn path_from(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_config_home {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            // An empty HOME would give a path relative to the current directory: no default then.
            let dir = home.filter(|dir| !dir.is_empty())?;
            Path::new(dir).join(".config")
        }
    };
    Some(base.join("llamatop").join("config.toml"))
}

/// Reads and parses `path`. A missing file is `Ok(None)`; an unreadable file or invalid TOML is an error
/// whose message names the file and never contains the key.
pub fn load(path: &Path) -> Result<Option<Settings>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(anyhow::anyhow!("configuration file {}: {}", path.display(), err)),
    };
    let settings: Settings = toml::from_str(&text).map_err(|err| {
        anyhow::anyhow!(
            "{}: line {}: {}",
            path.display(),
            line_of(&text, err.span().unwrap_or(0..0)),
            err.message()
        )
    })?;
    Ok(Some(settings))
}

/// Writes `settings` as TOML to `path`, creating the parent directories (mode 700 for a directory it
/// creates) and the file with mode 600 **from the start** (`OpenOptionsExt::mode`, then `set_permissions`
/// to 600 again in case the file already existed with wider rights). Replaces the whole file.
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .context(format!("configuration file {}", path.display()))?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    let body = toml::to_string_pretty(settings).context("serialize settings")?;
    file.write_all(body.as_bytes())?;
    Ok(())
}

/// Creates `path` and every missing ancestor, each with mode 700.
fn ensure_dir(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            ensure_dir(parent)?;
        }
        fs::create_dir(path).with_context(|| format!("create directory {}", path.display()))?;
        fs::set_permissions(path, Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The 1-based line number of `span` (a byte range into `text`), or 1 when the span is unknown.
fn line_of(text: &str, span: std::ops::Range<usize>) -> usize {
    let start = span.start.min(text.len());
    text[..start].bytes().filter(|&byte| byte == b'\n').count() + 1
}
