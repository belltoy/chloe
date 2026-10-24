//! Consolidation (B1-04): turn captured sessions into reviewable candidates through the job
//! queue. `plan` decides which sessions are due and enqueues one job each; `run_next` runs
//! one job. Nothing here knows about a database or a model: stores and the extractor come in
//! through the core traits, and there is no daemon — the caller loops.

mod context;
mod embed;
mod eval;
mod hybrid;
mod policy;
mod propose;

pub use context::{
    ContextPack, DEFAULT_BUDGET, MAX_BUDGET, MIN_BUDGET, PackGroup, PackItem, context_pack,
};
pub use embed::{EMBED_JOB_KIND, EmbedDone, EmbedPlan, plan_embeddings, run_next_embedding};
pub use eval::{Dataset, EvalMemory, EvalQuery, MODES, Report, Row, run_eval};
pub use hybrid::{HybridHit, HybridResult, SearchMode, fts_text, hybrid_search, search};
pub use policy::{Outcome, PolicyReport, apply_policy, evaluate};
pub use propose::{ProposeReport, propose_pending};

use memory_core::extract::{CandidateStore, ExtractionInput, MemoryExtractor};
use memory_core::job::JobQueue;
use memory_core::{AgentEvent, Error, EventStore, Result, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Duration, OffsetDateTime};

pub const JOB_KIND: &str = "extract_session";
/// Longer than the model call's timeout, so a slow call is not taken over mid-flight.
const LEASE: Duration = Duration::minutes(10);
/// Sessions looked at per `plan` call.
const SESSION_SCAN: usize = 1000;
/// Rough per-event cost in the prompt besides its text (ref, kind, tool name).
const EVENT_OVERHEAD: usize = 40;

/// How much of a session the model sees. The most recent events are kept: the end of a
/// session usually holds the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub max_events: usize,
    pub max_chars: usize,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            max_events: 200,
            max_chars: 20_000,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub enqueued: usize,
    /// Due sessions that already have a job (done, waiting or failed).
    pub already_queued: usize,
}

/// Enqueues one job per session that is due: ended, or quiet for at least `idle`. With `only`
/// (a native session id) that session is enqueued whatever its state. Only sessions with events
/// no earlier job covered are enqueued, one job per `last_seq`, so a session resumed after
/// extraction is extracted again from where it stopped.
pub fn plan<S: EventStore + JobQueue>(
    store: &S,
    now: OffsetDateTime,
    idle: Duration,
    window: Window,
    only: Option<&str>,
) -> Result<Plan> {
    let mut plan = Plan::default();
    let sessions = if only.is_some() {
        store.sessions(SESSION_SCAN)?
    } else {
        store.sessions_to_extract(SESSION_SCAN)?
    };
    if let Some(id) = only
        && !sessions.iter().any(|s| s.native_session_id == id)
    {
        return Err(Error::NotFound(format!("session {id}")));
    }
    for s in sessions {
        if s.last_seq <= s.extracted_seq {
            if only.is_some_and(|id| s.native_session_id == id) {
                plan.already_queued += 1;
            }
            continue;
        }
        let due = match only {
            Some(id) => s.native_session_id == id,
            None => s.state == SessionState::Ended || s.last_event_at + idle <= now,
        };
        if !due {
            continue;
        }
        let payload = json!({
            "agent": s.native_agent,
            "session": s.native_session_id,
            "window": window,
            "through": s.last_seq,
        });
        let key = format!("{}|{}|{}", s.native_agent, s.native_session_id, s.last_seq);
        if store.enqueue(JOB_KIND, &key, payload, now)?.created {
            plan.enqueued += 1;
        } else {
            plan.already_queued += 1;
        }
    }
    Ok(plan)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Done {
    pub session: String,
    pub result: std::result::Result<usize, Failure>,
}

/// A failed attempt: events are kept and the job retries on its backoff, or waits in the
/// dead letter for `chloe jobs retry`.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub error: String,
    pub dead_letter: bool,
}

#[derive(Deserialize)]
struct Payload {
    agent: String,
    session: String,
    window: Window,
    /// Last event the job covers; jobs queued before watermarks existed lack it.
    through: Option<i64>,
}

/// The newest events that fit the window; at least the last one.
pub fn select(mut events: Vec<AgentEvent>, w: Window) -> Vec<AgentEvent> {
    let skip = events.len().saturating_sub(w.max_events);
    events.drain(..skip);
    let mut used = 0;
    let mut keep = 0;
    for e in events.iter().rev() {
        used += e
            .text
            .as_deref()
            .map_or(0, |t| t.chars().count().min(w.max_chars))
            + EVENT_OVERHEAD;
        if used > w.max_chars && keep > 0 {
            break;
        }
        keep += 1;
    }
    events.drain(..events.len() - keep);
    events
}

/// Leases and runs one job. `Ok(None)` when nothing is due. A provider that is down is an
/// ordinary failed attempt, not an error of this call.
pub fn run_next<S>(
    store: &S,
    extractor: &dyn MemoryExtractor,
    worker: &str,
    now: OffsetDateTime,
) -> Result<Option<Done>>
where
    S: EventStore + JobQueue + CandidateStore,
{
    let Some(job) = store.lease_kind(JOB_KIND, worker, LEASE, now)? else {
        return Ok(None);
    };
    let lease = job.lease_id.clone().unwrap_or_default();
    let fail = |session: String, error: String| -> Result<Option<Done>> {
        let j = store.fail(&job.id, &lease, &error, now)?;
        Ok(Some(Done {
            session,
            result: Err(Failure {
                error,
                dead_letter: j.state == memory_core::job::JobState::DeadLetter,
            }),
        }))
    };
    let p: Payload = match serde_json::from_value(job.payload.clone()) {
        Ok(p) => p,
        Err(_) => return fail(job.dedup_key.clone(), "unreadable job payload".into()),
    };
    let info = match store.session(&p.agent, &p.session) {
        Ok(i) => i,
        Err(e) => return fail(p.session, e.to_string()),
    };
    let through = p.through.unwrap_or(info.last_seq);
    let after = info.extracted_seq;
    if after >= through {
        // An earlier attempt already committed this range.
        store.complete(&job.id, &lease, now)?;
        return Ok(Some(Done {
            session: p.session,
            result: Ok(0),
        }));
    }
    let events = select(
        store.session_window(&p.agent, &p.session, after, through, p.window.max_events)?,
        p.window,
    );
    let has_text = events
        .iter()
        .any(|e| e.text.as_deref().is_some_and(|t| !t.trim().is_empty()));
    let found = if has_text {
        let input = ExtractionInput {
            native_agent: p.agent.clone(),
            native_session_id: p.session.clone(),
            events,
        };
        match extractor.extract(&input) {
            Ok(c) => store.add_candidates_through(&p.agent, &p.session, &c, through),
            Err(e) => return fail(p.session, e.to_string()),
        }
    } else {
        store.add_candidates_through(&p.agent, &p.session, &[], through)
    };
    match found {
        Ok(n) => {
            store.complete(&job.id, &lease, now)?;
            Ok(Some(Done {
                session: p.session,
                result: Ok(n),
            }))
        }
        Err(e) => fail(p.session, e.to_string()),
    }
}
