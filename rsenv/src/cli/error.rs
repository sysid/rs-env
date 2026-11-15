//! CLI-level errors (wraps infrastructure errors)

use thiserror::Error;

use crate::application::worktree::Borrower;
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

    /// A vault-changing command run in a linked git worktree. The vault belongs to the
    /// main checkout; the worktree only borrows main's files.
    #[error(
        "{} is a linked git worktree of {}.\n  The vault belongs to the main checkout; run this there:\n    {hint}",
        dir.display(),
        main_dir.display()
    )]
    LinkedWorktree {
        dir: std::path::PathBuf,
        main_dir: std::path::PathBuf,
        /// The command line to run instead, e.g. `rsenv -C <main> swap out`.
        hint: String,
    },

    /// Swapping out entries that linked worktrees borrow through symlinks: their links
    /// would point at the committed original, or at nothing.
    #[error("{}", describe_borrowers(.0))]
    Borrowed(Vec<Borrower>),
}

/// Result type for CLI operations.
pub type CliResult<T> = Result<T, CliError>;

impl CliError {
    /// Get the appropriate exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::InvalidArgs(_) | CliError::Usage(_) => crate::exitcode::USAGE,
            CliError::LinkedWorktree { .. } | CliError::Borrowed(_) => crate::exitcode::NOPERM,
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
                InfraError::Application(ApplicationError::WorktreeRefused { .. }) => {
                    crate::exitcode::NOPERM
                }
                InfraError::Application(_) => crate::exitcode::SOFTWARE,
            },
        }
    }
}

fn describe_borrowers(borrowers: &[Borrower]) -> String {
    let mut msg = String::from(
        "linked worktrees borrow these entries; swapping them out leaves their links \
         pointing at the committed original or at nothing:",
    );
    for b in borrowers {
        let paths: Vec<String> = b.paths.iter().map(|p| p.display().to_string()).collect();
        msg.push_str(&format!("\n    {}: {}", b.dir.display(), paths.join(", ")));
    }
    msg.push_str("\n  Detach them first (rsenv worktree detach <dir>), or pass --allow-borrowed.");
    msg
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
    fn given_linked_worktree_refusal_when_displayed_then_names_both_dirs_and_the_hint() {
        let err = CliError::LinkedWorktree {
            dir: PathBuf::from("/r/proj__worktrees/feat"),
            main_dir: PathBuf::from("/r/proj"),
            hint: "rsenv -C /r/proj swap out".into(),
        };

        assert_eq!(
            err.to_string(),
            "/r/proj__worktrees/feat is a linked git worktree of /r/proj.\n  \
             The vault belongs to the main checkout; run this there:\n    \
             rsenv -C /r/proj swap out"
        );
        assert_eq!(err.exit_code(), crate::exitcode::NOPERM);
    }

    #[test]
    fn given_delete_refused_only_for_safety_when_exit_code_then_noperm() {
        let err = refused(vec![DeleteProblem::NotGuardLink {
            path: PathBuf::from("a"),
        }]);
        assert_eq!(err.exit_code(), crate::exitcode::NOPERM);
    }
}
