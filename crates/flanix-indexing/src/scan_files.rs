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

/// Sync-internal reserve. Local trash moved here by `pull` is excluded from
/// scans, so it is never pushed or re-backed-up.
pub const TMP_TRASH_DIR: &str = ".flanix-trash";

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
            let full_path = entry.path();
            // The trash folder is a sync-internal staging area; files in it
            // must never be treated as part of the namespace.
            let relative = full_path.strip_prefix(&self.scan_path)?;
            if relative.starts_with(TMP_TRASH_DIR) {
                continue;
            }
            if entry.file_type().is_file() {
                file_paths.push(full_path.clone());
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

    #[test]
    fn scan_skips_trash_dir() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("keep.txt"), "keep")?;
        fs::create_dir_all(dir.path().join(TMP_TRASH_DIR).join("123"))?;
        fs::write(
            dir.path()
                .join(TMP_TRASH_DIR)
                .join("123")
                .join("doomed.txt"),
            "doomed",
        )?;

        let files = Indexer::new(dir.path().to_path_buf()).scan()?;

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_path, PathBuf::from("keep.txt"));
        Ok(())
    }
}
