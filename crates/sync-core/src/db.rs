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
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS files (
            id uuid PRIMARY KEY,
            bucket_key TEXT NOT NULL,
            local_path TEXT NOT NULL,
            group_id uuid not null,
            created_at timestamp not null default NOW()
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS groups (
            id uuid PRIMARY KEY,
            name TEXT NOT NULL,
            created_at timestamp not null default NOW()
            )",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_file(
        &self,
        uuid: Uuid,
        bucket_key: Uuid,
        local_path: &str,
        group_id: Uuid,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO files (id, bucket_key, local_path, group_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(uuid)
        .bind(bucket_key)
        .bind(local_path)
        .bind(group_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_group(&self, uuid: Uuid, group_name: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO groups (id, name)
             VALUES ($1, $2)",
        )
        .bind(uuid)
        .bind(group_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn group_exists(&self, name: &str) -> Result<bool, DbError> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM groups WHERE name = $1) AS value_exists;",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    pub async fn get_group_id(&self, name: &str) -> Result<Uuid, DbError> {
        let uuid: Uuid = sqlx::query_scalar("SELECT id FROM groups WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("group '{}' not found", name)))?;

        Ok(uuid)
    }
}
