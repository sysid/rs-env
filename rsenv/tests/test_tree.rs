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
use rsenv::tree::{build_trees};
use rsenv::tree_stack::{transform_tree, transform_tree_recursive, transform_tree_unsafe};
use rsenv::tree_traits::TreeNodeConvert;

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

#[rstest]
/// tests recursive variants (tree building: stack based)
fn test_build_trees_complex() -> Result<()> {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex"))?;
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
/// tests recursive variants (tree building: stack based)
fn test_build_trees_tree() -> Result<()> {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree rooted at {}: {}", tree.borrow().node_data.file_path, tree.borrow().depth());
        assert_eq!(tree.borrow().depth(), 4);
    }
    for tree in &trees {
        let leaf_nodes = tree.borrow().leaf_nodes();
        println!("Tree Root: {}:", tree.borrow().node_data.file_path);
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 4);
        assert!(leaf_nodes[0].ends_with("level11.env"));
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
fn test_print_leaf_paths() -> Result<()> {
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree"))?;
    assert_eq!(trees.len(), 1);
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
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
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
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
    for t in trees {
        println!("{}", Tree::new(t.borrow()));
    }
}

#[rstest]
#[ignore = "Only for interactive exploration"]
fn test_print_tree_recursive() {
    // let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
    // let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree")).unwrap();
    let trees = build_trees(Utf8Path::new("./tests/resources/environments/parallel")).unwrap();
    for t in &trees {
        println!("{}", transform_tree_recursive(t));
        // println!("{}", transform_tree(t));
        // println!("{}", transform_tree_unsafe(t));
        // println!("{}", t.borrow().print_tree());
    }
}

#[rstest]
fn test_print_tree_recursive_data() {
    let result = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/dot.envrc
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level1.env
    └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level2.env
        └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/a/level3.env
            └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level4.env\n";

    let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
    assert_eq!(trees.len(), 1);
    for t in &trees {
        println!("{}", transform_tree_recursive(t));
        assert_eq!(format!("{}", transform_tree_recursive(t)), result)
    }
}

#[rstest]
fn test_print_tree_recursive_parallel() {
    let result = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/a_test.env
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/b_test.env
    └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/test.env\n";

    let trees = build_trees(Utf8Path::new("./tests/resources/environments/parallel")).unwrap();
    assert_eq!(trees.len(), 3);
    for t in &trees {
        println!("{}", transform_tree_recursive(t));
        if t.borrow().node_data.file_path.ends_with("test.env") {
            assert_eq!(format!("{}", transform_tree_recursive(t)), result)
        }
    }
}

#[rstest]
fn test_print_tree_recursive_tree() {
    let result = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env
├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level11.env
├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level13.env
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level12.env
    ├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level22.env
    │   └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level32.env
    └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level21.env\n";

    let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree")).unwrap();
    assert_eq!(trees.len(), 1);
    for t in &trees {
        println!("{}", transform_tree_recursive(t));
        assert_eq!(format!("{}", transform_tree_recursive(t)), result)
    }
}
