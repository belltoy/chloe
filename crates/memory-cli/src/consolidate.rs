//! `chloe consolidate`: import the capture queue, queue every session that is due, and run
//! the extraction jobs in this process. Candidates land in `chloe candidates list`; nothing
//! becomes a memory without review.

use clap::Args;
use memory_core::Config;
use memory_core::job::JobQueue;
use memory_engine::{Window, apply_policy, plan, propose_pending, run_next};
use memory_ingest::spool;
use memory_store::SqliteStore;
use serde_json::json;
use time::{Duration, OffsetDateTime};

use crate::{events, llm::Provider, print_json};

#[derive(Args)]
pub struct Consolidate {
    /// Extract this session (the agent's own id) even if it has not ended or gone idle
    #[arg(long)]
    session: Option<String>,
    /// A session with no event for this long counts as finished
    #[arg(long, default_value_t = 30)]
    idle_minutes: i64,
    /// Most recent events shown to the model
    #[arg(long, default_value_t = Window::default().max_events)]
    max_events: usize,
    /// Rough character budget of those events
    #[arg(long, default_value_t = Window::default().max_chars)]
    max_chars: usize,
    /// Only suggest: do not auto-merge low-risk duplicates or flag conflicts
    #[arg(long)]
    no_auto_apply: bool,
    #[command(flatten)]
    provider: Provider,
}

pub fn run(
    store: &SqliteStore,
    config: &Config,
    args: Consolidate,
    json: bool,
) -> anyhow::Result<()> {
    let extractor = args.provider.open()?;
    let imported = events::import(store, &spool::dir_for(&config.db_path))?;
    let window = Window {
        max_events: args.max_events.max(1),
        max_chars: args.max_chars.max(1),
    };
    let planned = plan(
        store,
        OffsetDateTime::now_utc(),
        Duration::minutes(args.idle_minutes.max(0)),
        window,
        args.session.as_deref(),
    )?;

    let (mut extracted, mut candidates, mut failed) = (0, 0, vec![]);
    let worker = format!("cli-{}", std::process::id());
    while let Some(done) = run_next(store, &extractor, &worker, OffsetDateTime::now_utc())? {
        match done.result {
            Ok(n) => {
                extracted += 1;
                candidates += n;
            }
            Err(f) => failed.push((done.session, f)),
        }
    }

    // The same model compares new candidates with similar memories; when it is down those
    // candidates stay without a proposal and the next run tries again.
    let proposals = propose_pending(store, Some(&extractor))?;
    let applied = if args.no_auto_apply {
        Default::default()
    } else {
        apply_policy(store)?
    };
    let counts = store.counts()?;
    if json {
        return print_json(&json!({
            "imported_events": imported.inserted,
            "sessions_queued": planned.enqueued,
            "sessions_extracted": extracted,
            "new_candidates": candidates,
            "proposed": proposals.proposed,
            "proposals_deferred": proposals.deferred,
            "auto_merged": applied.merged,
            "flagged_conflicts": applied.flagged,
            "needs_review": applied.needs_review,
            "failed": failed.iter().map(|(s, f)| json!({
                "session": s, "error": f.error, "dead_letter": f.dead_letter,
            })).collect::<Vec<_>>(),
            "jobs": counts,
        }));
    }
    println!(
        "imported {} events, queued {} sessions ({} already queued), extracted {} sessions: {} new candidates",
        imported.inserted, planned.enqueued, planned.already_queued, extracted, candidates
    );
    for (session, f) in &failed {
        println!(
            "  {session}: {} ({})",
            f.error,
            if f.dead_letter {
                "gave up; `chloe jobs retry` after fixing the cause"
            } else {
                "events kept, will retry"
            }
        );
    }
    if proposals.proposed + proposals.deferred > 0 {
        println!(
            "proposals: {} made, {} deferred (model unavailable)",
            proposals.proposed, proposals.deferred
        );
    }
    if applied.merged + applied.flagged + applied.needs_review > 0 {
        println!(
            "policy: {} duplicates auto-merged, {} conflicts flagged, {} need a person",
            applied.merged, applied.flagged, applied.needs_review
        );
    }
    if candidates > 0 {
        println!("review them with `chloe candidates list`");
    }
    Ok(())
}

pub fn list(store: &SqliteStore, limit: usize, json: bool) -> anyhow::Result<()> {
    use memory_core::extract::{CandidateState, CandidateStore};
    // Open = waiting for a decision: pending, or flagged as a conflict.
    let mut all = store.candidates(Some(CandidateState::Pending), limit)?;
    all.extend(store.candidates(Some(CandidateState::Conflict), limit)?);
    all.sort_by_key(|c| std::cmp::Reverse(c.created_at));
    all.truncate(limit);
    if json {
        return print_json(&all);
    }
    for c in &all {
        println!(
            "{}  {:?}{}  {}  ({} event(s), session {})",
            c.id,
            c.candidate.kind,
            if c.state == CandidateState::Conflict {
                " [conflict]"
            } else {
                ""
            },
            c.candidate.title,
            c.candidate.evidence.events.len(),
            c.candidate.evidence.native_session_id
        );
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
    if all.is_empty() {
        println!("no pending candidates");
    }
    Ok(())
}
