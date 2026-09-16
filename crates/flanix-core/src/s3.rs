use crate::errors::SyncError;
use aws_config::BehaviorVersion;
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider},
};
use aws_sdk_s3::{
    Client,
    config::Builder as S3ConfigBuilder,
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use sync_config::Config;

const GLOBAL_REGION: &str = "us-east-1";

pub fn bucket_key(namespace: &str, path: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    hasher.update(&[0]);
    hasher.update(path.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub struct Bucket {
    pub name: String,
    pub location: String,
    pub client: Client,
}

impl Bucket {
    /// S3-compatible services that don't support virtual-host (bucket.fqdn)
    /// addressing (e.g. Garage) require path-style requests like
    /// `/<bucket>/<key>`, so disable virtual-host addressing.
    fn client_config(config_builder: S3ConfigBuilder) -> S3ConfigBuilder {
        config_builder.force_path_style(true)
    }

    pub async fn new(config: &Config) -> Self {
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(config.aws_default_region.clone()))
            .endpoint_url(&config.aws_endpoint)
            .load()
            .await;

        // Prefer the AWS SDK credential chain (environment, profiles, IAM
        // roles, etc.). Keep config-file credentials as a fallback for local
        // development and existing configs.
        let fallback_credentials = (!config.aws_access_key_id.is_empty()
            && !config.aws_secret_access_key.is_empty())
        .then(|| {
            SharedCredentialsProvider::new(Credentials::new(
                config.aws_access_key_id.clone(),
                config.aws_secret_access_key.clone(),
                None,
                None,
                "flanix-config",
            ))
        });

        let credentials = match sdk_config.credentials_provider() {
            Some(provider) => {
                if provider.provide_credentials().await.is_ok() {
                    None
                } else {
                    fallback_credentials
                }
            }
            None => fallback_credentials,
        };
        let mut config_builder = Self::client_config(S3ConfigBuilder::from(&sdk_config));
        if let Some(credentials) = credentials {
            config_builder = config_builder.credentials_provider(credentials);
        }
        Self {
            name: config.bucket_name.clone(),
            location: config.aws_default_region.clone(),
            client: Client::from_conf(config_builder.build()),
        }
    }

    pub async fn from_env_vars(bucket_name: &str) -> Result<Self, SyncError> {
        use std::env;
        let config = Self::client_config(S3ConfigBuilder::from(
            &aws_config::load_defaults(BehaviorVersion::latest()).await,
        ))
        .build();
        Ok(Self {
            name: bucket_name.to_string(),
            location: env::var("AWS_DEFAULT_REGION")?,
            client: Client::from_conf(config),
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
        match self.client.head_bucket().bucket(&self.name).send().await {
            Ok(_) => Ok(true),
            Err(err) => {
                if let Some(svc) = err.as_service_error() {
                    if svc.is_not_found() {
                        return Ok(false);
                    }
                    // 403 Forbidden means the bucket exists but we lack permission,
                    // but it means that it still exists so treat it as such
                    if svc.meta().code() == Some("AccessDenied") {
                        return Ok(true);
                    }
                }
                Err(err.into())
            }
        }
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

    pub async fn upload_object(&self, key: &str, file_path: &str) -> Result<(), SyncError> {
        // Buffer the file so the body has a known length. The SDK otherwise
        // signs streaming bodies with `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`,
        // which Garage rejects with "Invalid payload signature".
        let bytes = tokio::fs::read(file_path).await?;
        self.client
            .put_object()
            .bucket(&self.name)
            .key(key)
            .body(ByteStream::from(bytes))
            .send()
            .await?;

        Ok(())
    }

    pub async fn object_exists(&self, key: &str) -> Result<bool, SyncError> {
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

    pub async fn delete_object(&self, key: &str) -> Result<(), SyncError> {
        self.client
            .delete_object()
            .key(key)
            .bucket(&self.name)
            .send()
            .await?;

        Ok(())
    }

    pub async fn download_object(&self, key: &str, path: &str) -> Result<(), SyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.name)
            .key(key)
            .send()
            .await?;

        // Steam file to disk
        let mut file = tokio::fs::File::create(path).await?;
        let mut body = response.body.into_async_read();
        tokio::io::copy(&mut body, &mut file).await?;
        file.sync_all().await?;

        Ok(())
    }
}
