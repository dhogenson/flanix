/*
 * This file is for putting everything together, like for putting db and s3 together
 */
#[cfg(test)]
mod tests;

use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

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

    async fn prune_trash(&self, root: &Path, before: DateTime<Utc>) -> Result<(), SyncError> {
        let trash_root = root.join(flanix_indexing::TMP_TRASH_DIR);
        if !trash_root.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&trash_root)? {
            let entry = entry?;
            let entry_mtime: DateTime<Utc> = entry.metadata()?.modified()?.into();
            if entry_mtime < before && self.database.devices_seen_since(entry_mtime).await? {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }

    /// blake3 of a local file's content, hex-encoded.
    async fn hash_file(path: &Path) -> Result<String, SyncError> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    }

    async fn verify_upload(&self, object_key: &str, local_path: &Path) -> Result<(), SyncError> {
        let local_hash = Self::hash_file(local_path).await?;
        let local_len = fs::metadata(local_path)?.len();
        let (remote_len, remote_hash) = self.bucket.object_content_hash(object_key).await?;

        if local_len != remote_len || local_hash != remote_hash {
            return Err(SyncError::Anyhow(anyhow::anyhow!(
                "integrity check failed for '{}': object has size {remote_len} and hash \
                 {remote_hash}, but the local file is {local_len} bytes with hash {local_hash}",
                local_path.display()
            )));
        }

        Ok(())
    }

    async fn ensure_uploaded(&self, object_key: &str, local_path: &Path) -> Result<(), SyncError> {
        if self.bucket.object_exists(object_key).await? {
            let local_len = fs::metadata(local_path)?.len();
            let local_hash = Self::hash_file(local_path).await?;
            let (remote_len, remote_hash) = self.bucket.object_content_hash(object_key).await?;
            if remote_len == local_len && remote_hash == local_hash {
                return Ok(());
            }
        }
        self.bucket
            .upload_object(object_key, &local_path.to_string_lossy())
            .await?;
        self.verify_upload(object_key, local_path).await?;
        Ok(())
    }

    fn confirm_yes(answer: &str) -> bool {
        matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }

    fn ask_to_trash(path: &Path) -> bool {
        eprint!(
            "'{}' was deleted on the cloud but modified locally; move it to the trash \
             (.flanix-trash) anyway? [y/N] ",
            path.display()
        );
        let mut line = String::new();
        use std::io::BufRead;
        let answered = std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map(|n| n > 0)
            .unwrap_or(false);
        answered && Self::confirm_yes(&line)
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

        let cloud_files = self.database.get_files(&namespace).await?;
        let cloud_paths: HashSet<PathBuf> = cloud_files
            .iter()
            .map(|f| PathBuf::from(f.local_path.clone()))
            .collect();

        let deletes = Self::files_to_delete(&previously_had, &local_files, &cloud_paths);

        if !self.config.allow_mass_deletes
            && Self::is_mass_delete(&deletes, &previously_had, &cloud_paths)
        {
            let owned_pct = deletes.len() as f64 / previously_had.len() as f64 * 100.0;
            let cloud_pct = deletes.len() as f64 / cloud_paths.len() as f64 * 100.0;
            return Err(SyncError::Anyhow(anyhow::anyhow!(
                "refusing to delete {} cloud object(s) in namespace '{namespace}': the delete \
                 set is {owned_pct:.0}% of the files this device previously had and {cloud_pct:.0}% \
                 of the namespace's cloud objects, which looks like the sync folder moved or was \
                 misconfigured rather than a real deletion. If you genuinely deleted these files, \
                 set allow_mass_deletes = true in the config and re-run.",
                deletes.len()
            )));
        }

        let local_by_hash: HashMap<String, Vec<PathBuf>> = {
            let mut index: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for file in &local_files {
                if let Some(hash) = file.file_hash {
                    index
                        .entry(hash.to_hex().to_string())
                        .or_default()
                        .push(file.file_path.clone());
                }
            }
            index
        };
        let cloud_hashes: HashMap<PathBuf, Option<String>> = cloud_files
            .iter()
            .map(|f| (PathBuf::from(f.local_path.clone()), f.file_hash.clone()))
            .collect();
        let moved = Self::filter_moved_deletes(&deletes, &cloud_hashes, &local_by_hash);

        let mut delete_objects = Vec::new();
        for file in &deletes {
            if moved.contains(file) {
                continue;
            }
            let object_key = bucket_key(&namespace, &file.to_string_lossy());
            delete_objects.push((file.clone(), object_key));
        }

        for target in &uploads {
            self.ensure_uploaded(&target.bucket_key, &target.full_path)
                .await?;
        }

        let mut tx = self.database.begin().await?;

        for target in &uploads {
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

        for (file, _) in &delete_objects {
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

        for (_, object_key) in &delete_objects {
            self.bucket.delete_object(object_key).await?;
        }

        // Refresh this device's manifest to the current local state, dropping
        // the paths that were deleted above so they aren't re-considered.
        let current_paths: Vec<PathBuf> = local_files.iter().map(|f| f.file_path.clone()).collect();
        self.database
            .replace_device_files(&self.database.pool, device_id, &namespace, &current_paths)
            .await?;

        if self.config.sweep_orphans {
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
        }

        Ok(())
    }

    /// Downloads the cloud state for a namespace.
    ///
    /// With `dry_run` nothing is modified: no uploads, no trash moves, no
    /// downloads, no pruning, and no manifest refresh. It only reports what a
    /// real pull would do.
    pub async fn pull(
        &self,
        namespace: String,
        dry_run: bool,
        assume_yes: bool,
    ) -> Result<(), SyncError> {
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
        if !dry_run {
            self.database
                .register_device(device_id, &self.device_name(device_id))
                .await?;
        }

        // Back up local files that were never pushed. This is what makes pull
        // non-destructive: local files are only ever protected by being
        // registered, never blindly treated as "extra" to delete.
        let to_backup: HashSet<PathBuf> = self
            .files_to_backup(&namespace, &local_files)
            .await?
            .into_iter()
            .collect();

        if dry_run {
            for file in &to_backup {
                println!("would back up '{}'", file.display());
            }
        } else if !to_backup.is_empty() {
            let mut tx = self.database.begin().await?;

            for local_file in &local_files {
                if !to_backup.contains(&local_file.file_path) {
                    continue;
                }

                let path_str = local_file.file_path.to_string_lossy();
                let full_path = root.join(&local_file.file_path);

                let key = bucket_key(&namespace, &path_str);
                self.ensure_uploaded(&key, &full_path).await?;

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
        let tombstones = self.database.get_tombstones(&namespace).await?;
        let tombstoned: HashSet<PathBuf> = tombstones
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect();
        let tombstone_times: HashMap<PathBuf, DateTime<Utc>> = tombstones
            .into_iter()
            .map(|(path, at)| (PathBuf::from(path), at))
            .collect();

        let mut removed_local: HashSet<PathBuf> = HashSet::new();
        let retention = Utc::now() - TimeDelta::days(self.config.trash_retention_days as i64);

        // Propagate cloud deletions by trashing, never unlinking: tombstoned
        // files are moved into .flanix-trash/<stamp>/ so a mistake can be
        // recovered until the trash is pruned.
        let trash_root = root.join(flanix_indexing::TMP_TRASH_DIR);
        let stamp = Utc::now().timestamp_millis().to_string();
        for local_file in &local_files {
            if !tombstoned.contains(&local_file.file_path) {
                continue;
            }

            let modified_after_deletion = tombstone_times
                .get(&local_file.file_path)
                .is_some_and(|deleted_at| local_file.modified_time > *deleted_at);

            if dry_run {
                if modified_after_deletion {
                    println!(
                        "would ask before trashing '{}' (modified locally after it was deleted \
                         on the cloud)",
                        local_file.file_path.display()
                    );
                } else {
                    println!(
                        "would trash '{}' (deleted on the cloud)",
                        local_file.file_path.display()
                    );
                }
                continue;
            }

            if modified_after_deletion && !assume_yes && !Self::ask_to_trash(&local_file.file_path)
            {
                eprintln!(
                    "kept '{}' locally (declined to trash a file modified after the cloud \
                     deletion); the next push will re-upload it",
                    local_file.file_path.display()
                );
                continue;
            }

            let src = root.join(&local_file.file_path);
            let dst = trash_root.join(&stamp).join(&local_file.file_path);
            let move_result = (|| -> std::io::Result<()> {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&src, &dst)?;
                Ok(())
            })();
            match move_result {
                Ok(()) => {
                    removed_local.insert(local_file.file_path.clone());
                }
                Err(e) => {
                    eprintln!(
                        "warning: could not move '{}' to the trash ({}); keeping the local copy",
                        local_file.file_path.display(),
                        e
                    );
                }
            }
        }

        if !dry_run {
            self.prune_trash(&root, retention).await?;
            self.database
                .prune_tombstones(&namespace, retention)
                .await?;
        }

        // download files that are newer on the cloud
        let files_to_download = self.files_to_pull(&namespace, &local_files).await?;

        let conflicts: HashSet<PathBuf> = self
            .files_to_conflict(&namespace, &local_files, &files_to_download)
            .await?
            .into_iter()
            .collect();

        if dry_run {
            for file in &conflicts {
                println!(
                    "would stash '{}' as a conflict (differs from the newer cloud copy)",
                    file.display()
                );
            }
            for file in &files_to_download {
                println!("would download '{}'", file.display());
            }
            return Ok(());
        }

        for file in &files_to_download {
            if conflicts.contains(file) {
                let src = root.join(file);
                let dst = trash_root.join(&stamp).join(file);
                let move_result = (|| -> std::io::Result<()> {
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&src, &dst)?;
                    Ok(())
                })();
                match move_result {
                    Ok(()) => {
                        eprintln!(
                            "conflict: '{}' differs from the cloud copy and the cloud copy is \
                             newer; local edition preserved in '{}'",
                            file.display(),
                            dst.display()
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "warning: conflict on '{}' could not be stashed in the trash ({}); \
                             leaving the local copy in place and skipping the download",
                            file.display(),
                            e
                        );
                        continue;
                    }
                }
            }

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
