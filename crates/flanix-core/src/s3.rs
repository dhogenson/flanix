use crate::errors::SyncError;
use aws_config::BehaviorVersion;
use aws_credential_types::{
    Credentials,
    provider::{ProvideCredentials, SharedCredentialsProvider},
};
use aws_sdk_s3::{
    Client,
    config::{Builder as S3ConfigBuilder, RequestChecksumCalculation},
    primitives::ByteStream,
    types::{BucketLocationConstraint, CreateBucketConfiguration},
};
use flanix_config::Config;
use std::path::Path;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

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
    ///
    /// The SDK's default `RequestChecksumCalculation::WhenSupported` makes
    /// `put_object` add a CRC32 checksum, which forces request bodies into
    /// aws-chunked streaming-signed payloads. Garage does not support this
    /// format and rejects it with "Invalid payload signature", so only send
    /// checksums when the operation actually requires one.
    fn client_config(config_builder: S3ConfigBuilder) -> S3ConfigBuilder {
        config_builder
            .force_path_style(true)
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
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
        // Stream the file from disk in chunks instead of buffering it in
        // memory. A path-based stream reports a known content-length, so the
        // SDK signs with `AWS4-HMAC-SHA256-PAYLOAD` rather than
        // `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, which Garage rejects with
        // "Invalid payload signature".
        let body = ByteStream::read_from().path(file_path).build().await?;
        self.client
            .put_object()
            .bucket(&self.name)
            .key(key)
            .body(body)
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

    /// Streams an object back from the bucket and returns its byte size plus
    /// the blake3 of its content. Used to verify an upload actually landed
    /// intact instead of trusting that a successful PUT means the bytes match.
    pub async fn object_content_hash(&self, key: &str) -> Result<(u64, String), SyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.name)
            .key(key)
            .send()
            .await?;

        let size = response.content_length().unwrap_or_default() as u64;
        let mut reader = response.body.into_async_read();
        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }

        Ok((size, hasher.finalize().to_hex().to_string()))
    }

    pub async fn list_objects(&self) -> Result<Vec<String>, SyncError> {
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self.client.list_objects_v2().bucket(&self.name);
            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }

            let output = request.send().await?;

            for object in output.contents() {
                if let Some(key) = object.key() {
                    keys.push(key.to_string());
                }
            }

            match output.next_continuation_token() {
                Some(token) => continuation_token = Some(token.to_string()),
                None => break,
            }
        }

        Ok(keys)
    }

    pub async fn download_object(&self, key: &str, path: &str) -> Result<(), SyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.name)
            .key(key)
            .send()
            .await?;

        // Never write directly to the destination: `File::create` truncates it
        // before a byte of the stream has arrived, so an interrupted or failed
        // download would corrupt the pre-existing local copy. Stream into a
        // temp file in the same directory, sync it, then atomically rename it
        // over the destination. Until the rename the old file is untouched; on
        // any error the temp file is removed.
        let destination = Path::new(path);
        let file_name = destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_string());
        let tmp_path =
            destination.with_file_name(format!(".flanix-tmp-{file_name}-{}", Uuid::new_v4()));

        let result = async {
            let mut file = tokio::fs::File::create(&tmp_path).await?;
            let mut body = response.body.into_async_read();
            tokio::io::copy(&mut body, &mut file).await?;
            file.sync_all().await?;
            tokio::fs::rename(&tmp_path, destination).await?;
            Ok::<(), SyncError>(())
        }
        .await;

        if result.is_err() {
            let _ = tokio::fs::remove_file(&tmp_path).await;
        }

        result
    }
}
