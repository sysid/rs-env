use crate::errors::{TreeError, TreeResult};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub trait PathExt {
    fn is_env_file(&self) -> bool;
    fn to_canonical(&self) -> TreeResult<PathBuf>;
    fn to_string_lossy_cached(&self) -> String;
}

impl PathExt for Path {
    fn is_env_file(&self) -> bool {
        self.extension() == Some(OsStr::new("env"))
    }

    fn to_canonical(&self) -> TreeResult<PathBuf> {
        self.canonicalize().map_err(|e| TreeError::PathResolution {
            path: self.to_path_buf(),
            reason: e.to_string(),
        })
    }

    fn to_string_lossy_cached(&self) -> String {
        self.to_string_lossy().into_owned()
    }
}

pub fn ensure_file_exists(path: &Path) -> TreeResult<()> {
    if !path.exists() {
        Err(TreeError::FileNotFound(path.to_path_buf()))
    } else if !path.is_file() {
        Err(TreeError::InvalidFormat {
            path: path.to_path_buf(),
            reason: "Not a file".to_string(),
        })
    } else {
        Ok(())
    }
}

pub fn get_relative_path(from: &Path, to: &Path) -> TreeResult<PathBuf> {
    pathdiff::diff_paths(to, from).ok_or_else(|| TreeError::PathResolution {
        path: to.to_path_buf(),
        reason: "Could not compute relative path".to_string(),
    })
}

// Helper function for cross-platform path comparison
pub fn normalize_path_separator(s: &str) -> String {
    s.replace('\\', "/")
}

pub fn relativize_tree_str(tree_str: &str, start: &str) -> String {
    tree_str
        .lines()
        .map(|line| {
            // Preserve indentation and tree characters
            let prefix_end = line.find('/').unwrap_or(0);
            let prefix = &line[..prefix_end];
            let path = &line[prefix_end..];

            if path.contains('/') {
                format!("{}{}", prefix, relativize_path(path, start))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
pub fn relativize_path(path: &str, start: &str) -> String {
    // Convert a single full path to a relative path starting at "start"
    match path.find(start) {
        Some(pos) => path[pos..].to_string(),
        None => path.to_string(), // If "start" is not found, return the original path
    }
}

pub fn relativize_paths(leaf_nodes: Vec<String>, start: &str) -> Vec<String> {
    // Convert a Vec of paths to relative paths starting at "start"
    leaf_nodes
        .iter()
        .map(|path| relativize_path(path, start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relativize_path_with_matching_path() {
        let path = "/some/dir/tests/foo/bar";
        let start = "tests/";
        let result = relativize_path(path, start);
        assert_eq!(result, "tests/foo/bar");
    }

    #[test]
    fn test_relativize_path_with_no_matching_start() {
        let path = "/some/dir/foo/bar";
        let start = "tests/";
        let result = relativize_path(path, start);
        assert_eq!(result, "/some/dir/foo/bar");
    }

    #[test]
    fn test_relativize_path_start_at_beginning() {
        let path = "tests/foo/bar";
        let start = "tests/";
        let result = relativize_path(path, start);
        assert_eq!(result, "tests/foo/bar");
    }

    #[test]
    fn test_relativize_paths_with_mixed_cases() {
        let paths = vec![
            "/some/dir/tests/foo/bar".to_string(),
            "/other/tests/baz".to_string(),
            "/not/matching/path".to_string(),
        ];
        let start = "tests/";
        let result = relativize_paths(paths, start);
        assert_eq!(
            result,
            vec![
                "tests/foo/bar".to_string(),
                "tests/baz".to_string(),
                "/not/matching/path".to_string(),
            ]
        );
    }

    #[test]
    fn test_relativize_paths_all_matching() {
        let paths = vec![
            "tests/foo/bar".to_string(),
            "tests/baz".to_string(),
            "tests/another/file".to_string(),
        ];
        let start = "tests/";
        let result = relativize_paths(paths, start);
        assert_eq!(
            result,
            vec![
                "tests/foo/bar".to_string(),
                "tests/baz".to_string(),
                "tests/another/file".to_string(),
            ]
        );
    }
}
