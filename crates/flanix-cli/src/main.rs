mod verify_inputs;

use anyhow::Result;
use clap::{Parser, Subcommand};
use flanix_config::{App, Config};
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

    /// Edit config through tui
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::new()?;

    let result = match cli.command {
        Functions::Add { namespace, path } => {
            let mut sync = Sync::new(config).await?;
            // verify paths and expand them etc
            let path = fs::canonicalize(path)?;
            sync.add(namespace, path.to_string_lossy().to_string())
                .await
        }
        // The push function already verifies the namespace
        Functions::Push { namespace } => {
            let sync = Sync::new(config).await?;
            sync.push(namespace).await
        }
        Functions::Pull { namespace } => {
            let sync = Sync::new(config).await?;
            sync.pull(namespace).await
        }
        Functions::Config => {
            let mut app = App::new()?;
            app.run()?;
            Ok(())
        } // None => Ok(()),
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    Ok(())
}
