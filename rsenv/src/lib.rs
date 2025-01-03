// #![allow(unused_imports)]

use std::{env, fs};
use std::collections::BTreeMap;
use std::fs::{File, symlink_metadata};
use std::io::{BufRead, BufReader};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use pathdiff::diff_utf8_paths;
use regex::Regex;
use tracing::{debug, instrument};
use walkdir::WalkDir;

pub mod envrc;
pub mod edit;
pub mod tree;
pub mod tree_stack;
pub mod tree_traits;
pub mod dag;
mod parser;
pub mod cli;
pub mod util;
// mod tree_queue;

#[instrument(level = "trace")]
pub fn get_files(file_path: &str) -> Result<Vec<Utf8PathBuf>> {
    let (_, files, _) = build_env(file_path)?;
    Ok(files)
}

#[instrument(level = "trace")]
pub fn print_files(file_path: &str) -> Result<()> {
    let (_, files, _) = build_env(file_path)?;
    for f in files {
        println!("{}", f);
    }
    Ok(())
}


#[instrument(level = "trace")]
pub fn build_env_vars(file_path: &str) -> Result<String> {
    let mut env_vars = String::new();
    if !Utf8Path::new(file_path).exists() {
        return Err(anyhow::anyhow!("{}: File does not exist: {}", line!(), file_path));
    }
    let (variables, _, _) = build_env(file_path)?;
    for (k, v) in variables {
        env_vars.push_str(&format!("export {}={}\n", k, v));
    }
    Ok(env_vars)
}

#[instrument(level = "trace")]
pub fn is_dag(dir_path: &str) -> Result<bool> {
    let re = Regex::new(r"# rsenv: (.+)")?;

    // Walk through each file in the directory
    for entry in WalkDir::new(dir_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let file = File::open(entry.path())?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if let Some(caps) = re.captures(&line) {
                    // Split on spaces to count the number of parents
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
pub fn build_env(file_path: &str) -> Result<(BTreeMap<String, String>, Vec<Utf8PathBuf>, bool)> {
    warn_if_symlink(file_path)?;
    let file_path = Utf8Path::new(file_path)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), file_path))?;
    if !file_path.exists() {
        return Err(anyhow::anyhow!("{}: File does not exist: {}", line!(), file_path));
    }
    debug!("Current file_path: {:?}", file_path);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut files_read: Vec<Utf8PathBuf> = Vec::new();
    let mut is_dag = false;

    let mut to_read_files: Vec<Utf8PathBuf> = vec![Utf8PathBuf::from(file_path.to_string())];

    while !to_read_files.is_empty() {
        debug!("to_read_files: {:?}", to_read_files);
        let current_file = to_read_files.pop().unwrap();
        if !current_file.exists() {
            return Err(anyhow::anyhow!("{}: File does not exist: {}", line!(), current_file));
        }
        if files_read.contains(&current_file) {
            continue;
        }

        files_read.push(current_file.clone());

        let (vars, parents) = extract_env(current_file.as_ref())?;
        is_dag = is_dag || parents.len() > 1;

        debug!("vars: {:?}, parents: {:?}, is_dag: {:?}", vars, parents, is_dag);

        for (k, v) in vars {
            variables.entry(k).or_insert(v);  // first entry wins
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
pub fn extract_env(file_path: &str) -> Result<(BTreeMap<String, String>, Vec<Utf8PathBuf>)> {
    // Check if the file is a symbolic link before canonicalizing
    warn_if_symlink(file_path)?;

    let file_path = Utf8Path::new(file_path)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), file_path))?;
    debug!("Current file_path: {:?}", file_path);

    // Save the original current directory, to restore it later
    let original_dir = env::current_dir()?;
    // Change the current directory in order to construct correct parent path
    env::set_current_dir(file_path.parent().unwrap())?;
    debug!("Current directory: {:?}", env::current_dir()?);


    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut parent_paths: Vec<Utf8PathBuf> = Vec::new();


    for line in reader.lines() {
        let line = line?;
        // Check for the rsenv comment
        if line.starts_with("# rsenv:") {
            let parents: Vec<&str> = line.trim_start_matches("# rsenv:").split_whitespace().collect();
            for parent in parents {
                let parent_path = Utf8PathBuf::from(parent.to_string())
                    .canonicalize_utf8()
                    .context(format!("{}: Invalid path: {}", line!(), parent))?;
                parent_paths.push(parent_path);
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
    env::set_current_dir(original_dir)?;
    Ok((variables, parent_paths))
}

#[instrument(level = "trace")]
fn warn_if_symlink(file_path: &str) -> Result<()> {
    let metadata = symlink_metadata(file_path)?;
    if metadata.file_type().is_symlink() {
        eprintln!("Warning: The file {} is a symbolic link.", file_path);
    }
    Ok(())
}

/// links two env files together
#[instrument(level = "debug")]
pub fn link(parent: &str, child: &str) -> Result<()> {
    let parent = Utf8Path::new(parent)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), parent))?;
    let child = Utf8Path::new(child)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), child))?;
    debug!("parent: {:?} <- child: {:?}", parent, child);

    let mut child_contents = fs::read_to_string(&child)?;
    let mut lines: Vec<_> = child_contents.lines().map(|s| s.to_string()).collect();

    // Calculate the relative path from child to parent
    let relative_path = diff_utf8_paths(parent, child.parent().unwrap()).unwrap();

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
            lines.insert(0, format!("# rsenv: {}", relative_path));
        }
        1 => {
            // One "# rsenv:" line found, so we replace it
            if let Some(index) = rsenv_index {
                lines[index] = format!("# rsenv: {}", relative_path);
            }
        }
        _ => {
            // More than one "# rsenv:" line found, we throw an error
            return Err(anyhow::anyhow!("Multiple '# rsenv:' lines found in {}", child));
        }
    }
    // Write the modified content back to the child file
    child_contents = lines.join("\n");
    fs::write(&child, child_contents)?;

    Ok(())
}


#[instrument(level = "debug")]
pub fn unlink(child: &str) -> Result<()> {
    let child = Utf8Path::new(child)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), child))?;
    debug!("child: {:?}", child);

    let mut child_contents = fs::read_to_string(&child)?;
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
    // Based on the count, perform the necessary operations
    match rsenv_lines {
        0 => {}
        1 => {
            // One "# rsenv:" line found, so we replace it
            if let Some(index) = rsenv_index {
                lines[index] = "# rsenv:".to_string();
            }
        }
        _ => {
            return Err(anyhow::anyhow!("Multiple '# rsenv:' lines found in {}", child));
        }
    }
    // Write the modified content back to the child file
    child_contents = lines.join("\n");
    fs::write(&child, child_contents)?;

    Ok(())
}


/// links a list of env files together and build the hierarchical environment variables tree
#[instrument(level = "debug")]
pub fn link_all(nodes: &[String]) {
    debug!("nodes: {:?}", nodes);
    let mut parent = None;
    for node in nodes {
        // todo: error handling
        if parent.is_some() {
            link(parent.unwrap(), node).expect("Failed to link");  // todo: error handling
        } else {
            unlink(node).unwrap();
        }
        parent = Some(node);
    }
}

#[cfg(test)]
mod tests {
    use tracing::debug;
    use crate::util::testing;
    

    #[ctor::ctor]
    fn init() {
        testing::init_test_setup();
    }

    #[test]
    fn test_debug_macro() {
        let test_var = vec![1, 2, 3];
        debug!("Test variable: {:?}", &test_var);
        debug!("Test variable: {:?}, {:?}", &test_var, "string");
    }
}
