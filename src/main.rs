mod db;
mod s3;

use s3::Bucket;
use db::Database;
use anyhow::Result;


#[tokio::main]
async fn main() -> Result<()> {
    let mut database = Database::new("postgres://user:password@localhost/mydb").await?;
    database.init().await?;

    let bucket = Bucket::new("test", "us-west-2").await;
    bucket.init().await?;

    // bucket.upload_file("./test.txt").await?;

    // bucket.upload_directory("./test").await?;

    let uuid = uuid::Uuid::new_v4();

    database.add_file(uuid, "hell").await?;
    Ok(())
}
