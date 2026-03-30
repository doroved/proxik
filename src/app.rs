use crate::cli::{Cli, Commands, ProxyCommands};
use crate::config::{Config, ProxyConfig};
use crate::{daemon, http, socks5, updater};
use anyhow::Result;
use colored::{ColoredString, Colorize};

pub struct App {
    config: Config,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        Ok(Self { config })
    }

    pub fn run(mut self, cli: Cli) -> Result<()> {
        match cli.command {
            Commands::Add { cmd } => self.add(cmd)?,
            Commands::Rm { port, all } => self.remove(port, all)?,
            Commands::Run { cmd } => self.run_cmd(cmd)?,
            Commands::Start => self.start()?,
            Commands::Restart => self.restart()?,
            Commands::Stop => self.stop()?,
            Commands::Log => self.log()?,
            Commands::Ls => self.list()?,
            Commands::Update => self.update()?,
        }
        Ok(())
    }

    fn add(&mut self, cmd: ProxyCommands) -> Result<()> {
        let proxy = cmd.into_config()?;

        match self.config.add_proxy(proxy.clone()) {
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
        self.config.save()?;

        if daemon::is_running() {
            println!("Daemon is running. Restarting to apply changes...");
            daemon::stop_daemon()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            daemon::start_daemon()?;

            let proxies_to_run = self.config.proxies.clone();
            let rt = tokio::runtime::Runtime::new()?;
            self.update_public_ip(&rt);
            rt.block_on(Self::run_proxies(proxies_to_run))?;
        }
        Ok(())
    }

    fn remove(&mut self, port: Option<u16>, all: bool) -> Result<()> {
        if all {
            if self.config.proxies.is_empty() {
                println!("No proxies found in config.");
                return Ok(());
            }
            self.config.proxies.clear();
            self.config.save()?;
            println!("All proxies removed from config.");
            if daemon::is_running() {
                println!("Daemon is running. Stopping daemon...");
                daemon::stop_daemon()?;
            }
        } else if let Some(p) = port {
            if let Some(removed) = self.config.remove_proxy(p) {
                self.config.save()?;
                println!(
                    "Proxy on port {} ({}) removed from config.",
                    p, removed.protocol
                );
                if daemon::is_running() {
                    println!("Daemon is running. Restarting to apply changes...");
                    daemon::stop_daemon()?;
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    daemon::start_daemon()?;

                    let proxies_to_run = self.config.proxies.clone();
                    let rt = tokio::runtime::Runtime::new()?;
                    self.update_public_ip(&rt);
                    rt.block_on(Self::run_proxies(proxies_to_run))?;
                }
            } else {
                println!("No proxy found on port {} in config.", p);
            }
        }
        Ok(())
    }

    fn run_cmd(&mut self, cmd: Option<ProxyCommands>) -> Result<()> {
        let proxies_to_run = if let Some(run_cmd) = cmd {
            vec![run_cmd.into_config()?]
        } else {
            if self.config.proxies.is_empty() {
                println!(
                    "No proxies configured. Use 'add' to save one to config or pass proxy arguments."
                );
                return Ok(());
            }
            self.config.proxies.clone()
        };

        let rt = tokio::runtime::Runtime::new()?;
        self.update_public_ip(&rt);

        rt.block_on(Self::run_proxies(proxies_to_run))?;
        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        let proxies_to_run = self.config.proxies.clone();
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
        self.update_public_ip(&rt);
        rt.block_on(Self::run_proxies(proxies_to_run))?;
        Ok(())
    }

    fn restart(&mut self) -> Result<()> {
        if !daemon::is_running() {
            println!("Proxik is not running. Starting it...");
        } else {
            println!("Restarting Proxik daemon...");
            daemon::stop_daemon()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        let proxies_to_run = self.config.proxies.clone();
        if proxies_to_run.is_empty() {
            println!("No proxies configured in ~/.proxik/config.toml");
            return Ok(());
        }

        daemon::start_daemon()?;
        let rt = tokio::runtime::Runtime::new()?;
        self.update_public_ip(&rt);
        rt.block_on(Self::run_proxies(proxies_to_run))?;
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        daemon::stop_daemon()?;
        Ok(())
    }

    fn log(&self) -> Result<()> {
        daemon::show_logs()?;
        Ok(())
    }

    fn list(&self) -> Result<()> {
        if self.config.proxies.is_empty() {
            println!("No proxies configured in ~/.proxik/config.toml");
            return Ok(());
        }

        let rt = tokio::runtime::Runtime::new()?;
        let public_ip = self.config.public_ip.as_deref().unwrap_or("0.0.0.0");

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
            for proxy in &self.config.proxies {
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
                    protocol: proxy.protocol.clone(),
                    bind,
                    status,
                    user: proxy.username.clone().unwrap_or_else(|| "-".to_string()),
                    pass: proxy.password.clone().unwrap_or_else(|| "-".to_string()),
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
        Ok(())
    }

    fn update(&self) -> Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(updater::update())?;
        Ok(())
    }

    fn update_public_ip(&mut self, rt: &tokio::runtime::Runtime) {
        if self.config.public_ip.is_none() {
            let _ = rt.block_on(Self::fetch_public_ip()).map(|ip| {
                self.config.public_ip = Some(ip);
                let _ = self.config.save();
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
                        if let Err(e) =
                            socks5::run(&bind, port, proxy.username, proxy.password).await
                        {
                            tracing::error!("SOCKS5 server error on {}: {}", bind, e);
                        }
                    });
                    handles.push(h);
                }
                "http" => {
                    let port = proxy.port;
                    let h = tokio::spawn(async move {
                        if let Err(e) = http::run(&bind, port, proxy.username, proxy.password).await
                        {
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
}
