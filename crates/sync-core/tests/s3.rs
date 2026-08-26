use anyhow::Result;
use std::io::Write;
use sync_core::Bucket;
use sync_core::Config;
use tempfile::NamedTempFile;
use uuid::Uuid;

pub async fn create_bucket() -> Result<Bucket> {
    let config = Config::new()?;
    let bucket = Bucket::new(
        &config.bucket_name.to_string(),
        &config.aws_default_region.to_string(),
    )
    .await;

    Ok(bucket)
}

// Also tests for the bucket exists function
#[tokio::test]
async fn test_create_bucket_with_bucket() -> Result<()> {
    let bucket = Bucket::new("test", "us-west-2").await;
    bucket.create_bucket().await?;

    assert!(bucket.bucket_exists().await?);

    Ok(())
}

// Also tests for the bucket exists function
// this code does not remove the bucket
#[tokio::test]
async fn test_create_bucket_without_bucket() -> Result<()> {
    let bucket = create_bucket().await?;

    assert!(!bucket.bucket_exists().await?);

    Ok(())
}

#[tokio::test]
async fn test_upload_object() -> Result<()> {
    let bucket = create_bucket().await?;

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
