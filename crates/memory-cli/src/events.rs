//! `chloe events`: import the hook spool into the event table and inspect what was captured.

use std::path::PathBuf;

use anyhow::Context;
use clap::Subcommand;
use memory_core::{Config, EventStore};
use memory_ingest::spool;
use memory_store::SqliteStore;
use serde::Serialize;
use serde_json::json;

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Import the hook queue into the database; running it again is harmless
    Import {
        /// Queue directory [default: `spool` next to the database]
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
        /// The agent, when several used the same session id
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
}

/// The agent a session id belongs to. Ids are only unique per agent, so an id used by several
/// agents needs `--agent`.
pub fn resolve_agent(
    store: &SqliteStore,
    session: &str,
    agent: Option<String>,
) -> memory_core::Result<String> {
    if let Some(a) = agent {
        return Ok(a);
    }
    let mut agents: Vec<String> = store
        .sessions(1000)?
        .into_iter()
        .filter(|s| s.native_session_id == session)
        .map(|s| s.native_agent)
        .collect();
    agents.sort();
    agents.dedup();
    match agents.len() {
        0 => Err(memory_core::Error::NotFound(format!("session {session}"))),
        1 => Ok(agents.remove(0)),
        _ => Err(memory_core::Error::InvalidArgument(format!(
            "session {session} exists for several agents ({}); pass --agent",
            agents.join(", ")
        ))),
    }
}

/// Files handled per run (and per transaction below); the rest wait for the next run.
const MAX_PER_RUN: usize = 10_000;
const BATCH: usize = 500;

#[derive(Debug, Default, Serialize)]
pub struct Totals {
    pub inserted: usize,
    pub duplicates: usize,
    /// Queue files that were not a readable event.
    pub unreadable: usize,
    /// Events the store refused (unsupported version, invalid field, too long).
    pub refused: usize,
}

/// Drain the queue into the database. A file is deleted only after the transaction that holds
/// its event has committed, so a crash anywhere re-imports it as a duplicate; two importers
/// running at once are harmless for the same reason.
pub fn import(store: &SqliteStore, dir: &std::path::Path) -> anyhow::Result<Totals> {
    let mut totals = Totals::default();
    let entries = spool::pending(dir, MAX_PER_RUN)
        .with_context(|| format!("reading the queue in {}", dir.display()))?;
    for chunk in entries.chunks(BATCH) {
        let (readable, unreadable): (Vec<_>, Vec<_>) =
            chunk.iter().partition(|e| e.event.is_some());
        for e in unreadable {
            spool::quarantine(dir, &e.path)?;
            totals.unreadable += 1;
        }
        let events: Vec<_> = readable.iter().filter_map(|e| e.event.clone()).collect();
        let report = store.ingest(&events)?;
        totals.inserted += report.inserted;
        totals.duplicates += report.duplicates;
        totals.refused += report.rejected.len();
        let refused: std::collections::HashSet<_> =
            report.rejected.iter().map(|r| r.index).collect();
        for (i, e) in readable.iter().enumerate() {
            if refused.contains(&i) {
                spool::quarantine(dir, &e.path)?;
            } else {
                spool::done(&e.path)?;
            }
        }
    }
    Ok(totals)
}

pub fn run(
    store: &SqliteStore,
    config: &Config,
    command: EventsCommand,
    json: bool,
) -> anyhow::Result<()> {
    match command {
        EventsCommand::Import { spool } => {
            let dir = spool.unwrap_or_else(|| spool::dir_for(&config.db_path));
            let totals = import(store, &dir)?;
            if json {
                crate::print_json(&json!({"spool": dir, "totals": totals}))?;
            } else {
                println!(
                    "{}: {} new, {} already stored, {} quarantined ({} unreadable, {} refused by the store)",
                    dir.display(),
                    totals.inserted,
                    totals.duplicates,
                    totals.unreadable + totals.refused,
                    totals.unreadable,
                    totals.refused
                );
                if totals.unreadable + totals.refused > 0 {
                    println!(
                        "  quarantined files are in {}",
                        dir.join("rejected").display()
                    );
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
        EventsCommand::List {
            session,
            agent,
            limit,
        } => {
            let agent = resolve_agent(store, &session, agent)?;
            let events = store.session_events(&agent, &session, limit)?;
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
