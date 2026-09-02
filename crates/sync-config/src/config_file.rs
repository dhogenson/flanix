use crate::Config;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sync_errors::SyncError;

use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ConfigFile {
    namespaces: Vec<Namespace>,
}

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

impl Config {
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

pub fn load_namespaces() -> Result<Vec<Namespace>, SyncError> {
    let mut config_folder: PathBuf = PathBuf::new();

    if let Some(project_dirs) = ProjectDirs::from("dev", "hogenson", "sync") {
        config_folder = PathBuf::from(project_dirs.config_dir())
    }

    fs::create_dir_all(&config_folder)?;

    let config_file = config_folder.join(PathBuf::from("config.json"));

    let file = match File::open(&config_file) {
        Ok(file) => file,
        Err(_) => {
            File::create(&config_file)?;
            return Ok(Vec::new());
        }
    };

    let reader = BufReader::new(file);

    let config_data: ConfigFile = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    Ok(config_data.namespaces)
}
