/*
 * This file is for putting everything together, like for putting db and s3 together
 */
#[cfg(test)]
mod tests;

use crate::db::truncate_to_micros;
use chrono::DateTime;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::Bucket;
use crate::Config;
use crate::Database;
use crate::errors::SyncError;
use crate::scan_files;
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
            let bucket_key = Uuid::new_v4();
            let file_uuid = Uuid::new_v4();
            self.bucket
                .upload_object(bucket_key, &file.to_string_lossy())
                .await?;
            self.database
                .add_file(file_uuid, bucket_key, &file.to_string_lossy(), namespace_id)
                .await?;
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

    pub fn pull(&self, _id: String, _path: String) {}

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
    ) -> Result<Vec<PathBuf>, SyncError> {
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
                None => to_upload.push(local_file.path),
                // in cloud, but local file is newer — needs re-uploading
                Some(cloud_mtime) if local_mtime > *cloud_mtime => to_upload.push(local_file.path),
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
}
