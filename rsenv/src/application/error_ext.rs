//! Error conversion helpers for common I/O operations
//!
//! Provides extension traits for cleaner error handling with path context.

use std::io;
use std::path::Path;

use crate::application::{ApplicationError, ApplicationResult};

/// Extension trait for converting `io::Result` to `ApplicationResult` with context.
pub trait IoResultExt<T> {
    /// Add path context to an I/O error.
    ///
    /// # Example
    /// ```ignore
    /// fs.copy_any(&src, &dst)
    ///     .with_path_context("copy file", &src)?;
    /// ```
    fn with_path_context(self, action: &str, path: &Path) -> ApplicationResult<T>;
}

impl<T> IoResultExt<T> for io::Result<T> {
    fn with_path_context(self, action: &str, path: &Path) -> ApplicationResult<T> {
        self.map_err(|e| ApplicationError::OperationFailed {
            context: format!("{}: {}", action, path.display()),
            source: Box::new(e),
        })
    }
}
