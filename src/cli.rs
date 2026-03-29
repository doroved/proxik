use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "proxik", version)]
#[command(about = "Lightweight SOCKS5/HTTP/HTTPS proxy server", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run server
    Run {
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:1080")]
        bind: String,

        /// Save proxy to configuration file
        #[arg(short, long)]
        save: bool,

        #[command(subcommand)]
        cmd: Option<RunCommands>,
    },
    /// Start server daemon
    Start,
    /// Restart server daemon
    Restart,
    /// Stop server daemon
    Stop,
    /// Show the server daemon log
    Log,
    /// List configured proxies
    List,
    /// Update the application
    Update,
}

#[derive(Subcommand)]
pub enum RunCommands {
    /// Http server
    Http,
    /// Socks5 server
    Socks5 {
        /// Username for authentication
        #[arg(short, long)]
        username: Option<String>,
        /// Password for authentication
        #[arg(short, long)]
        password: Option<String>,
    },
}
