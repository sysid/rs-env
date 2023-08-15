#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use anyhow::{Context, Result};
use log::{debug, info};
use std::env;
use camino::{Utf8Path, Utf8PathBuf};
use stdext::function_name;

pub mod macros;
pub mod envrc;

// create function which print the hashmap from build_env
pub fn print_files(file_path: &str) -> Result<()> {
    let (_, files) = build_env(file_path)?;
    for f in files {
        println!("{}", f);
    }
    Ok(())
}

pub fn build_env_vars(file_path: &str) -> Result<String> {
    let mut env_vars = String::new();
    let (variables, _) = build_env(file_path)?;
    for (k, v) in variables {
        env_vars.push_str(&format!("export {}={}\n", k, v));
    }
    Ok(env_vars)
}

/// Recursively builds map of environment variables from the specified file and its parents.
///
/// This function reads the specified `file_path` and extracts environment variables from it.
/// It recognizes `export` statements to capture key-value pairs and uses special `# rsenv:`
/// comments to identify parent files for further extraction.
///
/// The extraction prioritizes variables found in the initial file, i.e., if a variable is
/// found with the same key in both the child and parent files, the value from the child
/// will be retained.
///
/// # Arguments
///
/// * `file_path` - A string slice representing the path to the .env file.
///                The function will attempt to canonicalize this path.
///
/// # Returns
///
/// A `Result` containing:
///
/// * A `BTreeMap` with the key as the variable name and the value as its corresponding value.
/// * An error if there's any problem reading the file, or if the path is invalid.
///
/// # Errors
///
/// This function will return an error in the following situations:
///
/// * The provided `file_path` is invalid.
/// * There's an issue reading or processing the env file or any of its parent env files.
pub fn build_env(file_path: &str) -> Result<(BTreeMap<String, String>, Vec<Utf8PathBuf>)> {
    let file_path = Utf8Path::new(file_path)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), file_path))?;
    dlog!("Current file_path: {:?}", file_path);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut parent: Option<Utf8PathBuf>;

    let mut current_file = file_path.to_string();
    let mut files_read: Vec<Utf8PathBuf> = Vec::new();

    loop {
        files_read.push(Utf8PathBuf::from(&current_file));

        let (vars, par) = extract_env(&current_file)?;
        for (k, v) in vars {
            variables.entry(k).or_insert(v);
        }
        parent = par;
        if let Some(p) = parent {
            current_file = p.to_string();
        } else {
            break;
        }
    }

    Ok((variables, files_read))
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
pub fn extract_env(file_path: &str) -> Result<(BTreeMap<String, String>, Option<Utf8PathBuf>)> {
    let file_path = Utf8Path::new(file_path)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), file_path))?;
    dlog!("Current file_path: {:?}", file_path);

    // Save the original current directory, to restore it later
    let original_dir = env::current_dir()?;
    // Change the current directory in order to construct correct parent path
    env::set_current_dir(file_path.parent().unwrap())?;
    dlog!("Current directory: {:?}", env::current_dir()?);


    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut parent_path: Option<Utf8PathBuf> = None;

    for line in reader.lines() {
        let line = line?;
        // Check for the rsenv comment
        if line.starts_with("# rsenv:") {
            let parent = line.trim_start_matches("# rsenv:").trim().to_string();
            if parent.is_empty() {
                return Err(anyhow::anyhow!("Invalid rsenv line comment: {}", line));
            }
            parent_path = Some(Utf8PathBuf::from(parent.clone())
                .canonicalize_utf8()
                .context(format!("{}: Invalid path: {}", line!(), parent))?);
            dlog!("parent_path: {:?}", parent_path);
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
    Ok((variables, parent_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};

    #[ctor::ctor]
    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::max())
            .is_test(true)
            .try_init();
    }

    #[test]
    fn test_dlog_macro() {
        let test_var = vec![1, 2, 3];
        dlog!("Test variable: {:?}", &test_var);
        dlog!("Test variable: {:?}, {:?}", &test_var, "string");
    }
}
