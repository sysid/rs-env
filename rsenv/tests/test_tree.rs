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
use rsenv::tree::build_trees;

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
        println!("Depth of tree rooted at {}: {}", tree.file_path, tree.depth());
        assert_eq!(tree.depth(), 5);
    }
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Leaf nodes of tree rooted at {}:", tree.file_path);
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 1);
        assert!(leaf_nodes[0].ends_with("level4.env"));
    }
    for tree in &trees {
        let mut path = vec![&tree.file_path];
        println!("Leaf paths of tree rooted at {}:", tree.file_path);
        tree.print_leaf_paths(&mut path);
    }
    Ok(())
}

#[rstest]
fn test_build_trees_stack() -> Result<()> {
    let trees = build_trees(Utf8Path::new("./tests/resources/data"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree rooted at {}: {}", tree.file_path, tree.depth());
        assert_eq!(tree.depth(), 5);
    }
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Leaf nodes of tree rooted at {}:", tree.file_path);
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 1);
        assert!(leaf_nodes[0].ends_with("level4.env"));
    }
    for tree in &trees {
        let mut path = vec![&tree.file_path];
        println!("Leaf paths of tree rooted at {}:", tree.file_path);
        tree.print_leaf_paths(&mut path);
    }
    Ok(())
}
