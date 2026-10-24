//! `chloe jobs`: inspect the background job queue and retry failures.

use clap::Subcommand;
use memory_core::job::{Job, JobQueue, JobState};
use memory_store::SqliteStore;
use time::OffsetDateTime;

use crate::print_json;

#[derive(Subcommand)]
pub enum JobsCommand {
    /// Recently changed jobs
    List {
        /// pending, leased, succeeded or dead_letter
        #[arg(long, value_parser = parse_state)]
        state: Option<JobState>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// One job in full, including its last error
    Show { id: String },
    /// Run a dead-lettered (or waiting) job again, now
    Retry { id: String },
}

fn parse_state(s: &str) -> Result<JobState, String> {
    serde_json::from_value(s.into())
        .map_err(|_| "expected one of: pending, leased, succeeded, dead_letter".into())
}

fn line(j: &Job) -> String {
    // A pending job that already ran is waiting for its next attempt.
    let state = match (j.state, j.attempts) {
        (JobState::Pending, n) if n > 0 => "retry",
        (s, _) => s.as_str(),
    };
    format!(
        "{}  {state}  {}  {}/{}  {}",
        j.id, j.kind, j.attempts, j.max_attempts, j.dedup_key
    )
}

pub fn run(store: &SqliteStore, command: JobsCommand, json: bool) -> anyhow::Result<()> {
    match command {
        JobsCommand::List { state, limit } => {
            let jobs = store.jobs(state, limit)?;
            if json {
                return print_json(&jobs);
            }
            jobs.iter().for_each(|j| println!("{}", line(j)));
        }
        JobsCommand::Show { id } => {
            let j = store.job(&id)?;
            if json {
                return print_json(&j);
            }
            println!("{}", line(&j));
            println!("next_run_at = {}", j.next_run_at);
            if let Some(e) = &j.last_error {
                println!("last_error  = {e}");
            }
            println!("payload     = {}", j.payload);
        }
        JobsCommand::Retry { id } => {
            let j = store.retry(&id, OffsetDateTime::now_utc())?;
            if json {
                return print_json(&j);
            }
            println!("{}", line(&j));
        }
    }
    Ok(())
}
