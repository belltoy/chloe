//! `chloe candidates`: the review queue. Candidates are proposals from extraction; a person
//! decides here. Every decision is stored on the candidate and, for accept and merge, in the
//! memory's sources (`chloe memory sources`).

use clap::Subcommand;
use memory_core::MemoryId;
use memory_core::extract::{
    CandidateState, CandidateStore, MemorySource, Resolution, StoredCandidate,
};
use memory_store::SqliteStore;

use crate::print_json;

#[derive(Subcommand)]
pub enum CandidatesCommand {
    /// Candidates waiting for a decision (pending or flagged as a conflict)
    List {
        /// pending, accepted, rejected, merged, conflict, or all [default: open ones]
        #[arg(long, value_parser = parse_filter)]
        state: Option<Filter>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// One candidate in full: text, evidence, suggestion and decision
    Show { id: String },
    /// Turn the candidate into a memory
    Accept {
        id: String,
        /// Store at global scope (required when the session's directory is not a project)
        #[arg(long)]
        global: bool,
    },
    Reject {
        id: String,
        #[arg(long, default_value = "rejected by reviewer")]
        reason: String,
    },
    /// Keep an existing memory and record the candidate as another source of it
    Merge {
        id: String,
        #[arg(long)]
        into: MemoryId,
        #[arg(long, default_value = "merged by reviewer")]
        reason: String,
    },
}

#[derive(Clone, Copy)]
pub enum Filter {
    All,
    State(CandidateState),
}

fn parse_filter(s: &str) -> Result<Filter, String> {
    if s == "all" {
        return Ok(Filter::All);
    }
    serde_json::from_value(s.into())
        .map(Filter::State)
        .map_err(|_| "expected one of: pending, accepted, rejected, merged, conflict, all".into())
}

fn line(c: &StoredCandidate) -> String {
    format!(
        "{}  {:?}  {}  {}  ({} event(s), session {})",
        c.id,
        c.candidate.kind,
        c.state.as_str(),
        c.candidate.title,
        c.candidate.evidence.events.len(),
        c.candidate.evidence.native_session_id
    )
}

fn suggestion(c: &StoredCandidate) {
    if let Some(p) = &c.proposal {
        println!(
            "    suggest {:?}{}: {} [{:?}]",
            p.action,
            p.target.map(|t| format!(" {t}")).unwrap_or_default(),
            p.reason,
            p.method
        );
        if let Some(why) = &p.needs_review {
            println!("    needs a person: {why}");
        }
    }
}

pub fn run(store: &SqliteStore, command: CandidatesCommand, json: bool) -> anyhow::Result<()> {
    match command {
        CandidatesCommand::List { state, limit } => {
            let mut all = match state {
                None => {
                    let mut v = store.candidates(Some(CandidateState::Pending), limit)?;
                    v.extend(store.candidates(Some(CandidateState::Conflict), limit)?);
                    v.sort_by_key(|c| std::cmp::Reverse(c.created_at));
                    v.truncate(limit);
                    v
                }
                Some(Filter::All) => store.candidates(None, limit)?,
                Some(Filter::State(s)) => store.candidates(Some(s), limit)?,
            };
            all.truncate(limit);
            if json {
                return print_json(&all);
            }
            for c in &all {
                println!("{}", line(c));
                suggestion(c);
            }
            if all.is_empty() {
                println!("no candidates");
            }
        }
        CandidatesCommand::Show { id } => {
            let c = store.candidate(&id)?;
            if json {
                return print_json(&c);
            }
            let e = &c.candidate.evidence;
            println!("{}\n\n{}\n", line(&c), c.candidate.content);
            println!(
                "evidence: {} session {}",
                e.native_agent, e.native_session_id
            );
            for k in &e.events {
                println!("  event {k}");
            }
            if let Some(f) = &e.file {
                println!("  file {f}");
            }
            if let Some(h) = &e.commit {
                println!("  commit {h}");
            }
            suggestion(&c);
            if let (Some(at), Some(r)) = (c.decided_at, &c.reason) {
                println!("decided {at}: {r}");
            }
            if let Some(m) = c.memory_id {
                println!("memory {m}");
            }
        }
        CandidatesCommand::Accept { id, global } => {
            decide(store, &id, Resolution::Accept { global }, json)?
        }
        CandidatesCommand::Reject { id, reason } => {
            decide(store, &id, Resolution::Reject { reason }, json)?
        }
        CandidatesCommand::Merge { id, into, reason } => {
            decide(store, &id, Resolution::Merge { into, reason }, json)?
        }
    }
    Ok(())
}

fn decide(store: &SqliteStore, id: &str, resolution: Resolution, json: bool) -> anyhow::Result<()> {
    let c = store.resolve_candidate(id, resolution)?;
    if json {
        return print_json(&c);
    }
    println!("{}", line(&c));
    if let Some(m) = c.memory_id {
        println!("memory {m}");
    }
    Ok(())
}

/// `chloe memory sources`: the candidates a memory came from or absorbed.
pub fn print_sources(sources: &[MemorySource], json: bool) -> anyhow::Result<()> {
    if json {
        return print_json(&sources);
    }
    for s in sources {
        let e = &s.evidence;
        println!(
            "{}  {:?}  {}  {} session {}, {} event(s){}{}",
            s.recorded_at,
            s.relation,
            s.candidate_id,
            e.native_agent,
            e.native_session_id,
            e.events.len(),
            e.file
                .as_ref()
                .map(|f| format!(", file {f}"))
                .unwrap_or_default(),
            e.commit
                .as_ref()
                .map(|c| format!(", commit {c}"))
                .unwrap_or_default(),
        );
    }
    if sources.is_empty() {
        println!("no extracted sources (created or edited by hand)");
    }
    Ok(())
}
