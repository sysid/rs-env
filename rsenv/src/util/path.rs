use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use crate::errors::{TreeError, TreeResult};

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
        self.canonicalize()
            .map_err(|e| TreeError::PathResolution {
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
            reason: "Not a file".to_string()
        })
    } else {
        Ok(())
    }
}

pub fn get_relative_path(from: &Path, to: &Path) -> TreeResult<PathBuf> {
    pathdiff::diff_paths(to, from)
        .ok_or_else(|| TreeError::PathResolution {
            path: to.to_path_buf(),
            reason: "Could not compute relative path".to_string()
        })
}

// Helper function for cross-platform path comparison
pub fn normalize_path_separator(s: &str) -> String {
    s.replace('\\', "/")
}