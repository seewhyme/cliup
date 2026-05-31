use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Local;

use crate::config;

pub struct Logger {
    file: File,
}

impl Logger {
    pub fn open() -> Result<Self> {
        let log_dir = config::log_dir()?;
        fs::create_dir_all(&log_dir)
            .with_context(|| format!("failed to create log directory {}", log_dir.display()))?;

        let path = config::log_path()?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open log file {}", path.display()))?;

        Ok(Self { file })
    }

    pub fn run_header(&mut self) -> Result<()> {
        self.line("")?;
        self.line("============================================================")?;
        self.line(&format!("run started at {}", timestamp()))?;
        self.line("============================================================")?;
        Ok(())
    }

    pub fn line(&mut self, message: &str) -> Result<()> {
        writeln!(self.file, "{message}").context("failed to write log")?;
        self.file.flush().context("failed to flush log")?;
        Ok(())
    }

    pub fn info(&mut self, message: &str) -> Result<()> {
        self.line(&format!("[{}] {message}", timestamp()))
    }
}

pub fn log_command_result(
    logger: &mut Logger,
    command: &str,
    stdout: &[u8],
    stderr: &[u8],
    code: Option<i32>,
) -> Result<()> {
    logger.line(&format!("$ {command}"))?;
    logger.line(&format!("exit code: {}", format_exit_code(code)))?;
    logger.line("stdout:")?;
    write_block(logger, stdout)?;
    logger.line("stderr:")?;
    write_block(logger, stderr)?;
    Ok(())
}

pub fn print_tail(path: &Path, lines: usize) -> Result<()> {
    if !path.exists() {
        println!("no log found");
        return Ok(());
    }
    if lines == 0 {
        return Ok(());
    }

    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut tail = VecDeque::with_capacity(lines);

    for line in reader.lines() {
        let line = line.context("failed to read log line")?;
        if tail.len() == lines {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    for line in tail {
        println!("{line}");
    }

    Ok(())
}

pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn write_block(logger: &mut Logger, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        logger.line("(empty)")?;
        return Ok(());
    }

    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        logger.line(line)?;
    }
    Ok(())
}

fn format_exit_code(code: Option<i32>) -> String {
    code.map_or_else(
        || "terminated by signal".to_string(),
        |code| code.to_string(),
    )
}
