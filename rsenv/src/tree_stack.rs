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

impl TreeNode {
    /// We initialize the stack with the root node of the tree and a depth of 1.
    /// For each node we pop from the stack, we push its children onto the stack with a depth
    /// one greater than the current node. We keep track of the maximum depth we've seen as we go:
    pub fn depth2(&self) -> usize {
        let mut max_depth = 0;
        let mut stack = vec![(self, 1)]; // (node, depth)

        // main loop of the algorithm. On each iteration,
        // we pop a node and its depth off the stack, and we then process that node.
        while let Some((node, depth)) = stack.pop() {
            if depth > max_depth {
                max_depth = depth;
            }
            for child in &node.children {
                stack.push((child, depth + 1));
            }
        }

        max_depth
    }
    pub fn leaf_nodes2(&self) -> Vec<&String> {
        let mut leaves = Vec::new();
        let mut stack = vec![self];

        // On each iteration, we pop a node off the stack, and we then process that node.
        while let Some(node) = stack.pop() {
            if node.children.is_empty() {
                leaves.push(&node.file_path);
            } else {
                for child in &node.children {
                    stack.push(child);
                }
            }
        }

        leaves
    }

    pub fn print_leaf_paths2(&self) {
        // single stack, node_stack, where each element is a tuple consisting of a node
        // and the path from the root to that node.
        let mut node_stack = vec![(self, vec![&self.file_path])];

        // On each iteration, we pop a node and its associated path off the node_stack,
        // and then process that node.
        while let Some((node, path)) = node_stack.pop() {
            if node.children.is_empty() {
                // Construct the path string for this leaf node
                let path_strs: Vec<&str> = path.iter()
                    .map(|s| s.as_str().strip_prefix(&node.base_path).unwrap().strip_prefix("/").unwrap_or(s.as_str()))
                    .collect();
                println!("{}", path_strs.join(" <- "));
            } else {
                // For each child, we create a new path that extends the current path with the child’s file_path.
                // Then we push the child and the new path onto the node_stack.
                for child in &node.children {
                    let mut new_path = path.clone();
                    new_path.push(&child.file_path);  // append the child’s file_path,
                    node_stack.push((child, new_path));
                }
            }
        }
    }
}
