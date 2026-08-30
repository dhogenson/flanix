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
    Push { namespace: String, path: String },
    /// Remove an item
    Pull { namespace: String, path: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let functions = Sync::new().await?;

    match cli.command {
        Functions::Add { name } => functions.add(name).await?,
        Functions::Push { namespace, path } => functions.push(namespace, path).await?,
        Functions::Pull { namespace, path } => functions.pull(namespace, path),
    }

    Ok(())
}
