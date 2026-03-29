# Proxik

Lightweight and minimalistic HTTP, HTTPS, and SOCKS5 proxy server written in Rust.

## Features

- **SOCKS5 Support**: Full support for TCP and UDP association (NAT-like handling).
- **Authentication**: User/Password authentication support for SOCKS5.
- **CLI Management**: Powerful CLI inspired by industry standards.
- **Flexible Execution**: Run specific proxies via CLI or all at once from the config.
- **Auto-Config**: Save proxies to `~/.proxik/config.toml` with duplicate detection using the `--save` flag.
- **Auto-Update**: Built-in self-update mechanism via GitHub Releases.
- **Daemon Mode**: (Planned) Manage the proxy as a background service.
- **HTTPS & Let's Encrypt**: (Planned) Automatic certificate management for HTTPS proxies.

## Installation

```bash
cargo build --release
```

The binary will be available at `target/release/proxik`.

## Usage

### Run from Configuration

To run all proxies defined in `~/.proxik/config.toml`:

```bash
proxik run
```

### Run and Save via CLI

To start a specific proxy and **optionally save** it to the config (with duplicate check):

```bash
# Start SOCKS5 and save to config (~/.proxik/config.toml)
proxik run --bind 0.0.0.0:1080 --save socks5 --username admin --password hello
```

To start a proxy **without** saving it to the config:

```bash
proxik run --bind 0.0.0.0:1081 socks5
```

*Note: If a protocol subcommand (like `socks5`) is provided, only that specific proxy will run. If omitted, all proxies from the config will start.*

### Self-Update

To update the application to the latest version from GitHub:

```bash
proxik update
```

### Command Overview

- `run`: Run server.
  - `-b, --bind <BIND>`: Bind address (default: `0.0.0.0:1080`).
  - `-s, --save`: Save this proxy configuration to `~/.proxik/config.toml`.
  - `socks5`: SOCKS5 protocol subcommand.
    - `-u, --username <USER>`: Optional username for auth.
    - `-p, --password <PASS>`: Optional password for auth.
  - `http`: HTTP protocol subcommand (Planned).
- `update`: Update the application.
- `start/stop/restart/log`: Daemon management (Planned).

## Configuration

Proxies are stored in `~/.proxik/config.toml`. The binary and config paths are printed every time you start the server.

```toml
[[proxies]]
protocol = "socks5"
bind = "0.0.0.0:1080"
username = "admin"
password = "password123"
```

## License

MIT
