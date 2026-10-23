mod doctor;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use memory_core::service::{MemoryChanges, MemoryService, NewMemory, SearchQuery};
use memory_core::{Config, Kind, Memory, MemoryId, ProjectId, Source};
use memory_store::SqliteStore;
use serde::Serialize;
use tracing_subscriber::EnvFilter;

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
    /// Check database, configuration and a full MCP round trip (non-zero exit on failure)
    Doctor {
        /// Rebuild the full-text index first
        #[arg(long)]
        reindex: bool,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve MCP over stdio, scoped to the project of --dir / the current directory
    Serve,
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

fn print_json(v: &impl Serialize) -> anyhow::Result<()> {
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
    let config = Config::resolve(cli.db_path, cli.log_level)?;
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
            command: McpCommand::Serve,
        } => memory_mcp::serve(
            &store,
            project()?,
            std::io::stdin().lock(),
            std::io::stdout().lock(),
        )?,
        Command::Doctor { .. } => unreachable!(),
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
            MemoryCommand::Search {
                query,
                limit,
                global,
                all,
            } => {
                let hits = store.search(
                    Some(project()?),
                    SearchQuery {
                        query,
                        limit,
                        include_global: global,
                        include_inactive: all,
                    },
                )?;
                if json {
                    print_json(&hits)?;
                } else {
                    for h in hits {
                        println!("{:.3}  {}", h.score, line(&h.memory));
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
