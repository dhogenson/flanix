// use anyhow::Result;
use crate::errors::DbError;
use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use tokio::fs;
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

impl Database {
    pub async fn new(url: &str) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;

        Ok(Self { pool: pool })
    }

    pub async fn from_pool(pool: Pool<Postgres>) -> Self {
        Self { pool: pool }
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
        namespace_id: Uuid,
    ) -> Result<(), DbError> {
        let meta_data = fs::metadata(local_path).await?;
        let modified_time: std::time::SystemTime = meta_data.modified()?;
        let modified_time_utc: DateTime<Utc> = modified_time.into();

        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, modified_at, namespace_id)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(uuid)
        .bind(bucket_key)
        .bind(local_path)
        .bind(modified_time_utc)
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

    // TODO: i want to rename everything related to files, because im using files to much in here
    pub async fn get_files(&self, namespace: &str) -> Result<Vec<File>, DbError> {
        let namespace_id = self.get_namespace_id(namespace).await?;

        let files = sqlx::query_as::<_, File>("SELECT id, bucket_key, local_path, modified_at, namespace_id FROM files WHERE namespace_id = $1")
            .bind(namespace_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(files)
    }

    // pub async fn get_modifaction_times(
    //     &self,
    //     namespace: &str,
    // ) -> Result<Vec<DateTime<Utc>>, DbError> {
    //     let namespace_id = self.get_namespace_id(namespace).await?;

    //     let modified_times: Vec<DateTime<Utc>> =
    //         sqlx::query_scalar("SELECT id FROM files WHERE namespace_id = $1")
    //             .bind(namespace_id)
    //             .fetch_all(&self.pool)
    //             .await?;

    //     Ok(modified_times)
    // }
}
