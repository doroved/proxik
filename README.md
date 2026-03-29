# Proxik

Lightweight and minimalistic HTTP, HTTPS, and SOCKS5 proxy server written in Rust.

## Features

- **SOCKS5 Support**: Full support for TCP and UDP association (NAT-like handling).
- **Authentication**: User/Password authentication support for SOCKS5.
- **CLI Management**: Powerful CLI inspired by industry standards.
- **Single Process Daemon**: Manage all your proxies via a single background process.
- **Auto-Config**: Save proxies to `~/.proxik/config.toml` with duplicate detection using the `--save` flag.
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

To run a specific proxy (without starting others from config):

```bash
proxik run --bind 0.0.0.0:1080 socks5
```

### Daemon Management

You can run Proxik in the background as a single daemon process.

```bash
# Start all configured proxies in background
proxik start

# Add a new proxy to config and save it (~/.proxik/config.toml)
# Note: This updates the config even if the daemon is already running.
proxik start --bind 0.0.0.0:1080 --save socks5 --username admin --password hello

# Stop the daemon
proxik stop
```

*Logs and PID files are stored in `~/.proxik/` (`proxik.log`, `proxik.pid`).*

### List and Status

To see all configured proxies and their current status:

```bash
proxik list
```

Example output:
```text
PROTOCOL   BIND                 STATUS     USER            PASS
---------- -------------------- ---------- --------------- ---------------
socks5     0.0.0.0:1080         RUNNING    -               -
socks5     0.0.0.0:1088         STOPPED    admin           hello
```

### Self-Update

To update the application to the latest version from GitHub:

```bash
proxik update
```

### Command Overview

- `run`: Run in foreground.
  - `-b, --bind <BIND>`: Bind address (default: `0.0.0.0:1080`).
  - `-s, --save`: Save this proxy configuration to `~/.proxik/config.toml`.
  - `socks5`: SOCKS5 protocol subcommand.
    - `-u, --username <USER>`: Optional username for auth.
    - `-p, --password <PASS>`: Optional password for auth.
  - `http`: HTTP protocol subcommand (Planned).
- `start`: Start as a background daemon. Supports the same arguments as `run`.
- `stop`: Stop the background daemon.
- `list`: Show all configured proxies and their status.
- `update`: Update the application.
- `restart/log`: (Planned) Additional daemon management.

## Configuration

Proxies are stored in `~/.proxik/config.toml`.

```toml
[[proxies]]
protocol = "socks5"
bind = "0.0.0.0:1080"
username = "admin"
password = "password123"
```

## License

MIT
