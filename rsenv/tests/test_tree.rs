#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap};
use std::fs;
use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::tempdir;
use fs_extra::{copy_items, dir};
use rstest::{fixture, rstest};
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files, link, link_all, unlink};
use log::{debug, info};
use stdext::function_name;
use termtree::Tree;
use rsenv::tree::{build_trees, TreeNodeConvert};
use rsenv::tree_stack::{transform_tree, transform_tree_recursive, transform_tree_unsafe};

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

#[rstest]
fn test_build_trees() -> Result<()> {
    let trees = build_trees(Utf8Path::new("./tests/resources/data"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree rooted at {}: {}", tree.borrow().node_data.file_path, tree.borrow().depth());
        assert_eq!(tree.borrow().depth(), 5);
    }
    for tree in &trees {
        let leaf_nodes = tree.borrow().leaf_nodes();
        println!("Leaf nodes of tree rooted at {}:", tree.borrow().node_data.file_path);
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 1);
        assert!(leaf_nodes[0].ends_with("level4.env"));
    }
    for tree in &trees {
        let p = &tree.borrow().node_data.file_path;
        let mut path = vec![p.to_string()];
        println!("Leaf paths of tree rooted at {}:", tree.borrow().node_data.file_path);
        tree.borrow().print_leaf_paths(&mut path);
    }
    Ok(())
}

#[rstest]
fn test_print() {
    let trees = build_trees(Utf8Path::new("./tests/resources/data")).unwrap();
    for t in &trees {
        println!("{}", t.to_tree_string());
    }
}

#[rstest]
fn test_try_tree() {
    let mut tree1 = Tree::new("111");
    let mut tree2 = Tree::new("222");

    let mut tree = Tree::new("xxx");
    tree.push(Tree::new("yyy"));
    tree.push(Tree::new("zzz"));

    tree2.push(tree);
    tree1.push(tree2);
    println!("{}", tree1);
}

#[rstest]
fn test_print_tree() {
    let trees = build_trees(Utf8Path::new("./tests/resources/data")).unwrap();
    for t in trees {
        println!("{}", Tree::new(t.borrow()));
    }
}
#[rstest]
fn test_print_tree_stack() {
    let trees = build_trees(Utf8Path::new("./tests/resources/data")).unwrap();
    for t in &trees {
        println!("{}", t.borrow().print_tree());
    }
}
#[rstest]
fn test_print_tree_stack2() {
    let trees = build_trees(Utf8Path::new("./tests/resources/data")).unwrap();
    for t in &trees {
        // println!("{}", transform_tree_recursive(t));
        println!("{}", transform_tree(t));
        // println!("{}", transform_tree_unsafe(t));
        // println!("{}", t.borrow().print_tree());
    }
}
