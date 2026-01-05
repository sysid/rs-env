//! CLI-level errors (wraps infrastructure errors)

use thiserror::Error;

use crate::infrastructure::InfraError;

/// CLI errors are the top-level error type.
/// These are what get displayed to the user.
#[derive(Error, Debug)]
pub enum CliError {
    #[error("{0}")]
    Infra(#[from] InfraError),

    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("{0}")]
    Usage(String),
}

/// Result type for CLI operations.
pub type CliResult<T> = Result<T, CliError>;

impl CliError {
    /// Get the appropriate exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::InvalidArgs(_) | CliError::Usage(_) => crate::exitcode::USAGE,
            CliError::Infra(e) => match e {
                InfraError::Io { .. } => crate::exitcode::IOERR,
                InfraError::Sops { .. } => crate::exitcode::SOFTWARE,
                InfraError::Editor { .. } => crate::exitcode::SOFTWARE,
                InfraError::Application(_) => crate::exitcode::SOFTWARE,
            },
        }
    }
}
