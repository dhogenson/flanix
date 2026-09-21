/*
 * This file is for putting everything together, like for putting db and s3 together
 */
#[cfg(test)]
mod tests;

use chrono::{DateTime, TimeDelta, Utc};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

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

    /// The stable per-machine id from the config, which scopes the
    /// per-device manifest.
    fn device_id(&self) -> Result<Uuid, SyncError> {
        let id = self.config.device_id.as_deref().ok_or_else(|| {
            SyncError::Anyhow(anyhow::anyhow!(
                "no device_id in config; run any flanix command to generate one"
            ))
        })?;
        Uuid::parse_str(id).map_err(|e| SyncError::Anyhow(anyhow::Error::from(e)))
    }

    /// A unique, human-readable registration name for this device.
    fn device_name(&self, device_id: Uuid) -> String {
        let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "flanix".to_string());
        format!("{host}-{}", &device_id.to_string()[..8])
    }

    /// Removes trash entries older than `before`. Runs on every `pull` so the
    /// stash is bounded but never deleted immediately.
    fn prune_trash(root: &Path, before: DateTime<Utc>) -> Result<(), std::io::Error> {
        let trash_root = root.join(flanix_indexing::TMP_TRASH_DIR);
        if !trash_root.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&trash_root)? {
            let entry = entry?;
            let entry_mtime: DateTime<Utc> = entry.metadata()?.modified()?.into();
            if entry_mtime < before {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
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

        let device_id = self.device_id()?;
        self.database
            .register_device(device_id, &self.device_name(device_id))
            .await?;

        // Per-device deletion. Only files that THIS device previously had and
        // no longer has are deletions. Absence on one machine must never
        // destroy files another device uploaded, so the global `files` table
        // is never diffed against the local disk directly.
        let previously_had: HashSet<PathBuf> = self
            .database
            .get_device_files(device_id, &namespace)
            .await?
            .into_iter()
            .collect();

        let files = self.files_to_upload(&namespace, &local_files).await?;

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

        // Only tombstone/delete cloud files this device actually used to have
        // and that are still tracked globally (another machine may already
        // have deleted them). Files we merely never had are left untouched.
        let cloud_paths: HashSet<PathBuf> = self
            .database
            .get_files(&namespace)
            .await?
            .into_iter()
            .map(|f| PathBuf::from(f.local_path))
            .collect();

        let deletes = Self::files_to_delete(&previously_had, &local_files, &cloud_paths);

        let mut delete_objects = Vec::new();
        for file in &deletes {
            let object_key = bucket_key(&namespace, &file.to_string_lossy());
            delete_objects.push((file.clone(), object_key));
        }

        for target in &uploads {
            self.bucket
                .upload_object(&target.bucket_key, &target.full_path.to_string_lossy())
                .await?;
        }

        for (_, object_key) in &delete_objects {
            self.bucket.delete_object(object_key).await?;
        }

        // Phase 2: record the result in the DB as a single transaction, so the
        // ledger is either fully updated or not changed at all.
        let mut tx = self.database.begin().await?;

        for target in uploads {
            let path_str = target.file.file_path.to_string_lossy();
            let file_hash = target.file.file_hash.map(|h| h.to_hex().to_string());

            if target.is_new {
                self.database
                    .add_file(
                        &mut *tx,
                        Uuid::new_v4(),
                        &target.bucket_key,
                        &path_str,
                        target.file.modified_time,
                        file_hash.as_deref(),
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
                        file_hash.as_deref(),
                    )
                    .await?;
            }

            // The file is uploaded again, so it is no longer deleted.
            self.database
                .remove_tombstone(&mut *tx, namespace_id, &path_str)
                .await?;
        }

        for (file, _) in delete_objects {
            let path = file.to_string_lossy();

            self.database
                .delete_file(&mut *tx, namespace_id, &path)
                .await?;

            // Record the deletion so other machines' `pull` can remove their
            // local copies instead of re-uploading the file as new.
            self.database
                .add_tombstone(&mut *tx, namespace_id, &path)
                .await?;
        }

        tx.commit().await.map_err(crate::errors::DbError::Sqlx)?;

        // Refresh this device's manifest to the current local state, dropping
        // the paths that were deleted above so they aren't re-considered.
        let current_paths: Vec<PathBuf> = local_files.iter().map(|f| f.file_path.clone()).collect();
        self.database
            .replace_device_files(&self.database.pool, device_id, &namespace, &current_paths)
            .await?;

        let expected_keys: HashSet<String> = self
            .database
            .get_all_files()
            .await?
            .into_iter()
            .map(|file| file.bucket_key)
            .collect();

        for object_key in self.bucket.list_objects().await? {
            if !expected_keys.contains(&object_key) {
                self.bucket.delete_object(&object_key).await?;
            }
        }

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

        let namespace_id = self.database.get_namespace_id(&namespace).await?;

        let device_id = self.device_id()?;
        self.database
            .register_device(device_id, &self.device_name(device_id))
            .await?;

        // Back up local files that were never pushed. This is what makes pull
        // non-destructive: local files are only ever protected by being
        // registered, never blindly treated as "extra" to delete.
        let to_backup: HashSet<PathBuf> = self
            .files_to_backup(&namespace, &local_files)
            .await?
            .into_iter()
            .collect();

        if !to_backup.is_empty() {
            let mut tx = self.database.begin().await?;

            for local_file in &local_files {
                if !to_backup.contains(&local_file.file_path) {
                    continue;
                }

                let path_str = local_file.file_path.to_string_lossy();
                let full_path = root.join(&local_file.file_path);

                let key = bucket_key(&namespace, &path_str);
                self.bucket
                    .upload_object(&key, &full_path.to_string_lossy())
                    .await?;

                let file_hash = local_file.file_hash.map(|h| h.to_hex().to_string());
                self.database
                    .add_file(
                        &mut *tx,
                        Uuid::new_v4(),
                        &key,
                        &path_str,
                        local_file.modified_time,
                        file_hash.as_deref(),
                        namespace_id,
                    )
                    .await?;
            }

            tx.commit().await.map_err(crate::errors::DbError::Sqlx)?;
        }

        // Propagate cloud deletions: remove local copies of tombstoned files.
        let tombstoned: HashSet<PathBuf> = self
            .database
            .get_tombstones(&namespace)
            .await?
            .into_iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect();

        let mut removed_local: HashSet<PathBuf> = HashSet::new();
        let retention = Utc::now() - TimeDelta::days(30);

        // Propagate cloud deletions by trashing, never unlinking: tombstoned
        // files are moved into .flanix-trash/<stamp>/ so a mistake can be
        // recovered until the trash is pruned.
        let trash_root = root.join(flanix_indexing::TMP_TRASH_DIR);
        let stamp = Utc::now().timestamp_millis().to_string();
        for local_file in &local_files {
            if tombstoned.contains(&local_file.file_path) {
                let src = root.join(&local_file.file_path);
                let dst = trash_root.join(&stamp).join(&local_file.file_path);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&src, &dst)?;
                removed_local.insert(local_file.file_path.clone());
            }
        }

        // Bound the trash and tombstone tables. Devices that haven't pulled
        // within the retention window may re-back-up the file as new next time
        // they do.
        Self::prune_trash(&root, retention)?;
        self.database
            .prune_tombstones(&namespace, retention)
            .await?;

        // download files that are newer on the cloud
        let files_to_download = self.files_to_pull(&namespace, &local_files).await?;

        for file in &files_to_download {
            let path_str = file.to_string_lossy().to_string();

            let bucket_key = bucket_key(&namespace, &path_str);

            let modified_at = self
                .database
                .modified_at_for_path(&namespace, &path_str)
                .await?;

            let full_path = root.join(file);

            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            self.bucket
                .download_object(&bucket_key, &full_path.to_string_lossy())
                .await?;

            std::fs::File::open(&full_path)?.set_modified(modified_at.into())?;
        }

        // Refresh this device's manifest to what it now has on disk:
        // everything scanned, everything freshly downloaded, minus anything
        // removed because the cloud deletion was propagated.
        let mut manifest: HashSet<PathBuf> =
            local_files.iter().map(|f| f.file_path.clone()).collect();
        manifest.extend(files_to_download);
        for path in &removed_local {
            manifest.remove(path);
        }
        let manifest_paths: Vec<PathBuf> = manifest.into_iter().collect();
        self.database
            .replace_device_files(&self.database.pool, device_id, &namespace, &manifest_paths)
            .await?;

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
