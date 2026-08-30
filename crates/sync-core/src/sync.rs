/*
 * This file is for putting everything together, like for putting db and s3 together
 */

use anyhow::Result;
use chrono::DateTime;
use chrono::TimeDelta;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::Bucket;
use crate::Config;
use crate::Database;
use crate::scan_files;
use uuid::Uuid;

fn truncate_to_micros(t: DateTime<Utc>) -> DateTime<Utc> {
    let sub_micro_ns = (t.timestamp_subsec_nanos() % 1000) as i64;
    if sub_micro_ns == 0 {
        t
    } else {
        t - TimeDelta::nanoseconds(sub_micro_ns)
    }
}

pub struct Sync {
    database: Database,
    bucket: Bucket,
}

impl Sync {
    pub async fn new() -> Result<Self> {
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

    pub async fn push(&self, namespace: String, path: String) -> Result<()> {
        // self.files_to_upload(&namespace.to_string(), PathBuf::from(&path))
        //     .await?;
        // return Ok(());
        // TODO: make a error type and return that error
        if !self
            .database
            .namespace_exists(&namespace.to_string())
            .await?
        {
            return Ok(());
        }

        let namespace_id = self.database.get_namespace_id(&namespace).await?;

        // let files = self
        //     .files_to_upload(&namespace.to_string(), PathBuf::from(&path))
        //     .await?;

        let files = self
            .files_to_upload(&namespace, PathBuf::from(&path))
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
        Ok(())
    }

    pub fn pull(&self, _id: String, _path: String) {}

    pub async fn add(&self, name: String) -> Result<()> {
        if !self.database.namespace_exists(&name.to_string()).await? {
            let uuid = Uuid::new_v4();
            self.database
                .create_namespace(uuid, &name.to_string())
                .await?;
        }
        Ok(())
    }

    async fn files_to_upload(&self, namespace: &str, path: PathBuf) -> Result<Vec<PathBuf>> {
        let local_files = scan_files(path)?;
        let cloud_files = self.database.get_files(namespace).await?;

        let cloud_index: HashMap<PathBuf, DateTime<Utc>> = cloud_files
            .into_iter()
            .map(|f| (PathBuf::from(f.local_path), f.modified_at))
            .collect();

        let mut to_upload = Vec::new();

        for local_file in local_files {
            let local_mtime = local_file.modified;
            let local_mtime = truncate_to_micros(local_mtime);

            match cloud_index.get(&local_file.path) {
                // Not in cloud at all -> needs uploading
                None => to_upload.push(local_file.path),
                // In cloud, but local file is newer -> needs re-uploading
                Some(cloud_mtime) if local_mtime > *cloud_mtime => to_upload.push(local_file.path),
                // Otherwise, up to date, skip
                _ => {}
            }
        }

        println!("{:?}", to_upload);

        Ok(to_upload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta, Utc};
    use sqlx::PgPool;
    use std::fs;
    use tempfile::TempDir;

    // Build a Sync wired to the isolated #[sqlx::test] database. The bucket is
    // not exercised by files_to_upload, so a real client is only constructed to
    // satisfy the struct's fields.
    async fn test_sync(pool: PgPool) -> Sync {
        let database = Database::from_pool(pool).await;
        let bucket = Bucket::new("test-bucket", "us-west-2").await;
        Sync { database, bucket }
    }

    async fn create_namespace(db: &Database, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        db.create_namespace(id, name).await.unwrap();
        id
    }

    // Insert a record that simulates a file already stored in the "cloud" for
    // the given namespace, with a specific modification time.
    async fn insert_cloud_file(
        db: &Database,
        local_path: &str,
        modified_at: DateTime<Utc>,
        namespace_id: Uuid,
    ) {
        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, modified_at, namespace_id)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(local_path)
        .bind(modified_at)
        .bind(namespace_id)
        .execute(&db.pool)
        .await
        .unwrap();
    }

    fn write_file(dir: &TempDir, name: &str) -> Result<PathBuf> {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, "test content")?;
        Ok(path)
    }

    fn file_modified(path: &PathBuf) -> Result<DateTime<Utc>> {
        let meta = fs::metadata(path)?;
        Ok(meta.modified()?.into())
    }

    fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
        paths.sort();
        paths
    }

    // The files.modified_at column is TIMESTAMPTZ (µs precision) while filesystem
    // mtimes have nanosecond precision, so the exact mtime read back from the DB
    // is truncated. Return the smallest value >= t that the column can store so
    // that "equal at DB precision" cases compare as up-to-date and are skipped.
    fn ceil_to_db_precision(t: DateTime<Utc>) -> DateTime<Utc> {
        let sub_micro_ns = (t.timestamp_subsec_nanos() % 1000) as i64;
        if sub_micro_ns == 0 {
            t
        } else {
            t + TimeDelta::nanoseconds(1000 - sub_micro_ns)
        }
    }

    #[sqlx::test]
    async fn skips_untouched_file_after_previous_upload(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let a = write_file(&dir, "a.txt")?;
        let mtime = file_modified(&a)?;
        let ns_id = create_namespace(&sync.database, "ns-reupload").await;

        // Simulate a prior add_file: the DB stores the local mtime truncated down
        // to microsecond precision (not ceil'ed). This is what actually happens.
        insert_cloud_file(
            &sync.database,
            &a.to_string_lossy(),
            truncate_to_micros(mtime),
            ns_id,
        )
        .await;

        let to_upload = sync
            .files_to_upload("ns-reupload", dir.path().to_path_buf())
            .await?;

        // Even though local ns mtime is strictly greater than the stored cloud
        // mtime, they are equal at DB precision, so it must not re-upload.
        assert!(to_upload.is_empty());
        Ok(())
    }

    #[sqlx::test]
    async fn selects_files_not_in_cloud(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let a = write_file(&dir, "a.txt")?;
        let b = write_file(&dir, "sub/b.txt")?;

        create_namespace(&sync.database, "ns-new").await;

        let to_upload = sync
            .files_to_upload("ns-new", dir.path().to_path_buf())
            .await?;

        // Neither file is in the cloud, so both must be selected.
        assert_eq!(sorted(to_upload), sorted(vec![a, b]));
        Ok(())
    }

    #[sqlx::test]
    async fn skips_files_up_to_date(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let a = write_file(&dir, "a.txt")?;
        let mtime = file_modified(&a)?;
        let ns_id = create_namespace(&sync.database, "ns-skip").await;

        // Cloud copy is not older than the local file -> up to date, skip.
        insert_cloud_file(
            &sync.database,
            &a.to_string_lossy(),
            ceil_to_db_precision(mtime),
            ns_id,
        )
        .await;

        let to_upload = sync
            .files_to_upload("ns-skip", dir.path().to_path_buf())
            .await?;

        assert!(to_upload.is_empty());
        Ok(())
    }

    #[sqlx::test]
    async fn skips_when_cloud_copy_is_newer(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let a = write_file(&dir, "a.txt")?;
        let mtime = file_modified(&a)?;
        let ns_id = create_namespace(&sync.database, "ns-cloud-newer").await;

        // Cloud copy was modified after the local file -> no re-upload.
        insert_cloud_file(
            &sync.database,
            &a.to_string_lossy(),
            mtime + TimeDelta::hours(1),
            ns_id,
        )
        .await;

        let to_upload = sync
            .files_to_upload("ns-cloud-newer", dir.path().to_path_buf())
            .await?;

        assert!(to_upload.is_empty());
        Ok(())
    }

    #[sqlx::test]
    async fn selects_when_local_copy_is_newer(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let a = write_file(&dir, "a.txt")?;
        let mtime = file_modified(&a)?;
        let ns_id = create_namespace(&sync.database, "ns-local-newer").await;

        // Cloud copy is older than the local file -> re-upload.
        insert_cloud_file(
            &sync.database,
            &a.to_string_lossy(),
            mtime - TimeDelta::hours(1),
            ns_id,
        )
        .await;

        let to_upload = sync
            .files_to_upload("ns-local-newer", dir.path().to_path_buf())
            .await?;

        assert_eq!(to_upload, vec![a]);
        Ok(())
    }

    #[sqlx::test]
    async fn mixes_upload_and_skip_decisions(pool: PgPool) -> Result<()> {
        let sync = test_sync(pool.clone()).await;
        let dir = tempfile::tempdir()?;
        let new_file = write_file(&dir, "new.txt")?; // no cloud copy -> upload
        let older = write_file(&dir, "older.txt")?; // local newer -> upload
        let same = write_file(&dir, "same.txt")?; // equal -> skip
        let cloud_newer = write_file(&dir, "cloud-newer.txt")?; // cloud newer -> skip
        let ns_id = create_namespace(&sync.database, "ns-mixed").await;

        insert_cloud_file(
            &sync.database,
            &older.to_string_lossy(),
            file_modified(&older)? - TimeDelta::hours(1),
            ns_id,
        )
        .await;
        insert_cloud_file(
            &sync.database,
            &same.to_string_lossy(),
            ceil_to_db_precision(file_modified(&same)?),
            ns_id,
        )
        .await;
        insert_cloud_file(
            &sync.database,
            &cloud_newer.to_string_lossy(),
            file_modified(&cloud_newer)? + TimeDelta::hours(1),
            ns_id,
        )
        .await;

        let to_upload = sync
            .files_to_upload("ns-mixed", dir.path().to_path_buf())
            .await?;

        assert_eq!(sorted(to_upload), sorted(vec![new_file, older]));
        Ok(())
    }
}
