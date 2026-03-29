use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::process::Command;

#[derive(Deserialize, Debug)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Debug)]
struct Asset {
    name: String,
    url: String,
}

pub fn update() -> Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    rt.block_on(run_update_async())
}

async fn run_update_async() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");

    println!("Checking for updates (current version: v{})...", version);

    let client = reqwest::Client::builder()
        .user_agent(format!("{}-updater", name))
        .build()?;

    let repo = "doroved/proxik";
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let release: Release = client
        .get(&url)
        .send()
        .await?
        .error_for_status()
        .context("Failed to get latest release from GitHub API")?
        .json()
        .await?;

    let latest_tag = release.tag_name.trim_start_matches('v');

    // Manual version comparison (semantic versioning: major.minor.patch)
    if !is_newer(latest_tag, version) {
        println!("You are already using the latest version (v{}).", version);
        return Ok(());
    }

    println!("New version found: v{}. Downloading...", latest_tag);

    // Find the correct asset for Linux x64 (musl)
    let expected_asset_name = format!("{}-{}-x86_64-unknown-linux-musl.tar.gz", name, latest_tag);
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected_asset_name)
        .context(format!(
            "Could not find {} in the latest release",
            expected_asset_name
        ))?;

    let response = client
        .get(&asset.url)
        .header("Accept", "application/octet-stream")
        .send()
        .await?
        .error_for_status()
        .context("Failed to download release asset")?;

    let content = response.bytes().await?;

    println!("Unpacking using system tar...");
    let temp_dir = std::env::temp_dir().join(format!("{}_update_dir", name));
    let archive_path = std::env::temp_dir().join(format!("{}_update.tar.gz", name));

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    // Write content to temporary file
    {
        let mut f = fs::File::create(&archive_path)?;
        f.write_all(&content)?;
    }

    // Use system tar for unpacking
    let tar_output = Command::new("tar")
        .args([
            "-xzf",
            &archive_path.to_string_lossy(),
            "--no-same-owner",
            "-C",
            &temp_dir.to_string_lossy(),
        ])
        .output()
        .context("Failed to execute system tar command")?;

    if !tar_output.status.success() {
        let err = String::from_utf8_lossy(&tar_output.stderr);
        return Err(anyhow::anyhow!("Tar extraction failed: {}", err));
    }

    let new_binary = temp_dir.join(name);
    if !new_binary.exists() {
        return Err(anyhow::anyhow!(
            "Binary '{}' not found in the downloaded archive",
            name
        ));
    }

    let target_path = format!("/usr/local/bin/{}", name);

    println!("Stopping daemon...");
    let _ = Command::new(&target_path).arg("stop").output();

    println!("Installing new binary to {}...", target_path);

    // To bypass "Text file busy" (OS error 26), we use fs::rename instead of fs::copy.
    if fs::rename(&new_binary, &target_path).is_err() {
        let _ = fs::remove_file(&target_path);
        fs::copy(&new_binary, &target_path)
            .context("Failed to overwrite the binary. Try running with sudo.")?;
    }

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_path, perms)?;
    }

    println!("Starting daemon...");
    let _ = Command::new(&target_path).arg("start").spawn()?;

    println!("Successfully updated to v{}!", latest_tag);

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
    let _ = fs::remove_file(&archive_path);

    Ok(())
}

/// Simple manual semantic version comparison.
/// Returns true if 'latest' is strictly greater than 'current'.
fn is_newer(latest: &str, current: &str) -> bool {
    let latest_parts: Vec<u32> = latest.split('.').filter_map(|s| s.parse().ok()).collect();
    let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..std::cmp::max(latest_parts.len(), current_parts.len()) {
        let l = latest_parts.get(i).cloned().unwrap_or(0);
        let c = current_parts.get(i).cloned().unwrap_or(0);

        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}
