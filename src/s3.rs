use std::path::Path;
use uuid::Uuid;
use anyhow::Result;
use aws_config::{BehaviorVersion};
use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use glob::glob;

pub struct Bucket {
    pub name: String,
    pub location: String,
    pub client: Client
}

impl Bucket {
    pub async fn new(name: &str, location: &str) -> Self {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        Self { name: name.to_string(), location: location.to_string(), client: Client::new(&config) }
    }

    pub async fn init(&self) -> Result<()> {
        let result = self.client.list_buckets().send().await?;
        let mut has_bucket = false;

        if let Some(buckets) = result.buckets {
            for bucket in buckets {
                if bucket.name.unwrap_or_default() == self.name {
                    has_bucket = true;
                    break
                }
            }
        }

        if !has_bucket {
            self.create_bucket().await?;
        }

        Ok(())
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

    pub async fn upload_file(&self, file_path: &str) -> Result<()> {
        let file = ByteStream::from_path(Path::new(file_path)).await?;
        let id = Uuid::new_v4();

        self.client
            .put_object()
            .bucket(&self.name)
            .key(id)
            .body(file)
            .send()
            .await?;

        Ok(())
    }

    pub async fn upload_directory(&self, path: &str) -> Result<()> {
        for entry in glob(&format!("{}/**/*", path)).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    if path.is_dir() {
                        continue;
                    }
                    self.upload_file(&path.to_string_lossy()).await?;
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        Ok(())
    }
}
