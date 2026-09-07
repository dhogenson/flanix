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
use sync_indexing::LocalFile;

use crate::Bucket;
use crate::Config;
use crate::Database;
use crate::errors::SyncError;
use sync_indexing::Indexer;
use uuid::Uuid;

pub struct Sync {
    database: Database,
    bucket: Bucket,
    config: Config,
}

impl Sync {
    pub async fn new() -> Result<Self, SyncError> {
        let config = Config::new()?;
        let mut database = Database::new(&config.database_url).await?;
        let bucket = Bucket::new(&config).await;
        database.init().await?;
        bucket.init().await?;
        Ok(Self {
            database,
            bucket,
            config,
        })
    }

    /// Uploads new and changed files to the cloud
    pub async fn push(&self, namespace: String) -> Result<(), SyncError> {
        let namespace_id = self.database.get_namespace_id(&namespace).await?;

        let path = match self.config.find_namespace_path(&namespace.to_string()) {
            Some(path) => path,
            None => {
                return Err(SyncError::NamespaceNotFound(
                    format!("namespace: {}", namespace).into(),
                ));
            }
        };

        let indexer = Indexer::new(path.clone());
        let local_files = indexer.scan()?;

        let files = self.files_to_upload(&namespace, &local_files).await?;

        let files_to_delete = self.files_to_delete(&namespace, &local_files).await?;

        for file in files {
            // scan_files returns paths relative to the pushed folder, so join
            // the folder back on to read the actual file from disk.
            let full_path = PathBuf::from(&path).join(&file.file_path);

            if let Some(bucket_key) = self
                .database
                .bucket_key_for_path_opt(&namespace, &file.file_path.to_string_lossy())
                .await?
            {
                self.bucket
                    .upload_object(bucket_key, &full_path.to_string_lossy())
                    .await?;
                self.database
                    .update_file_modified_at(
                        &namespace,
                        &file.file_path.to_string_lossy(),
                        file.modified_time,
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
                        &file.file_path.to_string_lossy(),
                        file.modified_time,
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

    pub async fn pull(&self, namespace: String) -> Result<(), SyncError> {
        let path = match self.config.find_namespace_path(&namespace.to_string()) {
            Some(path) => path,
            None => {
                return Err(SyncError::NamespaceNotFound(
                    format!("namespace: {}", namespace).into(),
                ));
            }
        };

        let root = PathBuf::from(&path);

        let indexer = Indexer::new(root.clone());
        let local_files = indexer.scan()?;

        // first check for files that are newer on the cloud
        let files_to_download = self.files_to_pull(&namespace, &local_files).await?;

        for file in files_to_download {
            let path_str = file.to_string_lossy().to_string();

            let bucket_key = self
                .database
                .bucket_key_for_path(&namespace, &path_str)
                .await?;

            let modified_at = self
                .database
                .modified_at_for_path(&namespace, &path_str)
                .await?;

            let full_path = root.join(&file);

            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            self.bucket
                .download_object(bucket_key, &full_path.to_string_lossy())
                .await?;

            std::fs::File::open(&full_path)?.set_modified(modified_at.into())?;
        }

        let files_to_delete = self.files_to_delete_local(&namespace, &local_files).await?;

        for file in files_to_delete {
            fs::remove_file(root.join(&file))?;
        }

        Ok(())
    }

    pub async fn add(&mut self, name: String, path: String) -> Result<(), SyncError> {
        let path = fs::canonicalize(&path)?.to_string_lossy().to_string();

        if !self.config.contains_namespace(&name) {
            self.config.create_namespace(&name, path)?;
        }
        if !self.database.namespace_exists(&name).await? {
            let uuid = Uuid::new_v4();
            self.database.create_namespace(uuid, &name).await?;
        }
        Ok(())
    }

    async fn files_to_upload(
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

    async fn files_to_delete(
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

    async fn files_to_pull(
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

    async fn files_to_delete_local(
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
