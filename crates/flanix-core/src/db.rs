use crate::errors::DbError;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgExecutor, Pool, Postgres, postgres::PgPoolOptions};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct File {
    pub id: Uuid,
    pub bucket_key: String,
    pub local_path: String,
    pub modified_at: DateTime<Utc>,
    pub namespace_id: Uuid,
    pub file_hash: Option<String>,
}

pub struct Database {
    pub pool: Pool<Postgres>,
}

pub fn truncate_to_micros(t: DateTime<Utc>) -> DateTime<Utc> {
    let sub_micro_ns = (t.timestamp_subsec_nanos() % 1000) as i64;
    if sub_micro_ns == 0 {
        t
    } else {
        t - TimeDelta::nanoseconds(sub_micro_ns)
    }
}

impl Database {
    pub async fn new(url: &str) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;

        Ok(Self { pool })
    }

    pub async fn from_pool(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn init(&mut self) -> Result<(), DbError> {
        sqlx::migrate!().run(&self.pool).await?;

        Ok(())
    }

    pub async fn begin(&self) -> Result<sqlx::Transaction<'_, Postgres>, DbError> {
        Ok(self.pool.begin().await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_file(
        &self,
        executor: impl PgExecutor<'_>,
        uuid: Uuid,
        bucket_key: &str,
        local_path: &str,
        modified_at: DateTime<Utc>,
        file_hash: Option<&str>,
        namespace_id: Uuid,
    ) -> Result<(), DbError> {
        // The filesystem gives nanosecond precision, but the DB column only
        // stores microseconds. Truncate explicitly (rather than relying on
        // Postgres to silently do it) so the stored value matches the precision
        // that `files_to_upload` compares against.
        let modified_at = truncate_to_micros(modified_at);

        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, modified_at, file_hash, namespace_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(uuid)
        .bind(bucket_key)
        .bind(local_path)
        .bind(modified_at)
        .bind(file_hash)
        .bind(namespace_id)
        .execute(executor)
        .await?;

        Ok(())
    }

    pub async fn create_namespace(&self, uuid: Uuid, namespace: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO namespaces (id, name)
             VALUES ($1, $2)",
        )
        .bind(uuid)
        .bind(namespace)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn namespace_exists(&self, namespace: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM namespaces WHERE name = $1) AS value_exists;",
        )
        .bind(namespace)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    pub async fn get_namespace_id(&self, namespace: &str) -> Result<Uuid, DbError> {
        let uuid: Uuid = sqlx::query_scalar("SELECT id FROM namespaces WHERE name = $1")
            .bind(namespace)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("namespace '{}' not found", namespace)))?;

        Ok(uuid)
    }

    pub async fn delete_file(
        &self,
        executor: impl PgExecutor<'_>,
        namespace_id: Uuid,
        local_path: &str,
    ) -> Result<(), DbError> {
        sqlx::query("DELETE FROM files WHERE namespace_id = $1 AND local_path = $2")
            .bind(namespace_id)
            .bind(local_path)
            .execute(executor)
            .await?;

        Ok(())
    }

    pub async fn get_files(&self, namespace: &str) -> Result<Vec<File>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let files = sqlx::query_as::<_, File>("SELECT id, bucket_key, local_path, modified_at, namespace_id, file_hash FROM files WHERE namespace_id = $1")
            .bind(namespace_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(files)
    }

    /// Returns every tracked file across all namespaces.
    pub async fn get_all_files(&self) -> Result<Vec<File>, DbError> {
        let files = sqlx::query_as::<_, File>(
            "SELECT id, bucket_key, local_path, modified_at, namespace_id, file_hash FROM files",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    pub async fn file_exists(&self, namespace_id: Uuid, local_path: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM files WHERE namespace_id = $1 AND local_path = $2) AS value_exists",
        )
        .bind(namespace_id)
        .bind(local_path)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    /// Returns the cloud-side modification time recorded for a file path,
    /// i.e. the mtime of the object as it sits in the bucket.
    pub async fn modified_at_for_path(
        &self,
        namespace: &str,
        path: &str,
    ) -> Result<DateTime<Utc>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let modified_at = sqlx::query_scalar(
            "SELECT modified_at FROM files WHERE namespace_id = $1 AND local_path = $2",
        )
        .bind(namespace_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            DbError::NotFound(format!(
                "no file '{}' tracked in namespace '{}'",
                path, namespace
            ))
        })?;

        Ok(modified_at)
    }

    pub async fn update_file_modified_at(
        &self,
        executor: impl PgExecutor<'_>,
        namespace_id: Uuid,
        local_path: &str,
        modified_at: DateTime<Utc>,
        file_hash: Option<&str>,
    ) -> Result<(), DbError> {
        let modified_at = truncate_to_micros(modified_at);

        sqlx::query(
            "UPDATE files SET modified_at = $1, file_hash = $2 WHERE namespace_id = $3 AND local_path = $4",
        )
        .bind(modified_at)
        .bind(file_hash)
        .bind(namespace_id)
        .bind(local_path)
        .execute(executor)
        .await?;

        Ok(())
    }

    /// Records that a file was deleted on the cloud. `pull` uses these
    /// tombstones to know it should remove the local copy instead of backing
    /// the file back up as if it were new.
    pub async fn add_tombstone(
        &self,
        executor: impl PgExecutor<'_>,
        namespace_id: Uuid,
        local_path: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO deleted_files (id, namespace_id, local_path, deleted_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (namespace_id, local_path) DO UPDATE SET deleted_at = NOW()",
        )
        .bind(Uuid::new_v4())
        .bind(namespace_id)
        .bind(local_path)
        .execute(executor)
        .await?;

        Ok(())
    }

    /// Clears the tombstone for a path, used when the file is re-uploaded.
    pub async fn remove_tombstone(
        &self,
        executor: impl PgExecutor<'_>,
        namespace_id: Uuid,
        local_path: &str,
    ) -> Result<(), DbError> {
        sqlx::query("DELETE FROM deleted_files WHERE namespace_id = $1 AND local_path = $2")
            .bind(namespace_id)
            .bind(local_path)
            .execute(executor)
            .await?;

        Ok(())
    }

    /// Returns `(local_path, deleted_at)` for every tombstone in a namespace.
    pub async fn get_tombstones(
        &self,
        namespace: &str,
    ) -> Result<Vec<(String, DateTime<Utc>)>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let tombstones = sqlx::query_as::<_, (String, DateTime<Utc>)>(
            "SELECT local_path, deleted_at FROM deleted_files WHERE namespace_id = $1",
        )
        .bind(namespace_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(tombstones)
    }

    /// Removes tombstones older than `before`, bounding table growth. Any
    /// device that hasn't pulled within the TTL may re-back-up the file as
    /// new the next time it does.
    pub async fn prune_tombstones(
        &self,
        namespace: &str,
        before: DateTime<Utc>,
    ) -> Result<u64, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let result =
            sqlx::query("DELETE FROM deleted_files WHERE namespace_id = $1 AND deleted_at < $2")
                .bind(namespace_id)
                .bind(before)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected())
    }

    /// Inserts the device or refreshes its heartbeat. Kept separate from the
    /// per-device manifest so a future step can use `last_seen_at` to decide
    /// when every connected device has observed a deletion before garbage
    /// collecting its tombstone.
    pub async fn register_device(&self, device_id: Uuid, name: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO devices (id, name, last_seen_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (id) DO UPDATE SET last_seen_at = NOW()",
        )
        .bind(device_id)
        .bind(name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Returns every path this device previously had for a namespace. This is
    /// the device's own manifest; absence here is never treated as a deletion.
    pub async fn get_device_files(
        &self,
        device_id: Uuid,
        namespace: &str,
    ) -> Result<Vec<PathBuf>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let paths: Vec<String> = sqlx::query_scalar(
            "SELECT local_path FROM device_files WHERE device_id = $1 AND namespace_id = $2",
        )
        .bind(device_id)
        .bind(namespace_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(paths.into_iter().map(PathBuf::from).collect())
    }

    /// Replaces this device's manifest for a namespace with `paths`. Idempotent,
    /// so it may be run with the pool even after the sync transaction commits.
    pub async fn replace_device_files(
        &self,
        executor: impl PgExecutor<'_>,
        device_id: Uuid,
        namespace: &str,
        paths: &[PathBuf],
    ) -> Result<(), DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let paths: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        // One statement: drop every manifest entry not in `paths`, then insert
        // every entry that is. Data-modifying CTEs operate row-disjointly here
        // (the delete targets paths absent from the list, the insert the
        // listed ones), so they can share a single executed query.
        sqlx::query(
            "WITH input AS (
                 SELECT $1::uuid AS device_id,
                        $2::uuid AS namespace_id,
                        unnest($3::text[]) AS local_path
             ),
             to_delete AS (
                 DELETE FROM device_files df
                 USING input
                 WHERE df.device_id = input.device_id
                   AND df.namespace_id = input.namespace_id
                   AND df.local_path NOT IN (SELECT local_path FROM input)
             )
             INSERT INTO device_files (device_id, namespace_id, local_path)
             SELECT device_id, namespace_id, local_path FROM input
             ON CONFLICT (device_id, namespace_id, local_path) DO NOTHING",
        )
        .bind(device_id)
        .bind(namespace_id)
        .bind(paths)
        .execute(executor)
        .await?;

        Ok(())
    }
}
