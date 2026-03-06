use clap::{Parser, Subcommand};
use std::path::Path;
use anyhow::Result;

// Import the logic from our library
use med::commands;

#[derive(Parser)]
#[command(name = "med")]
#[command(about = "Medulla: A cognitive memory layer for agent swarms")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the .medulla directory and protocol files
    Init,

    /// Encode a new observation into local working memory
    Learn {
        /// The content of the memory
        content: String,

        /// Explicit associations for Hebbian wiring (repeatable flag)
        #[arg(short, long)]
        tags: Vec<String>,

        /// Optional: specific ID (if updating an existing fact)
        #[arg(short, long)]
        id: Option<String>,
    },

    /// Compile logs into the high-performance Parquet cache using ACT-R logic
    Think,

    /// Query the hybrid memory (Global Brain + Local Musings)
    Query {
            /// The text pattern to search for
            text: String,

            /// Number of results to return
            #[arg(short, long, default_value_t = 5)]
            limit: usize,
        },

    /// Consolidate local musings into the global brain.ndjson
    Consolidate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // --- The Sanity Guard ---
    // We strictly forbid any command (except 'init') from running
    // if the .medulla environment hasn't been established.
    if !matches!(cli.command, Commands::Init) && !Path::new(".medulla").exists() {
        anyhow::bail!(
            "Fatal: .medulla directory not found.\n\
            Please run 'med init' to set up your cognitive environment."
        );
    }

    // --- Command Routing ---
    match cli.command {
        Commands::Init => {
            commands::init::run()?;
        }

        Commands::Learn { content, tags, id } => {
            commands::learn::run(&content, tags, id)?;
        }

        Commands::Think => {
            commands::think::run()?;
        }

        Commands::Query { text, limit } => {
                med::commands::query::run(&text, limit)?;
            }

        // Commands::Consolidate => {
        //     commands::consolidate::run()?;
        // }
        _ => {
            todo!("Unimplemented")
        }
    }

    Ok(())
}
