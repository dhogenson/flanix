mod verify_inputs;

use anyhow::Result;
use clap::{Parser, Subcommand};
use flanix_core::Sync;
use std::fs;
use verify_inputs::validate_namespace;

use crate::verify_inputs::validate_path;

#[derive(Parser)]
#[command(
    name = "Flanix",
    version,
    about = "A sync program",
    arg_required_else_help = true
)]

struct Cli {
    #[command(subcommand)]
    command: Option<Functions>,
}

#[derive(Subcommand)]
enum Functions {
    /// Add a new sync namespace
    Add {
        #[arg(value_parser = validate_namespace)]
        namespace: String,
        #[arg(value_parser = validate_path)]
        path: String,
    },
    /// Push a namespace
    Push {
        #[arg(value_parser = validate_namespace)]
        namespace: String,
    },
    /// Pull a namespace
    Pull {
        #[arg(value_parser = validate_namespace)]
        namespace: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut sync = Sync::new().await?;
    // verify paths and expand them etc

    let result = match cli.command {
        Some(Functions::Add { namespace, path }) => {
            let path = fs::canonicalize(path)?;
            sync.add(namespace, path.to_string_lossy().to_string())
                .await
        }
        // The push function already verifies the namespace
        Some(Functions::Push { namespace }) => sync.push(namespace).await,
        Some(Functions::Pull { namespace }) => sync.pull(namespace).await,
        None => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
