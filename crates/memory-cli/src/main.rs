mod candidates;
mod consolidate;
mod doctor;
mod embeddings;
mod eval;
mod events;
mod hook;
mod jobs;
mod llm;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use memory_core::extract::CandidateStore;
use memory_core::service::{MemoryChanges, MemoryService, NewMemory, SearchQuery};
use memory_core::{Config, Kind, Memory, MemoryId, ProjectId, Source};
use memory_store::SqliteStore;
use serde::Serialize;
use tracing_subscriber::EnvFilter;

const MCP_EMBED_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Parser)]
#[command(
    name = "chloe",
    version,
    about = "Local-first long-term memory for coding agents"
)]
struct Cli {
    /// Path to the SQLite database [default: platform data dir]
    #[arg(long, global = true, env = "CHLOE_DB")]
    db_path: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, global = true, env = "CHLOE_LOG")]
    log_level: Option<String>,

    /// Emit machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    /// Directory that selects the project [default: current directory]
    #[arg(long, global = true)]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the resolved configuration
    Config,
    /// Create the database and register the current project
    Init,
    /// Project commands
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Memory commands
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// MCP server commands
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Captured agent events: import the hook spool, list sessions and events
    Events {
        #[command(subcommand)]
        command: events::EventsCommand,
    },
    /// Extract memory candidates from captured sessions (imports the capture queue first)
    Consolidate(consolidate::Consolidate),
    /// Memory candidates waiting for review
    Candidates {
        #[command(subcommand)]
        command: candidates::CandidatesCommand,
    },
    /// Background jobs: list the queue, inspect failures, retry them
    Jobs {
        #[command(subcommand)]
        command: jobs::JobsCommand,
    },
    /// Local model provider: check it, try extraction on a captured session
    Llm {
        #[command(subcommand)]
        command: llm::LlmCommand,
    },
    /// Embedding index: which model it is pinned to, backfill missing vectors
    Embeddings {
        #[command(subcommand)]
        command: embeddings::EmbeddingsCommand,
    },
    /// Score FTS, Vector and Hybrid retrieval on a labelled dataset (temporary database)
    Eval(eval::Eval),
    /// Agent hook capture (Claude Code, Codex)
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
    /// Check database, configuration and a full MCP round trip (non-zero exit on failure)
    Doctor {
        /// Rebuild the full-text index first
        #[arg(long)]
        reindex: bool,
    },
}

