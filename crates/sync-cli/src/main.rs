use anyhow::Result;
use clap::{Parser, Subcommand};

use sync_core::Sync;

#[derive(Parser)]
#[command(name = "myapp", about = "A sync program")]

struct Cli {
    #[command(subcommand)]
    command: Functions,
}

#[derive(Subcommand)]
enum Functions {
    /// Add a new sync folder
    Add { name: String },
    /// Sync
    Push { id: String, path: String },
    /// Remove an item
    Pull { id: String, path: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let functions = Sync::new().await?;

    match cli.command {
        Functions::Add { name } => functions.add(name).await?,
        Functions::Push { id, path } => functions.push(id, path).await?,
        Functions::Pull { id, path } => functions.pull(id, path),
    }

    Ok(())
}
