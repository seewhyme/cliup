use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_HOUR: u8 = 10;
pub const DEFAULT_MINUTE: u8 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schedule: Schedule,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    #[serde(rename = "type")]
    pub package_type: PackageType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageType {
    Npm,
    Brew,
    Cask,
    #[serde(rename = "self")]
    SelfUpdate,
}

impl Config {
    pub fn default_config() -> Self {
        Self {
            schedule: Schedule {
                hour: DEFAULT_HOUR,
                minute: DEFAULT_MINUTE,
            },
            packages: Vec::new(),
        }
    }

    pub fn read() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            bail!(
                "config not found at {}. Run `cliup init` first.",
                path.display()
            );
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let config: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn write(&self) -> Result<()> {
        self.validate()?;
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self).context("failed to serialize config")?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, format!("{content}\n")).with_context(|| {
            format!("failed to write temporary config at {}", tmp_path.display())
        })?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to replace config at {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.schedule.hour > 23 {
            bail!("schedule.hour must be between 0 and 23");
        }
        if self.schedule.minute > 59 {
            bail!("schedule.minute must be between 0 and 59");
        }

        for package in &self.packages {
            package.validate()?;
        }

        Ok(())
    }

    pub fn add_package(&mut self, package: Package) -> bool {
        if self.packages.iter().any(|existing| {
            existing.package_type == package.package_type && existing.name == package.name
        }) {
            return false;
        }

        self.packages.push(package);
        true
    }

    pub fn remove_by_name(&mut self, name: &str) -> usize {
        let original_len = self.packages.len();
        self.packages.retain(|package| package.name != name);
        original_len - self.packages.len()
    }
}

impl Package {
    pub fn new(package_type: PackageType, name: String, command: Option<String>) -> Result<Self> {
        let package = Self {
            package_type,
            name,
            command,
        };
        package.validate()?;
        Ok(package)
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("package name cannot be empty");
        }

        match self.package_type {
            PackageType::SelfUpdate => {
                if self.command.as_deref().unwrap_or("").trim().is_empty() {
                    bail!("self package `{}` requires a command", self.name);
                }
            }
            PackageType::Npm | PackageType::Brew | PackageType::Cask => {
                if self.command.is_some() {
                    bail!(
                        "{} package `{}` must not include a command",
                        self.package_type,
                        self.name
                    );
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Npm => write!(f, "npm"),
            Self::Brew => write!(f, "brew"),
            Self::Cask => write!(f, "cask"),
            Self::SelfUpdate => write!(f, "self"),
        }
    }
}

impl FromStr for PackageType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "npm" => Ok(Self::Npm),
            "brew" => Ok(Self::Brew),
            "cask" => Ok(Self::Cask),
            "self" => Ok(Self::SelfUpdate),
            _ => Err(anyhow!("type must be one of: npm, brew, cask, self")),
        }
    }
}

pub fn cliup_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".cliup"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(cliup_dir()?.join("config.json"))
}

pub fn log_dir() -> Result<PathBuf> {
    Ok(cliup_dir()?.join("logs"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(log_dir()?.join("update.log"))
}

pub fn cliup_bin_dir() -> Result<PathBuf> {
    Ok(cliup_dir()?.join("bin"))
}

pub fn wrapper_path() -> Result<PathBuf> {
    Ok(cliup_bin_dir()?.join("cliup-run.sh"))
}

pub fn launch_agents_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join("Library").join("LaunchAgents"))
}

pub fn plist_path() -> Result<PathBuf> {
    Ok(launch_agents_dir()?.join("com.user.cliup.plist"))
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow!("failed to find home directory"))
}
