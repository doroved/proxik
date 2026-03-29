mod cli;
mod config;
mod socks5;
mod updater;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, RunCommands};
use config::{Config, ProxyConfig};
use std::env;

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
                        println!("Proxy {} added to config.", proxy.bind);
                    } else {
                        println!(
                            "Proxy {} already exists in config, skipping save.",
                            proxy.bind
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
                                eprintln!("SOCKS5 server error on {}: {}", proxy.bind, e);
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
