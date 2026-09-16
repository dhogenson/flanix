use anyhow::Result;
use chrono::{DateTime, Utc};
use flanix_core::Database;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[sqlx::test]
async fn test_insert_record(pool: PgPool) -> Result<()> {
    // pool is already migrated and isolated
    sqlx::query("INSERT INTO namespaces (id, name) VALUES ($1, $2)")
        .bind(uuid::Uuid::new_v4())
        .bind("test")
        .execute(&pool)
        .await?;

    Ok(())
}

#[sqlx::test]
async fn test_add_file(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await;

    // add_file now takes the modified time as an argument; the DB layer no
    // longer reads the file from disk.
    let dir = tempfile::tempdir()?;
    let local_path = dir.path().join("test.txt");
    tokio::fs::write(&local_path, "test content").await?;
    let modified_at: DateTime<Utc> = tokio::fs::metadata(&local_path).await?.modified()?.into();

    let uuid = Uuid::new_v4();
    let bucket_key = "some-bucket-key";
    let namespace_id = Uuid::new_v4();

    database.create_namespace(namespace_id, "test").await?;

    database
        .add_file(
            &database.pool,
            uuid,
            bucket_key,
            &local_path.to_string_lossy(),
            modified_at,
            namespace_id,
        )
        .await?;

    let row = sqlx::query("SELECT id FROM files WHERE id = $1")
        .bind(uuid)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.get::<Uuid, _>("id"), uuid);

    Ok(())
}

#[sqlx::test]
async fn test_create_namespace(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await;

    let uuid = Uuid::new_v4();

    database.create_namespace(uuid, "test").await?;

    let row = sqlx::query("SELECT id FROM namespaces WHERE id = $1")
        .bind(uuid)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.get::<Uuid, _>("id"), uuid);

    Ok(())
}

#[sqlx::test]
async fn test_namespace_exists(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await;

    let uuid = Uuid::new_v4();

    database.create_namespace(uuid, "test").await?;
    assert!(database.namespace_exists("test").await?);

    Ok(())
}

#[sqlx::test]
async fn test_get_namespace_id(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await;
    let uuid = Uuid::new_v4();

    database.create_namespace(uuid, "test").await?;

    assert_eq!(database.get_namespace_id("test").await?, uuid);

    Ok(())
}
