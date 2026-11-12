//! Command line and environment.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

use crate::settings::Settings;

/// Terminal dashboard for a llama.cpp server and the machine that runs it.
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
pub struct Cli {
    /// Base URL of the llama.cpp server (router or single model).
    #[arg(long, env = "LLAMATOP_URL", default_value = "http://127.0.0.1:8080")]
    pub url: String,

    /// File holding the API key (first line). `LLAMA_API_KEY` wins when both are set.
    #[arg(long, env = "LLAMATOP_API_KEY_FILE")]
    pub api_key_file: Option<PathBuf>,

    /// Configuration file; default `~/.config/llamatop/config.toml`.
    #[arg(long, env = "LLAMATOP_CONFIG")]
    pub config: Option<PathBuf>,

    /// Root under which `proc/` and `sys/` are read. Tests point it at `tests/fixtures/sysroot`.
    #[arg(long, default_value = "/")]
    pub sysroot: PathBuf,

    /// Mount points whose usage is shown. A path that does not exist is skipped.
    #[arg(long = "mount", default_values = ["/", "/srv/models", "/srv/data"])]
    pub mounts: Vec<PathBuf>,

    /// Command whose standard output is followed as the server log, split on spaces.
    #[arg(
        long,
        env = "LLAMATOP_LOG_CMD",
        default_value = "journalctl -u llama-server -f -n 2000 --no-pager -q -o short-iso-precise"
    )]
    pub log_cmd: String,

    /// Refresh period of the system and server panels, in milliseconds.
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(200..=10_000))]
    pub interval_ms: u64,

    /// Print one JSON snapshot of every data source on standard output, then exit.
    #[arg(long)]
    pub dump: bool,

    /// Show generated data instead of the real server and machine.
    #[arg(long)]
    pub demo: bool,
    /// With `--snapshot`: the demo state to render.
    #[arg(long, value_enum, default_value = "generating")]
    pub demo_state: crate::app::demo::DemoState,
    /// Render one frame of the demo at `WxH` (e.g. `140x40`) on standard output, then exit.
    #[arg(long, value_name = "WxH")]
    pub snapshot: Option<String>,
    /// With `--snapshot`: keep the colours as ANSI escape sequences.
    #[arg(long)]
    pub ansi: bool,
    /// With `--snapshot`: the screen to render, 1–4 (Overview, Models, Logs, System).
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
    pub screen: u8,
    /// The key from the configuration file.
    #[arg(skip)]
    pub api_key_inline: Option<String>,

    /// Subcommand: the first-run setup dialogue.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// A subcommand of `llamatop`.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
    /// Ask for the server address and key, check them, save the configuration file.
    Setup,
    /// Publish this machine's panels (system and log) for remote llamatop dashboards.
    Agent {
        /// Address and port to listen on.
        #[arg(long, default_value = "0.0.0.0:9099")]
        listen: SocketAddr,
        /// File holding the token clients must send (first line).
        #[arg(long, env = "LLAMATOP_AGENT_TOKEN_FILE")]
        token_file: Option<PathBuf>,
        /// Accept clients without a token (only sensible on a trusted network).
        #[arg(long)]
        no_auth: bool,
    },
}

impl Cli {
    /// The API key: `LLAMA_API_KEY`, else the first line of `--api-key-file`, else `api_key_inline`, else
    /// none. The key is never printed, logged, or put in an error message.
    pub fn api_key(&self) -> Option<String> {
        if let Ok(key) = std::env::var("LLAMA_API_KEY")
            && !key.trim().is_empty()
        {
            return Some(key.trim().to_string());
        }
        if let Some(path) = self.api_key_file.as_ref()
            && let Ok(text) = std::fs::read_to_string(path)
            && let Some(key) = text.lines().next()
            && !key.trim().is_empty()
        {
            return Some(key.trim().to_string());
        }
        // Same hygiene as the other sources: `api_key = " sk-x "` or `""` must not reach the header as is.
        self.api_key_inline.as_deref().map(str::trim).filter(|key| !key.is_empty()).map(str::to_string)
    }

    /// Fills, from `settings`, every value the user did not give by flag or environment variable: `url` and
    /// `log_cmd` when `matches.value_source(id)` is `Some(ValueSource::DefaultValue)`; `api_key_file` when it is
    /// `None`; `api_key_inline` always (it is only used last). Values given by flag or environment win.
    pub fn apply_settings(&mut self, matches: &clap::ArgMatches, settings: &Settings) {
        let is_default = |id: &str| matches.value_source(id) == Some(clap::parser::ValueSource::DefaultValue);

        if is_default("url")
            && let Some(url) = settings.llama.url.clone()
        {
            self.url = url;
        }
        if is_default("log_cmd")
            && let Some(log_cmd) = settings.llama.log_cmd.clone()
        {
            self.log_cmd = log_cmd;
        }
        if matches.value_source("api_key_file").is_none()
            && let Some(api_key_file) = settings.llama.api_key_file.clone()
        {
            self.api_key_file = Some(api_key_file);
        }
        self.api_key_inline = settings.llama.api_key.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cli = Cli::parse_from(["llamatop"]);
        assert_eq!(cli.url, "http://127.0.0.1:8080");
        assert_eq!(cli.sysroot, PathBuf::from("/"));
        assert_eq!(cli.mounts.len(), 3);
        assert_eq!(cli.interval_ms, 1000);
        assert!(!cli.dump);
    }

    #[test]
    fn interval_is_bounded() {
        assert!(Cli::try_parse_from(["llamatop", "--interval-ms", "50"]).is_err());
    }
}
