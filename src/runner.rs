use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result};

use crate::config::{self, Config, Package, PackageType};
use crate::logging::{self, Logger};

const BASE_PATHS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

pub fn run_updates(config: &Config, dry_run: bool) -> Result<()> {
    let mut logger = Logger::open()?;
    logger.run_header()?;

    if dry_run {
        logger.info("dry-run mode enabled; no update commands will be executed")?;
    }

    let env_path = effective_path();
    let mut brew_updated = false;

    for package in &config.packages {
        println!("checking {} {}", package.package_type, package.name);
        if let Err(error) = run_package(package, dry_run, &env_path, &mut brew_updated, &mut logger)
        {
            logger.info(&format!(
                "error while updating {} {}: {error:#}",
                package.package_type, package.name
            ))?;
            println!("error: {} {} ({error})", package.package_type, package.name);
        }
    }

    logger.info("run finished")?;
    println!("done");
    Ok(())
}

pub fn doctor(config: Option<&Config>) -> Result<()> {
    let env_path = effective_path();

    println!(
        "cliup: {}",
        env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    );
    println!("PATH: {}", env_path.to_string_lossy());

    let config_path = config::config_path()?;
    println!(
        "config: {} ({})",
        config_path.display(),
        if config_path.exists() {
            "exists"
        } else {
            "missing"
        }
    );

    let log_dir = config::log_dir()?;
    println!(
        "log dir: {} ({})",
        log_dir.display(),
        if log_dir.exists() {
            "exists"
        } else {
            "missing"
        }
    );

    print_tool("node", &["--version"], &env_path)?;
    print_tool("npm", &["--version"], &env_path)?;
    print_tool("brew", &["--version"], &env_path)?;
    print_path_only("launchctl", &env_path);

    if let Some(config) = config {
        if config.packages.is_empty() {
            println!("packages: empty");
        } else {
            println!("packages:");
            for package in &config.packages {
                let status = package_status(package, &env_path);
                println!(
                    "  {:<5} {:<36} {}",
                    package.package_type, package.name, status
                );
            }
        }
    }

    Ok(())
}

// Common user-level bin directories where tools managed by `self` packages
// (e.g. amp, rustup-installed binaries, bun) are installed. We add these
// unconditionally so that `sh -lc` invocations run under launchd — where the
// user's interactive shell rc files are not sourced — can still resolve the
// commands.
const USER_BIN_SUFFIXES: &[&str] = &[".local/bin", ".cargo/bin", ".bun/bin", "bin"];

pub fn effective_path() -> OsString {
    let mut entries: Vec<PathBuf> = BASE_PATHS
        .iter()
        .map(|entry| PathBuf::from(*entry))
        .collect();

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        for suffix in USER_BIN_SUFFIXES {
            let candidate = home.join(suffix);
            if !entries.iter().any(|entry| entry == &candidate) {
                entries.push(candidate);
            }
        }
    }

    if let Some(existing) = env::var_os("PATH") {
        for path in env::split_paths(&existing) {
            if !path.as_os_str().is_empty() && !entries.iter().any(|entry| entry == &path) {
                entries.push(path);
            }
        }
    }

    env::join_paths(entries).unwrap_or_else(|_| OsString::from(BASE_PATHS.join(":")))
}

pub fn command_exists(command: &str, env_path: &OsString) -> bool {
    find_command(command, env_path).is_some()
}

