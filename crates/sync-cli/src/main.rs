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
    /// Add a new sync namespace
    Add { name: String },
    /// Push a namespace
    Push { namespace: String, path: String },
    /// Pull a namespace
    Pull { namespace: String, path: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let functions = Sync::new().await?;

    let result = match cli.command {
        Functions::Add { name } => functions.add(name).await,
        Functions::Push { namespace, path } => functions.push(namespace, path).await,
        Functions::Pull { namespace, path } => functions.pull(namespace, path).await,
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
