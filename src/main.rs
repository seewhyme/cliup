mod config;
mod launchd;
mod logging;
mod runner;

use std::fs;
use std::str::FromStr;

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};

use crate::config::{Config, Package, PackageType};

#[derive(Debug, Parser)]
#[command(
    name = "cliup",
    version,
    about = "Update a whitelisted set of macOS CLI tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Add(AddArgs),
    Remove { name: String },
    List,
    Schedule(ScheduleArgs),
    Run(RunArgs),
    Log(LogArgs),
    Doctor,
    InstallLaunchd,
    UninstallLaunchd,
    Status,
}

#[derive(Debug, Args)]
struct AddArgs {
    #[arg(value_name = "type")]
    package_type: String,
    name: String,
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ScheduleArgs {
    hour: u8,
    minute: u8,
}

#[derive(Debug, Args)]
struct LogArgs {
    #[arg(short = 'n', default_value_t = 100)]
    lines: usize,
}

fn main() {
    if let Err(error) = try_main() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init(),
        Commands::Add(args) => add(args),
        Commands::Remove { name } => remove(&name),
        Commands::List => list(),
        Commands::Schedule(args) => schedule(args),
        Commands::Run(args) => {
            let config = Config::read()?;
            runner::run_updates(&config, args.dry_run)
        }
        Commands::Log(args) => logging::print_tail(&config::log_path()?, args.lines),
        Commands::Doctor => {
            let config = Config::read().ok();
            runner::doctor(config.as_ref())
        }
        Commands::InstallLaunchd => {
            let config = Config::read()?;
            launchd::install(&config)
        }
        Commands::UninstallLaunchd => launchd::uninstall(),
        Commands::Status => launchd::status(),
    }
}

fn init() -> Result<()> {
    let config_path = config::config_path()?;
    if config_path.exists() {
        println!("config already exists: {}", config_path.display());
        return Ok(());
    }

    fs::create_dir_all(config::log_dir()?)?;
    Config::default_config().write()?;
    println!("created {}", config_path.display());
    Ok(())
}

fn add(args: AddArgs) -> Result<()> {
    let package_type = PackageType::from_str(&args.package_type)?;
    let command = if package_type == PackageType::SelfUpdate {
        if args.command.is_empty() {
            bail!(
                "self package requires command, for example: cliup add self claude claude update"
            );
        }
        Some(args.command.join(" "))
    } else {
        if !args.command.is_empty() {
            bail!("npm/brew/cask packages do not accept command arguments");
        }
        None
    };

    let mut config = Config::read()?;
    let package = Package::new(package_type, args.name, command)?;
    let added = config.add_package(package.clone());

    if added {
        config.write()?;
        print_package("added", &package);
    } else {
        print_package("already exists", &package);
    }

    Ok(())
}

fn remove(name: &str) -> Result<()> {
    let mut config = Config::read()?;
    let removed = config.remove_by_name(name);
    if removed == 0 {
        println!("not found: {name}");
    } else {
        config.write()?;
        println!("removed {removed} item(s)");
    }
    Ok(())
}

fn list() -> Result<()> {
    let config = Config::read()?;
    if config.packages.is_empty() {
        println!("empty");
        return Ok(());
    }

    for package in &config.packages {
        match package.package_type {
            PackageType::SelfUpdate => println!(
                "{:<6} {} -> {}",
                package.package_type,
                package.name,
                package.command.as_deref().unwrap_or("")
            ),
            _ => println!("{:<6} {}", package.package_type, package.name),
        }
    }

    Ok(())
}

fn schedule(args: ScheduleArgs) -> Result<()> {
    let mut config = Config::read()?;
    config.schedule.hour = args.hour;
    config.schedule.minute = args.minute;
    config.write()?;
    println!("schedule updated: {:02}:{:02}", args.hour, args.minute);
    println!("run `cliup install-launchd` to apply it to launchd");
    Ok(())
}

fn print_package(prefix: &str, package: &Package) {
    match package.package_type {
        PackageType::SelfUpdate => println!(
            "{}: {} {} -> {}",
            prefix,
            package.package_type,
            package.name,
            package.command.as_deref().unwrap_or("")
        ),
        _ => println!("{}: {} {}", prefix, package.package_type, package.name),
    }
}
