use clap::Parser;

use llamatop::config::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.dump {
        anyhow::bail!("--dump is not implemented yet (task T08)");
    }
    anyhow::bail!("the dashboard is not implemented yet (batch 2)")
}
