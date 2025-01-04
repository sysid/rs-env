/*
Workaround for error: https://doc.rust-lang.org/error_codes/E0116.html
Cannot define inherent `impl` for a type outside of the crate where the type is defined

define a trait that has the desired associated functions/types/constants and implement the trait for the type in question
 */
use crate::arena::TreeArena;
use generational_arena::Index;
use termtree::Tree;
use tracing::instrument;

pub trait TreeNodeConvert {
    fn to_tree_string(&self) -> Tree<String>;
}

impl TreeNodeConvert for TreeArena {
    #[instrument(level = "trace", skip(self))]
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

#[instrument(level = "trace", skip(tree))]
pub fn build_tree_representation(tree: &TreeArena, node_idx: Index, tree_repr: &mut Tree<String>) {
    if let Some(node) = tree.get_node(node_idx) {
        // Sort children only for display purposes
        let mut children = node.children.clone();
        children.sort_by(|a, b| {
            let a_node = tree.get_node(*a).unwrap();
            let b_node = tree.get_node(*b).unwrap();
            a_node.data.file_path.cmp(&b_node.data.file_path)
        });

        for &child_idx in &children {
            if let Some(child_node) = tree.get_node(child_idx) {
                let mut child_tree = Tree::new(child_node.data.file_path.to_string_lossy().to_string());
                build_tree_representation(tree, child_idx, &mut child_tree);
                tree_repr.push(child_tree);
            }
        }
    }
}