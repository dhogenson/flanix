use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use sync_core::Sync;

#[derive(Parser)]
#[command(name = "Sync", about = "A sync program")]

struct Cli {
    #[command(subcommand)]
    command: Functions,
}

#[derive(Subcommand)]
enum Functions {
    /// Add a new sync namespace
    Add { name: String, path: String },
    /// Push a namespace
    Push { namespace: String },
    /// Pull a namespace
    Pull { namespace: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut functions = Sync::new().await?;

    // verify paths and expand them etc

    let result = match cli.command {
        Functions::Add { name, path } => {
            let path = fs::canonicalize(path)?;
            functions
                .add(name, path.to_string_lossy().to_string())
                .await
        }
        Functions::Push { namespace } => functions.push(namespace).await,
        Functions::Pull { namespace } => functions.pull(namespace).await,
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
