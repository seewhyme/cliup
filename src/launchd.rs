use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::{self, Config};
use crate::runner;

const LABEL: &str = "com.user.cliup";

pub fn install(config: &Config) -> Result<()> {
    let wrapper_path = config::wrapper_path()?;
    let plist_path = config::plist_path()?;
    let log_dir = config::log_dir()?;

    fs::create_dir_all(config::cliup_bin_dir()?)?;
    fs::create_dir_all(config::launch_agents_dir()?)?;
    fs::create_dir_all(&log_dir)?;

    let _ = Command::new("launchctl")
        .arg("unload")
        .arg(&plist_path)
        .env("PATH", runner::effective_path())
        .output();

    let cliup_path = std::env::current_exe().context("failed to find current cliup executable")?;
    let wrapper = format!(
        "#!/bin/sh\nexport PATH=\"/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH\"\n{} run\n",
        shell_quote(&cliup_path.display().to_string())
    );
    fs::write(&wrapper_path, wrapper)
        .with_context(|| format!("failed to write wrapper {}", wrapper_path.display()))?;

    let mut permissions = fs::metadata(&wrapper_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&wrapper_path, permissions)
        .with_context(|| format!("failed to chmod {}", wrapper_path.display()))?;

    let plist = plist_content(config)?;
    fs::write(&plist_path, plist)
        .with_context(|| format!("failed to write plist {}", plist_path.display()))?;

    let output = Command::new("launchctl")
        .arg("load")
        .arg(&plist_path)
        .env("PATH", runner::effective_path())
        .output()
        .context("failed to run launchctl load")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("launchctl load failed: {}", stderr.trim());
    }

    println!("launchd installed");
    println!("cliup status 查看状态");
    println!("cliup log 查看日志");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist_path = config::plist_path()?;
    let _ = Command::new("launchctl")
        .arg("unload")
        .arg(&plist_path)
        .env("PATH", runner::effective_path())
        .output();

    if plist_path.exists() {
        fs::remove_file(&plist_path)
            .with_context(|| format!("failed to remove plist {}", plist_path.display()))?;
    }

    println!("launchd uninstalled");
    Ok(())
}

pub fn status() -> Result<()> {
    let output = Command::new("launchctl")
        .arg("list")
        .env("PATH", runner::effective_path())
        .output()
        .context("failed to run launchctl list")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains(LABEL) {
        println!("loaded");
    } else {
        println!("not loaded");
    }

    Ok(())
}

fn plist_content(config: &Config) -> Result<String> {
    let wrapper_path = config::wrapper_path()?;
    let out_path = config::log_dir()?.join("launchd.out.log");
    let err_path = config::log_dir()?.join("launchd.err.log");

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{wrapper}</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key>
    <integer>{hour}</integer>
    <key>Minute</key>
    <integer>{minute}</integer>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>StandardOutPath</key>
  <string>{out}</string>
  <key>StandardErrorPath</key>
  <string>{err}</string>
</dict>
</plist>
"#,
        wrapper = escape_xml(&wrapper_path.display().to_string()),
        hour = config.schedule.hour,
        minute = config.schedule.minute,
        out = escape_xml(&out_path.display().to_string()),
        err = escape_xml(&err_path.display().to_string())
    ))
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
