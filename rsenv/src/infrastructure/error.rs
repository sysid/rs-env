//! Infrastructure-level errors (wraps application errors)

use thiserror::Error;

use crate::application::ApplicationError;

/// Infrastructure errors wrap application errors and add I/O-level concerns.
#[derive(Error, Debug)]
pub enum InfraError {
    #[error("{0}")]
    Application(#[from] ApplicationError),

    #[error("I/O error: {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("sops command failed: {message}")]
    Sops {
        message: String,
        exit_code: Option<i32>,
    },

    #[error("editor command failed: {message}")]
    Editor { message: String },
}

impl InfraError {
    /// Create an I/O error with context.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// Result type for infrastructure layer operations.
pub type InfraResult<T> = Result<T, InfraError>;
