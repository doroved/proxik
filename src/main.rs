mod cli;
mod config;
mod daemon;
mod http;
mod socks5;
mod updater;
pub mod utils;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, RunCommands};
use colored::*;
use config::{Config, ProxyConfig};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_ansi(true)
        .with_ansi_sanitization(false)
        .init();

    let cli = Cli::parse();
    let mut config = Config::load()?;

    match cli.command {
        Commands::Run { port, save, cmd } => {
            let proxies_to_run = get_proxies_to_run(&mut config, port, save, cmd)?;
            if proxies_to_run.is_empty() {
                return Ok(());
            }

            let rt = tokio::runtime::Runtime::new()?;
            update_public_ip(&rt, &mut config);

            rt.block_on(run_proxies(proxies_to_run))?;
        }
        Commands::Start { port, save, cmd } => {
            let mut overwritten = false;

            if let Some(run_cmd) = save.then_some(cmd).flatten() {
                let proxy = match run_cmd {
                    RunCommands::Socks5 { username, password } => ProxyConfig {
                        protocol: "socks5".to_string(),
                        port,
                        username,
                        password,
                    },
                    RunCommands::Http => ProxyConfig {
                        protocol: "http".to_string(),
                        port,
                        username: None,
                        password: None,
                    },
                };

                match config.add_proxy(proxy.clone()) {
                    Some(old) => {
                        config.save()?;
                        tracing::info!(
                            "Proxy on port {} {} (User: {}, Pass: {}) → (User: {}, Pass: {})",
                            proxy.port,
                            "overwritten:",
                            old.username.unwrap_or_else(|| "-".to_string()),
                            old.password.unwrap_or_else(|| "-".to_string()),
                            proxy.username.clone().unwrap_or_else(|| "-".to_string()),
                            proxy.password.clone().unwrap_or_else(|| "-".to_string())
                        );
                        overwritten = true;
                    }
                    None => {
                        config.save()?;
                        tracing::info!("Proxy on port {} added to config.", proxy.port);
                    }
                }
            }

            if daemon::is_running() {
                if overwritten {
                    tracing::info!("Restarting Proxik daemon to apply changes...");
                    daemon::stop_daemon()?;
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    tracing::info!("Proxik daemon is already running.");
                    tracing::info!(
                        "To apply new config, please restart the daemon: stop then start or use 'restart'."
                    );
                    return Ok(());
                }
            }

            let proxies_to_run = config.proxies.clone();
            if proxies_to_run.is_empty() {
                tracing::warn!("No proxies configured. Use '--save' to add one.");
                return Ok(());
            }

            daemon::start_daemon()?;
            let rt = tokio::runtime::Runtime::new()?;
            update_public_ip(&rt, &mut config);
            rt.block_on(run_proxies(proxies_to_run))?;
        }
        Commands::Restart => {
            if !daemon::is_running() {
                tracing::info!("Proxik is not running. Starting it...");
            } else {
                tracing::info!("Restarting Proxik daemon...");
                daemon::stop_daemon()?;
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            let proxies_to_run = config.proxies.clone();
            if proxies_to_run.is_empty() {
                tracing::warn!("No proxies configured in ~/.proxik/config.toml");
                return Ok(());
            }

            daemon::start_daemon()?;
            let rt = tokio::runtime::Runtime::new()?;
            update_public_ip(&rt, &mut config);
            rt.block_on(run_proxies(proxies_to_run))?;
        }
        Commands::Update => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(updater::update())?;
        }
        Commands::Stop => {
            daemon::stop_daemon()?;
        }
        Commands::List => {
            if config.proxies.is_empty() {
                println!("No proxies configured in ~/.proxik/config.toml");
            } else {
                let rt = tokio::runtime::Runtime::new()?;
                let public_ip = config.public_ip.as_deref().unwrap_or("IP");

                struct Row {
                    protocol: String,
                    bind: String,
                    status: ColoredString,
                    user: String,
                    pass: String,
                    url: String,
                }

                let rows: Vec<Row> = rt.block_on(async {
                    let mut res = Vec::new();
                    for proxy in config.proxies {
                        let bind = format!("0.0.0.0:{}", proxy.port);
                        let status_raw = match tokio::time::timeout(
                            std::time::Duration::from_millis(200),
                            tokio::net::TcpStream::connect(&bind),
                        )
                        .await
                        {
                            Ok(Ok(_)) => "RUNNING",
                            _ => "STOPPED",
                        };

                        let status = if status_raw == "RUNNING" {
                            status_raw.green()
                        } else {
                            status_raw.red()
                        };

                        let protocol_prefix = if proxy.protocol == "socks5" {
                            "socks5h"
                        } else {
                            &proxy.protocol
                        };

                        let url = match (&proxy.username, &proxy.password) {
                            (Some(u), Some(p)) => {
                                format!(
                                    "{}://{}:{}@{}:{}",
                                    protocol_prefix, u, p, public_ip, proxy.port
                                )
                            }
                            _ => format!("{}://{}:{}", protocol_prefix, public_ip, proxy.port),
                        };

                        res.push(Row {
                            protocol: proxy.protocol,
                            bind,
                            status,
                            user: proxy.username.unwrap_or_else(|| "-".to_string()),
                            pass: proxy.password.unwrap_or_else(|| "-".to_string()),
                            url,
                        });
                    }
                    res
                });

                let mut w_proto = "PROTOCOL".len();
                let mut w_bind = "BIND".len();
                let mut w_status = "STATUS".len();
                let mut w_user = "USER".len();
                let mut w_pass = "PASS".len();

                for r in &rows {
                    w_proto = std::cmp::max(w_proto, r.protocol.len());
                    w_bind = std::cmp::max(w_bind, r.bind.len());
                    w_status = std::cmp::max(w_status, 7);
                    w_user = std::cmp::max(w_user, r.user.len());
                    w_pass = std::cmp::max(w_pass, r.pass.len());
                }

                w_proto += 2;
                w_bind += 2;
                w_status += 2;
                w_user += 2;
                w_pass += 2;

                println!(
                    "{:<w_proto$} {:<w_bind$} {:<w_status$} {:<w_user$} {:<w_pass$} URL",
                    "PROTOCOL",
                    "BIND",
                    "STATUS",
                    "USER",
                    "PASS",
                    w_proto = w_proto,
                    w_bind = w_bind,
                    w_status = w_status,
                    w_user = w_user,
                    w_pass = w_pass
                );

                println!(
                    "{:-<w_proto$} {:-<w_bind$} {:-<w_status$} {:-<w_user$} {:-<w_pass$} {:-<40}",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    w_proto = w_proto,
                    w_bind = w_bind,
                    w_status = w_status,
                    w_user = w_user,
                    w_pass = w_pass
                );

                for r in &rows {
                    println!(
                        "{:<w_proto$} {:<w_bind$} {:<w_status$} {:<w_user$} {:<w_pass$} {}",
                        r.protocol,
                        r.bind,
                        r.status,
                        r.user,
                        r.pass,
                        r.url.cyan(),
                        w_proto = w_proto,
                        w_bind = w_bind,
                        w_status = w_status,
                        w_user = w_user,
                        w_pass = w_pass
                    );
                }
            }
        }
        _ => {
            tracing::warn!("Command not fully implemented yet.");
        }
    }

    Ok(())
}

