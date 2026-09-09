use crate::errors::DbError;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct File {
    pub id: Uuid,
    pub bucket_key: Uuid,
    pub local_path: String,
    pub modified_at: DateTime<Utc>,
    pub namespace_id: Uuid,
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

    pub async fn add_file(
        &self,
        uuid: Uuid,
        bucket_key: Uuid,
        local_path: &str,
        modified_at: DateTime<Utc>,
        namespace_id: Uuid,
    ) -> Result<(), DbError> {
        // The filesystem gives nanosecond precision, but the DB column only
        // stores microseconds. Truncate explicitly (rather than relying on
        // Postgres to silently do it) so the stored value matches the precision
        // that `files_to_upload` compares against.
        let modified_at = truncate_to_micros(modified_at);

        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, modified_at, namespace_id)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(uuid)
        .bind(bucket_key)
        .bind(local_path)
        .bind(modified_at)
        .bind(namespace_id)
        .execute(&self.pool)
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

    pub async fn get_file_ids(&self, namespace: &str) -> Result<Vec<Uuid>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let uuids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM files WHERE namespace_id = $1")
            .bind(namespace_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(uuids)
    }

    pub async fn delete_file(&self, namespace: &str, local_path: &str) -> Result<(), DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        sqlx::query("DELETE FROM files WHERE namespace_id = $1 AND local_path = $2")
            .bind(namespace_id)
            .bind(local_path)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn get_files(&self, namespace: &str) -> Result<Vec<File>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let files = sqlx::query_as::<_, File>("SELECT id, bucket_key, local_path, modified_at, namespace_id FROM files WHERE namespace_id = $1")
            .bind(namespace_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(files)
    }

    pub async fn bucket_key_for_path_opt(
        &self,
        namespace: &str,
        path: &str,
    ) -> Result<Option<Uuid>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let file = sqlx::query_scalar(
            "SELECT bucket_key FROM files WHERE namespace_id = $1 AND local_path = $2",
        )
        .bind(namespace_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;

        Ok(file)
    }

    pub async fn bucket_key_for_path(&self, namespace: &str, path: &str) -> Result<Uuid, DbError> {
        self.bucket_key_for_path_opt(namespace, path)
            .await?
            .ok_or_else(|| {
                DbError::NotFound(format!(
                    "no file '{}' tracked in namespace '{}'",
                    path, namespace
                ))
            })
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
        namespace: &str,
        local_path: &str,
        modified_at: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let modified_at = truncate_to_micros(modified_at);

        sqlx::query(
            "UPDATE files SET modified_at = $1 WHERE namespace_id = $2 AND local_path = $3",
        )
        .bind(modified_at)
        .bind(namespace_id)
        .bind(local_path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
