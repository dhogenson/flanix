use anyhow::Result;

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_record(pool: sqlx::PgPool) -> Result<()> {
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
