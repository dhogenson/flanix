use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sync_errors::SyncError;

use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Namespace {
    pub namespace: String,
    pub path: String,
}

impl Namespace {
    pub fn new(namespace: String, path: String) -> Self {
        Self { namespace, path }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    // config_path: PathBuf,
    pub bucket_name: String,
    pub aws_endpoint: String,
    pub aws_default_region: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub database_url: String,
    pub max_database_connections: u64,
    pub namespaces: Vec<Namespace>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bucket_name: "test".to_string(),
            aws_endpoint: "http://localhost:4566".to_string(),
            aws_default_region: "us-east-1".to_string(),
            aws_access_key_id: "test".to_string(),
            aws_secret_access_key: "test".to_string(),
            database_url: "postgres://user:password@localhost/mydb".to_string(),
            max_database_connections: 5,
            namespaces: Vec::new(),
        }
    }
}

impl Config {
    pub fn new() -> Result<Self, SyncError> {
        let path = Self::get_config_file()?;

        let content = fs::read_to_string(path)?;

        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    pub fn find_namespace_path(&self, namespace: &str) -> Option<PathBuf> {
        let namespace_path = self.namespaces.iter().find(|n| n.namespace == namespace)?;
        Some(PathBuf::from(&namespace_path.path))
    }

    pub fn contains_namespace(&self, namespace: &str) -> bool {
        self.namespaces.iter().any(|n| n.namespace == namespace)
    }

    pub fn create_namespace(&mut self, namespace: &str, path: String) -> Result<(), SyncError> {
        self.namespaces
            .push(Namespace::new(namespace.to_string(), path));

        let toml_string = toml::to_string_pretty(self)?;
        let config_file = Self::get_config_file()?;
        let file = File::create(config_file)?;
        let mut writer = BufWriter::new(file);

        write!(writer, "{}", toml_string)?;

        Ok(())
    }

    fn get_config_file() -> Result<PathBuf, SyncError> {
        let config_folder = match ProjectDirs::from("dev", "hogenson", "sync") {
            Some(project_dirs) => PathBuf::from(project_dirs.config_dir()),
            None => {
                return Err(SyncError::ConfigDirNotFound(
                    "unable to determine config directory for this platform".into(),
                ));
            }
        };

        fs::create_dir_all(&config_folder)?;

        let config_file = config_folder.join(PathBuf::from("config.toml"));

        if !config_file.exists() {
            let toml_string = toml::to_string_pretty(&Config::default())?;
            let file = File::create(&config_file)?;
            let mut writer = BufWriter::new(file);

            write!(writer, "{}", toml_string)?;
            writer.flush()?;
        }

        Ok(config_file)
    }
}
