use crate::Sync;
use crate::db::truncate_to_micros;
use crate::errors::SyncError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use sync_indexing::LocalFile;

impl Sync {
    pub(crate) async fn files_to_upload(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<LocalFile>, SyncError> {
        let cloud_files = self.database.get_files(namespace).await?;

        // Turn the cloud files into a index, where the key is the PathBuf and the value is the date time
        let cloud_index: HashMap<PathBuf, DateTime<Utc>> = cloud_files
            .into_iter()
            .map(|f| (PathBuf::from(f.local_path), f.modified_at))
            .collect();

        let mut to_upload = Vec::new();

        for local_file in local_files {
            let local_mtime = local_file.modified_time;
            let local_mtime = truncate_to_micros(local_mtime);

            match cloud_index.get(&local_file.file_path) {
                // not in cloud at all — needs uploading
                None => to_upload.push(local_file.clone()),
                // in cloud, but local file is newer — needs re-uploading
                Some(cloud_mtime) if local_mtime > *cloud_mtime => {
                    to_upload.push(local_file.clone())
                }
                _ => {}
            }
        }

        Ok(to_upload)
    }

    pub(crate) async fn files_to_delete(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<PathBuf>, SyncError> {
        let cloud_files = self.database.get_files(namespace).await?;

        let mut to_delete: Vec<PathBuf> = Vec::new();

        for cloud_file in cloud_files {
            let cloud_path = PathBuf::from(&cloud_file.local_path);
            if !local_files.iter().any(|f| f.file_path == cloud_path) {
                to_delete.push(cloud_path);
            }
        }

        Ok(to_delete)
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
            .into_iter()
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

    pub(crate) async fn files_to_delete_local(
        &self,
        namespace: &str,
        local_files: &[LocalFile],
    ) -> Result<Vec<PathBuf>, SyncError> {
        // let local_files: Vec<PathBuf> = scan_files(path)?.into_iter().map(|f| f.path).collect();
        let cloud_files = self.database.get_files(namespace).await?;

        let cloud_paths: Vec<PathBuf> = cloud_files
            .into_iter()
            .map(|f| PathBuf::from(f.local_path))
            .collect();

        let mut to_delete: Vec<PathBuf> = Vec::new();

        for local_file in local_files {
            if !cloud_paths.contains(&local_file.file_path) {
                to_delete.push(local_file.file_path.clone());
            }
        }

        Ok(to_delete)
    }
}
