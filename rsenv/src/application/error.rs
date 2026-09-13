//! Application-level errors (wraps domain errors)

use std::path::PathBuf;
use thiserror::Error;

use crate::domain::DomainError;

/// Application errors wrap domain errors and add application-level context.
#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("{0}")]
    Domain(#[from] DomainError),

    #[error("vault not initialized for project: {0}")]
    VaultNotInitialized(PathBuf),

    #[error("file already guarded: {0}")]
    AlreadyGuarded(PathBuf),

    #[error("config error: {message}")]
    Config { message: String },

    #[error("git {command} failed: {stderr}")]
    GitFailed { command: String, stderr: String },

    #[error(
        "refusing to commit unencrypted vault secrets:\n  {}\n\
         Encrypt them (`rsenv sops encrypt`) or add them to the vault repo's .gitignore.",
        paths.join("\n  ")
    )]
    UnencryptedVaultSecrets { paths: Vec<String> },

    #[error("operation failed: {context}")]
    OperationFailed {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Result type for application layer operations.
pub type ApplicationResult<T> = Result<T, ApplicationError>;
