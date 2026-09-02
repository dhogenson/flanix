mod config_file;

use config_file::Namespace;
use config_file::load_namespaces;
use std::{env, path::PathBuf};
use sync_errors::SyncError;

pub struct Config {
    pub bucket_name: String,
    pub aws_endpoint: String,
    pub aws_default_region: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub database_url: String,
    pub namespaces: Vec<Namespace>,
}

impl Config {
    pub fn new() -> Result<Self, SyncError> {
        dotenvy::dotenv().ok();

        Ok(Self {
            bucket_name: "test".to_string(),
            aws_endpoint: env::var("AWS_ENDPOINT_URL")?,
            aws_default_region: env::var("AWS_DEFAULT_REGION")?,
            aws_access_key_id: env::var("AWS_ACCESS_KEY_ID")?,
            aws_secret_access_key: env::var("AWS_SECRET_ACCESS_KEY")?,
            database_url: env::var("DATABASE_URL")?,
            namespaces: load_namespaces()?,
        })
    }

    pub fn find_namespace_path(&self, namespace: &str) -> Option<PathBuf> {
        let namespace_path = self.namespaces.iter().find(|n| n.namespace == namespace)?;
        Some(PathBuf::from(&namespace_path.path))
    }

    pub fn contains_namespace(&self, namespace: &str) -> bool {
        self.namespaces.iter().any(|n| n.namespace == namespace)
    }
}
