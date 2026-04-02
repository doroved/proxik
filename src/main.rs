mod cli;
mod config;
mod daemon;
mod http;
mod socks5;
mod updater;
pub mod utils;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, ProxyCommands};
use colored::*;
use config::{Config, ProxyConfig};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        // .with_target(false)
        .with_ansi(true)
        .with_ansi_sanitization(false)
        .init();

    let cli = Cli::parse();
    let mut config = Config::load()?;

    match cli.command {
        Commands::Add { cmd } => {
            let proxy = match cmd {
                ProxyCommands::Socks5 { port, auth } => {
                    let (username, password) = if let Some(a) = auth {
                        let parts: Vec<&str> = a.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            (Some(parts[0].to_string()), Some(parts[1].to_string()))
                        } else {
                            println!("Invalid auth format. Use user:pass");
                            return Ok(());
                        }
                    } else {
                        (None, None)
                    };
                    ProxyConfig {
                        protocol: "socks5".to_string(),
                        port,
                        username,
                        password,
                    }
                }
                ProxyCommands::Http { port, auth } => {
                    let (username, password) = if let Some(a) = auth {
                        let parts: Vec<&str> = a.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            (Some(parts[0].to_string()), Some(parts[1].to_string()))
                        } else {
                            println!("Invalid auth format. Use user:pass");
                            return Ok(());
                        }
                    } else {
                        (None, None)
                    };
                    ProxyConfig {
                        protocol: "http".to_string(),
                        port,
                        username,
                        password,
                    }
                }
            };

            match config.add_proxy(proxy.clone()) {
                Some(old) => {
                    println!(
                        "Proxy on port {} overwritten: (User: {}, Pass: {}) → (User: {}, Pass: {})",
                        proxy.port,
                        old.username.unwrap_or_else(|| "-".to_string()),
                        old.password.unwrap_or_else(|| "-".to_string()),
                        proxy.username.clone().unwrap_or_else(|| "-".to_string()),
                        proxy.password.clone().unwrap_or_else(|| "-".to_string())
                    );
                }
                None => {
                    println!("Proxy on port {} added to config.", proxy.port);
                }
            }
            config.save()?;

            if daemon::is_running() {
                println!("Daemon is running. Restarting to apply changes...");
                daemon::stop_daemon()?;
                std::thread::sleep(std::time::Duration::from_millis(500));
                daemon::start_daemon()?;

                let proxies_to_run = config.proxies.clone();
                let rt = tokio::runtime::Runtime::new()?;
                update_public_ip(&rt, &mut config);
                rt.block_on(run_proxies(proxies_to_run))?;
            }
        }
        Commands::Rm { port, all } => {
            if all {
                if config.proxies.is_empty() {
                    println!("No proxies found in config.");
                    return Ok(());
                }
                config.proxies.clear();
                config.save()?;
                println!("All proxies removed from config.");
                if daemon::is_running() {
                    println!("Daemon is running. Stopping daemon...");
                    daemon::stop_daemon()?;
                }
            } else if let Some(p) = port {
                if let Some(removed) = config.remove_proxy(p) {
                    config.save()?;
                    println!(
                        "Proxy on port {} ({}) removed from config.",
                        p, removed.protocol
                    );
                    if daemon::is_running() {
                        println!("Daemon is running. Restarting to apply changes...");
                        daemon::stop_daemon()?;
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        daemon::start_daemon()?;

                        let proxies_to_run = config.proxies.clone();
                        let rt = tokio::runtime::Runtime::new()?;
                        update_public_ip(&rt, &mut config);
                        rt.block_on(run_proxies(proxies_to_run))?;
                    }
                } else {
                    println!("No proxy found on port {} in config.", p);
                }
            }
        }
        Commands::Run { cmd } => {
            let proxies_to_run = if let Some(run_cmd) = cmd {
                vec![match run_cmd {
                    ProxyCommands::Socks5 { port, auth } => {
                        let (username, password) = if let Some(a) = auth {
                            let parts: Vec<&str> = a.splitn(2, ':').collect();
                            if parts.len() == 2 {
                                (Some(parts[0].to_string()), Some(parts[1].to_string()))
                            } else {
                                println!("Invalid auth format. Use user:pass");
                                return Ok(());
                            }
                        } else {
                            (None, None)
                        };
                        ProxyConfig {
                            protocol: "socks5".to_string(),
                            port,
                            username,
                            password,
                        }
                    }
                    ProxyCommands::Http { port, auth } => {
                        let (username, password) = if let Some(a) = auth {
                            let parts: Vec<&str> = a.splitn(2, ':').collect();
                            if parts.len() == 2 {
                                (Some(parts[0].to_string()), Some(parts[1].to_string()))
                            } else {
                                println!("Invalid auth format. Use user:pass");
                                return Ok(());
                            }
                        } else {
                            (None, None)
                        };
                        ProxyConfig {
                            protocol: "http".to_string(),
                            port,
                            username,
                            password,
                        }
                    }
                }]
            } else {
                if config.proxies.is_empty() {
                    println!(
                        "No proxies configured. Use 'add' to save one to config or pass proxy arguments."
                    );
                    return Ok(());
                }
                config.proxies.clone()
            };

            let rt = tokio::runtime::Runtime::new()?;
            update_public_ip(&rt, &mut config);

            rt.block_on(run_proxies(proxies_to_run))?;
        }
        Commands::Start => {
            let proxies_to_run = config.proxies.clone();
            if proxies_to_run.is_empty() {
                println!("No proxies configured. Use 'add' to add one.");
                return Ok(());
            }

            if daemon::is_running() {
                println!("Proxik daemon is already running.");
                return Ok(());
            }

            daemon::start_daemon()?;
            let rt = tokio::runtime::Runtime::new()?;
            update_public_ip(&rt, &mut config);
            rt.block_on(run_proxies(proxies_to_run))?;
        }
        Commands::Restart => {
            if !daemon::is_running() {
                println!("Proxik is not running. Starting it...");
            } else {
                println!("Restarting Proxik daemon...");
                daemon::stop_daemon()?;
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            let proxies_to_run = config.proxies.clone();
            if proxies_to_run.is_empty() {
                println!("No proxies configured in ~/.proxik/config.toml");
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
        Commands::Log => {
            daemon::show_logs()?;
        }
        Commands::Stop => {
            daemon::stop_daemon()?;
        }
        Commands::Ls => {
            if config.proxies.is_empty() {
                println!("No proxies configured in ~/.proxik/config.toml");
            } else {
                let rt = tokio::runtime::Runtime::new()?;
                let public_ip = config.public_ip.as_deref().unwrap_or("0.0.0.0");

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
                        let is_running = utils::check_process_name_on_port(proxy.port, "proxik");

                        let status_raw = if is_running { "RUNNING" } else { "STOPPED" };

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
                        r.url.bright_blue(),
                        w_proto = w_proto,
                        w_bind = w_bind,
                        w_status = w_status,
                        w_user = w_user,
                        w_pass = w_pass
                    );
                }
            }
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
    let services = [
        "https://api.ipify.org",
        "https://api.ip.sb/ip",
        "https://4.ident.me",
    ];

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    for service in services {
        let Ok(resp) = client.get(service).send().await else {
            continue;
        };
        let Ok(ip) = resp.text().await else {
            continue;
        };

        let ip = ip.trim();
        if !ip.is_empty() {
            return Ok(ip.to_string());
        }
    }

    anyhow::bail!("Failed to fetch public IP from all services")
}

async fn run_proxies(proxies: Vec<ProxyConfig>) -> Result<()> {
    let mut handles = vec![];
    for proxy in proxies {
        let bind = format!("0.0.0.0:{}", proxy.port);
        match proxy.protocol.as_str() {
            "socks5" => {
                let port = proxy.port;
                let h = tokio::spawn(async move {
                    if let Err(e) = socks5::run(&bind, port, proxy.username, proxy.password).await {
                        tracing::error!("SOCKS5 server error on {}: {}", bind, e);
                    }
                });
                handles.push(h);
            }
            "http" => {
                let port = proxy.port;
                let h = tokio::spawn(async move {
                    if let Err(e) = http::run(&bind, port, proxy.username, proxy.password).await {
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

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Shutting down...");
        }
        _ = async {
            for h in handles {
                let _ = h.await;
            }
        } => {}
    }
    Ok(())
}
