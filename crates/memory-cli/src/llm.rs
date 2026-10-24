//! `chloe llm`: check the local model server and try extraction on a captured session.
//! Nothing here saves memories; `extract` only prints what the model proposes.

use clap::{Args, Subcommand};
use memory_core::EventStore;
use memory_core::extract::{ExtractionInput, MemoryExtractor};
use memory_llm::{Ollama, OllamaConfig};
use memory_store::SqliteStore;

use crate::print_json;

#[derive(Args)]
pub struct Provider {
    /// Ollama server (plain http)
    #[arg(
        long,
        env = "CHLOE_OLLAMA_URL",
        default_value = "http://127.0.0.1:11434"
    )]
    url: String,
    #[arg(long, env = "CHLOE_LLM_MODEL", default_value = "qwen3:8b")]
    model: String,
    /// Allow a server that is not on this machine (session text is sent to it)
    #[arg(long)]
    allow_remote: bool,
}

impl Provider {
    pub fn open(self) -> memory_core::Result<Ollama> {
        Ollama::new(OllamaConfig {
            base_url: self.url,
            model: self.model,
            allow_remote: self.allow_remote,
            ..Default::default()
        })
    }
}

#[derive(Subcommand)]
pub enum LlmCommand {
    /// Is the model server reachable and the model pulled?
    Check(Provider),
    /// Propose memories for a captured session (prints them, saves nothing)
    Extract {
        /// The agent's own session id
        session: String,
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[command(flatten)]
        provider: Provider,
    },
}

/// `Ok(detail)` when usable, for both `llm check` and `doctor`.
pub fn status(ollama: &Ollama) -> memory_core::Result<String> {
    let model = &ollama.config().model;
    let have = ollama.models()?;
    if have
        .iter()
        .any(|m| m == model || m == &format!("{model}:latest"))
    {
        Ok(format!("{model} ready"))
    } else {
        Err(memory_core::Error::Unavailable(format!(
            "model {model} is not pulled (`ollama pull {model}`)"
        )))
    }
}

pub fn run(store: &SqliteStore, command: LlmCommand, json: bool) -> anyhow::Result<()> {
    match command {
        LlmCommand::Check(p) => {
            let detail = status(&p.open()?)?;
            if json {
                return print_json(&serde_json::json!({"ok": true, "detail": detail}));
            }
            println!("{detail}");
        }
        LlmCommand::Extract {
            session,
            limit,
            provider,
        } => {
            let events = store.session_events(&session, limit)?;
            let Some(first) = events.first() else {
                return Err(memory_core::Error::NotFound(format!("session {session}")).into());
            };
            let input = ExtractionInput {
                native_agent: first.native_agent.clone(),
                native_session_id: session,
                events,
            };
            let candidates = provider.open()?.extract(&input)?;
            if json {
                return print_json(&candidates);
            }
            for c in &candidates {
                println!("{:?}  {}\n  {}", c.kind, c.title, c.content);
            }
            if candidates.is_empty() {
                println!("no candidates");
            }
        }
    }
    Ok(())
}
