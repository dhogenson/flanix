use crate::Config;
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub enum ConfigSelection {
    DatabaseUrl,
    MaxDatabaseConnections,
    AwsEndpoint,
    AwsAccessKeyID,
    AwsSecretAccessKey,
    AwsDefaultRegion,
    BucketName,
    DeviceId,
}

impl ConfigSelection {
    pub fn from_u8(n: u8) -> Option<ConfigSelection> {
        match n {
            0 => Some(Self::DatabaseUrl),
            1 => Some(Self::MaxDatabaseConnections),
            2 => Some(Self::AwsEndpoint),
            3 => Some(Self::AwsAccessKeyID),
            4 => Some(Self::AwsSecretAccessKey),
            5 => Some(Self::AwsDefaultRegion),
            6 => Some(Self::BucketName),
            7 => Some(Self::DeviceId),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::DatabaseUrl => "Database URL",
            Self::MaxDatabaseConnections => "Max Database Connections",
            Self::AwsEndpoint => "AWS Endpoint",
            Self::AwsAccessKeyID => "AWS Access Key ID",
            Self::AwsSecretAccessKey => "AWS Secret Access Key",
            Self::AwsDefaultRegion => "AWS Default Region",
            Self::BucketName => "Bucket Name",
            Self::DeviceId => "Device ID",
        }
    }

    pub fn all() -> [ConfigSelection; 8] {
        [
            Self::DatabaseUrl,
            Self::MaxDatabaseConnections,
            Self::AwsEndpoint,
            Self::AwsAccessKeyID,
            Self::AwsSecretAccessKey,
            Self::AwsDefaultRegion,
            Self::BucketName,
            Self::DeviceId,
        ]
    }

    pub fn value<'a>(&self, config: &'a Config) -> Cow<'a, str> {
        match self {
            Self::DatabaseUrl => Cow::Borrowed(&config.database_url),
            Self::MaxDatabaseConnections => Cow::Owned(config.max_database_connections.to_string()),
            Self::AwsEndpoint => Cow::Borrowed(&config.aws_endpoint),
            Self::AwsAccessKeyID => Cow::Borrowed(&config.aws_access_key_id),
            Self::AwsSecretAccessKey => Cow::Borrowed(&config.aws_secret_access_key),
            Self::AwsDefaultRegion => Cow::Borrowed(&config.aws_default_region),
            Self::BucketName => Cow::Borrowed(&config.bucket_name),
            Self::DeviceId => config
                .device_id
                .as_deref()
                .map_or(Cow::Owned(String::new()), Cow::Borrowed),
        }
    }

    pub fn value_mut<'a>(&self, config: &'a mut Config) -> Option<&'a mut String> {
        match self {
            Self::DatabaseUrl => Some(&mut config.database_url),
            Self::MaxDatabaseConnections => None,
            Self::AwsEndpoint => Some(&mut config.aws_endpoint),
            Self::AwsAccessKeyID => Some(&mut config.aws_access_key_id),
            Self::AwsSecretAccessKey => Some(&mut config.aws_secret_access_key),
            Self::AwsDefaultRegion => Some(&mut config.aws_default_region),
            Self::BucketName => Some(&mut config.bucket_name),
            Self::DeviceId => config.device_id.as_mut(),
        }
    }
}
