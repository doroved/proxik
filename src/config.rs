use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProxyConfig {
    pub protocol: String,
    pub bind: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Config {
    pub public_ip: Option<String>,
    pub proxies: Vec<ProxyConfig>,
}

impl Config {
    pub fn get_config_path() -> Result<PathBuf> {
        let name = env!("CARGO_PKG_NAME");
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Ok(PathBuf::from(home)
            .join(format!(".{}", name))
            .join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::get_config_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path)
            .context(format!("Failed to read config file at {:?}", path))?;
        let config: Config = toml::from_str(&content).context("Failed to parse config as TOML")?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::get_config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context(format!("Failed to create config directory at {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;
        fs::write(&path, content).context(format!("Failed to write config file to {:?}", path))?;
        Ok(())
    }

    /// Adds a proxy or overwrites an existing one with the same bind address.
    /// Returns the old config if it was overwritten.
    pub fn add_proxy(&mut self, proxy: ProxyConfig) -> Option<ProxyConfig> {
        if let Some(index) = self.proxies.iter().position(|p| p.bind == proxy.bind) {
            let old = self.proxies[index].clone();
            self.proxies[index] = proxy;
            Some(old)
        } else {
            self.proxies.push(proxy);
            None
        }
    }
}
