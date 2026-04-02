use std::net::SocketAddr;

pub async fn resolve_ipv4(target: &str) -> std::io::Result<SocketAddr> {
    tokio::net::lookup_host(target)
        .await?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No IPv4 address found for domain",
            )
        })
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn check_process_name_on_port(port: u16, process_name: &str) -> bool {
    if let Ok(output) = std::process::Command::new("lsof")
        .arg("-nP")
        .arg(format!("-iTCP:{}", port))
        .arg("-sTCP:LISTEN")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout
            .lines()
            .skip(1)
            .any(|line| line.starts_with(process_name))
        {
            return true;
        }
    }

    if let Ok(output) = std::process::Command::new("ss")
        .arg("-lptn")
        .arg(format!("sport = :{}", port))
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains(process_name) {
            return true;
        }
    }

    false
}
