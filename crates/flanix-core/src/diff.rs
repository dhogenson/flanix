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

    /// Deletion candidates for a push: paths this device previously had, no
    /// longer has on disk, and that are still tracked in the cloud.
    ///
    /// Absence on one machine never marks a file for deletion — a path must
    /// have been in THIS device's previous manifest, or it belongs to some
    /// other machine and is left untouched. Files already removed from the
    /// cloud (e.g. deleted by another device) also drop out of the candidates.
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

    /// Local files with no cloud/DB record and no deletion tombstone.
    ///
    /// These are the files `pull` must back up first — never delete them, or
    /// pull would destroy local data that was never pushed. Files that were
    /// deleted on the cloud (tombstoned) are excluded: their local copies are
    /// removed instead.
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
