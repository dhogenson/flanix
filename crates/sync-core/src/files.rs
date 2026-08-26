use anyhow::Result;
use chrono::{DateTime, Utc};
use glob::glob;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[allow(dead_code)]
#[derive(Debug)]
pub struct File {
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
}

impl File {
    pub fn new(path: PathBuf, modified: DateTime<Utc>) -> Self {
        Self { path, modified }
    }
}

pub fn scan_files(path: PathBuf) -> Result<Vec<File>> {
    let mut files: Vec<File> = Vec::new();
    for entry in glob(&format!("{}/**/*", path.to_string_lossy())).expect("Failed to read glob") {
        match entry {
            Ok(path) => {
                if path.is_dir() {
                    continue;
                };

                let metadata = fs::metadata(&path)?;

                let modified: SystemTime = metadata.modified()?;
                let file = File::new(path, modified.into());
                files.push(file);
            }
            Err(error) => {
                eprintln!("Error: {}", error)
            }
        }
    }

    Ok(files)
}
