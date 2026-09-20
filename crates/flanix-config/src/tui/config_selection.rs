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
}

impl ConfigSelection {
    pub fn from_u8(n: u8) -> Option<ConfigSelection> {
        match n {
            0 => Some(ConfigSelection::DatabaseUrl),
            1 => Some(ConfigSelection::MaxDatabaseConnections),
            2 => Some(ConfigSelection::AwsEndpoint),
            3 => Some(ConfigSelection::AwsAccessKeyID),
            4 => Some(ConfigSelection::AwsSecretAccessKey),
            5 => Some(ConfigSelection::AwsDefaultRegion),
            6 => Some(ConfigSelection::BucketName),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ConfigSelection::DatabaseUrl => "Database URL",
            ConfigSelection::MaxDatabaseConnections => "Max Database Connections",
            ConfigSelection::AwsEndpoint => "AWS Endpoint",
            ConfigSelection::AwsAccessKeyID => "AWS Access Key ID",
            ConfigSelection::AwsSecretAccessKey => "AWS Secret Access Key",
            ConfigSelection::AwsDefaultRegion => "AWS Default Region",
            ConfigSelection::BucketName => "Bucket Name",
        }
    }

    pub fn all() -> [ConfigSelection; 7] {
        [
            ConfigSelection::DatabaseUrl,
            ConfigSelection::MaxDatabaseConnections,
            ConfigSelection::AwsEndpoint,
            ConfigSelection::AwsAccessKeyID,
            ConfigSelection::AwsSecretAccessKey,
            ConfigSelection::AwsDefaultRegion,
            ConfigSelection::BucketName,
        ]
    }

    pub fn value<'a>(&self, config: &'a Config) -> Cow<'a, str> {
        match self {
            ConfigSelection::DatabaseUrl => Cow::Borrowed(&config.database_url),
            ConfigSelection::MaxDatabaseConnections => {
                Cow::Owned(config.max_database_connections.to_string())
            }
            ConfigSelection::AwsEndpoint => Cow::Borrowed(&config.aws_endpoint),
            ConfigSelection::AwsAccessKeyID => Cow::Borrowed(&config.aws_access_key_id),
            ConfigSelection::AwsSecretAccessKey => Cow::Borrowed(&config.aws_secret_access_key),
            ConfigSelection::AwsDefaultRegion => Cow::Borrowed(&config.aws_default_region),
            ConfigSelection::BucketName => Cow::Borrowed(&config.bucket_name),
        }
    }

    pub fn value_mut<'a>(&self, config: &'a mut Config) -> Option<&'a mut String> {
        match self {
            ConfigSelection::DatabaseUrl => Some(&mut config.database_url),
            ConfigSelection::MaxDatabaseConnections => None,
            ConfigSelection::AwsEndpoint => Some(&mut config.aws_endpoint),
            ConfigSelection::AwsAccessKeyID => Some(&mut config.aws_access_key_id),
            ConfigSelection::AwsSecretAccessKey => Some(&mut config.aws_secret_access_key),
            ConfigSelection::AwsDefaultRegion => Some(&mut config.aws_default_region),
            ConfigSelection::BucketName => Some(&mut config.bucket_name),
        }
    }
}
