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
    pub base_path: String,
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
            let path_strs: Vec<&str> = path.iter()
                .map(|s| s.as_str().strip_prefix(&self.base_path).unwrap().strip_prefix("/").unwrap())
                .collect();
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
    let directory_path = directory_path.canonicalize_utf8().context("Failed to canonicalize the path")?;

    for entry in WalkDir::new(directory_path.as_path()) {
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
        trees.push(build_tree(&root, &relationships, directory_path.as_path()));
    }

    Ok(trees)
}

// Recursively builds a tree structure starting from the given file_name
fn build_tree(file_name: &str, relationships: &HashMap<String, Vec<String>>, directory_path: &Utf8Path) -> TreeNode {
    // Attempt to fetch the children of this file from the relationships HashMap.
    // If this file has entries in the HashMap, it means it has child files.
    let children = relationships
        .get(file_name)
        // until it reaches files that have no children, at which point the recursion starts to unwind.
        .map(|children| {
            // For each child in `children`, recursively call `build_tree` to build
            // the tree for that child. This is where the recursive step happens.
            // After mapping, collect the resulting TreeNode instances into a Vec.
            dlog!("children: {:?}", children);
            children
                .iter()
                .map(|child| build_tree(child, relationships, directory_path))
                .collect()
        })
        // If `file_name` is not present in the HashMap, it means this file has no children,
        // so we return an empty Vec.
        .unwrap_or_else(Vec::new);

    // Construct and return a TreeNode instance with the file name as `file_path`
    // and the previously computed `children` vector.
    TreeNode {
        base_path: directory_path.to_string(),
        file_path: file_name.to_string(),
        children,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[ctor::ctor]
    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::max())
            .is_test(true)
            .try_init();
    }


    //      root
    //      /  \
    // child1 child2
    //    |
    // grandchild1
    #[test]
    fn test_build_tree() {
        // Set up a HashMap to represent the relationships between files
        let mut relationships = HashMap::new();
        relationships.insert("root".to_string(), vec!["child1".to_string(), "child2".to_string()]);
        relationships.insert("child1".to_string(), vec!["grandchild1".to_string()]);

        // Build the tree starting from "root"
        let tree = build_tree("root", &relationships, &Utf8PathBuf::from(""));

        // Check the root node
        assert_eq!(tree.file_path, "root");
        assert_eq!(tree.children.len(), 2);

        // Check the first child node
        let child1 = &tree.children[0];
        assert_eq!(child1.file_path, "child1");
        assert_eq!(child1.children.len(), 1);

        // Check the grandchild node
        let grandchild1 = &child1.children[0];
        assert_eq!(grandchild1.file_path, "grandchild1");
        assert_eq!(grandchild1.children.len(), 0);

        // Check the second child node
        let child2 = &tree.children[1];
        assert_eq!(child2.file_path, "child2");
        assert_eq!(child2.children.len(), 0);
    }
}
