use anyhow::Result;
use flanix_core::Bucket;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use sync_config::Config;
use tempfile::NamedTempFile;
use uuid::Uuid;

fn unique_bucket_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("sync-test-{}-{}", std::process::id(), n)
}

#[allow(dead_code)]
fn test_config(bucket_name: String) -> Config {
    let mut config = Config::default();
    config.bucket_name = bucket_name;
    config.aws_default_region = "us-west-2".to_string();
    config
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
    let uuid = Uuid::new_v4();
    let mut file = NamedTempFile::new()?;
    write!(file, "this is a fake file")?;

    bucket
        .upload_object(uuid, &file.path().to_string_lossy())
        .await?;

    assert!(bucket.object_exists(uuid).await?);

    Ok(())
}
