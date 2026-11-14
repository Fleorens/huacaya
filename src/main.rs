use std::io::IsTerminal;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, FromArgMatches, parser::ValueSource};

use huacaya::app::run::run;
use huacaya::app::{Screen, demo, demo_agents};
use huacaya::config::{Cli, Command};
use huacaya::settings::{self, Settings};
use huacaya::setup;
use huacaya::ui::snapshot::{self, parse_size};
use huacaya::ui::theme::{ColorMode, Theme};

fn main() -> Result<()> {
    let matches = Cli::command().get_matches();
    let mut cli = Cli::from_arg_matches(&matches)?;
    let path = cli.config.clone().or_else(settings::default_path);
    // The project was called llamatop until 0.4: its configuration file is taken over once.
    if cli.config.is_none()
        && let Some(path) = path.as_deref()
        && let Some(legacy) = settings::migrate_legacy(path)?
    {
        eprintln!("huacaya: configuration copied from {} to {}", legacy.display(), path.display());
    }
    let current = match path.as_deref() {
        Some(path) => match settings::load(path) {
            Ok(Some(settings)) => {
                cli.apply_settings(&matches, &settings);
                Some(settings)
            }
            Ok(None) => None,
            Err(err) => return Err(err.context(format!("configuration file {}", path.display()))),
        },
        None => None,
    };
    // `--agent 192.168.1.203:9099` without a scheme would never connect: same normalisation as setup's.
    cli.agent = cli.agent.as_deref().map(setup::normalize_url);
    cli.url = setup::normalize_url(&cli.url);

    // `huacaya setup`: run the dialogue and save, then exit.
    if let Some(Command::Setup) = cli.command {
        let path = path.as_deref().ok_or_else(|| anyhow!("no configuration file path"))?;
        setup::run_with_key(path, &current.unwrap_or_default(), cli.api_key())?;
        return Ok(());
    }

    // `huacaya agent`: publish this machine's panels over HTTP.
    if let Some(Command::Agent { listen, token_file, no_auth }) = cli.command {
        let token = match token_file {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading token file {}", path.display()))?;
                match text.lines().next().map(str::trim).filter(|line| !line.is_empty()) {
                    Some(token) => Some(token.to_string()),
                    None => return Err(anyhow!("token file {} is empty", path.display())),
                }
            }
            None => None,
        };
        huacaya::agent::server::check_token_policy(&listen, token.as_deref(), no_auth)?;
        let config = huacaya::agent::server::AgentConfig {
            listen,
            token,
            sysroot: cli.sysroot.clone(),
            mounts: cli.mounts.clone(),
            log_cmd: cli.log_cmd.clone(),
            interval: Duration::from_millis(cli.interval_ms),
        };
        huacaya::agent::server::run(config)?;
        return Ok(());
    }

    if cli.dump {
        println!("{}", huacaya::dump::run(&cli)?);
        return Ok(());
    }
    if let Some(size) = &cli.snapshot {
        let (w, h) = parse_size(size).ok_or_else(|| anyhow!("invalid --snapshot size, expected WxH"))?;
        let mut app = demo::app(cli.demo_state);
        app.screen = Screen::ALL[(cli.screen - 1) as usize];
        // Screen 5 (Agents): a simulated OpenCode session, so the map comes alive in `--snapshot` too.
        if app.screen == Screen::Agents {
            app.opencode = Some(demo_agents::replay(demo_agents::DEMO_START_MS, demo_agents::SNAPSHOT_T));
            app.now_ms = demo_agents::DEMO_START_MS + 24_300;
            app.utc_offset_s = 0;
        }
        print!("{}", snapshot::snapshot(&app, &Theme::new(ColorMode::detect()), w, h, cli.ansi));
        return Ok(());
    }

    // First run: no configuration file yet, no URL by flag or environment, demo off, both a terminal.
    let url_is_default = matches.value_source("url") == Some(ValueSource::DefaultValue);
    if current.is_none()
        && path.is_some()
        && url_is_default
        && !cli.demo
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
    {
        let path = path.ok_or_else(|| anyhow!("no configuration file path"))?;
        println!("no configuration yet: {}", path.display());
        let saved = setup::run_with_key(&path, &Settings::default(), cli.api_key())?;
        cli.apply_settings(&matches, &saved);
    }

    run(&cli)
}
