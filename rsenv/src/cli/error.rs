//! CLI-level errors (wraps infrastructure errors)

use thiserror::Error;

use crate::application::{ApplicationError, DeleteProblem};
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
                // A typo'd path must be distinguishable from "refused for safety".
                InfraError::Application(ApplicationError::DeleteRefused { problems }) => {
                    if problems
                        .iter()
                        .any(|p| matches!(p, DeleteProblem::NotManaged { .. }))
                    {
                        crate::exitcode::NOINPUT
                    } else {
                        crate::exitcode::NOPERM
                    }
                }
                InfraError::Application(_) => crate::exitcode::SOFTWARE,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn refused(problems: Vec<DeleteProblem>) -> CliError {
        CliError::Infra(InfraError::Application(ApplicationError::DeleteRefused {
            problems,
        }))
    }

    #[test]
    fn given_delete_of_unmanaged_path_when_exit_code_then_noinput() {
        let err = refused(vec![
            DeleteProblem::SwappedInElsewhere {
                path: PathBuf::from("a"),
                hostname: "h".into(),
            },
            DeleteProblem::NotManaged {
                path: PathBuf::from("typo"),
                list_cmd: "rsenv swap status",
            },
        ]);
        assert_eq!(err.exit_code(), crate::exitcode::NOINPUT);
    }

    #[test]
    fn given_delete_refused_only_for_safety_when_exit_code_then_noperm() {
        let err = refused(vec![DeleteProblem::NotGuardLink {
            path: PathBuf::from("a"),
        }]);
        assert_eq!(err.exit_code(), crate::exitcode::NOPERM);
    }
}
