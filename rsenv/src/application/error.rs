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

    #[error(
        "refusing to commit while the project is swapped in:\n  {}\n\
         The vault holds frozen sentinels for these paths, so this commit would record \
         stale content. Run `rsenv swap out` first.",
        paths.join("\n  ")
    )]
    ProjectSwappedIn { paths: Vec<String> },

    /// A delete named paths it may not touch. Validation is all-or-nothing, so nothing
    /// was deleted, and every offending path is reported at once.
    #[error(
        "nothing was deleted:\n  {}",
        problems.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n  ")
    )]
    DeleteRefused { problems: Vec<DeleteProblem> },

    #[error("operation failed: {context}")]
    OperationFailed {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Why one path of a delete request was refused.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DeleteProblem {
    /// Nothing in the vault belongs to this path.
    #[error("not managed: {path} (see `{list_cmd}`)")]
    NotManaged {
        path: PathBuf,
        list_cmd: &'static str,
    },

    /// Swapped in on another machine, whose project we cannot restore from here.
    #[error("{path}: swapped in by host '{hostname}', swap it out there first")]
    SwappedInElsewhere { path: PathBuf, hostname: String },

    /// The vault has an entry, but the project path is not the link pointing to it.
    #[error("{path}: not a guarded link (regular file or foreign symlink), refusing to touch it")]
    NotGuardLink { path: PathBuf },
}

/// Result type for application layer operations.
pub type ApplicationResult<T> = Result<T, ApplicationError>;