pub fn find_command(command: &str, env_path: &OsString) -> Option<PathBuf> {
    if command.contains('/') {
        let path = PathBuf::from(command);
        return is_executable_file(&path).then_some(path);
    }

    for dir in env::split_paths(env_path) {
        let candidate = dir.join(command);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn run_package(
    package: &Package,
    dry_run: bool,
    env_path: &OsString,
    brew_updated: &mut bool,
    logger: &mut Logger,
) -> Result<()> {
    match package.package_type {
        PackageType::Npm => run_npm(package, dry_run, env_path, logger),
        PackageType::Brew => run_brew_formula(package, dry_run, env_path, brew_updated, logger),
        PackageType::Cask => run_brew_cask(package, dry_run, env_path, brew_updated, logger),
        PackageType::SelfUpdate => run_self(package, dry_run, env_path, logger),
    }
}

fn run_npm(
    package: &Package,
    dry_run: bool,
    env_path: &OsString,
    logger: &mut Logger,
) -> Result<()> {
    if !command_exists("npm", env_path) {
        skip(logger, &format!("npm not found; skipping {}", package.name))?;
        return Ok(());
    }

    if dry_run {
        dry_run_log(
            logger,
            &command_display("npm", &["ls", "-g", "--depth=0", &package.name]),
        )?;
        let latest = format!("{}@latest", package.name);
        dry_run_log(logger, &command_display("npm", &["i", "-g", &latest]))?;
        println!(
            "dry-run: would check and update npm package {} if installed",
            package.name
        );
        return Ok(());
    }

    let installed = run_logged(
        "npm",
        &["ls", "-g", "--depth=0", &package.name],
        env_path,
        logger,
    )?
    .status
    .success();

    if !installed {
        skip(
            logger,
            &format!("npm package {} is not installed globally", package.name),
        )?;
        return Ok(());
    }

    let latest = format!("{}@latest", package.name);
    run_update_or_dry_run("npm", &["i", "-g", &latest], false, env_path, logger)
}

fn run_brew_formula(
    package: &Package,
    dry_run: bool,
    env_path: &OsString,
    brew_updated: &mut bool,
    logger: &mut Logger,
) -> Result<()> {
    if !command_exists("brew", env_path) {
        skip(
            logger,
            &format!("brew not found; skipping {}", package.name),
        )?;
        return Ok(());
    }

    if dry_run {
        maybe_brew_update(true, env_path, brew_updated, logger)?;
        dry_run_log(
            logger,
            &command_display("brew", &["list", "--formula", &package.name]),
        )?;
        dry_run_log(
            logger,
            &command_display("brew", &["upgrade", &package.name]),
        )?;
        println!(
            "dry-run: would check and update brew formula {} if installed",
            package.name
        );
        return Ok(());
    }

    maybe_brew_update(false, env_path, brew_updated, logger)?;

    let installed = run_logged(
        "brew",
        &["list", "--formula", &package.name],
        env_path,
        logger,
    )?
    .status
    .success();

    if !installed {
        skip(
            logger,
            &format!("brew formula {} is not installed", package.name),
        )?;
        return Ok(());
    }

    run_update_or_dry_run("brew", &["upgrade", &package.name], false, env_path, logger)
}

fn run_brew_cask(
    package: &Package,
    dry_run: bool,
    env_path: &OsString,
    brew_updated: &mut bool,
    logger: &mut Logger,
) -> Result<()> {
    if !command_exists("brew", env_path) {
        skip(
            logger,
            &format!("brew not found; skipping {}", package.name),
        )?;
        return Ok(());
    }

    if dry_run {
        maybe_brew_update(true, env_path, brew_updated, logger)?;
        dry_run_log(
            logger,
            &command_display("brew", &["list", "--cask", &package.name]),
        )?;
        dry_run_log(
            logger,
            &command_display("brew", &["upgrade", "--cask", &package.name]),
        )?;
        println!(
            "dry-run: would check and update brew cask {} if installed",
            package.name
        );
        return Ok(());
    }

    maybe_brew_update(false, env_path, brew_updated, logger)?;

    let installed = run_logged("brew", &["list", "--cask", &package.name], env_path, logger)?
        .status
        .success();

    if !installed {
        skip(
            logger,
            &format!("brew cask {} is not installed", package.name),
        )?;
        return Ok(());
    }

    run_update_or_dry_run(
        "brew",
        &["upgrade", "--cask", &package.name],
        false,
        env_path,
        logger,
    )
}

fn run_self(
    package: &Package,
    dry_run: bool,
    env_path: &OsString,
    logger: &mut Logger,
) -> Result<()> {
    // `self` packages run an arbitrary user-supplied command, so we trust the
    // command string and let the login shell resolve it. This avoids skipping
    // when the binary lives in a user-specific dir (e.g. ~/.local/bin) that
    // is not in the launchd-inherited PATH.
    let command = package.command.as_deref().unwrap_or_default();
    if dry_run {
        dry_run_log(logger, &format!("sh -lc {}", shell_quote(command)))?;
        println!("dry-run: sh -lc {}", shell_quote(command));
        return Ok(());
    }

    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .env("PATH", env_path)
        .output()
        .with_context(|| format!("failed to run self command for {}", package.name))?;
    logging::log_command_result(
        logger,
        &format!("sh -lc {}", shell_quote(command)),
        &output.stdout,
        &output.stderr,
        output.status.code(),
    )?;
    if !output.status.success() {
        logger.info(&format!(
            "command failed but run will continue: sh -lc {}",
            shell_quote(command)
        ))?;
    }
    Ok(())
}

fn maybe_brew_update(
    dry_run: bool,
    env_path: &OsString,
    brew_updated: &mut bool,
    logger: &mut Logger,
) -> Result<()> {
    if *brew_updated {
        return Ok(());
    }

    *brew_updated = true;
    run_update_or_dry_run("brew", &["update"], dry_run, env_path, logger)
}

fn run_update_or_dry_run(
    program: &str,
    args: &[&str],
    dry_run: bool,
    env_path: &OsString,
    logger: &mut Logger,
) -> Result<()> {
    if dry_run {
        dry_run_log(logger, &command_display(program, args))?;
        println!("dry-run: {}", command_display(program, args));
        return Ok(());
    }

    let output = run_logged(program, args, env_path, logger)?;
    if !output.status.success() {
        logger.info(&format!(
            "command failed but run will continue: {}",
            command_display(program, args)
        ))?;
    }
    Ok(())
}

fn run_logged(
    program: &str,
    args: &[&str],
    env_path: &OsString,
    logger: &mut Logger,
) -> Result<Output> {
    let display = command_display(program, args);
    let output = Command::new(program)
        .args(args)
        .env("PATH", env_path)
        .output()
        .with_context(|| format!("failed to run {display}"))?;
    logging::log_command_result(
        logger,
        &display,
        &output.stdout,
        &output.stderr,
        output.status.code(),
    )?;
    Ok(output)
}

fn package_status(package: &Package, env_path: &OsString) -> &'static str {
    match package.package_type {
        PackageType::Npm => {
            if !command_exists("npm", env_path) {
                return "unknown";
            }
            status_from_command("npm", &["ls", "-g", "--depth=0", &package.name], env_path)
        }
        PackageType::Brew => {
            if !command_exists("brew", env_path) {
                return "unknown";
            }
            status_from_command("brew", &["list", "--formula", &package.name], env_path)
        }
        PackageType::Cask => {
            if !command_exists("brew", env_path) {
                return "unknown";
            }
            status_from_command("brew", &["list", "--cask", &package.name], env_path)
        }
        PackageType::SelfUpdate => {
            if command_exists(&package.name, env_path) {
                "installed"
            } else {
                "missing"
            }
        }
    }
}

fn status_from_command(program: &str, args: &[&str], env_path: &OsString) -> &'static str {
    match Command::new(program)
        .args(args)
        .env("PATH", env_path)
        .output()
    {
        Ok(output) if output.status.success() => "installed",
        Ok(_) => "missing",
        Err(_) => "unknown",
    }
}