fn update_public_ip(rt: &tokio::runtime::Runtime, config: &mut Config) {
    if config.public_ip.is_none() {
        let _ = rt.block_on(fetch_public_ip()).map(|ip| {
            config.public_ip = Some(ip);
            config.save()
        });
    }
}

async fn fetch_public_ip() -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client.get("https://api.ipify.org").send().await?;
    let ip = resp.text().await?;
    Ok(ip)
}

fn get_proxies_to_run(
    config: &mut Config,
    port: u16,
    save: bool,
    cmd: Option<RunCommands>,
) -> Result<Vec<ProxyConfig>> {
    if let Some(run_cmd) = cmd {
        let proxy = match run_cmd {
            RunCommands::Socks5 { username, password } => ProxyConfig {
                protocol: "socks5".to_string(),
                port,
                username,
                password,
            },
            RunCommands::Http => ProxyConfig {
                protocol: "http".to_string(),
                port,
                username: None,
                password: None,
            },
        };

        if save {
            match config.add_proxy(proxy.clone()) {
                Some(old) => {
                    config.save()?;
                    tracing::info!(
                        "Proxy on port {} {} (User: {}, Pass: {}) → (User: {}, Pass: {})",
                        proxy.port,
                        "overwritten:",
                        old.username.unwrap_or_else(|| "-".to_string()),
                        old.password.unwrap_or_else(|| "-".to_string()),
                        proxy.username.clone().unwrap_or_else(|| "-".to_string()),
                        proxy.password.clone().unwrap_or_else(|| "-".to_string())
                    );
                }
                None => {
                    config.save()?;
                    tracing::info!("Proxy on port {} added to config.", proxy.port);
                }
            }
        }

        Ok(vec![proxy])
    } else {
        if config.proxies.is_empty() {
            tracing::warn!(
                "No proxies configured. Use 'run --save <PROTOCOL>' to add one or check ~/.proxik/config.toml"
            );
            return Ok(vec![]);
        }
        Ok(config.proxies.clone())
    }
}

async fn run_proxies(proxies: Vec<ProxyConfig>) -> Result<()> {
    let mut handles = vec![];
    for proxy in proxies {
        let bind = format!("0.0.0.0:{}", proxy.port);
        match proxy.protocol.as_str() {
            "socks5" => {
                let h = tokio::spawn(async move {
                    if let Err(e) = socks5::run(&bind, proxy.username, proxy.password).await {
                        tracing::error!("SOCKS5 server error on {}: {}", bind, e);
                    }
                });
                handles.push(h);
            }
            "http" => {
                let h = tokio::spawn(async move {
                    if let Err(e) = http::run(&bind, proxy.username, proxy.password).await {
                        tracing::error!("HTTP server error on {}: {}", bind, e);
                    }
                });
                handles.push(h);
            }
            _ => {
                tracing::warn!("Protocol {} is not supported", proxy.protocol);
            }
        }
    }

    for h in handles {
        let _ = h.await;
    }
    Ok(())
}
