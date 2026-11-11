use clap::Parser;

use llamatop::config::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.dump {
        println!("{}", llamatop::dump::run(&cli)?);
        return Ok(());
    }
    anyhow::bail!("the dashboard is not implemented yet (batch 2)")
}
