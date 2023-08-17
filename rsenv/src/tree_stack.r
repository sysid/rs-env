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

use std::cell::RefCell;
use std::rc::Rc;

/*
Stacks with Rc<RefCell<TreeNode>>:

Instead of just having the TreeNode on the stack, we now have an Rc<RefCell<TreeNode>>.
This requires us to clone the Rc when pushing onto the stack. The RefCell inside lets us borrow its contents immutably or mutably (we use immutable borrows here).
Accessing Nodes from the Rc<RefCell<TreeNode>>:

Before we can work with a TreeNode we need to borrow it from the RefCell using the borrow() method.
Pushing Children to Stacks:

When pushing a child node to a stack, we first clone the Rc and then push it.
 */

impl TreeNode {
    pub fn depth2(&self) -> usize {
        let mut max_depth = 0;
        let mut stack = vec![(Rc::new(RefCell::new(self.clone())), 1)]; // (node, depth)

        while let Some((node_rc, depth)) = stack.pop() {
            if depth > max_depth {
                max_depth = depth;
            }
            let node = node_rc.borrow();
            for child_rc in &node.children {
                stack.push((child_rc.clone(), depth + 1));
            }
        }

        max_depth
    }

    pub fn leaf_nodes2(&self) -> Vec<&String> {
        let mut leaves = Vec::new();
        let mut stack = vec![Rc::new(RefCell::new(self.clone()))];

        while let Some(node_rc) = stack.pop() {
            let node = node_rc.borrow();
            if node.children.is_empty() {
                leaves.push(&node.file_path);
            } else {
                for child_rc in &node.children {
                    stack.push(child_rc.clone());
                }
            }
        }

        leaves
    }

    pub fn print_leaf_paths2(&self) {
        let mut node_stack = vec![(Rc::new(RefCell::new(self.clone())), vec![&self.file_path])];

        while let Some((node_rc, path)) = node_stack.pop() {
            let node = node_rc.borrow();
            if node.children.is_empty() {
                let path_strs: Vec<&str> = path.iter()
                    .map(|s| s.as_str().strip_prefix(&node.base_path).unwrap().strip_prefix("/").unwrap_or(s.as_str()))
                    .collect();
                println!("{}", path_strs.join(" <- "));
            } else {
                for child_rc in &node.children {
                    let mut new_path = path.clone();
                    new_path.push(&child_rc.borrow().file_path);
                    node_stack.push((child_rc.clone(), new_path));
                }
            }
        }
    }
}

