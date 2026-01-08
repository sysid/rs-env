//! Unified dot-file handling for vault storage
//!
//! Dot-files (`.gitignore`, `.envrc`, etc.) need to be "neutralized" when stored
//! in the vault to prevent them from having active effects (e.g., `.gitignore`
//! affecting git's view of the vault directory).
//!
//! This module provides consistent transformation:
//! - `.gitignore` → `dot.gitignore` (neutralized)
//! - `dot.gitignore` → `.gitignore` (restored)

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const DOT_PREFIX: &str = "dot.";

/// Check if a path component is a dot-file or dot-directory.
///
/// Returns true for names starting with `.` (except `.` and `..`).
pub fn is_dotfile(name: &OsStr) -> bool {
    name.to_str()
        .map(|s| s.starts_with('.') && s.len() > 1 && s != "..")
        .unwrap_or(false)
}

/// Transform a dot-file name to neutralized form: `.foo` → `dot.foo`
///
/// - Regular names are unchanged
/// - Already neutralized names (`dot.foo`) are unchanged
/// - `.` and `..` are unchanged
pub fn neutralize_name(name: &str) -> String {
    // Don't neutralize . or ..
    if name == "." || name == ".." {
        return name.to_string();
    }

    // Don't double-neutralize
    if name.starts_with(DOT_PREFIX) {
        return name.to_string();
    }

    // Transform .foo → dot.foo
    if name.starts_with('.') && name.len() > 1 {
        format!("dot{}", name)
    } else {
        name.to_string()
    }
}

/// Transform a neutralized name back to dot-file form: `dot.foo` → `.foo`
///
/// - Regular names are unchanged
/// - Dot-file names (`.foo`) are unchanged
/// - `dot.` alone is unchanged
pub fn restore_name(name: &str) -> String {
    // Must start with "dot." and have something after it
    if name.starts_with(DOT_PREFIX) && name.len() > DOT_PREFIX.len() {
        format!(".{}", &name[DOT_PREFIX.len()..])
    } else {
        name.to_string()
    }
}

/// Transform an entire path, neutralizing each dot-component.
///
/// Example: `.hidden/.secret/file.txt` → `dot.hidden/dot.secret/file.txt`
pub fn neutralize_path(path: &Path) -> PathBuf {
    path.iter()
        .map(|component| {
            let s = component.to_string_lossy();
            neutralize_name(&s)
        })
        .collect()
}

/// Transform an entire path, restoring each neutralized component.
///
/// Example: `dot.hidden/dot.secret/file.txt` → `.hidden/.secret/file.txt`
pub fn restore_path(path: &Path) -> PathBuf {
    path.iter()
        .map(|component| {
            let s = component.to_string_lossy();
            restore_name(&s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutralize_restore_roundtrip() {
        let original = ".gitignore";
        let neutralized = neutralize_name(original);
        let restored = restore_name(&neutralized);
        assert_eq!(restored, original);
    }
}
