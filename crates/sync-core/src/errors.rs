use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("migrate error: {0}")]
    MigrateError(#[from] sqlx::migrate::MigrateError),

    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("not found: {0}")]
    NotFound(String),
}
