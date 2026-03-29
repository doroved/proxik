# Proxik

Lightweight and minimalistic HTTP, HTTPS, and SOCKS5 proxy server written in Rust.

## Features

- **SOCKS5 Support**: Full support for TCP and UDP association (NAT-like handling).
- **Authentication**: User/Password authentication support for SOCKS5.
- **CLI Management**: Powerful CLI inspired by industry standards.
- **Single Process Daemon**: Manage all your proxies via a single background process.
- **Auto-Config**: Save proxies to `~/.proxik/config.toml` and manage them easily via CLI commands.
- **Auto-Update**: Built-in self-update mechanism via GitHub Releases.
- **Daemon Mode**: Run and manage Proxik as a background service.
- **HTTPS & Let's Encrypt**: (Planned) Automatic certificate management for HTTPS proxies.

## Installation

The easiest way to install Proxik on Linux (x86_64 or aarch64) is:

```bash
curl -fsSL https://proxik.pages.dev | bash
```

<details>
<summary>Alternative installation method</summary>

```bash
curl -sSL https://raw.githubusercontent.com/doroved/proxik/main/cf/install.sh | bash
```

</details>

### Manual Build

```bash
cargo build --release
```

The binary will be available at `target/release/proxik`.

## Usage

### Run in Foreground

To run all proxies defined in `~/.proxik/config.toml` in the foreground:

```bash
proxik run
```

To run a specific proxy on the fly (without saving it to config):

```bash
proxik run socks5 --port 1080
```

### Configuration Management

Manage your `~/.proxik/config.toml` configuration easily:

```bash
# Add a new proxy to config
# Note: If the daemon is running, it will automatically restart to apply changes!
proxik add socks5 --port 1080 --auth admin:hello

# Add an HTTP proxy
proxik add http --port 8080

# Remove a proxy from config by port
proxik rm 1080

# Remove all proxies from config
proxik rm --all
```

### Daemon Management

You can run Proxik in the background as a single daemon process. It will run all proxies defined in your configuration.

```bash
# Start all configured proxies in background
proxik start

# Restart the daemon
proxik restart

# Stop the daemon
proxik stop

# View daemon logs
proxik log
```

*Logs and PID files are stored in `~/.proxik/` (`proxik.log`, `proxik.pid`).*

### List and Status

To see all configured proxies and their current status:

```bash
proxik ls
```

Example output:
```text
PROTOCOL   BIND           STATUS    USER      PASS   URL
---------- -------------- --------- --------- ------ ----------------------------------------
socks5     0.0.0.0:9999   RUNNING   doroved   hello    socks5h://doroved:hello@92.123.135.139:1080
socks5     0.0.0.0:1099   STOPPED   -       -          socks5h://92.123.135.139:1099
```

### Self-Update

To update the application to the latest version from GitHub:

```bash
proxik update
```

### Command Overview

- `add`: Add a proxy to configuration.
  - `socks5`: SOCKS5 protocol.
    - `-p, --port <PORT>`: Listen port (default: `1080`).
    - `-a, --auth <user:pass>`: Optional credentials for authentication.
  - `http`: HTTP protocol.
    - `-p, --port <PORT>`: Listen port (default: `1080`).
- `rm <PORT>`: Remove a proxy from configuration.
  - `-a, --all`: Remove all proxies from configuration.
- `run`: Run proxies in foreground. Without arguments, runs all from config. Can accept `socks5`/`http` to run a single proxy on the fly.
- `start`: Start daemon (runs all configured proxies).
- `stop`: Stop the background daemon.
- `restart`: Restart the background daemon.
- `log`: View daemon logs.
- `ls`: Show all configured proxies and their status.
- `update`: Update the application from GitHub.

## Configuration

Proxies are stored in `~/.proxik/config.toml`.

```toml
[[proxies]]
protocol = "socks5"
port = 1080
username = "admin"
password = "password123"

[[proxies]]
protocol = "http"
port = 8080
username = "admin"
password = "password123"
```

## License

MIT
