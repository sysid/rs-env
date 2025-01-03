use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TreeError {
    #[error("Invalid parent path: {0}")]
    InvalidParent(PathBuf),

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Failed to read file: {0}")]
    FileReadError(#[from] std::io::Error),

    #[error("Invalid environment file format in {path}: {reason}")]
    InvalidFormat {
        path: PathBuf,
        reason: String,
    },

    #[error("Cycle detected in environment hierarchy starting at: {0}")]
    CycleDetected(PathBuf),

    #[error("Path resolution failed: {path}, reason: {reason}")]
    PathResolution {
        path: PathBuf,
        reason: String,
    },

    #[error("Multiple parent declarations found in: {0}")]
    MultipleParents(PathBuf),

    #[error("Internal tree operation failed: {0}")]
    InternalError(String),
}

pub type TreeResult<T> = Result<T, TreeError>;