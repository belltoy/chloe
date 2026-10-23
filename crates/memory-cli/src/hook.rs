//! `chloe hook ...`: the Claude Code hook entry point and its installer.

use std::io::Read;
use std::path::PathBuf;

use anyhow::Context;
use clap::ValueEnum;
use memory_core::Config;
use memory_ingest::{claude, spool};
use time::OffsetDateTime;

/// Payloads above this are dropped rather than cut, since a cut JSON object cannot be parsed.
const MAX_STDIN_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
}

/// The hook process. It must never block or fail the agent: no database, no logging setup,
/// nothing on stdout (Claude Code feeds some hook stdout back to the model), exit code 0.
/// `CHLOE_CAPTURE=off` turns capture off.
pub fn run_claude(db_path: Option<PathBuf>) {
    if std::env::var("CHLOE_CAPTURE").is_ok_and(|v| v == "off") {
        return;
    }
    if let Err(e) = capture(db_path) {
        eprintln!("chloe hook: {e:#}");
    }
}

fn capture(db_path: Option<PathBuf>) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_end(&mut buf)?;
    anyhow::ensure!(
        buf.len() as u64 <= MAX_STDIN_BYTES,
        "payload over {MAX_STDIN_BYTES} bytes dropped"
    );
    let Some(event) = claude::parse_hook(&buf, OffsetDateTime::now_utc())? else {
        return Ok(());
    };
    let config = Config::resolve(db_path, None)?;
    spool::append(&spool::path_for(&config.db_path), &event).context("writing the event spool")
}

fn default_settings(agent: Agent) -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("cannot find the home directory; pass --settings")?;
    Ok(match agent {
        Agent::Claude => PathBuf::from(home).join(".claude").join("settings.json"),
    })
}

pub fn edit(agent: Agent, settings: Option<PathBuf>, install: bool) -> anyhow::Result<()> {
    let path = match settings {
        Some(p) => p,
        None => default_settings(agent)?,
    };
    let changed = claude::edit_settings_file(&path, install)?;
    let verb = if install { "installed" } else { "removed" };
    if changed {
        println!(
            "{verb} chloe hooks ({}) in {}",
            claude::EVENTS.join(", "),
            path.display()
        );
        if install {
            println!(
                "restart running Claude Code sessions to pick them up; set CHLOE_CAPTURE=off to pause capture"
            );
        }
    } else {
        println!(
            "nothing to do: {} already {}",
            path.display(),
            if install {
                "has chloe hooks"
            } else {
                "lacks them"
            }
        );
    }
    Ok(())
}
