#![allow(unused_imports)]

use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use anyhow::{Context, Result};
use log::{debug, info};
use std::{env, fmt, fs};
use std::cell::RefCell;
use camino::{Utf8Path, Utf8PathBuf};
use pathdiff::diff_utf8_paths;
use stdext::function_name;

use walkdir::WalkDir;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::rc::{Rc, Weak};
use termtree::Tree;
use crate::dlog;

#[derive(Debug, Clone)]
pub struct NodeData {
    pub base_path: String,
    pub file_path: String,
}

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file_path)
    }
}

/// Parent relationship is one of non-ownership.
/// This is not a `Rc<TreeNode<T>>` which would cause memory leak.
pub type WeakTreeNodeRef<> = Weak<RefCell<TreeNode>>;

pub type TreeNodeRef<> = Rc<RefCell<TreeNode>>;

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub node_data: NodeData,
    pub parent: Option<WeakTreeNodeRef>,
    pub children: Vec<TreeNodeRef>,
}

impl fmt::Display for TreeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Write the current node's data
        write!(f, "{}", self.node_data)?;

        // If there are children, recursively write them
        if !self.children.is_empty() {
            writeln!(f, " [")?;
            for child in &self.children {
                // For better formatting, we can add indentation for child nodes
                write!(f, "  {}\n", child.borrow())?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

/*
The RefCell allows us to borrow the contents, and the Rc allows for shared ownership.

Accessing the Children:
Instead of directly accessing child, we first need to borrow the value inside the RefCell.
We do this using the borrow() method, which gives us an immutable reference to the value inside the RefCell.
For example, in the depth function, we replace child.depth() with child_rc.borrow().depth().
In the above line, child_rc is a reference-counted pointer to the RefCell that wraps the TreeNode.
We call borrow() to get a reference to the TreeNode and then call depth() on that.

Iterating over the Children:
In the loop where we iterate over the children, we’ve renamed the loop variable to child_rc to emphasize that it is a reference-counted pointer to a RefCell, not a direct reference to a TreeNode.
Inside the loop, we borrow the TreeNode from the RefCell in order to call methods on it or access its fields.
 */
impl TreeNode {
    pub fn depth(&self) -> usize {
        1 + self.children.iter()
            .map(|child_rc| child_rc.borrow().depth())
            .max()
            .unwrap_or(0)
    }

    pub fn leaf_nodes(&self) -> Vec<String> {
        if self.children.is_empty() {
            vec![self.node_data.file_path.clone()]
        } else {
            let mut leaves = Vec::new();
            for child_rc in &self.children {
                leaves.extend(child_rc.borrow().leaf_nodes());
            }
            leaves
        }
    }

    /// Prints the leaf paths of the tree, starting from 'self'
    ///
    /// This function traverses the tree and prints the paths to leaf nodes. For each leaf node,
    /// the path is constructed by stripping the base path and joining the segments using " <- ".
    /// Paths are printed to the console.
    ///
    /// # Arguments
    ///
    /// * `path`: A mutable vector of strings which temporarily stores the path segments as the tree
    ///   is traversed. It should typically be initialized as an empty vector before calling this function.
    ///
    pub fn print_leaf_paths(&self, path: &mut Vec<String>) {
        if self.children.is_empty() {
            let root_path = Utf8Path::new(&path[0]).parent().unwrap();
            let path_strs: Vec<&str> = path.iter()
                .map(
                    |s| {
                        dlog!("s: {}, root {}", s, root_path);
                        s.as_str().strip_prefix(root_path.as_str()).unwrap().strip_prefix("/").unwrap_or(s.as_str())
                    }
                )
                .collect();
            println!("{}", path_strs.join(" <- "));
        } else {
            for child_rc in &self.children {
                let child = child_rc.borrow();
                path.push(child.node_data.file_path.clone());
                dlog!("path: {:?}", path);
                child.print_leaf_paths(path);
                path.pop();  // backtracking
            }
        }
    }
}


/// Builds a vector of environment tree structures from a given directory path.
///
/// This function scans all the files within the specified directory and its subdirectories,
/// looking for lines that match the pattern `# rsenv: (.+)`.
/// Each matched line indicates a parent-child relationship between files.
///
/// For example, if a file `child.env` contains the line `# rsenv: parent.env`, then `child.env`
/// is considered a child of `parent.env`.
///
/// The resulting trees are constructed based on these relationships. Each tree has a root file
/// (a file that's not a child of any other file) and zero or more child files. Child files
/// can further have their own children, forming the tree structure.
///
/// # Arguments
///
/// * `directory_path`: The path of the directory where the scan starts.
///
/// # Returns
///
/// Returns a `Result` containing a vector of `Rc<RefCell<TreeNode>>` structures. Each `TreeNode`
/// represents the root of a tree. In case of errors, such as IO failures or issues with the path,
/// an error variant is returned.
///
/// # Panics
///
/// This function may panic if it encounters issues with regex compilation or
/// if it fails to unwrap certain expected values, which should be typically present.
///
pub fn build_trees(directory_path: &Utf8Path) -> Result<Vec<Rc<RefCell<TreeNode>>>> {
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

    // root nodes are the ones which do not show up as values in the relationships map
    let root_files: Vec<String> = relationships
        .keys()
        .filter(|&key| !relationships.values().any(|v| v.contains(key)))
        .cloned()
        .collect();

    let mut trees = Vec::new();
    for root in root_files {
        trees.push(build_tree_stack(&root, &relationships, directory_path.as_path()));
    }

    // make tree order stable
    let mut sorted_trees = trees.clone();
    sorted_trees.sort_by(|a, b| a.borrow().node_data.file_path.cmp(&b.borrow().node_data.file_path));

    Ok(sorted_trees)
}

/*
Changing the Type of Children Vector:
We've changed the children field in the TreeNode struct to hold a vector of Rc<RefCell<TreeNode>> instead of just TreeNode.
This means that each node in the tree is now wrapped in a RefCell, which allows for interior mutability
(i.e., we can now change the contents of a TreeNode even when we have an immutable reference to it), and an Rc,
which allows for multiple ownership (i.e., we can have multiple references to the same TreeNode).

Creating New Nodes:
When we create a new node (new_node), we immediately wrap it in an Rc and a RefCell.

Storing Nodes in Stack:
We push clones of the Rc<RefCell<TreeNode>> to the stack, rather than pushing the node itself.
This allows us to keep multiple references to the same node without duplicating the node itself.

Adding Children to a Node:
When we want to add a child to a node, we first get a mutable reference to the parent node by calling
borrow_mut() on the RefCell, and then we push the child node onto the children vector.
We are cloning the Rc, not the TreeNode itself, so this doesn't duplicate the node.

Returning the Root Node:
The function now returns an Rc<RefCell<TreeNode>> instead of a TreeNode.
This is consistent with the fact that all nodes in the tree are now wrapped in Rc<RefCell<...>>.
 */
/// non-recursive (iterative) version of the build_tree function using a stack data structure.
/// This approach mimics the call stack that is used in the recursive approach, but with an explicit stack data structure:
pub fn build_tree_stack(file_name: &str, relationships: &HashMap<String, Vec<String>>, directory_path: &Utf8Path) -> Rc<RefCell<TreeNode>> {
    let mut stack = Vec::new();
    let root = Rc::new(RefCell::new(TreeNode {
        node_data: NodeData { base_path: directory_path.to_string(), file_path: file_name.to_string() },
        parent: None,
        children: Vec::new(),
    }));

    stack.push((file_name.to_string(), Rc::clone(&root)));  // immutable borrow of root

    while let Some((node_name, parent_node)) = stack.pop() {
        if let Some(children_names) = relationships.get(&node_name) {
            for child_name in children_names {
                let new_node = Rc::new(RefCell::new(TreeNode {
                    node_data: NodeData { base_path: directory_path.to_string(), file_path: child_name.clone() },
                    parent: Some(Rc::downgrade(&parent_node)),
                    children: Vec::new(),
                }));

                stack.push((child_name.clone(), Rc::clone(&new_node)));
                parent_node.borrow_mut().children.push(Rc::clone(&new_node));
            }
        }
    }

    root
}


#[cfg(test)]
mod tests {
    use rstest::rstest;
    use super::*;

    #[ctor::ctor]
    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::max())
            .is_test(true)
            .try_init();
    }

    // root
    // ├── child1
    // │   └── grandchild1
    // └── child2

    //      root
    //      /  \
    // child1 child2
    //    |
    // grandchild1
    #[test]
    // #[ignore = "Implementation not working"]
    fn test_build_tree_stack() {
        // Set up a HashMap to represent the relationships between files
        let mut relationships = HashMap::new();
        relationships.insert("root".to_string(), vec!["child1".to_string(), "child2".to_string()]);
        relationships.insert("child1".to_string(), vec!["grandchild1".to_string()]);

        // Build the tree starting from "root"
        let tree = build_tree_stack("root", &relationships, &Utf8PathBuf::from(""));
        println!("{:#?}", tree);

        // Check the root node
        assert_eq!(tree.borrow().node_data.file_path, "root");
        assert_eq!(tree.borrow().children.len(), 2);

        // Check the first child node
        let child1 = &tree.borrow().children[0];
        assert_eq!(child1.borrow().node_data.file_path, "child1");
        assert_eq!(child1.borrow().children.len(), 1);

        // Check the grandchild node
        let grandchild1 = &child1.borrow().children[0];
        assert_eq!(grandchild1.borrow().node_data.file_path, "grandchild1");
        assert_eq!(grandchild1.borrow().children.len(), 0);

        // Check the second child node
        let child2 = &tree.borrow().children[1];
        assert_eq!(child2.borrow().node_data.file_path, "child2");
        assert_eq!(child2.borrow().children.len(), 0);
    }

    #[rstest]
    fn test_display() {
        let d = NodeData {
            base_path: "base_path".to_string(),
            file_path: "file_path".to_string(),
        };
        println!("{}", d);
        assert_eq!(format!("{}", d), "file_path")
    }
}

