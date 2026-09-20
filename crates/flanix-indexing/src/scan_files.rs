use chrono::SubsecRound;
use std::fs::{self, File};
use std::io::{BufReader, Read};

use anyhow::Result;
use chrono::{DateTime, Utc};
use jwalk::WalkDir;
use std::path::{Path, PathBuf};

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
    pub fn new(file_path: PathBuf, modified_time: DateTime<Utc>, file_size: u64) -> Result<Self> {
        let mtime = modified_time.trunc_subsecs(6);

        Ok(Self {
            file_path,
            modified_time: mtime,
            file_size,
            file_hash: None,
        })
    }

    pub fn hash_file(&mut self) -> Result<()> {
        self.hash_from(&self.file_path.clone())
    }

    pub fn hash_from(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path)?;
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
        let mut file_paths: Vec<PathBuf> = Vec::new();

        for entry in WalkDir::new(&self.scan_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                file_paths.push(entry.path().to_path_buf());
            }
        }

        let mut files: Vec<LocalFile> = Vec::new();

        // Collect file metadata and store it
        for file_path in file_paths {
            let metadata = fs::metadata(&file_path)?;
            let modified_time = metadata.modified()?;
            let file_size = metadata.len();

            let mut local_file = LocalFile::new(
                file_path.strip_prefix(&self.scan_path)?.to_path_buf(),
                modified_time.into(),
                file_size,
            )?;

            // Hash the contents while we still have the full path; the
            // stored path is relative to the scan root.
            local_file.hash_from(&file_path)?;

            files.push(local_file);
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_file() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;

        let content = b"hello world";
        temp_file.write_all(content)?;

        let file_path = temp_file.path().to_path_buf();
        let metadata = fs::metadata(&file_path)?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        let mut local_file = LocalFile::new(file_path, modified_time.into(), file_size)?;

        local_file.hash_file()?;

        let expected_hash = blake3::hash(content);
        assert_eq!(local_file.file_hash, Some(expected_hash));
        Ok(())
    }
}
