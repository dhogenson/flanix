use crate::errors::SyncError;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use sync_config::Config;
use tokio::io::AsyncWriteExt;

const GLOBAL_REGION: &str = "us-east-1";

use std::path::Path;
use uuid::Uuid;

pub struct Bucket {
    pub name: String,
    pub location: String,
    pub client: Client,
}

impl Bucket {
    pub async fn new(config: &Config) -> Self {
        let mut sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(config.aws_default_region.clone()))
            .endpoint_url(&config.aws_endpoint);

        // Only set credentials if they're non-empty (avoids overriding IAM roles)
        if !config.aws_access_key_id.is_empty() && !config.aws_secret_access_key.is_empty() {
            sdk_config = sdk_config.credentials_provider(aws_sdk_s3::config::Credentials::new(
                config.aws_access_key_id.clone(),
                config.aws_secret_access_key.clone(),
                None,
                None,
                "sync-config",
            ));
        }

        let sdk_config = sdk_config.load().await;
        Self {
            name: config.bucket_name.clone(),
            location: config.aws_default_region.clone(),
            client: Client::new(&sdk_config),
        }
    }

    pub async fn from_env_vars(bucket_name: &str) -> Result<Self, SyncError> {
        use std::env;
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        Ok(Self {
            name: bucket_name.to_string(),
            location: env::var("AWS_DEFAULT_REGION")?,
            client: Client::new(&config),
        })
    }

    /// Created bucket if it does not exists in s3
    pub async fn init(&self) -> Result<(), SyncError> {
        if !self.bucket_exists().await? {
            self.create_bucket().await?;
        }

        Ok(())
    }

    pub async fn bucket_exists(&self) -> Result<bool, SyncError> {
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

    pub async fn create_bucket(&self) -> Result<(), SyncError> {
        let mut req = self.client.create_bucket().bucket(&self.name);

        // Only set a location constraint for non-default regions. For us-east-1
        // the constraint must be omitted entirely or S3 rejects the request.
        if self.location != GLOBAL_REGION {
            let bucket_config = CreateBucketConfiguration::builder()
                .location_constraint(BucketLocationConstraint::from(self.location.as_str()))
                .build();
            req = req.create_bucket_configuration(bucket_config);
        }

        req.send().await?;

        Ok(())
    }

    pub async fn upload_object(&self, uuid: Uuid, file_path: &str) -> Result<(), SyncError> {
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

    pub async fn object_exists(&self, key: Uuid) -> Result<bool, SyncError> {
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
                if let Some(serice_err) = err.as_service_error()
                    && serice_err.is_not_found()
                {
                    return Ok(false);
                }
                Err(err.into())
            }
        }
    }

    pub async fn delete_object(&self, key: Uuid) -> Result<(), SyncError> {
        self.client
            .delete_object()
            .key(key)
            .bucket(&self.name)
            .send()
            .await?;

        Ok(())
    }

    pub async fn download_object(&self, key: Uuid, path: &str) -> Result<(), SyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.name)
            .key(key)
            .send()
            .await?;

        let data = response.body.collect().await?.into_bytes();
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;

        Ok(())
    }
}
