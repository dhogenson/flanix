use anyhow::Result;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use sync_core::Bucket;
use tempfile::NamedTempFile;
use uuid::Uuid;

fn unique_bucket_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("sync-test-{}-{}", std::process::id(), n)
}

// Also tests for the bucket exists function
#[tokio::test]
async fn test_create_bucket_with_bucket() -> Result<()> {
    let bucket = Bucket::new(&unique_bucket_name(), "us-west-2").await;
    bucket.create_bucket().await?;

    assert!(bucket.bucket_exists().await?);

    Ok(())
}

// Also tests for the bucket exists function
// this code does not remove the bucket
#[tokio::test]
async fn test_create_bucket_without_bucket() -> Result<()> {
    let bucket = Bucket::new(&unique_bucket_name(), "us-west-2").await;

    assert!(!bucket.bucket_exists().await?);

    Ok(())
}

#[tokio::test]
async fn test_upload_object() -> Result<()> {
    let bucket = Bucket::new(&unique_bucket_name(), "us-west-2").await;

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