fn print_tool(program: &str, version_args: &[&str], env_path: &OsString) -> Result<()> {
    match find_command(program, env_path) {
        Some(path) => {
            let version = Command::new(program)
                .args(version_args)
                .env("PATH", env_path)
                .output()
                .ok()
                .and_then(|output| {
                    let text = if output.stdout.is_empty() {
                        String::from_utf8_lossy(&output.stderr).trim().to_string()
                    } else {
                        String::from_utf8_lossy(&output.stdout).trim().to_string()
                    };
                    (!text.is_empty()).then_some(text)
                })
                .unwrap_or_else(|| "version unknown".to_string());
            println!("{program}: {} ({})", path.display(), first_line(&version));
        }
        None => println!("{program}: missing"),
    }
    Ok(())
}

fn print_path_only(program: &str, env_path: &OsString) {
    match find_command(program, env_path) {
        Some(path) => println!("{program}: {}", path.display()),
        None => println!("{program}: missing"),
    }
}

fn skip(logger: &mut Logger, message: &str) -> Result<()> {
    logger.info(&format!("skip: {message}"))?;
    println!("skip: {message}");
    Ok(())
}

fn dry_run_log(logger: &mut Logger, command: &str) -> Result<()> {
    logger.info(&format!("dry-run: {command}"))
}

fn command_display(program: &str, args: &[&str]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '@' | ':' | '=' | '+')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
