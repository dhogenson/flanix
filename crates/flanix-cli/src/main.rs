mod verify_inputs;

use anyhow::Result;
use clap::{Parser, Subcommand};
use flanix_config::Config;
use flanix_core::Sync;
use std::fs;
use verify_inputs::validate_namespace;

use crate::verify_inputs::validate_path;

#[derive(Parser)]
#[command(name = "Flanix", version, about = "A sync program")]
struct Cli {
    #[command(subcommand)]
    command: Functions,
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
    let config = Config::new()?;
    let mut sync = Sync::new(config).await?;
    // verify paths and expand them etc

    let result = match cli.command {
        Functions::Add { namespace, path } => {
            let path = fs::canonicalize(path)?;
            sync.add(namespace, path.to_string_lossy().to_string())
                .await
        }
        // The push function already verifies the namespace
        Functions::Push { namespace } => sync.push(namespace).await,
        Functions::Pull { namespace } => sync.pull(namespace).await,
        // None => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
