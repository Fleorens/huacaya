use std::io::IsTerminal;

use anyhow::{Result, anyhow};
use clap::{CommandFactory, FromArgMatches, parser::ValueSource};

use llamatop::app::run::run;
use llamatop::app::{Screen, demo};
use llamatop::config::{Cli, Command};
use llamatop::settings::{self, Settings};
use llamatop::setup;
use llamatop::ui::snapshot::{self, parse_size};
use llamatop::ui::theme::{ColorMode, Theme};

fn main() -> Result<()> {
    let matches = Cli::command().get_matches();
    let mut cli = Cli::from_arg_matches(&matches)?;
    let path = cli.config.clone().or_else(settings::default_path);
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

    // `llamatop setup`: run the dialogue and save, then exit.
    if let Some(Command::Setup) = cli.command {
        let path = path.as_deref().ok_or_else(|| anyhow!("no configuration file path"))?;
        setup::run_with_key(path, &current.unwrap_or_default(), cli.api_key())?;
        return Ok(());
    }

    if cli.dump {
        println!("{}", llamatop::dump::run(&cli)?);
        return Ok(());
    }
    if let Some(size) = &cli.snapshot {
        let (w, h) = parse_size(size).ok_or_else(|| anyhow!("invalid --snapshot size, expected WxH"))?;
        let mut app = demo::app(cli.demo_state);
        app.screen = Screen::ALL[(cli.screen - 1) as usize];
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
