#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, fs};
use camino::{Utf8Path, Utf8PathBuf};
use nom::bytes::complete::{tag, take_while};
use nom::error::{dbg_dmp, Error, ParseError};
use nom::{AsBytes, IResult, Parser};
use nom::character::complete::multispace0;
use nom::sequence::delimited;
use pathdiff::diff_utf8_paths;
use stdext::function_name;
use std::rc::Rc;
use std::cell::RefCell;
use regex::Regex;
use walkdir::WalkDir;
use crate::tree::TreeNode;

pub mod macros;
pub mod envrc;
pub mod edit;
pub mod tree;
pub mod tree_stack;
pub mod tree_traits;
pub mod dag;
// mod tree_queue;

pub fn get_files(file_path: &str) -> Result<Vec<Utf8PathBuf>> {
    let (_, files, _) = build_env(file_path)?;
    Ok(files)
}

pub fn print_files(file_path: &str) -> Result<()> {
    let (_, files, _) = build_env(file_path)?;
    for f in files {
        println!("{}", f);
    }
    Ok(())
}


pub fn build_env_vars(file_path: &str) -> Result<String> {
    let mut env_vars = String::new();
    let (variables, _, _) = build_env(file_path)?;
    for (k, v) in variables {
        env_vars.push_str(&format!("export {}={}\n", k, v));
    }
    Ok(env_vars)
}

// pub fn is_dag(file_path: &str) -> bool {
//     let (_, _, is_dag) = build_env(file_path).expect("Failed to build env");
//     is_dag
// }
pub fn is_dag(dir_path: &str) -> Result<bool> {
    let re = Regex::new(r"# rsenv: (.+)").unwrap();

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

// Example usage:
// let multiple_parents = is_dag("path_to_directory").unwrap();


/// Recursively builds map of environment variables from the specified file and its parents.
///
/// This function reads the specified `file_path` and extracts environment variables from it.
/// It recognizes `export` statements to capture key-value pairs and uses special `# rsenv:`
/// comments to identify parent files for further extraction.
///
/// child wins against parent
/// rightmost sibling wins
pub fn build_env(file_path: &str) -> Result<(BTreeMap<String, String>, Vec<Utf8PathBuf>, bool)> {
    let file_path = Utf8Path::new(file_path)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), file_path))?;
    dlog!("Current file_path: {:?}", file_path);

    let mut variables: BTreeMap<String, String> = BTreeMap::new();
    let mut files_read: Vec<Utf8PathBuf> = Vec::new();
    let mut is_dag = false;

    let mut to_read_files: Vec<Utf8PathBuf> = vec![Utf8PathBuf::from(file_path.to_string())];

    while !to_read_files.is_empty() {
        dlog!("to_read_files: {:?}", to_read_files);
        let current_file = to_read_files.pop().unwrap();
        if files_read.contains(&current_file) {
            continue;
        }

        files_read.push(current_file.clone());

        let (vars, parents) = extract_env(&current_file.to_string())?;
        is_dag = if is_dag || parents.len() > 1 { true } else { false };

        dlog!("vars: {:?}, parents: {:?}, is_dag: {:?}", vars, parents, is_dag);

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
pub fn extract_env(file_path: &str) -> Result<(BTreeMap<String, String>, Vec<Utf8PathBuf>)> {
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
    let mut parent_paths: Vec<Utf8PathBuf> = Vec::new();


    for line in reader.lines() {
        let line = line?;
        // Check for the rsenv comment
        if line.starts_with("# rsenv:") {
            let parents: Vec<&str> = line.trim_start_matches("# rsenv:").trim().split_whitespace().collect();
            for parent in parents {
                let parent_path = Utf8PathBuf::from(parent.to_string())
                    .canonicalize_utf8()
                    .context(format!("{}: Invalid path: {}", line!(), parent))?;
                parent_paths.push(parent_path);
            }
            dlog!("parent_paths: {:?}", parent_paths);
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

/// links two env files together
pub fn link(parent: &str, child: &str) -> Result<()> {
    let parent = Utf8Path::new(parent)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), parent))?;
    let child = Utf8Path::new(child)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), child))?;
    dlog!("parent: {:?} <- child: {:?}", parent, child);

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


pub fn unlink(child: &str) -> Result<()> {
    let child = Utf8Path::new(child)
        .canonicalize_utf8()
        .context(format!("{}: Invalid path: {}", line!(), child))?;
    dlog!("child: {:?}", child);

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
                lines[index] = format!("# rsenv:");
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
pub fn link_all(nodes: &[String]) {
    dlog!("nodes: {:?}", nodes);
    let mut parent = None;
    for node in nodes {
        if parent.is_some() {
            link(parent.unwrap(), node).unwrap();
        } else {
            unlink(node).unwrap();
        }
        parent = Some(node);
    }
}

// Parser to skip whitespace
#[allow(dead_code)]
fn space(input: &str) -> IResult<&str, &str> {
    take_while(|c: char| c.is_whitespace())(input)
}

/// A combinator that takes a parser `inner` and produces a parser that also consumes both leading and
/// trailing whitespace, returning the output of `inner`.
#[allow(dead_code)]
fn ws<'a, F, O, E: ParseError<&'a str>>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
{
    delimited(
        multispace0,
        inner,
        multispace0,
    )
}

// Parser to extract the path after `# rsenv:`
#[allow(dead_code)]
fn extract_path(input: &str) -> IResult<&str, &str> {
    dlog!("input: {:?}", input);
    // dbg_dmp(tag::<&str, &[u8], Error<_>>("# rsenv:"),"xxx")(input.as_bytes());

    let (input, _) = multispace0(input)?; // Match optional whitespace or newlines
    let (input, _) = tag("# rsenv:")(input)?;
    dlog!("input: {:?}", input);
    // let (input, _) = space(input)?;
    // dlog!("input: {:?}", input);
    ws(take_while(|c: char| !c.is_whitespace()))(input)
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
    fn test_extract_path() {
        let content = r#"
# rsenv: level1.env

# Level2 overwrite
export VAR_4=var_42
export VAR_5=var_52
"#;

        match extract_path(content) {
            Ok((_, path)) => println!("Extracted path: {}", path),
            Err(e) => println!("Error: {:?}", e),
        }
    }

    #[test]
    fn test_dlog_macro() {
        let test_var = vec![1, 2, 3];
        dlog!("Test variable: {:?}", &test_var);
        dlog!("Test variable: {:?}, {:?}", &test_var, "string");
    }
}
