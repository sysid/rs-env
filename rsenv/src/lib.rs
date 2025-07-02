use std::collections::BTreeMap;
use std::env;
use std::fs::{symlink_metadata, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::errors::{TreeError, TreeResult};
use crate::util::path::{ensure_file_exists, PathExt};
use regex::Regex;
use tracing::{debug, instrument};
use walkdir::WalkDir;

pub mod arena;
pub mod builder;
pub mod cli;
pub mod edit;
pub mod envrc;
pub mod errors;
pub mod tree_traits;
pub mod util;

/// Expands environment variables in a path string
/// Supports both $VAR and ${VAR} syntax
pub fn expand_env_vars(path: &str) -> String {
    let mut result = path.to_string();

    // Find all occurrences of $VAR or ${VAR}
    let env_var_pattern = Regex::new(r"\$(\w+)|\$\{(\w+)\}").unwrap();

    // Collect all matches first to avoid borrow checker issues with replace_all
    let matches: Vec<_> = env_var_pattern.captures_iter(path).collect();

    for cap in matches {
        // Get the variable name from either $VAR or ${VAR} pattern
        let var_name = cap.get(1).or_else(|| cap.get(2)).unwrap().as_str();
        let var_placeholder = if cap.get(1).is_some() {
            format!("${}", var_name)
        } else {
            format!("${{{}}}", var_name)
        };

        // Replace with environment variable value or empty string if not found
        if let Ok(var_value) = std::env::var(var_name) {
            result = result.replace(&var_placeholder, &var_value);
        }
    }

    result
}

#[instrument(level = "trace")]
pub fn get_files(file_path: &Path) -> TreeResult<Vec<PathBuf>> {
    ensure_file_exists(file_path)?;
    let (_, files, _) = build_env(file_path)?;
    Ok(files)
}

#[instrument(level = "trace")]
pub fn print_files(file_path: &Path) -> TreeResult<()> {
    let files = get_files(file_path)?;
    for f in files {
        println!("{}", f.display());
    }
    Ok(())
}

#[instrument(level = "trace")]
pub fn build_env_vars(file_path: &Path) -> TreeResult<String> {
    ensure_file_exists(file_path)?;

    let mut env_vars = String::new();
    let (variables, _, _) = build_env(file_path)?;

    for (k, v) in variables {
        env_vars.push_str(&format!("export {}={}\n", k, v));
    }

    Ok(env_vars)
}

#[instrument(level = "trace")]
pub fn is_dag(dir_path: &Path) -> TreeResult<bool> {
    let re = Regex::new(r"# rsenv:\s*(.+)").map_err(|e| TreeError::InternalError(e.to_string()))?;

    // Walk through each file in the directory
    for entry in WalkDir::new(dir_path) {
        let entry = entry.map_err(|e| TreeError::PathResolution {
            path: dir_path.to_path_buf(),
            reason: e.to_string(),
        })?;

        if entry.file_type().is_file() {
            let file = File::open(entry.path()).map_err(TreeError::FileReadError)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line.map_err(TreeError::FileReadError)?;
                if let Some(caps) = re.captures(&line) {
                    let parent_references: Vec<&str> = caps[1].split_whitespace().collect();
                    if parent_references.len() > 1 {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Recursively builds map of environment variables from the specified file and its parents.
///
/// This function reads the specified `file_path` and extracts environment variables from it.
/// It recognizes `export` statements to capture key-value pairs and uses special `# rsenv:`
/// comments to identify parent files for further extraction.
///
/// child wins against parent
/// rightmost sibling wins
#[instrument(level = "debug")]
pub fn build_env(file_path: &Path) -> TreeResult<(BTreeMap<String, String>, Vec<PathBuf>, bool)> {
    warn_if_symlink(file_path)?;
    let file_path = file_path.to_canonical()?;
    ensure_file_exists(&file_path)?;
    debug!("Current file_path: {:?}", file_path);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut files_read: Vec<PathBuf> = Vec::new();
    let mut is_dag = false;

    let mut to_read_files: Vec<PathBuf> = vec![file_path];

    while let Some(current_file) = to_read_files.pop() {
        ensure_file_exists(&current_file)?;
        if files_read.contains(&current_file) {
            continue;
        }

        files_read.push(current_file.clone());

        let (vars, parents) = extract_env(&current_file)?;
        is_dag = is_dag || parents.len() > 1;

        debug!(
            "vars: {:?}, parents: {:?}, is_dag: {:?}",
            vars, parents, is_dag
        );

        for (k, v) in vars {
            variables.entry(k).or_insert(v); // first entry wins
        }

        for parent in parents {
            to_read_files.push(parent);
        }
    }

    Ok((variables, files_read, is_dag))
}

/// Extracts environment variables and the parent path from a specified file.
///
/// This function reads the given `file_path` to:
///
/// 1. Identify and extract environment variables specified using the `export` keyword.
/// 2. Identify any parent environment file via the special `# rsenv:` comment.
///    parent's path can be relative to the child's path.
///
/// The current working directory is temporarily changed to the directory of the `file_path`
/// during the extraction process to construct correct parent paths. It is restored
/// afterward.
///
/// # Arguments
///
/// * `file_path` - A string slice representing the path to the .env file. The function
///                will attempt to canonicalize this path.
///
/// # Returns
///
/// A `Result` containing:
///
/// * A tuple with:
///     - A `BTreeMap` with the key as the variable name and the value as its corresponding value.
///     - An `Option` containing a `Utf8PathBuf` pointing to the parent env file, if specified.
/// * An error if there's any problem reading the file, extracting the variables, or if the
///   path is invalid.
///
/// # Errors
///
/// This function will return an error in the following situations:
///
/// * The provided `file_path` is invalid.
/// * There's an issue reading or processing the env file.
/// * The parent path specified in `# rsenv:` is invalid or not specified properly.
#[instrument(level = "debug")]
pub fn extract_env(file_path: &Path) -> TreeResult<(BTreeMap<String, String>, Vec<PathBuf>)> {
    warn_if_symlink(file_path)?;
    let file_path = file_path.to_canonical()?;
    debug!("Current file_path: {:?}", file_path);

    // Save the original current directory, to restore it later
    let original_dir = env::current_dir()
        .map_err(|e| TreeError::InternalError(format!("Failed to get current dir: {}", e)))?;

    // Change the current directory in order to construct correct parent path
    let parent_dir = file_path
        .parent()
        .ok_or_else(|| TreeError::InvalidParent(file_path.clone()))?;
    env::set_current_dir(parent_dir)
        .map_err(|e| TreeError::InternalError(format!("Failed to change dir: {}", e)))?;

    debug!(
        "Current directory: {:?}",
        env::current_dir().unwrap_or_default()
    );

    let file = File::open(&file_path).map_err(TreeError::FileReadError)?;
    let reader = BufReader::new(file);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut parent_paths: Vec<PathBuf> = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(TreeError::FileReadError)?;

        // Check for the rsenv comment
        if line.starts_with("# rsenv:") {
            let parents: Vec<&str> = line
                .trim_start_matches("# rsenv:")
                .split_whitespace()
                .collect();
            for parent in parents {
                if !parent.is_empty() {
                    // Expand environment variables in the path
                    let expanded_path = expand_env_vars(parent);
                    let parent_path = PathBuf::from(expanded_path)
                        .to_canonical()
                        .map_err(|_| TreeError::InvalidParent(PathBuf::from(parent)))?;
                    parent_paths.push(parent_path);
                }
            }
            debug!("parent_paths: {:?}", parent_paths);
        }
        // Check for the export prefix
        else if line.starts_with("export ") {
            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() > 1 {
                let var_name: Vec<&str> = parts[0].split_whitespace().collect();
                if var_name.len() > 1 {
                    variables.insert(var_name[1].to_string(), parts[1].to_string());
                }
            }
        }
    }

    // After executing your code, restore the original current directory
    env::set_current_dir(original_dir)
        .map_err(|e| TreeError::InternalError(format!("Failed to restore dir: {}", e)))?;

    Ok((variables, parent_paths))
}

#[instrument(level = "trace")]
fn warn_if_symlink(file_path: &Path) -> TreeResult<()> {
    let metadata = symlink_metadata(file_path).map_err(TreeError::FileReadError)?;
    if metadata.file_type().is_symlink() {
        eprintln!(
            "Warning: The file {} is a symbolic link.",
            file_path.display()
        );
    }
    Ok(())
}

/// Links a parent file to a child file by adding a special comment to the child file.
/// The comment contains the relative path from the child to the parent.
/// If the child file already has a parent, the function will replace the existing parent.
/// If the child file has multiple parents, the function will return an error.
#[instrument(level = "debug")]
pub fn link(parent: &Path, child: &Path) -> TreeResult<()> {
    let parent = parent.to_canonical()?;
    let child = child.to_canonical()?;
    debug!("parent: {:?} <- child: {:?}", parent, child);

    let mut child_contents = std::fs::read_to_string(&child).map_err(TreeError::FileReadError)?;
    let mut lines: Vec<_> = child_contents.lines().map(|s| s.to_string()).collect();

    // Calculate the relative path from child to parent
    let relative_path =
        pathdiff::diff_paths(&parent, child.parent().unwrap()).ok_or_else(|| {
            TreeError::PathResolution {
                path: parent.clone(),
                reason: "Failed to compute relative path".to_string(),
            }
        })?;

    // Find and count the lines that start with "# rsenv:"
    let mut rsenv_lines = 0;
    let mut rsenv_index = None;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("# rsenv:") {
            rsenv_lines += 1;
            rsenv_index = Some(i);
        }
    }

    // Based on the count, perform the necessary operations
    match rsenv_lines {
        0 => {
            // No "# rsenv:" line found, so we add it
            lines.insert(0, format!("# rsenv: {}", relative_path.display()));
        }
        1 => {
            // One "# rsenv:" line found, so we replace it
            if let Some(index) = rsenv_index {
                lines[index] = format!("# rsenv: {}", relative_path.display());
            }
        }
        _ => {
            // More than one "# rsenv:" line found, we throw an error
            return Err(TreeError::MultipleParents(child));
        }
    }

    // Write the modified content back to the child file
    child_contents = lines.join("\n");
    std::fs::write(&child, child_contents).map_err(TreeError::FileReadError)?;

    Ok(())
}

#[instrument(level = "debug")]
pub fn unlink(child: &Path) -> TreeResult<()> {
    let child = child.to_canonical()?;
    debug!("child: {:?}", child);

    let mut child_contents = std::fs::read_to_string(&child).map_err(TreeError::FileReadError)?;
    let mut lines: Vec<_> = child_contents.lines().map(|s| s.to_string()).collect();

    // Find and count the lines that start with "# rsenv:"
    let mut rsenv_lines = 0;
    let mut rsenv_index = None;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("# rsenv:") {
            rsenv_lines += 1;
            rsenv_index = Some(i);
        }
    }

    match rsenv_lines {
        0 => {}
        1 => {
            // One "# rsenv:" line found, so we replace it
            if let Some(index) = rsenv_index {
                lines[index] = "# rsenv:".to_string();
            }
        }
        _ => {
            return Err(TreeError::MultipleParents(child));
        }
    }
    // Write the modified content back to the child file
    child_contents = lines.join("\n");
    std::fs::write(&child, child_contents).map_err(TreeError::FileReadError)?;

    Ok(())
}

/// links a list of env files together and build the hierarchical environment variables tree
#[instrument(level = "debug")]
pub fn link_all(nodes: &[PathBuf]) {
    debug!("nodes: {:?}", nodes);
    let mut parent = None;
    for node in nodes {
        if let Some(parent_path) = parent {
            link(parent_path, node).expect("Failed to link");
        } else {
            unlink(node).unwrap();
        }
        parent = Some(node);
    }
}
