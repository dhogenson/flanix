use anyhow::Result;
use sqlx::PgPool;
use utils::Database;
use uuid::Uuid;

#[sqlx::test]
async fn test_insert_record(pool: PgPool) -> Result<()> {
    // pool is already migrated and isolated
    sqlx::query!(
        "INSERT INTO groups (id, name) VALUES ($1, $2)",
        uuid::Uuid::new_v4(),
        "test"
    )
    .execute(&pool)
    .await?;

    Ok(())
}

#[sqlx::test]
async fn test_add_file(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await?;

    let uuid = Uuid::new_v4();
    let bucket_key = Uuid::new_v4();
    let local_path = "local_path/test.txt";
    let group_id = Uuid::new_v4();

    database
        .add_file(uuid, bucket_key, local_path, group_id)
        .await?;

    let row = sqlx::query!("SELECT id FROM files WHERE id = $1", uuid)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.id, uuid);

    Ok(())
}

#[sqlx::test]
async fn test_add_group(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await?;

    let uuid = Uuid::new_v4();

    database.add_group(uuid, "test").await?;

    let row = sqlx::query!("SELECT id FROM groups WHERE id = $1", uuid)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.id, uuid);

    Ok(())
}

#[sqlx::test]
async fn test_group_exists(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await?;

    let uuid = Uuid::new_v4();

    database.add_group(uuid, "test").await?;
    assert!(database.group_exists("test").await?);

    Ok(())
}

#[sqlx::test]
async fn test_get_group_id(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool.clone()).await?;
    let uuid = Uuid::new_v4();

    database.add_group(uuid, "test").await?;

    assert_eq!(database.get_group_id("test").await?, uuid);

    Ok(())
}