#[derive(Subcommand)]
enum HookCommand {
    /// Hook entry point for Claude Code: reads the payload on stdin, always exits 0
    Claude,
    /// Hook entry point for Codex: reads the payload on stdin, always exits 0
    Codex,
    /// Add chloe's hooks to the agent's settings file (other hooks are kept)
    Install {
        agent: hook::Agent,
        /// Hooks file [default: ~/.claude/settings.json or ~/.codex/hooks.json]
        #[arg(long)]
        settings: Option<PathBuf>,
    },
    /// Remove only chloe's hooks from the agent's settings file
    Uninstall {
        agent: hook::Agent,
        #[arg(long)]
        settings: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve MCP over stdio, scoped to the project of --dir / the current directory
    Serve {
        /// Retrieval paths (vector and hybrid embed each query with the local model and fall
        /// back to full-text when it is down)
        #[arg(long, value_enum, conflicts_with = "hybrid")]
        mode: Option<Mode>,
        /// Same as --mode hybrid
        #[arg(long)]
        hybrid: bool,
        #[command(flatten)]
        provider: llm::Provider,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Show the project resolved for the directory
    Show,
}

#[derive(Subcommand)]
enum MemoryCommand {
    Add {
        #[arg(long)]
        title: String,
        /// Memory body; `-` reads stdin
        #[arg(long)]
        content: String,
        #[arg(long, default_value = "fact", value_parser = parse_kind)]
        kind: Kind,
        /// Store at global scope instead of the current project
        #[arg(long)]
        global: bool,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Also return global memories
        #[arg(long)]
        global: bool,
        /// Include non-active memories
        #[arg(long)]
        all: bool,
        /// Retrieval paths: fts (default), vector, or hybrid (fused); the latter two fall back to
        /// full-text when the model is down
        #[arg(long, value_enum, conflicts_with = "hybrid")]
        mode: Option<Mode>,
        /// Same as --mode hybrid
        #[arg(long)]
        hybrid: bool,
        /// Show the paths taken, the filters applied and each hit's ranks
        #[arg(long)]
        explain: bool,
        #[command(flatten)]
        provider: llm::Provider,
    },
    /// A compact, source-linked briefing for a task, grouped by kind and bounded in size
    Context {
        /// What you are about to do
        task: String,
        /// Total characters of titles and summaries
        #[arg(long, default_value_t = memory_engine::DEFAULT_BUDGET)]
        budget: usize,
        /// Include global memories
        #[arg(long)]
        global: bool,
        /// Retrieval paths, as for `memory search`
        #[arg(long, value_enum, conflicts_with = "hybrid")]
        mode: Option<Mode>,
        /// Same as --mode hybrid
        #[arg(long)]
        hybrid: bool,
        #[command(flatten)]
        provider: llm::Provider,
    },
    List {
        /// List global memories instead of the project's
        #[arg(long)]
        global: bool,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Include non-active memories
        #[arg(long)]
        all: bool,
    },
    Show {
        id: MemoryId,
    },
    /// Show the append-only audit trail of a memory
    History {
        id: MemoryId,
    },
    /// Show which extracted candidates a memory was created from or merged with
    Sources {
        id: MemoryId,
    },
    /// Show what a memory superseded and what superseded it
    Links {
        id: MemoryId,
    },
    Update(UpdateArgs),
    Archive {
        id: MemoryId,
        /// Revision you last saw (optimistic concurrency)
        #[arg(long)]
        revision: u32,
    },
}

#[derive(Args)]
struct UpdateArgs {
    id: MemoryId,
    /// Revision you last saw (optimistic concurrency)
    #[arg(long)]
    revision: u32,
    #[arg(long)]
    title: Option<String>,
    /// New body; `-` reads stdin
    #[arg(long)]
    content: Option<String>,
    #[arg(long, value_parser = parse_kind)]
    kind: Option<Kind>,
}

fn parse_kind(s: &str) -> Result<Kind, String> {
    serde_json::from_value(s.into())
        .map_err(|_| "expected one of: fact, decision, procedure, constraint, gotcha".into())
}

fn body(s: String) -> anyhow::Result<String> {
    if s != "-" {
        return Ok(s);
    }
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

#[derive(Serialize)]
struct ProjectInfo {
    project_id: ProjectId,
    dir: PathBuf,
    db_path: PathBuf,
}

pub(crate) fn print_json(v: &impl Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn line(m: &Memory) -> String {
    format!(
        "{}  {:?}/{:?}  r{}  {}",
        m.id, m.kind, m.status, m.revision, m.title
    )
}

fn run(cli: Cli) -> anyhow::Result<bool> {
    let config = Config::resolve(cli.db_path.clone(), cli.log_level)?;
    // Logs go to stderr so stdout stays clean for JSON and MCP stdio.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let json = cli.json;

    if let Command::Config = cli.command {
        if json {
            print_json(&config)?;
        } else {
            println!("db_path   = {}", config.db_path.display());
            println!("log_level = {}", config.log_level);
        }
        return Ok(true);
    }

    if let Command::Eval(args) = cli.command {
        eval::run(args, json)?;
        return Ok(true);
    }

    if let Command::Hook { command } = cli.command {
        match command {
            HookCommand::Claude | HookCommand::Codex => unreachable!("handled in main"),
            HookCommand::Install { agent, settings } => {
                hook::edit(agent, settings, true)?;
                if cli.db_path.is_some() {
                    // The hook command line carries no database path.
                    eprintln!(
                        "note: --db-path is not recorded in the hook; hooks write to $CHLOE_DB or the default database. Set CHLOE_DB in the agent's environment to use this one."
                    );
                }
            }
            HookCommand::Uninstall { agent, settings } => hook::edit(agent, settings, false)?,
        }
        return Ok(true);
    }

    if let Command::Doctor { reindex } = cli.command {
        let checks = doctor::run(&config.db_path, reindex);
        let healthy = checks.iter().all(|c| c.ok);
        if json {
            print_json(&serde_json::json!({"healthy": healthy, "checks": checks}))?;
        } else {
            for c in &checks {
                println!(
                    "[{}] {}  {}",
                    if c.ok { "ok" } else { "FAIL" },
                    c.name,
                    c.detail
                );
            }
        }
        return Ok(healthy);
    }

    let actor = if matches!(cli.command, Command::Mcp { .. }) {
        "mcp"
    } else {
        "cli"
    };
    let store = SqliteStore::open(&config.db_path)?.with_actor(actor);
    let dir = match cli.dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let project = || store.resolve_project(&dir);

    match cli.command {
        Command::Config => unreachable!(),
        Command::Init | Command::Project { .. } => {
            let info = ProjectInfo {
                project_id: project()?,
                dir,
                db_path: config.db_path.clone(),
            };
            if json {
                print_json(&info)?;
            } else {
                println!("project_id = {}", info.project_id);
                println!("dir        = {}", info.dir.display());
                println!("db_path    = {}", info.db_path.display());
            }
        }
        Command::Mcp {
            command:
                McpCommand::Serve {
                    mode,
                    hybrid,
                    provider,
                },
        } => {
            let mode = Mode::pick(mode, hybrid);
            // A short timeout: an agent waits on every search, so a stalled model must not.
            let embedder = (mode != memory_engine::SearchMode::Fts)
                .then(|| provider.open_with_timeout(MCP_EMBED_TIMEOUT))
                .transpose()?;
            memory_mcp::serve_with(
                &store,
                mode,
                embedder
                    .as_ref()
                    .map(|e| e as &dyn memory_core::embed::Embedder),
                project()?,
                std::io::stdin().lock(),
                std::io::stdout().lock(),
            )?
        }
        Command::Events { command } => events::run(&store, &config, command, json)?,
        Command::Consolidate(args) => consolidate::run(&store, &config, args, json)?,
        Command::Candidates { command } => candidates::run(&store, command, json)?,
        Command::Llm { command } => llm::run(&store, command, json)?,
        Command::Embeddings { command } => embeddings::run(&store, command, json)?,
        Command::Jobs { command } => jobs::run(&store, command, json)?,
        Command::Doctor { .. } | Command::Hook { .. } | Command::Eval(_) => unreachable!(),
        Command::Memory { command } => match command {
            MemoryCommand::Add {
                title,
                content,
                kind,
                global,
            } => {
                let m = store.save(NewMemory {
                    project_id: if global { None } else { Some(project()?) },
                    kind,
                    title,
                    content: body(content)?,
                    source: Source::Manual,
                })?;
                if json {
                    print_json(&m)?
                } else {
                    println!("{}", line(&m))
                }
            }
            MemoryCommand::Context {
                task,
                budget,
                global,
                mode,
                hybrid,
                provider,
            } => {
                let mode = Mode::pick(mode, hybrid);
                let embedder = (mode != memory_engine::SearchMode::Fts)
                    .then(|| provider.open())
                    .transpose()?;
                let pack = memory_engine::context_pack(
                    &store,
                    mode,
                    embedder
                        .as_ref()
                        .map(|e| e as &dyn memory_core::embed::Embedder),
                    Some(project()?),
                    &task,
                    global,
                    budget,
                )?;
                if let Some(why) = &pack.degraded {
                    eprintln!("vector search skipped, full-text only: {why}");
                }
                if json {
                    print_json(&pack)?
                } else {
                    for g in &pack.groups {
                        println!("{:?}", g.kind);
                        for i in &g.items {
                            println!(
                                "  {}  {}  [{:?}, r{}]\n    {}{}",
                                i.id,
                                i.title,
                                i.source,
                                i.revision,
                                i.summary,
                                if i.summary_truncated {
                                    "  (cut: `chloe memory show` for all)"
                                } else {
                                    ""
                                }
                            );
                        }
                    }
                    println!(
                        "{} of {} characters used{}",
                        pack.used,
                        pack.budget,
                        if pack.omitted > 0 {
                            format!(", {} more relevant memories did not fit", pack.omitted)
                        } else {
                            String::new()
                        }
                    );
                }
            }
            MemoryCommand::Search {
                query,
                limit,
                global,
                all,
                mode,
                hybrid,
                explain,
                provider,
            } => {
                let mode = Mode::pick(mode, hybrid);
                let query = SearchQuery {
                    query,
                    limit,
                    include_global: global,
                    include_inactive: all,
                };
                if mode != memory_engine::SearchMode::Fts || explain {
                    let embedder = (mode != memory_engine::SearchMode::Fts)
                        .then(|| provider.open())
                        .transpose()?;
                    let (scope, include_global, include_inactive) =
                        (project()?, query.include_global, query.include_inactive);
                    let r = memory_engine::search(
                        &store,
                        mode,
                        embedder
                            .as_ref()
                            .map(|e| e as &dyn memory_core::embed::Embedder),
                        Some(scope),
                        query,
                    )?;
                    if let Some(why) = &r.degraded {
                        eprintln!("vector search skipped, full-text only: {why}");
                    }
                    if json {
                        print_json(&serde_json::json!({
                            "mode": r.mode,
                            "degraded": r.degraded,
                            "explain": explain.then(|| serde_json::json!({
                                "scope": scope,
                                "include_global": include_global,
                                "status_filter": if include_inactive { "any" } else { "active" },
                                "fts_returned": r.fts_returned,
                                "vector_returned": r.vector_returned,
                                "fusion": fusion_note(r.mode),
                            })),
                            "hits": r.hits,
                        }))?
                    } else {
                        if explain {
                            println!(
                                "scope: project {scope}{}; status: {}",
                                if include_global { " + global" } else { "" },
                                if include_inactive {
                                    "any"
                                } else {
                                    "active only"
                                },
                            );
                            println!(
                                "mode {}; paths: fts {} candidates, vector {}; {}",
                                r.mode.as_str(),
                                r.fts_returned,
                                match &r.degraded {
                                    Some(why) => format!("skipped ({why})"),
                                    None if r.mode == memory_engine::SearchMode::Fts =>
                                        "not used (mode fts)".into(),
                                    None => format!("{} candidates", r.vector_returned),
                                },
                                fusion_note(r.mode),
                            );
                        }
                        for h in r.hits {
                            print!("{:.4}  {}", h.hit.score, line(&h.hit.memory));
                            if explain {
                                let rank =
                                    |r: Option<usize>| r.map_or("-".into(), |r| r.to_string());
                                print!(
                                    "  [matched_by {}; fts #{}, vector #{}; source {:?}]",
                                    h.matched_by.join("+"),
                                    rank(h.fts_rank),
                                    rank(h.vector_rank),
                                    h.hit.memory.source,
                                );
                            }
                            println!();
                        }
                    }
                } else {
                    let hits = store.search(Some(project()?), query)?;
                    if json {
                        print_json(&hits)?;
                    } else {
                        for h in hits {
                            println!("{:.3}  {}", h.score, line(&h.memory));
                        }
                    }
                }
            }
            MemoryCommand::List { global, limit, all } => {
                let scope = if global { None } else { Some(project()?) };
                let ms = store.list(scope, all, limit)?;
                if json {
                    print_json(&ms)?;
                } else {
                    for m in ms {
                        println!("{}", line(&m));
                    }
                }
            }
            MemoryCommand::Show { id } => {
                let m = store.get(Some(project()?), id)?;
                if json {
                    print_json(&m)?;
                } else {
                    println!("{}\n\n{}", line(&m), m.content);
                }
            }
            MemoryCommand::Sources { id } => {
                // Checks the memory is visible from this project first.
                store.get(Some(project()?), id)?;
                candidates::print_sources(&store.memory_sources(id)?, json)?;
            }
            MemoryCommand::Links { id } => {
                store.get(Some(project()?), id)?;
                candidates::print_links(id, &store.memory_links(id)?, json)?;
            }
            MemoryCommand::History { id } => {
                let revs = store.history(Some(project()?), id)?;
                if json {
                    print_json(&revs)?;
                } else {
                    for r in revs {
                        println!(
                            "r{}  {}  {}  {}",
                            r.revision, r.cause, r.actor, r.snapshot.title
                        );
                    }
                }
            }
            MemoryCommand::Update(a) => {
                let m = store.update(
                    Some(project()?),
                    a.id,
                    a.revision,
                    MemoryChanges {
                        kind: a.kind,
                        title: a.title,
                        content: a.content.map(body).transpose()?,
                    },
                )?;
                if json {
                    print_json(&m)?
                } else {
                    println!("{}", line(&m))
                }
            }
            MemoryCommand::Archive { id, revision } => {
                let m = store.archive(Some(project()?), id, revision)?;
                if json {
                    print_json(&m)?
                } else {
                    println!("{}", line(&m))
                }
            }
        },
    }
    Ok(true)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // The hook runs inside the agent's loop: skip config, logging and error output.
    if let Command::Hook { command } = &cli.command {
        let agent = match command {
            HookCommand::Claude => Some(hook::Agent::Claude),
            HookCommand::Codex => Some(hook::Agent::Codex),
            _ => None,
        };
        if let Some(agent) = agent {
            hook::run(agent.spec(), cli.db_path);
            return ExitCode::SUCCESS;
        }
    }
    let json = cli.json;
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            let code = e
                .downcast_ref::<memory_core::Error>()
                .map_or("internal", |e| e.code());
            if json {
                let err = serde_json::json!({"error": {"code": code, "message": e.to_string()}});
                println!("{err}");
            } else {
                eprintln!("error ({code}): {e}");
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Mode {
    Fts,
    Vector,
    Hybrid,
}

impl Mode {
    /// `--hybrid` is kept as a shorthand; full-text is the default.
    fn pick(mode: Option<Mode>, hybrid: bool) -> memory_engine::SearchMode {
        use memory_engine::SearchMode as M;
        match (mode, hybrid) {
            (Some(Mode::Vector), _) => M::Vector,
            (Some(Mode::Hybrid), _) | (None, true) => M::Hybrid,
            _ => M::Fts,
        }
    }
}

fn fusion_note(mode: memory_engine::SearchMode) -> &'static str {
    match mode {
        memory_engine::SearchMode::Hybrid => "fused by RRF (k=60, top-50 per path)",
        memory_engine::SearchMode::Vector => "ranked by cosine similarity",
        memory_engine::SearchMode::Fts => "ranked by full-text only",
    }
}
