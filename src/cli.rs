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
    /// Add a proxy to configuration
    Add {
        #[command(subcommand)]
        cmd: ProxyCommands,
    },
    /// Remove a proxy from configuration
    Rm {
        /// Listen port to remove
        #[arg(required_unless_present = "all")]
        port: Option<u16>,
        /// Remove all proxies from configuration
        #[arg(short, long)]
        all: bool,
    },
    /// Run proxies in foreground (from config or single specific)
    Run {
        #[command(subcommand)]
        cmd: Option<ProxyCommands>,
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
    Ls,
    /// Update the application
    Update,
}

#[derive(Subcommand, Clone)]
pub enum ProxyCommands {
    /// Http server
    Http {
        /// Listen port
        #[arg(short, long, default_value = "1080")]
        port: u16,
        /// Credentials for authentication (format: user:pass)
        #[arg(short, long)]
        auth: Option<String>,
    },
    /// Socks5 server
    Socks5 {
        /// Listen port
        #[arg(short, long, default_value = "1080")]
        port: u16,
        /// Credentials for authentication (format: user:pass)
        #[arg(short, long)]
        auth: Option<String>,
    },
}
