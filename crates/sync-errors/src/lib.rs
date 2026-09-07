use thiserror::Error;

/// Database error returned by sync-core
#[derive(Error, Debug)]
pub enum DbError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("migrate error: {0}")]
    MigrateError(#[from] sqlx::migrate::MigrateError),

    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("{0}")]
    NotFound(String),
}

/// The single error type returned by the `sync-core` library.
///
/// The binary and tests may convert this into `anyhow::Error` for ergonomic
/// top-level handling.
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("database error: {0}")]
    DatabaseError(#[from] DbError),

    #[error("s3 error: {0}")]
    S3(#[from] aws_sdk_s3::Error),

    #[error("s3 stream error: {0}")]
    S3Stream(#[from] aws_sdk_s3::primitives::ByteStreamError),

    #[error("config error: {0}")]
    Config(#[from] std::env::VarError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("glob error: {0}")]
    Glob(#[from] glob::PatternError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("toml read error: {0}")]
    TomlRead(#[from] toml::de::Error),

    #[error("toml write error: {0}")]
    TomlWrite(#[from] toml::ser::Error),

    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),

    #[error("config directory not found: {0}")]
    ConfigDirNotFound(String),

    #[error("anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

// The AWS SDK generates `From<SdkError<OperationError, R>> for Error` for each
// operation, but `?` on `.send().await` yields the raw `SdkError<E, R>`. This
// bridges that gap so all S3 operation failures convert into `SyncError::S3`.
impl<E, R> From<aws_sdk_s3::error::SdkError<E, R>> for SyncError
where
    aws_sdk_s3::Error: From<aws_sdk_s3::error::SdkError<E, R>>,
{
    fn from(err: aws_sdk_s3::error::SdkError<E, R>) -> Self {
        SyncError::S3(err.into())
    }
}
