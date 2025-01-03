/*
Workaround for error: https://doc.rust-lang.org/error_codes/E0116.html
Cannot define inherent `impl` for a type outside of the crate where the type is defined

define a trait that has the desired associated functions/types/constants and implement the trait for the type in question
 */
use generational_arena::Index;
use termtree::Tree;
use tracing::instrument;
use crate::arena::TreeArena;
use crate::tree::TreeNodeRef;

pub trait TreeNodeConvert {
    fn to_tree_string(&self) -> Tree<String>;
}

impl TreeNodeConvert for TreeNodeRef {
    #[instrument(level = "debug")]
    fn to_tree_string(&self) -> Tree<String> {
        let node_borrowed = &self.borrow();

        // The root of the Tree<String> is the file_path of the TreeNode
        let root = node_borrowed.node_data.file_path.clone();

        // Recursively construct the children
        let leaves: Vec<_> = node_borrowed.children.iter()
            .map(|c| c.to_tree_string())
            .collect();

        Tree::new(root).with_leaves(leaves)
    }
}

// Implementation of to_tree_string for TreeArena
impl TreeNodeConvert for TreeArena {
    fn to_tree_string(&self) -> Tree<String> {
        if let Some(root_idx) = self.root() {
            let mut tree = Tree::new(self.get_node(root_idx).unwrap().data.file_path.display().to_string());

            fn build_tree(arena: &TreeArena, node_idx: Index, parent_tree: &mut Tree<String>) {
                if let Some(node) = arena.get_node(node_idx) {
                    for &child_idx in &node.children {
                        if let Some(child) = arena.get_node(child_idx) {
                            let mut child_tree = Tree::new(child.data.file_path.display().to_string());
                            build_tree(arena, child_idx, &mut child_tree);
                            parent_tree.push(child_tree);
                        }
                    }
                }
            }

            build_tree(self, root_idx, &mut tree);
            tree
        } else {
            Tree::new("Empty tree".to_string())
        }
    }
}

