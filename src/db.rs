use anyhow::Result;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use uuid::Uuid;

pub struct Database {
    pub pool: Pool<Postgres>
}

impl Database {
    pub async fn new(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;

        Ok(Self { pool: pool })
    }

    pub async fn init(&mut self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS person (
            id uuid PRIMARY KEY,
            name TEXT NOT NULL,
            local_path TEXT NOT NULL
            )"
        )

        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add_file(&self, uuid: Uuid, local_path: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO files (id, file_path)
             VALUES ($1, $2)"
        )
        .bind(uuid)
        .bind(local_path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

}
