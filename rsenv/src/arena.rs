use generational_arena::{Arena, Index};
use std::fmt;
use std::path::PathBuf;
use tracing::instrument;

/// Data payload for tree nodes representing environment files.
#[derive(Debug, Clone)]
pub struct NodeData {
    /// Directory containing the environment file
    pub base_path: PathBuf,
    /// Full path to the environment file
    pub file_path: PathBuf,
}

impl fmt::Display for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file_path.display())
    }
}

/// Tree node in the arena-based hierarchy structure.
#[derive(Debug)]
pub struct TreeNode {
    /// Environment file data for this node
    pub data: NodeData,
    /// Index of parent node in the arena, None for root nodes
    pub parent: Option<Index>,
    /// Indices of child nodes in the arena
    pub children: Vec<Index>,
}

/// Arena-based tree structure for efficient hierarchy management.
///
/// Uses generational arena for memory-safe node references and O(1) lookups.
/// Each tree represents one complete environment hierarchy.
#[derive(Debug)]
pub struct TreeArena {
    /// Arena storage for all tree nodes
    arena: Arena<TreeNode>,
    /// Index of the root node, None for empty trees
    root: Option<Index>,
}

impl Default for TreeArena {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeArena {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
            root: None,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn insert_node(&mut self, data: NodeData, parent: Option<Index>) -> Index {
        let node = TreeNode {
            data,
            parent,
            children: Vec::new(),
        };
        let node_idx = self.arena.insert(node);

        if let Some(parent_idx) = parent {
            if let Some(parent) = self.arena.get_mut(parent_idx) {
                parent.children.push(node_idx);
            }
        } else {
            self.root = Some(node_idx);
        }

        node_idx
    }

    #[instrument(level = "trace", skip(self))]
    pub fn get_node(&self, idx: Index) -> Option<&TreeNode> {
        self.arena.get(idx)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn get_node_mut(&mut self, idx: Index) -> Option<&mut TreeNode> {
        self.arena.get_mut(idx)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn root(&self) -> Option<Index> {
        self.root
    }

    #[instrument(level = "trace", skip(self))]
    pub fn iter(&self) -> TreeIterator {
        TreeIterator::new(self)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn iter_postorder(&self) -> PostOrderIterator {
        PostOrderIterator::new(self)
    }

    #[instrument(level = "debug", skip(self))]
    pub fn depth(&self) -> usize {
        if let Some(root) = self.root {
            self.calculate_depth(root)
        } else {
            0
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn calculate_depth(&self, node_idx: Index) -> usize {
        if let Some(node) = self.get_node(node_idx) {
            1 + node
                .children
                .iter()
                .map(|&child| self.calculate_depth(child))
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Collects all leaf nodes (nodes with no children) in the tree.
    ///
    /// Returns file paths as strings for easy display and processing.
    /// Empty trees return an empty vector.
    #[instrument(level = "debug", skip(self))]
    pub fn leaf_nodes(&self) -> Vec<String> {
        let mut leaves = Vec::new();
        if let Some(root) = self.root {
            self.collect_leaves(root, &mut leaves);
        }
        leaves
    }

    #[instrument(level = "trace", skip(self))]
    fn collect_leaves(&self, node_idx: Index, leaves: &mut Vec<String>) {
        if let Some(node) = self.get_node(node_idx) {
            if node.children.is_empty() {
                leaves.push(node.data.file_path.clone().to_string_lossy().to_string());
            } else {
                for &child in &node.children {
                    self.collect_leaves(child, leaves);
                }
            }
        }
    }
}

pub struct TreeIterator<'a> {
    arena: &'a TreeArena,
    stack: Vec<Index>,
}

impl<'a> TreeIterator<'a> {
    #[instrument(level = "trace")]
    fn new(arena: &'a TreeArena) -> Self {
        let mut stack = Vec::new();
        if let Some(root) = arena.root() {
            stack.push(root);
        }
        Self { arena, stack }
    }
}

impl<'a> Iterator for TreeIterator<'a> {
    type Item = (Index, &'a TreeNode);

    #[instrument(level = "trace", skip(self))]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current_idx) = self.stack.pop() {
            if let Some(node) = self.arena.get_node(current_idx) {
                // Push children in reverse order for left-to-right traversal
                for &child in node.children.iter().rev() {
                    self.stack.push(child);
                }
                return Some((current_idx, node));
            }
        }
        None
    }
}

pub struct PostOrderIterator<'a> {
    arena: &'a TreeArena,
    stack: Vec<(Index, bool)>,
}

impl<'a> PostOrderIterator<'a> {
    #[instrument(level = "trace")]
    fn new(arena: &'a TreeArena) -> Self {
        let mut stack = Vec::new();
        if let Some(root) = arena.root() {
            stack.push((root, false));
        }
        Self { arena, stack }
    }
}

impl<'a> Iterator for PostOrderIterator<'a> {
    type Item = (Index, &'a TreeNode);

    #[instrument(level = "trace", skip(self))]
    fn next(&mut self) -> Option<Self::Item> {
        while let Some((current_idx, visited)) = self.stack.pop() {
            if let Some(node) = self.arena.get_node(current_idx) {
                if !visited {
                    self.stack.push((current_idx, true));
                    for &child in node.children.iter().rev() {
                        self.stack.push((child, false));
                    }
                } else {
                    return Some((current_idx, node));
                }
            }
        }
        None
    }
}
