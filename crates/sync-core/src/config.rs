use anyhow::Result;
use std::env;

pub struct Config {
    bucket_name: String,
    aws_endpoint: String,
    aws_default_region: String,
    aws_access_key_id: String,
    aws_secret_access_key: String,
    database_url: String,
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

// TODO: what is a models folder?
