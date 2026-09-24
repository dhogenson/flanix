use crate::Sync;
use crate::db::truncate_to_micros;
use crate::errors::SyncError;
use chrono::{DateTime, Utc};
use flanix_indexing::LocalFile;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

impl Sync {
    pub(crate) async fn files_to_upload(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<LocalFile>, SyncError> {
        let cloud_files = self.database.get_files(namespace).await?;

        // Turn the cloud files into an index, where the key is the PathBuf and
        // the value is the (date time, content hash) recorded in the DB.
        let cloud_index: HashMap<PathBuf, (DateTime<Utc>, Option<String>)> = cloud_files
            .into_iter()
            .map(|f| (PathBuf::from(f.local_path), (f.modified_at, f.file_hash)))
            .collect();

        let mut to_upload = Vec::new();

        for local_file in local_files {
            let local_mtime = truncate_to_micros(local_file.modified_time);

            match cloud_index.get(&local_file.file_path) {
                // not in cloud at all needs uploading
                None => to_upload.push(local_file.clone()),
                // in cloud, but local file is newer needs re-uploading
                Some((cloud_mtime, _)) if local_mtime > *cloud_mtime => {
                    to_upload.push(local_file.clone())
                }
                // same mtime, but contents differ (or the cloud copy predates
                // content hashing) needs re-uploading
                Some((cloud_mtime, cloud_hash))
                    if local_mtime == *cloud_mtime
                        && *cloud_hash != local_file.file_hash.map(|h| h.to_hex().to_string()) =>
                {
                    to_upload.push(local_file.clone())
                }
                _ => {}
            }
        }

        Ok(to_upload)
    }

    pub(crate) fn files_to_delete(
        previously_had: &HashSet<PathBuf>,
        local_files: &[LocalFile],
        cloud_paths: &HashSet<PathBuf>,
    ) -> HashSet<PathBuf> {
        let local_paths: HashSet<PathBuf> =
            local_files.iter().map(|f| f.file_path.clone()).collect();

        previously_had
            .difference(&local_paths)
            .filter(|p| cloud_paths.contains(*p))
            .cloned()
            .collect()
    }

    pub(crate) fn is_mass_delete(
        deletes: &HashSet<PathBuf>,
        previously_had: &HashSet<PathBuf>,
        cloud_paths: &HashSet<PathBuf>,
    ) -> bool {
        if deletes.is_empty() || previously_had.is_empty() || cloud_paths.is_empty() {
            return false;
        }
        let share_of_device = deletes.len() as f64 / previously_had.len() as f64;
        let share_of_cloud = deletes.len() as f64 / cloud_paths.len() as f64;
        share_of_device >= 0.5 && share_of_cloud >= 0.5
    }

    pub(crate) fn filter_moved_deletes(
        deletes: &HashSet<PathBuf>,
        cloud_hashes: &HashMap<PathBuf, Option<String>>,
        local_hashes: &HashMap<String, Vec<PathBuf>>,
    ) -> HashSet<PathBuf> {
        deletes
            .iter()
            .filter(|path| !Sync::is_move_deleted(path, cloud_hashes, local_hashes))
            .cloned()
            .collect()
    }

    fn is_move_deleted(
        path: &PathBuf,
        cloud_hashes: &HashMap<PathBuf, Option<String>>,
        local_hashes: &HashMap<String, Vec<PathBuf>>,
    ) -> bool {
        match cloud_hashes.get(path) {
            Some(Some(cloud_hash)) => local_hashes
                .get(cloud_hash)
                .is_some_and(|paths| paths.iter().any(|p| p != path)),
            _ => false,
        }
    }

    pub(crate) async fn files_to_pull(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<PathBuf>, SyncError> {
        // let local_files = scan_files(path)?;
        let cloud_files = self.database.get_files(namespace).await?;

        // Turn the local files into an index, where the key is the relative
        // PathBuf and the value is the (truncated) date time.
        let local_index: HashMap<PathBuf, DateTime<Utc>> = local_files
            .iter()
            .map(|f| (f.file_path.clone(), truncate_to_micros(f.modified_time)))
            .collect();

        let mut to_download = Vec::new();

        for cloud_file in cloud_files {
            let cloud_path = PathBuf::from(&cloud_file.local_path);
            let cloud_mtime = truncate_to_micros(cloud_file.modified_at);

            match local_index.get(&cloud_path) {
                // not on disk at all — needs downloading
                None => to_download.push(cloud_path),
                // on disk, but cloud copy is newer — needs downloading
                Some(local_mtime) if cloud_mtime > *local_mtime => to_download.push(cloud_path),
                _ => {}
            }
        }

        Ok(to_download)
    }

    pub(crate) async fn files_to_conflict(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
        downloads: &[PathBuf],
    ) -> Result<Vec<PathBuf>, SyncError> {
        if downloads.is_empty() {
            return Ok(Vec::new());
        }

        let cloud_files = self.database.get_files(namespace).await?;

        let cloud_index: HashMap<PathBuf, Option<String>> = cloud_files
            .into_iter()
            .map(|f| (PathBuf::from(f.local_path), f.file_hash))
            .collect();

        let downloads: HashSet<&PathBuf> = downloads.iter().collect();

        let mut conflicts = Vec::new();
        for local_file in local_files {
            if !downloads.contains(&local_file.file_path) {
                continue;
            }

            let local_hash = local_file.file_hash.map(|h| h.to_hex().to_string());
            let cloud_hash = cloud_index
                .get(&local_file.file_path)
                .and_then(|h| h.as_deref());
            if local_hash.as_deref() != cloud_hash {
                conflicts.push(local_file.file_path.clone());
            }
        }

        Ok(conflicts)
    }

    pub(crate) async fn files_to_backup(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<PathBuf>, SyncError> {
        let cloud_files = self.database.get_files(namespace).await?;

        let cloud_paths: HashSet<PathBuf> = cloud_files
            .into_iter()
            .map(|f| PathBuf::from(f.local_path))
            .collect();

        let tombstoned_paths: HashSet<PathBuf> = self
            .database
            .get_tombstones(namespace)
            .await?
            .into_iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect();

        let to_backup: Vec<PathBuf> = local_files
            .iter()
            .filter(|f| !cloud_paths.contains(&f.file_path))
            .filter(|f| !tombstoned_paths.contains(&f.file_path))
            .map(|f| f.file_path.clone())
            .collect();

        Ok(to_backup)
    }
}
