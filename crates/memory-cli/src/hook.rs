//! `chloe hook ...`: the Claude Code hook entry point and its installer.

use std::io::Read;
use std::path::PathBuf;

use anyhow::Context;
use clap::ValueEnum;
use memory_core::Config;
use memory_ingest::hooks::{self, CLAUDE, CODEX, Spec};
use memory_ingest::spool;
use time::OffsetDateTime;

/// Payloads above this are dropped rather than cut, since a cut JSON object cannot be parsed.
const MAX_STDIN_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn spec(self) -> &'static Spec {
        match self {
            Agent::Claude => &CLAUDE,
            Agent::Codex => &CODEX,
        }
    }
}

/// The hook process (same for every agent; `spec` says which one). It must never block or fail the agent: no database, no logging setup,
/// nothing on stdout (Claude Code feeds some hook stdout back to the model), exit code 0.
/// `CHLOE_CAPTURE=off` turns capture off.
pub fn run(spec: &Spec, db_path: Option<PathBuf>) {
    if std::env::var("CHLOE_CAPTURE").is_ok_and(|v| v == "off") {
        return;
    }
    if let Err(e) = capture(spec, db_path) {
        eprintln!("chloe hook: {e:#}");
    }
}

fn capture(spec: &Spec, db_path: Option<PathBuf>) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_end(&mut buf)?;
    anyhow::ensure!(
        buf.len() as u64 <= MAX_STDIN_BYTES,
        "payload over {MAX_STDIN_BYTES} bytes dropped"
    );
    let Some(event) = hooks::parse_hook(spec, &buf, OffsetDateTime::now_utc())? else {
        return Ok(());
    };
    let config = Config::resolve(db_path, None)?;
    spool::enqueue(&spool::dir_for(&config.db_path), &event).context("queueing the event")
}

fn default_settings(agent: Agent) -> anyhow::Result<PathBuf> {
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .context("cannot find the home directory; pass --settings")
    };
    Ok(match agent {
        Agent::Claude => home()?.join(".claude").join("settings.json"),
        Agent::Codex => match std::env::var_os("CODEX_HOME") {
            Some(d) => PathBuf::from(d),
            None => home()?.join(".codex"),
        }
        .join("hooks.json"),
    })
}

pub fn edit(agent: Agent, settings: Option<PathBuf>, install: bool) -> anyhow::Result<()> {
    let path = match settings {
        Some(p) => p,
        None => default_settings(agent)?,
    };
    let spec = agent.spec();
    let changed = hooks::edit_settings_file(spec, &path, install)?;
    let verb = if install { "installed" } else { "removed" };
    if changed {
        println!(
            "{verb} chloe hooks ({}) in {}",
            spec.events.join(", "),
            path.display()
        );
        if install {
            match agent {
                Agent::Claude => println!("restart running Claude Code sessions to pick them up"),
                Agent::Codex => println!(
                    "Codex ignores a hook until you trust it: open Codex, run /hooks and approve the `{}` entries, then start a new session",
                    spec.command
                ),
            }
            println!("set CHLOE_CAPTURE=off to pause capture");
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
