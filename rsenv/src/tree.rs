#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, fs};
use camino::{Utf8Path, Utf8PathBuf};
use pathdiff::diff_utf8_paths;
use stdext::function_name;

use walkdir::WalkDir;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use crate::dlog;

#[derive(Debug)]
pub struct TreeNode {
    pub file_path: String,
    children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn depth(&self) -> usize {
        // self.children.iter().map(|child| child.depth()) computes the depths of all children recursively.
        // max() finds the maximum depth among the children.
        1 + self.children.iter().map(|child| child.depth()).max().unwrap_or(0)
    }
    pub fn leaf_nodes(&self) -> Vec<&String> {
        if self.children.is_empty() {
            vec![&self.file_path]
        } else {
            // We use a loop to iterate over each child node, calling leaf_nodes() recursively,
            // and we extend the leaves vector with the results.
            let mut leaves = Vec::new();
            for child in &self.children {
                leaves.extend(child.leaf_nodes());
            }
            leaves
        }
    }

    /// recursive function that takes two arguments:
    /// the current node (&self) and a mutable reference to a vector (path) that accumulates the paths from the root to the current node.
    pub fn print_leaf_paths<'a>(&'a self, path: &mut Vec<&'a String>) {
        dlog!("Enter function: {:?}", path);
        if self.children.is_empty() {
            // If it is a leaf node, it prints the accumulated path joined with " <- "
            let path_strs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
            println!("{}", path_strs.join(" <- "));
        } else {
            for child in &self.children {
                // For each child, it pushes the file_path of the child to the path vector,
                // calls print_leaf_paths recursively for the child (which will further populate path),
                // and then pops the last element off of path to backtrack as it moves back up the tree.
                path.push(&child.file_path);
                dlog!("path after push: {:?}", path);
                child.print_leaf_paths(path);
                path.pop();  // backtracking
                dlog!("path after pop: {:?}", path);
            }
        }
    }
}


pub fn build_trees(directory_path: &Utf8Path) -> Result<Vec<TreeNode>> {
    let mut relationships: HashMap<String, Vec<String>> = HashMap::new();
    let re = Regex::new(r"# rsenv: (.+)").unwrap();

    for entry in WalkDir::new(directory_path) {
        let entry = entry.unwrap();
        let abs_path = entry.path().canonicalize().unwrap();
        if entry.file_type().is_file() {
            let file = File::open(&abs_path).unwrap();
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line.unwrap();
                if let Some(caps) = re.captures(&line) {
                    // Save the original current directory, to restore it later
                    let original_dir = env::current_dir()?;
                    // Change the current directory
                    env::set_current_dir(abs_path.parent().unwrap())?;
                    let parent_path = Path::new(caps.get(1).unwrap().as_str()).canonicalize().unwrap();
                    relationships
                        .entry(parent_path.to_string_lossy().into_owned())
                        .or_insert_with(Vec::new)
                        .push(abs_path.to_string_lossy().into_owned());
                    env::set_current_dir(original_dir)?;
                }
            }
        }
    }

    let root_files: Vec<String> = relationships
        .keys()
        .filter(|&key| !relationships.values().any(|v| v.contains(key)))
        .cloned()
        .collect();

    let mut trees = Vec::new();
    for root in root_files {
        trees.push(build_tree(&root, &relationships));
    }

    Ok(trees)
}

fn build_tree(file_name: &str, relationships: &HashMap<String, Vec<String>>) -> TreeNode {
    let children = relationships
        .get(file_name)
        .map(|children| {
            children
                .iter()
                .map(|child| build_tree(child, relationships))
                .collect()
        })
        .unwrap_or_else(Vec::new);

    TreeNode {
        file_path: file_name.to_string(),
        children,
    }
}
