use directories::ProjectDirs;
use flanix_errors::SyncError;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use uuid::Uuid;

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
    pub device_id: Option<String>,
    /// Destructively remove every S3 object not tracked in the database after
    /// each push. Opt-in: with a shared/reused bucket, a database reset, or a
    /// second install pointed at the same bucket, this would empty the backup.
    pub sweep_orphans: bool,
    /// How long `.flanix-trash` entries and deletion tombstones are kept. A
    /// trash entry is only ever pruned once every registered device has pulled
    /// at/after it AND it is older than this many days.
    pub trash_retention_days: u64,
    /// Bypass the "mass delete" safety guard in `push`. The guard aborts a
    /// push whose delete set is a large fraction of both the files this device
    /// previously had and the namespace's cloud-tracked files, which almost
    /// always means the sync folder moved or was misconfigured. Set this only
    /// if you genuinely deleted most of a namespace and want it to proceed.
    pub allow_mass_deletes: bool,
}

impl Default for Config {
    /// These values are dev only
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
            device_id: None,
            sweep_orphans: false,
            trash_retention_days: 30,
            allow_mass_deletes: false,
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

        let config_path = Self::get_config_file()?;
        Self::write_config(&config_path, self)?;

        Ok(())
    }

    pub fn get_config_file() -> Result<PathBuf, SyncError> {
        let config_folder = match ProjectDirs::from("dev", "hogenson", "flanix") {
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
            Self::write_config(&config_file, &Config::default())?;
        }

        Ok(config_file)
    }

    pub fn write_config(path: &PathBuf, config: &Config) -> Result<(), SyncError> {
        let toml_string = toml::to_string_pretty(config)?;
        let tmp_path = path.with_extension("toml.tmp");

        fs::write(&tmp_path, &toml_string)?;
        fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// Generates and persists a stable device id if the config does not have
    /// one yet. This id scopes the per-device manifest so one machine can
    /// never delete another machine's files.
    pub fn ensure_device_id(&mut self) -> Result<(), SyncError> {
        if self.device_id.is_none() {
            self.device_id = Some(Uuid::new_v4().to_string());
            let config_path = Self::get_config_file()?;
            Self::write_config(&config_path, self)?;
        }
        Ok(())
    }
}
