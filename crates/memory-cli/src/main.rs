use std::path::PathBuf;

use clap::{Parser, Subcommand};
use memory_core::Config;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "agent-memory",
    version,
    about = "Local-first long-term memory for coding agents"
)]
struct Cli {
    /// Path to the SQLite database [default: platform data dir]
    #[arg(long, global = true, env = "AGENT_MEMORY_DB")]
    db_path: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, global = true, env = "AGENT_MEMORY_LOG")]
    log_level: Option<String>,

    /// Emit machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the resolved configuration
    Config,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = Config::resolve(cli.db_path, cli.log_level)?;
    // Logs go to stderr so stdout stays clean for JSON and MCP stdio.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        Command::Config if cli.json => println!("{}", serde_json::to_string_pretty(&config)?),
        Command::Config => {
            println!("db_path   = {}", config.db_path.display());
            println!("log_level = {}", config.log_level);
        }
    }
    Ok(())
}
