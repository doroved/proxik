mod cli;
mod config;
mod socks5;
mod updater;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, RunCommands};
use config::{Config, ProxyConfig};
use std::env;

use colored::*;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load()?;

    match cli.command {
        Commands::Run { bind, save, cmd } => {
            let exe_path = env::current_exe()?;
            println!("Binary: {:?}", exe_path);

            let proxies_to_run = if let Some(run_cmd) = cmd {
                // Run only the proxy specified in CLI
                let proxy = match run_cmd {
                    RunCommands::Socks5 { username, password } => ProxyConfig {
                        protocol: "socks5".to_string(),
                        bind: bind.clone(),
                        username,
                        password,
                    },
                    RunCommands::Http => ProxyConfig {
                        protocol: "http".to_string(),
                        bind: bind.clone(),
                        username: None,
                        password: None,
                    },
                };

                if save {
                    if config.add_proxy(proxy.clone()) {
                        config.save()?;
                        println!("Proxy {} added to config.", proxy.bind.green());
                    } else {
                        println!(
                            "Proxy {} already exists in config, skipping save.",
                            proxy.bind.yellow()
                        );
                    }
                }

                vec![proxy]
            } else {
                // Run all from config
                let config_path = Config::get_config_path()?;
                println!("Config: {:?}", config_path);

                if config.proxies.is_empty() {
                    println!(
                        "No proxies configured. Use 'run --save <PROTOCOL>' to add one or check ~/.proxik/config.toml"
                    );
                    return Ok(());
                }
                config.proxies
            };

            let mut handles = vec![];
            for proxy in proxies_to_run {
                match proxy.protocol.as_str() {
                    "socks5" => {
                        let h = tokio::spawn(async move {
                            if let Err(e) =
                                socks5::run(&proxy.bind, proxy.username, proxy.password).await
                            {
                                eprintln!("SOCKS5 server error on {}: {}", proxy.bind.red(), e);
                            }
                        });
                        handles.push(h);
                    }
                    "http" => {
                        println!("HTTP protocol is not yet implemented");
                    }
                    _ => {
                        println!("Protocol {} is not supported", proxy.protocol);
                    }
                }
            }

            for h in handles {
                let _ = h.await;
            }
        }
        Commands::Update => {
            updater::update()?;
        }
        Commands::List => {
            if config.proxies.is_empty() {
                println!("No proxies configured in ~/.proxik/config.toml");
            } else {
                println!(
                    "{:<10} {:<20} {:<10} {:<15} {:<15}",
                    "PROTOCOL", "BIND", "STATUS", "USER", "PASS"
                );
                println!(
                    "{:-<10} {:-<20} {:-<10} {:-<15} {:-<15}",
                    "", "", "", "", ""
                );
                for proxy in config.proxies {
                    let status = match tokio::time::timeout(
                        std::time::Duration::from_millis(200),
                        tokio::net::TcpStream::connect(&proxy.bind),
                    )
                    .await
                    {
                        Ok(Ok(_)) => "RUNNING".green(),
                        _ => "STOPPED".red(),
                    };

                    println!(
                        "{:<10} {:<20} {:<20} {:<15} {:<15}",
                        proxy.protocol,
                        proxy.bind,
                        status,
                        proxy.username.clone().unwrap_or_else(|| "-".to_string()),
                        proxy.password.unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        Commands::Start => {
            let exe_path = env::current_exe()?;
            let config_path = Config::get_config_path()?;
            println!("Binary: {:?}", exe_path);
            println!("Config: {:?}", config_path);
            println!("Starting daemon... (Not fully implemented yet)");
        }
        _ => {
            println!("Command not fully implemented yet.");
        }
    }

    Ok(())
}
