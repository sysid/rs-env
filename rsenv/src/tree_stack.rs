#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use anyhow::{Context, Result};
use std::{env, fs};
use camino::{Utf8Path, Utf8PathBuf};
use pathdiff::diff_utf8_paths;

use walkdir::WalkDir;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use crate::tree::{TreeNode, TreeNodeRef};

use std::cell::RefCell;
use std::rc::Rc;
use termtree::Tree;
use tracing::{debug, instrument};
/*
Stack based tree implementation algorithms, educational purposes only
Stacks with Rc<RefCell<TreeNode>>:

Instead of just having the TreeNode on the stack, we now have an Rc<RefCell<TreeNode>>.
This requires to clone the Rc when pushing onto the stack.
The RefCell inside lets us borrow its contents immutably or mutably (we use immutable borrows here).

Accessing Nodes from the Rc<RefCell<TreeNode>>:
Before we can work with a TreeNode we need to borrow it from the RefCell using the borrow() method.

Pushing Children to Stacks:
When pushing a child node to a stack, we first clone the Rc and then push it.
 */

impl TreeNode {
    #[instrument(level = "debug")]
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

    #[instrument(level = "debug")]
    pub fn leaf_nodes2(&self) -> Vec<String> {
        let mut leaves = Vec::new();
        let mut stack = vec![Rc::new(RefCell::new(self.clone()))];

        while let Some(node_rc) = stack.pop() {
            let node = node_rc.borrow();
            if node.children.is_empty() {
                let leaf = &node.node_data.file_path;
                leaves.push(leaf.to_string());
            } else {
                for child_rc in &node.children {
                    stack.push(child_rc.clone());
                }
            }
        }

        leaves
    }

    #[instrument(level = "debug")]
    pub fn print_leaf_paths2(&self) {
        let mut node_stack = vec![(Rc::new(RefCell::new(self.clone())), vec![self.node_data.file_path.clone()])];

        while let Some((node_rc, path)) = node_stack.pop() {
            let node = node_rc.borrow();
            if node.children.is_empty() {
                let path_strs: Vec<&str> = path.iter()
                    .map(|s| s.as_str().strip_prefix(&node.node_data.base_path).unwrap()
                        .strip_prefix("/").unwrap_or(s.as_str()))
                    .collect();
                println!("{}", path_strs.join(" <- "));
            } else {
                for child_rc in &node.children {
                    let mut new_path = path.clone();
                    let p = &child_rc.borrow().node_data.file_path;
                    new_path.push(p.to_string());
                    node_stack.push((child_rc.clone(), new_path));
                }
            }
        }
    }
}

/// using raw pointers and unsafe code (not used in the final implementation)
/// educational purposes only
#[instrument(level = "debug")]
pub fn transform_tree_unsafe(root: &TreeNodeRef) -> Tree<String> {
    #[derive(Debug)]
    struct StackItem {
        original: TreeNodeRef,
        parent_ref: Option<*mut Vec<Tree<String>>>,  // raw pointer to leaves of the parent
    }

    let mut stack = Vec::new();

    let mut new_root = Tree::new(root.borrow().node_data.file_path.to_string());

    stack.push(StackItem {
        original: Rc::clone(root),
        parent_ref: None,
    });

    while let Some(current_item) = stack.pop() {
        
        let current_node = current_item.original.borrow();

        let new_node = Tree::new(current_node.node_data.file_path.to_string());

        if let Some(parent_ref) = current_item.parent_ref {
            unsafe { (*parent_ref).push(new_node); }
        }

        let leaves_ref = if let Some(parent_ref) = current_item.parent_ref {
            unsafe { &mut (*parent_ref).last_mut().unwrap().leaves as *mut Vec<Tree<String>> }
        } else {
            &mut new_root.leaves as *mut Vec<Tree<String>>
        };

        for child in &current_node.children {
            stack.push(StackItem {
                original: Rc::clone(child),
                parent_ref: Some(leaves_ref),
            });
        }
    }

    new_root
}

/// stack based implementation (not used in the final implementation)
/// educational purposes only
#[instrument(level = "debug")]
pub fn transform_tree(root: &TreeNodeRef) -> Tree<String> {
    #[derive(Debug)]
    struct StackItem {
        original: TreeNodeRef,
        parent: Option<Rc<RefCell<Tree<String>>>>,
    }

    let mut stack = Vec::new();

    let new_root = Rc::new(RefCell::new(Tree::new(root.borrow().node_data.file_path.to_string())));
    // debug!("new_root: {:#?}", new_root);

    stack.push(StackItem {
        original: Rc::clone(root),
        parent: None,
    });

    while let Some(current_item) = stack.pop() {
        
        let current_node = current_item.original.borrow();

        let new_node = if let Some(parent) = &current_item.parent {
            let new_child = Rc::new(RefCell::new(Tree::new(current_node.node_data.file_path.to_string())));
            debug!("new_child: {:#?}", new_child);
            parent.borrow_mut().leaves.push(new_child.borrow().clone());
            debug!("parent: {:#?}", parent);
            new_child
        } else {
            debug!("new_root: {:#?}", new_root);
            Rc::clone(&new_root)
        };

        for child in &current_node.children {
            stack.push(StackItem {
                original: Rc::clone(child),
                parent: Some(Rc::clone(&new_node)),
            });
            // debug!("stack: {:#?}", stack);
            // debug!("new_root {:?}", new_root);
        }
    }

    // debug!("new_root final {:?}", new_root);
    Rc::try_unwrap(new_root).unwrap().into_inner()
}
