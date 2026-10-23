//! `chloe events`: import the hook spool into the event table and inspect what was captured.

use std::path::PathBuf;

use anyhow::Context;
use clap::Subcommand;
use memory_core::{Config, EventStore};
use memory_ingest::spool;
use memory_store::SqliteStore;
use serde_json::json;

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Import the hook spool into the database; importing twice adds nothing
    Import {
        /// Spool file [default: next to the database]
        #[arg(long)]
        spool: Option<PathBuf>,
    },
    /// Most recently active sessions
    Sessions {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Events of one session, by the agent's own session id
    List {
        session: String,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
}

pub fn run(
    store: &SqliteStore,
    config: &Config,
    command: EventsCommand,
    json: bool,
) -> anyhow::Result<()> {
    match command {
        EventsCommand::Import { spool } => {
            let path = spool.unwrap_or_else(|| spool::path_for(&config.db_path));
            let read = spool::read(&path).with_context(|| format!("reading {}", path.display()))?;
            let report = store.ingest(&read.events)?;
            if json {
                crate::print_json(
                    &json!({"spool": path, "report": report, "bad_lines": read.bad_lines}),
                )?;
            } else {
                println!(
                    "{}: {} new, {} already stored, {} rejected, {} unreadable lines",
                    path.display(),
                    report.inserted,
                    report.duplicates,
                    report.rejected.len(),
                    read.bad_lines.len()
                );
                for r in &report.rejected {
                    println!("  event #{} rejected: {}", r.index + 1, r.reason);
                }
                if !read.bad_lines.is_empty() {
                    println!("  unreadable line numbers: {:?}", read.bad_lines);
                }
            }
        }
        EventsCommand::Sessions { limit } => {
            let sessions = store.sessions(limit)?;
            if json {
                crate::print_json(&sessions)?;
            }
            for s in sessions.iter().filter(|_| !json) {
                println!(
                    "{}  {:?}  {} events  {}  {}  project={}",
                    s.native_session_id,
                    s.state,
                    s.events,
                    s.native_agent,
                    s.last_event_at,
                    s.project_id.map_or("-".into(), |p| p.to_string())
                );
            }
        }
        EventsCommand::List { session, limit } => {
            let events = store.session_events(&session, limit)?;
            if json {
                crate::print_json(&events)?;
            }
            for e in events.iter().filter(|_| !json) {
                println!(
                    "{}  {}  {}  {}",
                    e.occurred_at,
                    e.kind.as_str(),
                    e.tool_name.as_deref().unwrap_or("-"),
                    e.text.as_deref().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}
