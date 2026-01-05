//! Domain-level errors (no external dependencies)

use std::path::PathBuf;
use thiserror::Error;

/// Domain errors represent business logic violations.
/// These are independent of infrastructure concerns.
#[derive(Error, Debug)]
pub enum DomainError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("invalid parent path: {0}")]
    InvalidParent(PathBuf),

    #[error("cycle detected in hierarchy: {0}")]
    CycleDetected(PathBuf),

    #[error("file already swapped in on host: {hostname}")]
    SwappedOnOtherHost { path: PathBuf, hostname: String },

    #[error("vault already initialized: {0}")]
    VaultAlreadyInitialized(PathBuf),

    #[error("vault not found for project: {0}")]
    VaultNotFound(PathBuf),

    #[error("file already guarded: {0}")]
    FileAlreadyGuarded(PathBuf),

    #[error("invalid env file format: {message}")]
    InvalidEnvFile { path: PathBuf, message: String },
}
