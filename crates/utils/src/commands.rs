/*
 * This file is for putting everything together, like for putting db and s3 together
 */

use anyhow::Result;
use glob::glob;
use std::env;

use crate::Bucket;
use crate::Database;
use uuid::Uuid;

pub struct Commands {
    database: Database,
    bucket: Bucket,
}

impl Commands {
    pub async fn new() -> Result<Self> {
        let mut database = Database::new(&env::var("DATABASE_URL")?.to_string()).await?;
        let bucket = Bucket::new("test", "us-west-2").await;
        database.init().await?;
        bucket.init().await?;
        Ok(Self {
            database: database,
            bucket: bucket,
        })
    }

    pub async fn push(&self, group_name: String, path: String) -> Result<()> {
        if !self.database.group_exists(&group_name.to_string()).await? {
            return Ok(());
        }

        let group_id = self.database.get_group_id(&group_name).await?;

        for entry in glob(&format!("{}/**/*", path)).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    if path.is_dir() {
                        continue;
                    }
                    let bucket_key = Uuid::new_v4();
                    let file_uuid = Uuid::new_v4();
                    self.bucket
                        .upload_file(bucket_key, &path.to_string_lossy())
                        .await?;
                    self.database
                        .add_file(file_uuid, bucket_key, &path.to_string_lossy(), group_id)
                        .await?;
                }
                Err(error) => eprintln!("Error: {}", error),
            }
        }
        Ok(())
    }

    pub fn pull(&self, _id: String, _path: String) {}

    pub async fn add(&self, name: String) -> Result<()> {
        if !self.database.group_exists(&name.to_string()).await? {
            let uuid = Uuid::new_v4();
            self.database.add_group(uuid, &name.to_string()).await?;
        }
        Ok(())
    }
}
