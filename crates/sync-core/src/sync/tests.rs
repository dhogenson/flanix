use super::*;
use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::PgPool;
use std::fs;
use tempfile::TempDir;

// Build a Sync wired to the isolated #[sqlx::test] database. The bucket is
// not exercised by files_to_upload, so a real client is only constructed to
// satisfy the struct's fields.
async fn test_sync(pool: PgPool) -> Sync {
    let database = Database::from_pool(pool).await;
    let bucket = Bucket::new("test-bucket", "us-west-2").await;
    Sync { database, bucket }
}

async fn create_namespace(db: &Database, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    db.create_namespace(id, name).await.unwrap();
    id
}

// Insert a record that simulates a file already stored in the "cloud" for
// the given namespace, with a specific modification time.
async fn insert_cloud_file(
    db: &Database,
    local_path: &str,
    modified_at: DateTime<Utc>,
    namespace_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO files (id, bucket_key, local_path, modified_at, namespace_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(local_path)
    .bind(modified_at)
    .bind(namespace_id)
    .execute(&db.pool)
    .await
    .unwrap();
}

fn write_file(dir: &TempDir, name: &str) -> Result<PathBuf> {
    let path = dir.path().join(name);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, "test content")?;
    Ok(path)
}

fn file_modified(path: &PathBuf) -> Result<DateTime<Utc>> {
    let meta = fs::metadata(path)?;
    Ok(meta.modified()?.into())
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths
}

fn ceil_to_db_precision(t: DateTime<Utc>) -> DateTime<Utc> {
    let sub_micro_ns = (t.timestamp_subsec_nanos() % 1000) as i64;
    if sub_micro_ns == 0 {
        t
    } else {
        t + TimeDelta::nanoseconds(1000 - sub_micro_ns)
    }
}

#[sqlx::test]
async fn skips_untouched_file_after_previous_upload(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-reupload").await;

    // Simulate a prior add_file: the DB stores the local mtime truncated down
    // to microsecond precision (not ceil'ed). This is what actually happens.
    insert_cloud_file(
        &sync.database,
        &a.to_string_lossy(),
        truncate_to_micros(mtime),
        ns_id,
    )
    .await;

    let to_upload = sync
        .files_to_upload("ns-reupload", dir.path().to_path_buf())
        .await?;

    // Even though local ns mtime is strictly greater than the stored cloud
    // mtime, they are equal at DB precision, so it must not re-upload.
    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn selects_files_not_in_cloud(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let b = write_file(&dir, "sub/b.txt")?;

    create_namespace(&sync.database, "ns-new").await;

    let to_upload = sync
        .files_to_upload("ns-new", dir.path().to_path_buf())
        .await?;

    // Neither file is in the cloud, so both must be selected.
    assert_eq!(sorted(to_upload), sorted(vec![a, b]));
    Ok(())
}

#[sqlx::test]
async fn skips_files_up_to_date(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-skip").await;

    // Cloud copy is not older than the local file -> up to date, skip.
    insert_cloud_file(
        &sync.database,
        &a.to_string_lossy(),
        ceil_to_db_precision(mtime),
        ns_id,
    )
    .await;

    let to_upload = sync
        .files_to_upload("ns-skip", dir.path().to_path_buf())
        .await?;

    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn skips_when_cloud_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-cloud-newer").await;

    // Cloud copy was modified after the local file -> no re-upload.
    insert_cloud_file(
        &sync.database,
        &a.to_string_lossy(),
        mtime + TimeDelta::hours(1),
        ns_id,
    )
    .await;

    let to_upload = sync
        .files_to_upload("ns-cloud-newer", dir.path().to_path_buf())
        .await?;

    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn selects_when_local_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-local-newer").await;

    // Cloud copy is older than the local file -> re-upload.
    insert_cloud_file(
        &sync.database,
        &a.to_string_lossy(),
        mtime - TimeDelta::hours(1),
        ns_id,
    )
    .await;

    let to_upload = sync
        .files_to_upload("ns-local-newer", dir.path().to_path_buf())
        .await?;

    assert_eq!(to_upload, vec![a]);
    Ok(())
}

#[sqlx::test]
async fn mixes_upload_and_skip_decisions(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let new_file = write_file(&dir, "new.txt")?; // no cloud copy -> upload
    let older = write_file(&dir, "older.txt")?; // local newer -> upload
    let same = write_file(&dir, "same.txt")?; // equal -> skip
    let cloud_newer = write_file(&dir, "cloud-newer.txt")?; // cloud newer -> skip
    let ns_id = create_namespace(&sync.database, "ns-mixed").await;

    insert_cloud_file(
        &sync.database,
        &older.to_string_lossy(),
        file_modified(&older)? - TimeDelta::hours(1),
        ns_id,
    )
    .await;
    insert_cloud_file(
        &sync.database,
        &same.to_string_lossy(),
        ceil_to_db_precision(file_modified(&same)?),
        ns_id,
    )
    .await;
    insert_cloud_file(
        &sync.database,
        &cloud_newer.to_string_lossy(),
        file_modified(&cloud_newer)? + TimeDelta::hours(1),
        ns_id,
    )
    .await;

    let to_upload = sync
        .files_to_upload("ns-mixed", dir.path().to_path_buf())
        .await?;

    assert_eq!(sorted(to_upload), sorted(vec![new_file, older]));
    Ok(())
}
