use anyhow::Result;
use std::env;

pub struct Config {
    pub bucket_name: String,
    pub aws_endpoint: String,
    pub aws_default_region: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub database_url: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        Ok(Self {
            bucket_name: "test".to_string(),
            aws_endpoint: env::var("AWS_ENDPOINT_URL")?,
            aws_default_region: env::var("AWS_DEFAULT_REGION")?,
            aws_access_key_id: env::var("AWS_ACCESS_KEY_ID")?,
            aws_secret_access_key: env::var("AWS_SECRET_ACCESS_KEY")?,
            database_url: env::var("DATABASE_URL")?,
        })
    }
}
