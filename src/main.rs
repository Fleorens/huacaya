use clap::{CommandFactory, FromArgMatches};

use llamatop::app::run::run;
use llamatop::app::{Screen, demo};
use llamatop::config::Cli;
use llamatop::settings;
use llamatop::ui::snapshot::{self, parse_size};
use llamatop::ui::theme::{ColorMode, Theme};

fn main() -> anyhow::Result<()> {
    let matches = Cli::command().get_matches();
    let mut cli = Cli::from_arg_matches(&matches)?;
    if let Some(path) = cli.config.clone().or_else(settings::default_path) {
        match settings::load(&path) {
            Ok(Some(settings)) => cli.apply_settings(&matches, &settings),
            Ok(None) => {}
            Err(err) => return Err(err.context(format!("configuration file {}", path.display()))),
        }
    }
    if cli.dump {
        println!("{}", llamatop::dump::run(&cli)?);
        return Ok(());
    }
    if let Some(size) = &cli.snapshot {
        let (w, h) =
            parse_size(size).ok_or_else(|| anyhow::anyhow!("invalid --snapshot size, expected WxH"))?;
        let mut app = demo::app(cli.demo_state);
        app.screen = Screen::ALL[(cli.screen - 1) as usize];
        print!("{}", snapshot::snapshot(&app, &Theme::new(ColorMode::detect()), w, h, cli.ansi));
        return Ok(());
    }
    run(&cli)
}
