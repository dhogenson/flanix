/*
 * This file is for putting everything together, like for putting db and s3 together
 */
#[cfg(test)]
mod tests;

use crate::db::truncate_to_micros;
use chrono::DateTime;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::Bucket;
use crate::Config;
use crate::Database;
use crate::errors::SyncError;
use crate::scan_files;
use crate::scanner::File;
use uuid::Uuid;

pub struct Sync {
    database: Database,
    bucket: Bucket,
}

impl Sync {
    pub async fn new() -> Result<Self, SyncError> {
        let config = Config::new()?;
        let mut database = Database::new(&config.database_url).await?;
        let bucket = Bucket::new(&config.bucket_name, &config.aws_default_region).await;
        database.init().await?;
        bucket.init().await?;
        Ok(Self {
            database: database,
            bucket: bucket,
        })
    }

    /// Uploads new and changed files to the cloud
    pub async fn push(&self, namespace: String, path: String) -> Result<(), SyncError> {
        let namespace_id = self.database.get_namespace_id(&namespace).await?;

        let files = self
            .files_to_upload(&namespace, PathBuf::from(&path))
            .await?;

        let files_to_delete = self
            .files_to_delete(&namespace, PathBuf::from(&path))
            .await?;

        for file in files {
            // scan_files returns paths relative to the pushed folder, so join
            // the folder back on to read the actual file from disk.
            let full_path = PathBuf::from(&path).join(&file.path);

            // A re-uploaded file already has a DB row and an object in the
            // bucket: overwrite the existing object and update the row's mtime
            // in place. Inserting a fresh row would leave a duplicate DB row
            // behind and orphan the old object.
            if let Some(bucket_key) = self
                .database
                .bucket_key_for_path_opt(&namespace, &file.path.to_string_lossy())
                .await?
            {
                self.bucket
                    .upload_object(bucket_key, &full_path.to_string_lossy())
                    .await?;
                self.database
                    .update_file_modified_at(
                        &namespace,
                        &file.path.to_string_lossy(),
                        file.modified,
                    )
                    .await?;
            } else {
                let bucket_key = Uuid::new_v4();
                let file_uuid = Uuid::new_v4();

                self.bucket
                    .upload_object(bucket_key, &full_path.to_string_lossy())
                    .await?;
                self.database
                    .add_file(
                        file_uuid,
                        bucket_key,
                        &file.path.to_string_lossy(),
                        file.modified,
                        namespace_id,
                    )
                    .await?;
            }
        }

        for file in files_to_delete {
            let object_key = self
                .database
                .bucket_key_for_path(&namespace, &file.to_string_lossy())
                .await?;
            self.bucket.delete_object(object_key).await?;
            self.database
                .delete_file(&namespace, &file.to_string_lossy())
                .await?;
        }

        Ok(())
    }

    /// Downloads files that are newer on the cloud (or missing locally) and
    /// removes local files that no longer exist in the cloud.
    pub async fn pull(&self, namespace: String, path: String) -> Result<(), SyncError> {
        let root = PathBuf::from(&path);

        // First, check for files that are newer on the cloud.
        let files_to_download = self.files_to_pull(&namespace, root.clone()).await?;

        for file in files_to_download {
            let path_str = file.to_string_lossy().to_string();

            // Download the object the DB already tracks for this path, not a
            // freshly generated key.
            let bucket_key = self
                .database
                .bucket_key_for_path(&namespace, &path_str)
                .await?;

            // The mtime the cloud copy is recorded with. After the download,
            // restore it onto the local file (rsync-style) so the local copy
            // is indistinguishable from the cloud copy. Recording the download
            // time instead made the cloud copy look "newer" than every other
            // local copy of the same content, so any later pull into a folder
            // holding older copies re-downloaded every file even though
            // nothing had changed.
            let modified_at = self
                .database
                .modified_at_for_path(&namespace, &path_str)
                .await?;

            let full_path = root.join(&file);

            // `File::create` won't make parent directories, so a nested
            // cloud file (e.g. "folder/hello.txt") fails to download into a
            // fresh pull root unless the directory exists first.
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            self.bucket
                .download_object(bucket_key, &full_path.to_string_lossy())
                .await?;

            // Put the cloud copy's mtime back onto the downloaded file so a
            // later push or pull treats it as identical to the cloud copy
            // rather than as a brand-new local change.
            std::fs::File::open(&full_path)?.set_modified(modified_at.into())?;
        }

        // Next, check if files need deleted: local files that no longer exist
        // in the cloud get removed (the mirror of push, which deletes cloud
        // files that no longer exist locally).
        let files_to_delete = self.files_to_delete_local(&namespace, root.clone()).await?;

        for file in files_to_delete {
            fs::remove_file(root.join(&file))?;
        }

        Ok(())
    }

