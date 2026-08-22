use anyhow::Result;

use crate::Bucket;
use crate::Database;
use uuid::Uuid;

pub struct Commands {
    database: Database,
    bucket: Bucket,
}

impl Commands {
    pub async fn new() -> Result<Self> {
        let mut database = Database::new("postgres://user:password@localhost/mydb").await?;
        let bucket = Bucket::new("test", "us-west-2").await;
        database.init().await?;
        bucket.init().await?;
        Ok(Self {
            database: database,
            bucket: bucket,
        })
    }

    pub async fn push(&self, id: String, path: String) -> Result<()> {
        let uuid = Uuid::new_v4();
        // self.bucket.upload_directory(&path.to_string()).await?;
        self.database.add_file(uuid, &path.to_string()).await?;
        Ok(())
    }

    pub fn pull(&self, id: String, path: String) {}

    pub async fn add(&self, name: String) -> Result<()> {
        if !self.database.group_exists(&name.to_string()).await? {
            let uuid = Uuid::new_v4();
            self.database.add_group(uuid, &name.to_string()).await?;
        }
        Ok(())
    }
}
