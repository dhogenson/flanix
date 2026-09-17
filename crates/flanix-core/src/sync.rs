/*
 * This file is for putting everything together, like for putting db and s3 together
 */
#[cfg(test)]
mod tests;

use std::fs;
use std::path::PathBuf;

use crate::Bucket;
use crate::Database;
use crate::bucket_key;
use crate::errors::SyncError;
use flanix_config::Config;
use flanix_indexing::{Indexer, LocalFile};
use uuid::Uuid;

pub struct Sync {
    pub database: Database,
    pub bucket: Bucket,
    pub config: Config,
}

impl Sync {
    pub async fn new(config: Config) -> Result<Self, SyncError> {
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
                return Err(SyncError::NamespaceNotFound(format!(
                    "namespace: {}",
                    namespace
                )));
            }
        };

        let indexer = Indexer::new(path.clone());
        let local_files = indexer.scan()?;

        let files = self.files_to_upload(&namespace, &local_files).await?;

        let files_to_delete = self.files_to_delete(&namespace, &local_files).await?;

        struct UploadTarget {
            file: LocalFile,
            full_path: PathBuf,
            bucket_key: String,
            is_new: bool,
        }

        let mut uploads = Vec::with_capacity(files.len());
        for file in files {
            // scan_files returns paths relative to the pushed folder, so join
            // the folder back on to read the actual file from disk.
            let full_path = PathBuf::from(&path).join(&file.file_path);
            let path_str = file.file_path.to_string_lossy();

            let bucket_key = bucket_key(&namespace, &path_str);
            let is_new = !self.database.file_exists(namespace_id, &path_str).await?;

            uploads.push(UploadTarget {
                file,
                full_path,
                bucket_key,
                is_new,
            });
        }

        let mut deletes = Vec::with_capacity(files_to_delete.len());
        for file in files_to_delete {
            let object_key = bucket_key(&namespace, &file.to_string_lossy());
            deletes.push((file, object_key));
        }

        for target in &uploads {
            self.bucket
                .upload_object(&target.bucket_key, &target.full_path.to_string_lossy())
                .await?;
        }

        for (_, object_key) in &deletes {
            self.bucket.delete_object(object_key).await?;
        }

        // Phase 2: record the result in the DB as a single transaction, so the
        // ledger is either fully updated or not changed at all.
        let mut tx = self.database.begin().await?;

        for target in uploads {
            let path_str = target.file.file_path.to_string_lossy();

            if target.is_new {
                self.database
                    .add_file(
                        &mut *tx,
                        Uuid::new_v4(),
                        &target.bucket_key,
                        &path_str,
                        target.file.modified_time,
                        namespace_id,
                    )
                    .await?;
            } else {
                self.database
                    .update_file_modified_at(
                        &mut *tx,
                        namespace_id,
                        &path_str,
                        target.file.modified_time,
                    )
                    .await?;
            }
        }

        for (file, _) in deletes {
            self.database
                .delete_file(&mut *tx, namespace_id, &file.to_string_lossy())
                .await?;
        }

        tx.commit().await.map_err(crate::errors::DbError::Sqlx)?;

        Ok(())
    }

    pub async fn pull(&self, namespace: String) -> Result<(), SyncError> {
        let path = match self.config.find_namespace_path(&namespace.to_string()) {
            Some(path) => path,
            None => {
                return Err(SyncError::NamespaceNotFound(format!(
                    "namespace: {}",
                    namespace
                )));
            }
        };

        let root = PathBuf::from(&path);

        let indexer = Indexer::new(root.clone());
        let local_files = indexer.scan()?;

        // first check for files that are newer on the cloud
        let files_to_download = self.files_to_pull(&namespace, &local_files).await?;

        for file in files_to_download {
            let path_str = file.to_string_lossy().to_string();

            let bucket_key = bucket_key(&namespace, &path_str);

            let modified_at = self
                .database
                .modified_at_for_path(&namespace, &path_str)
                .await?;

            let full_path = root.join(&file);

            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            self.bucket
                .download_object(&bucket_key, &full_path.to_string_lossy())
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
}