    pub async fn add(&self, name: String) -> Result<(), SyncError> {
        if !self.database.namespace_exists(&name.to_string()).await? {
            let uuid = Uuid::new_v4();
            self.database
                .create_namespace(uuid, &name.to_string())
                .await?;
        }
        Ok(())
    }

    async fn files_to_upload(
        &self,
        namespace: &str,
        path: PathBuf,
    ) -> Result<Vec<File>, SyncError> {
        let local_files = scan_files(path)?;
        let cloud_files = self.database.get_files(namespace).await?;

        // Turn the cloud files into a index, where the key is the PathBuf and the value is the date time
        let cloud_index: HashMap<PathBuf, DateTime<Utc>> = cloud_files
            .into_iter()
            .map(|f| (PathBuf::from(f.local_path), f.modified_at))
            .collect();

        let mut to_upload = Vec::new();

        for local_file in local_files {
            let local_mtime = local_file.modified;
            let local_mtime = truncate_to_micros(local_mtime);

            match cloud_index.get(&local_file.path) {
                // not in cloud at all — needs uploading
                None => to_upload.push(local_file),
                // in cloud, but local file is newer — needs re-uploading
                Some(cloud_mtime) if local_mtime > *cloud_mtime => to_upload.push(local_file),
                _ => {}
            }
        }

        Ok(to_upload)
    }

    async fn files_to_delete(
        &self,
        namespace: &str,
        path: PathBuf,
    ) -> Result<Vec<PathBuf>, SyncError> {
        let local_files: Vec<PathBuf> = scan_files(path)?.into_iter().map(|f| f.path).collect();
        let cloud_files = self.database.get_files(namespace).await?;

        let mut to_delete: Vec<PathBuf> = Vec::new();

        for cloud_file in cloud_files {
            let cloud_path = PathBuf::from(&cloud_file.local_path);
            if !local_files.contains(&cloud_path) {
                to_delete.push(cloud_path);
            }
        }

        Ok(to_delete)
    }

    /// Returns the (namespace-relative) paths that need to be downloaded:
    /// files tracked in the cloud that are missing locally, or whose cloud
    /// copy is newer than the local copy.
    async fn files_to_pull(
        &self,
        namespace: &str,
        path: PathBuf,
    ) -> Result<Vec<PathBuf>, SyncError> {
        let local_files = scan_files(path)?;
        let cloud_files = self.database.get_files(namespace).await?;

        // Turn the local files into an index, where the key is the relative
        // PathBuf and the value is the (truncated) date time.
        let local_index: HashMap<PathBuf, DateTime<Utc>> = local_files
            .into_iter()
            .map(|f| (f.path, truncate_to_micros(f.modified)))
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

    /// Returns the (namespace-relative) local paths that exist on disk but
    /// are not tracked in the cloud, i.e. the files a pull should delete.
    async fn files_to_delete_local(
        &self,
        namespace: &str,
        path: PathBuf,
    ) -> Result<Vec<PathBuf>, SyncError> {
        let local_files: Vec<PathBuf> = scan_files(path)?.into_iter().map(|f| f.path).collect();
        let cloud_files = self.database.get_files(namespace).await?;

        let cloud_paths: Vec<PathBuf> = cloud_files
            .into_iter()
            .map(|f| PathBuf::from(f.local_path))
            .collect();

        let mut to_delete: Vec<PathBuf> = Vec::new();

        for local_file in local_files {
            if !cloud_paths.contains(&local_file) {
                to_delete.push(local_file);
            }
        }

        Ok(to_delete)
    }
}
