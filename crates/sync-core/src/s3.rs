use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};

use std::path::Path;
use uuid::Uuid;

pub struct Bucket {
    pub name: String,
    pub location: String,
    pub client: Client,
}

impl Bucket {
    pub async fn new(name: &str, location: &str) -> Self {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        Self {
            name: name.to_string(),
            location: location.to_string(),
            client: Client::new(&config),
        }
    }

    pub async fn init(&self) -> Result<()> {
        if !self.bucket_exists().await? {
            self.create_bucket().await?;
        }

        Ok(())
    }

    pub async fn bucket_exists(&self) -> Result<bool> {
        let result = self.client.list_buckets().send().await?;
        let mut has_bucket = false;

        if let Some(buckets) = result.buckets {
            for bucket in buckets {
                if bucket.name.unwrap_or_default() == self.name {
                    has_bucket = true;
                    break;
                }
            }
        }

        Ok(has_bucket)
    }

    pub async fn create_bucket(&self) -> Result<()> {
        let bucket_config = CreateBucketConfiguration::builder()
            .location_constraint(BucketLocationConstraint::from(self.location.as_str()))
            .build();

        self.client
            .create_bucket()
            .bucket(&self.name)
            .create_bucket_configuration(bucket_config)
            .send()
            .await?;

        Ok(())
    }

    pub async fn upload_object(&self, uuid: Uuid, file_path: &str) -> Result<()> {
        let file = ByteStream::from_path(Path::new(file_path)).await?;
        self.client
            .put_object()
            .bucket(&self.name)
            .key(uuid)
            .body(file)
            .send()
            .await?;

        Ok(())
    }

    pub async fn object_exists(&self, key: Uuid) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.name)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(err) => {
                // Check if err is not found
                if let Some(serice_err) = err.as_service_error() {
                    if serice_err.is_not_found() {
                        return Ok(false);
                    }
                }
                Err(err.into())
            }
        }
    }
}
