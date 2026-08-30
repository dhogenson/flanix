// use anyhow::Result;
use crate::errors::DbError;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use uuid::Uuid;

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
        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, namespace_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(uuid)
        .bind(bucket_key)
        .bind(local_path)
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

    pub async fn namespace_exists(&self, name: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM namespaces WHERE name = $1) AS value_exists;",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    pub async fn get_namespace_id(&self, name: &str) -> Result<Uuid, DbError> {
        let uuid: Uuid = sqlx::query_scalar("SELECT id FROM namespaces WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("namespace '{}' not found", name)))?;

        Ok(uuid)
    }
}
