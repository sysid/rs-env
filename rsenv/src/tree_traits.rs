/*
Workaround for error: https://doc.rust-lang.org/error_codes/E0116.html
Cannot define inherent `impl` for a type outside of the crate where the type is defined

define a trait that has the desired associated functions/types/constants and implement the trait for the type in question
 */
use termtree::Tree;
use tracing::instrument;
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
