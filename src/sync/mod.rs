//! Opt-in progress synchronization. The portable format is independent of Git and SQLite.
pub mod format;
pub mod merge;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Store(#[from] crate::store::StoreError),
    #[error("Sync schema version {0} is newer than supported; update woodpeck before syncing.")]
    NewerSchema(u32),
    #[error("Invalid sync data: {0}")]
    Invalid(String),
    #[error("{0}")]
    Git(String),
    #[error("Sync network command failed or timed out. Check connectivity and Git authentication.")]
    Offline,
}
pub type Result<T> = std::result::Result<T, SyncError>;
