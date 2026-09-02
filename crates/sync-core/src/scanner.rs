use crate::errors::SyncError;
use chrono::{DateTime, Utc};
use glob::glob;
use std::fs;
use std::path::{Component, Path, PathBuf};
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

/// Drop `.` components so `./test_files` and `test_files` compare equal.
/// The glob crate strips a leading `./` from its results, so prefix
/// comparisons must use a normalized base path.
fn normalize(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect()
}

pub fn scan_files(path: PathBuf) -> Result<Vec<File>, SyncError> {
    let base = normalize(&path);

    let mut files: Vec<File> = Vec::new();
    for entry in glob(&format!("{}/**/*", base.to_string_lossy()))? {
        match entry {
            Ok(found) => {
                if found.is_dir() {
                    continue;
                };
                let metadata = fs::metadata(&found)?;

                let modified: SystemTime = metadata.modified()?;

                // Store paths relative to the scanned folder (e.g. "a.txt",
                // "sub/b.txt" for a scan of "test_files") so the DB and S3
                // keys don't depend on where the folder lives locally.
                let relative = normalize(&found)
                    .strip_prefix(&base)
                    .map_err(|_| {
                        SyncError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "scanned path {} is not under {}",
                                found.to_string_lossy(),
                                base.to_string_lossy()
                            ),
                        ))
                    })?
                    .to_path_buf();

                let file = File::new(relative, modified.into());
                files.push(file);
            }
            Err(error) => {
                eprintln!("Error: {}", error)
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
        paths.sort();
        paths
    }

    #[test]
    fn returns_paths_relative_to_scanned_folder() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("test_files");
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("what.txt"), "a").unwrap();
        fs::write(base.join("sub/nested.txt"), "b").unwrap();

        let files = scan_files(base).unwrap();
        let paths: Vec<PathBuf> = files.into_iter().map(|f| f.path).collect();

        assert_eq!(
            sorted(paths),
            sorted(vec![
                PathBuf::from("what.txt"),
                PathBuf::from("sub/nested.txt")
            ])
        );
    }

    #[test]
    fn handles_dot_component_in_scanned_path() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("test_files");
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("what.txt"), "a").unwrap();
        fs::write(base.join("sub/nested.txt"), "b").unwrap();

        // `dir/./test_files` mirrors `./test_files` from the CLI: glob strips
        // the leading `./` from results, so the prefix must be normalized.
        let dotted = dir.path().join(".").join("test_files");
        let files = scan_files(dotted).unwrap();
        let paths: Vec<PathBuf> = files.into_iter().map(|f| f.path).collect();

        assert_eq!(
            sorted(paths),
            sorted(vec![
                PathBuf::from("what.txt"),
                PathBuf::from("sub/nested.txt")
            ])
        );
    }
}
