use anyhow::{Context, Result};
use daemonize::Daemonize;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;

fn get_base_dir() -> Result<PathBuf> {
    let name = env!("CARGO_PKG_NAME");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let base_dir = PathBuf::from(home).join(format!(".{}", name));
    if !base_dir.exists() {
        std::fs::create_dir_all(&base_dir)?;
    }
    Ok(base_dir)
}

pub fn is_running() -> bool {
    if let Ok(base_dir) = get_base_dir() {
        let pid_path = base_dir.join("proxik.pid");
        let pid = std::fs::read_to_string(&pid_path)
            .ok()
            .and_then(|c| c.trim().parse::<i32>().ok());

        if let Some(pid) = pid {
            // Check if process exists by sending signal 0
            let status = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            let running = status.map(|s| s.success()).unwrap_or(false);
            if !running {
                let _ = std::fs::remove_file(pid_path);
            }
            return running;
        }
    }
    false
}

pub fn start_daemon() -> Result<()> {
    let base_dir = get_base_dir()?;

    let pid_file = base_dir.join("proxik.pid");
    let stdout_log = base_dir.join("proxik.log");
    let stderr_log = base_dir.join("proxik.err");

    println!("Starting Proxik daemon...");
    println!("PID file: {:?}", pid_file);
    println!("Log file: {:?}", stdout_log);

    let stdout = File::create(&stdout_log).context("Failed to create stdout log file")?;
    let stderr = File::create(&stderr_log).context("Failed to create stderr log file")?;

    let daemonize = Daemonize::new()
        .pid_file(&pid_file)
        .chown_pid_file(true)
        .working_directory(&base_dir)
        .stdout(stdout)
        .stderr(stderr);

    daemonize.start().context("Failed to daemonize process")?;

    Ok(())
}

pub fn stop_daemon() -> Result<()> {
    let base_dir = get_base_dir()?;
    let pid_path = base_dir.join("proxik.pid");

    if !pid_path.exists() {
        println!("Proxik is not running (no PID file).");
        return Ok(());
    }

    let pid = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|c| c.trim().parse::<i32>().ok());

    if let Some(pid) = pid {
        println!("Stopping Proxik daemon (PID {})...", pid);

        let status = Command::new("kill")
            .arg("-15")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        match status {
            Ok(s) if s.success() => {
                let _ = std::fs::remove_file(&pid_path);
                println!("Daemon stopped successfully.");
            }
            _ => {
                println!("Failed to stop process or process already dead. Cleaning up PID file.");
                let _ = std::fs::remove_file(&pid_path);
            }
        }
    }

    Ok(())
}

pub fn show_logs() -> Result<()> {
    let base_dir = get_base_dir()?;
    let log_path = base_dir.join("proxik.log");

    if !log_path.exists() {
        println!("Log file not found at {:?}", log_path);
        return Ok(());
    }

    let mut child = Command::new("tail")
        .arg("-F")
        .arg(log_path)
        .spawn()
        .context("Failed to start tail command")?;

    let _ = child.wait()?;
    Ok(())
}
