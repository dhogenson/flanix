use chrono::SubsecRound;
use std::fs::{self, File};
use std::io::{BufReader, Read};

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::{Component, PathBuf};
use walkdir::WalkDir;

pub struct Indexer {
    scan_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LocalFile {
    pub file_path: PathBuf,
    pub modified_time: DateTime<Utc>,
    pub file_size: u64,
    pub file_hash: Option<blake3::Hash>,
}

impl LocalFile {
    pub fn new(file_path: PathBuf, modified_time: DateTime<Utc>, file_size: u64) -> Self {
        let mtime = modified_time.trunc_subsecs(6);
        let normalized_path = normalize(&file_path);

        LocalFile {
            file_path: normalized_path,
            modified_time: mtime,
            file_size,
            file_hash: None,
        }
    }

    pub fn hash_file(&mut self) -> Result<()> {
        let file = File::open(&self.file_path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = blake3::Hasher::new();

        let mut buf = [0u8; 65536]; // 64KB chunks

        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }

        self.file_hash = Some(hasher.finalize());

        Ok(())
    }
}

impl Indexer {
    pub fn new(scan_path: PathBuf) -> Self {
        Self { scan_path }
    }

    pub fn scan(&self) -> Result<Vec<LocalFile>> {
        let mut local_files: Vec<LocalFile> = Vec::new();
        let folder_paths: Vec<PathBuf> = WalkDir::new(&self.scan_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
            .map(|e| e.path().to_path_buf())
            .collect();

        for folder in &folder_paths {
            let mut folder_files = self.scan_folder(&folder)?;
            local_files.append(&mut folder_files);
        }

        Ok(local_files)
    }

    fn scan_folder(&self, folder_path: &PathBuf) -> Result<Vec<LocalFile>> {
        let file_paths: Vec<PathBuf> = WalkDir::new(&folder_path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        let mut files: Vec<LocalFile> = Vec::new();

        // Collect file metadata and store it
        for file_path in file_paths {
            let metadata = fs::metadata(&file_path)?;
            let modified_time = metadata.modified()?;
            let file_size = metadata.len();

            let local_file = LocalFile::new(
                file_path.strip_prefix(&self.scan_path)?.to_path_buf(),
                modified_time.into(),
                file_size,
            );

            files.push(local_file);
        }

        Ok(files)
    }
}

// Made by ai. Check what is does
pub fn normalize(path: &PathBuf) -> PathBuf {
    let mut out = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {
                // skip "." entirely
            }
            Component::ParentDir => {
                match out.last() {
                    // ".." after a normal segment cancels it out
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    // ".." at the root is a no-op (can't go above root)
                    Some(Component::RootDir) => {}
                    // nothing to pop, or the top is already "..": keep it
                    None | Some(Component::ParentDir) => {
                        out.push(component);
                    }
                    // Prefix (Windows drive letters) — keep the ".." after it
                    Some(Component::Prefix(_)) => {
                        out.push(component);
                    }
                    Some(Component::CurDir) => unreachable!("CurDir is never pushed"),
                }
            }
            other => out.push(other),
        }
    }

    out.into_iter().collect()
}
