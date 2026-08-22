use anyhow::Result;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use uuid::Uuid;

pub struct Database {
    pub pool: Pool<Postgres>,
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;

        Ok(Self { pool: pool })
    }

    pub async fn init(&mut self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS files (
            id uuid PRIMARY KEY,
            name TEXT NOT NULL,
            local_path TEXT NOT NULL,
            group_id uuid not null
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

    pub async fn add_file(&self, uuid: Uuid, local_path: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO files (id, file_path)
             VALUES ($1, $2)",
        )
        .bind(uuid)
        .bind(local_path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_group(&self, uuid: Uuid, group_name: &str) -> Result<()> {
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

    pub async fn group_exists(&self, name: &str) -> Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM groups WHERE name = $1) AS value_exists;",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }
}
