use thiserror::Error;

/// Database error returned by flanix-core
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

/// The single error type returned by the `flanix-core` library.
///
/// The binary and tests may convert this into `anyhow::Error` for ergonomic
/// top-level handling.
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("database error: {0}")]
    DatabaseError(#[from] DbError),

    #[error("s3 error: {0}")]
    S3(Box<aws_sdk_s3::Error>),

    #[error("s3 stream error: {0}")]
    S3Stream(Box<aws_sdk_s3::primitives::ByteStreamError>),

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

// Project that boxes S3-specific error types to keep the `Err` variant small
// (aws_sdk_s3::Error and ByteStreamError are large, and a large Err type
// penalises every `?` in the crate).
//
// The AWS SDK generates `From<SdkError<OperationError, R>> for Error` for each
// operation, but `?` on `.send().await` yields the raw `SdkError<E, R>`. This
// bridges that gap so all S3 operation failures convert into `SyncError::S3`.
impl From<aws_sdk_s3::Error> for SyncError {
    fn from(err: aws_sdk_s3::Error) -> Self {
        SyncError::S3(Box::new(err))
    }
}

impl From<aws_sdk_s3::primitives::ByteStreamError> for SyncError {
    fn from(err: aws_sdk_s3::primitives::ByteStreamError) -> Self {
        SyncError::S3Stream(Box::new(err))
    }
}

impl<E, R> From<aws_sdk_s3::error::SdkError<E, R>> for SyncError
where
    aws_sdk_s3::Error: From<aws_sdk_s3::error::SdkError<E, R>>,
{
    fn from(err: aws_sdk_s3::error::SdkError<E, R>) -> Self {
        SyncError::S3(Box::new(err.into()))
    }
}
