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
use crate::tree::TreeNode;
use std::collections::VecDeque;
use std::path::PathBuf;

impl TreeNode {
    /// Calculates the depth of the tree using a breadth-first traversal.
    /// Each element in the queue is a pair (node, depth).
    pub fn depth3(&self) -> usize {
        let mut max_depth = 0;
        let mut queue = VecDeque::new();
        queue.push_back((self, 1)); // (node, depth)

        while let Some((node, depth)) = queue.pop_front() {
            if depth > max_depth {
                max_depth = depth;
            }
            for child in &node.children {
                queue.push_back((child, depth + 1));
            }
        }

        max_depth
    }

    /// Returns a vector containing the file paths of all leaf nodes in the tree,
    /// using a breadth-first traversal.
    pub fn leaf_nodes3(&self) -> Vec<&String> {
        let mut leaves = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            if node.children.is_empty() {
                leaves.push(&node.file_path);
            } else {
                for child in &node.children {
                    queue.push_back(child);
                }
            }
        }

        leaves
    }

    /// Prints the path from the root to each leaf node,
    /// using a breadth-first traversal.
    pub fn print_leaf_paths3(&self) {
        let mut node_queue = VecDeque::new();
        let mut path_queue: VecDeque<Vec<&String>> = VecDeque::new();
        node_queue.push_back(self);
        path_queue.push_back(Vec::new());

        while let Some(node) = node_queue.pop_front() {
            let path = path_queue.pop_front().unwrap();
            if node.children.is_empty() {
                let path_strs: Vec<&str> = path.iter()
                    .map(|s| s.as_str().strip_prefix(&node.base_path).unwrap().strip_prefix("/").unwrap())
                    .collect();
                println!("{}", path_strs.join(" <- "));
            } else {
                for child in &node.children {
                    let mut new_path = path.clone();
                    new_path.push(&node.file_path);
                    node_queue.push_back(child);
                    path_queue.push_back(new_path);
                }
            }
        }
    }
}
