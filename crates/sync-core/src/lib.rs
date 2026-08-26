// TODO:
// 1. Create error.rs - Define a SyncError enum with thiserror (already in deps)
// 2. Move SQL to migrations/ - Use sqlx::migrate!() instead of inline CREATE TABLE
// 3. Split db.rs into db/files.rs and db/groups.rs by entity
// 4. Create models/ - Extract File struct, add Group struct
// 5. Rename files.rs → scanner.rs - Clearer intent (it scans files, doesn't manage them)

mod commands;
mod config;
mod db;
mod s3;
mod scanner;

pub use commands::Commands;
pub use config::Config;
pub use db::Database;
pub use s3::Bucket;
pub use scanner::scan_files;
