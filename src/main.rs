mod app;
mod cli;
mod config;
mod daemon;
mod http;
mod socks5;
mod updater;
pub mod utils;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(true)
        .with_ansi_sanitization(false)
        .init();

    let cli = cli::Cli::parse();
    let app = app::App::new()?;

    app.run(cli)?;

    Ok(())
}
