use clap::Parser;

use llamatop::app::run::run;
use llamatop::app::{Screen, demo};
use llamatop::config::Cli;
use llamatop::ui::snapshot::{self, parse_size};
use llamatop::ui::theme::{ColorMode, Theme};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
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
