use anyhow::Result;
use flanix_config::{Config, Namespace};
use flanix_core::{Bucket, Database, Sync};
use sqlx::PgPool;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::NamedTempFile;
use uuid::Uuid;

fn unique_bucket_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("sync-test-{}-{}", std::process::id(), n)
}

#[allow(dead_code)]
fn test_config(bucket_name: String) -> Config {
    Config {
        bucket_name,
        aws_default_region: "us-west-2".to_string(),
        ..Default::default()
    }
}

// Also tests for the bucket exists function
#[tokio::test]
async fn test_create_bucket_with_bucket() -> Result<()> {
    let bucket = Bucket::from_env_vars(&unique_bucket_name()).await?;
    bucket.create_bucket().await?;

    assert!(bucket.bucket_exists().await?);

    Ok(())
}

// Also tests for the bucket exists function
// this code does not remove the bucket
#[tokio::test]
async fn test_create_bucket_without_bucket() -> Result<()> {
    let bucket = Bucket::from_env_vars(&unique_bucket_name()).await?;

    assert!(!bucket.bucket_exists().await?);

    Ok(())
}

#[tokio::test]
async fn test_upload_object() -> Result<()> {
    let bucket = Bucket::from_env_vars(&unique_bucket_name()).await?;

    bucket.init().await?;
    let key = "test-key";
    let mut file = NamedTempFile::new()?;
    write!(file, "this is a fake file")?;

    bucket
        .upload_object(key, &file.path().to_string_lossy())
        .await?;

    assert!(bucket.object_exists(key).await?);

    Ok(())
}

#[tokio::test]
async fn test_object_content_hash_after_upload() -> Result<()> {
    let bucket = Bucket::from_env_vars(&unique_bucket_name()).await?;

    bucket.init().await?;
    let key = "test-key-hash";
    let content = "this is a fake file";
    let mut file = NamedTempFile::new()?;
    write!(file, "{content}")?;

    bucket
        .upload_object(key, &file.path().to_string_lossy())
        .await?;

    let (size, hash) = bucket.object_content_hash(key).await?;
    assert_eq!(size, content.len() as u64);
    assert_eq!(hash, blake3::hash(content.as_bytes()).to_hex().to_string());

    Ok(())
}

// A Config wired to the real test bucket (env overrides the repo defaults) and
// a local namespace root, for full `Sync` integration tests.
fn sync_test_config(bucket_name: String, namespace: &str, root: &Path) -> Config {
    Config {
        bucket_name,
        aws_default_region: std::env::var("AWS_DEFAULT_REGION")
            .unwrap_or_else(|_| "us-east-1".to_string()),
        aws_endpoint: std::env::var("AWS_ENDPOINT_URL")
            .unwrap_or_else(|_| "http://localhost:4567".to_string()),
        aws_access_key_id: std::env::var("AWS_ACCESS_KEY_ID")
            .unwrap_or_else(|_| "test".to_string()),
        aws_secret_access_key: std::env::var("AWS_SECRET_ACCESS_KEY")
            .unwrap_or_else(|_| "test".to_string()),
        device_id: Some(Uuid::new_v4().to_string()),
        namespaces: vec![Namespace::new(
            namespace.to_string(),
            root.to_string_lossy().into_owned(),
        )],
        ..Default::default()
    }
}

// A push that crashed between PUT and the DB commit leaves an orphan object in
// the bucket with no ledger record. The next push must self-heal: it detects
// the object already matches the local file, skips the PUT, and just records
// the file in the database.
#[sqlx::test]
async fn push_recovers_orphan_object_from_crashed_attempt(pool: PgPool) -> Result<()> {
    let dir = tempfile::tempdir()?;
    let local = dir.path().join("a.txt");
    tokio::fs::write(&local, "hello flanix").await?;

    let bucket_name = unique_bucket_name();
    let config = sync_test_config(bucket_name.clone(), "rec-ns", dir.path());
    let database = Database::from_pool(pool).await;
    database.create_namespace(Uuid::new_v4(), "rec-ns").await?;

    let bucket = Bucket::new(&config).await;
    bucket.init().await?;

    let sync = Sync {
        database,
        bucket,
        config,
    };

    // Simulate the crashed attempt: the object is already in the bucket but
    // the ledger knows nothing about it.
    let object_key = flanix_core::bucket_key("rec-ns", "a.txt");
    sync.bucket
        .upload_object(&object_key, &local.to_string_lossy())
        .await?;

    // The next push must succeed and catch the ledger up, not error out.
    sync.push("rec-ns".to_string()).await?;

    let files = sync.database.get_files("rec-ns").await?;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].local_path, "a.txt");
    assert_eq!(files[0].bucket_key, object_key);

    // The object still holds exactly the local content.
    let (_, remote_hash) = sync.bucket.object_content_hash(&object_key).await?;
    assert_eq!(
        remote_hash,
        blake3::hash(b"hello flanix").to_hex().to_string()
    );

    Ok(())
}
