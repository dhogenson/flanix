use crate::errors::SyncError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

impl Namespace {
    pub fn new(namespace: String, path: String) -> Self {
        Self { namespace, path }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Namespace {
    pub namespace: String,
    pub path: String,
}

// #[derive(Debug, Serialize, Deserialize)]
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
        // let fake_namespace: Vec<Namespace> =
        //     Vec::from([Namespace::new("test".to_string(), "something".to_string())]);

        Ok(Self {
            bucket_name: "test".to_string(),
            aws_endpoint: env::var("AWS_ENDPOINT_URL")?,
            aws_default_region: env::var("AWS_DEFAULT_REGION")?,
            aws_access_key_id: env::var("AWS_ACCESS_KEY_ID")?,
            aws_secret_access_key: env::var("AWS_SECRET_ACCESS_KEY")?,
            database_url: env::var("DATABASE_URL")?,
            // config_path: config_path,
            namespaces: load_namespaces()?,
        })
    }

    pub fn find_namespace_path(&self, namespace: &str) -> Option<PathBuf> {
        let namespace_path = self
            .namespaces
            .iter()
            .find(|n| n.namespace == namespace)?;

        Some(PathBuf::from(&namespace_path.path))
    }

    pub fn contains_namespace(&self, namespace: &str) -> bool {
        self.namespaces.iter().any(|n| n.namespace == namespace)
    }

    pub fn create_namespace(&mut self, namespace: &str, path: String) -> Result<(), SyncError> {
        self.namespaces
            .push(Namespace::new(namespace.to_string(), path));

        let config_file = get_config_file()?;
        let file = File::create(config_file)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(
            writer,
            &ConfigFile {
                namespaces: self.namespaces.clone(),
            },
        )?;

        Ok(())
        // todo!()
    }
}

fn get_config_file() -> Result<PathBuf, SyncError> {
    let mut config_folder: PathBuf = PathBuf::new();
    fs::create_dir_all(&config_folder)?;
    if let Some(project_dirs) = ProjectDirs::from("dev", "hogenson", "sync") {
        config_folder = PathBuf::from(project_dirs.config_dir())
    }

    let config_file = config_folder.join(PathBuf::from("config.json"));

    Ok(config_file)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ConfigFile {
    namespaces: Vec<Namespace>,
}

fn load_namespaces() -> Result<Vec<Namespace>, SyncError> {
    let mut config_folder: PathBuf = PathBuf::new();
    fs::create_dir_all(&config_folder)?;
    if let Some(project_dirs) = ProjectDirs::from("dev", "hogenson", "sync") {
        config_folder = PathBuf::from(project_dirs.config_dir())
    }

    let config_file = config_folder.join(PathBuf::from("config.json"));

    let file = match File::open(&config_file) {
        Ok(file) => file,
        Err(_) => {
            File::create(&config_file)?;
            return Ok(Vec::new());
        }
    };

    let reader = BufReader::new(file);

    let config_data: ConfigFile = serde_json::from_reader(reader)?;

    Ok(config_data.namespaces)
}
