#![allow(unused_imports)]

use std::collections::{BTreeMap, HashMap};
use std::{env, fs};
use std::path::{Path, PathBuf};
use anyhow::Result;
use fs_extra::{copy_items, dir};
use rsenv::{build_env, build_env_vars, extract_env, link, link_all, print_files, tree_traits, unlink};
use termtree::Tree;
use rsenv::builder::TreeBuilder;
use rsenv::arena::TreeArena;
use generational_arena::Index;
use rstest::rstest;
use rsenv::util::path::normalize_path_separator;

#[rstest]
fn test_build_trees_fail_invalid_parent_path() -> Result<()> {
    let original_dir = env::current_dir()?;
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/fail"));
    assert!(trees.is_err());

    // Get the error message and print it for debugging
    let err_msg = trees.err().unwrap().to_string();
    println!("Actual error message: {}", err_msg);

    // Check for PathResolution error with "not-existing.env"
    assert!(err_msg.contains("not-existing.env"));
    assert!(err_msg.contains("No such file or directory"));

    env::set_current_dir(original_dir)?;
    Ok(())
}


#[rstest]
fn test_build_trees_complex() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/complex"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree: {}", tree.depth());
        assert_eq!(tree.depth(), 5);
    }
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Leaf nodes:");
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 1);
        assert!(leaf_nodes[0].ends_with("level4.env"));
    }
    Ok(())
}

#[rstest]
fn test_build_trees_tree_and_leaf_paths() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/tree"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree: {}", tree.depth());
        assert_eq!(tree.depth(), 4);
    }
    for tree in &trees {
        assert_eq!(trees.len(), 1);
        println!("Tree Root:");

        let mut leaf_nodes = tree.leaf_nodes();
        leaf_nodes.sort();
        println!("Tree paths:");
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 4);
        assert!(leaf_nodes[0].ends_with("level11.env"));
    }
    Ok(())
}

#[rstest]
fn test_print_leaf_paths_when_root_path_only_matches_partially() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/max_prefix/confguard/xxx"))?;
    assert_eq!(trees.len(), 1);
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Tree paths:");
        for path in leaf_nodes {
            println!("{}", path);
        }
    }
    Ok(())
}

#[rstest]
fn test_print_leaf_paths_when_not_in_root() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/tree2/confguard"))?;
    assert_eq!(trees.len(), 1);

    for tree in &trees {
        let mut leaf_nodes = tree.leaf_nodes();
        println!("Tree paths:");
        assert_eq!(tree.depth(), 4);
        for path in &leaf_nodes {
            println!("{}", path);
        }

        // Convert full paths to relative paths starting at "tests/"
        let mut leaf_nodes: Vec<String> = leaf_nodes.iter()
            .map(|path| {
                let pos = path.find("tests/").unwrap();
                path[pos..].to_string()
            })
            .collect();

        // Sort for consistent comparison
        let mut expected = vec![
            "tests/resources/environments/tree2/confguard/level11.env",
            "tests/resources/environments/tree2/confguard/level13.env",
            "tests/resources/environments/tree2/confguard/level21.env",
            "tests/resources/environments/tree2/confguard/subdir/level32.env",
        ];
        expected.sort();
        leaf_nodes.sort();

        assert_eq!(leaf_nodes, expected);
    }
    Ok(())
}



#[rstest]
fn test_print() {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/complex")).unwrap();
    for tree in &trees {
        for (idx, node) in tree.iter() {
            println!("{}", node.data.file_path.display());
        }
    }
    // todo: assert order?
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
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/complex")).unwrap();
    for tree in trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                println!("{}", Tree::new(&root_node.data.file_path.to_string_lossy()));
                // todo: what should it show?
            }
        }
    }
}

#[rstest]
#[ignore = "Only for interactive exploration"]
fn test_print_tree_recursive() {
    // let trees = build_trees(Utf8Path::new("./tests/resources/environments/complex")).unwrap();
    // let trees = build_trees(Utf8Path::new("./tests/resources/environments/tree")).unwrap();
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/parallel")).unwrap();
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr = Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                println!("{}", tree_repr);
            }
        }
    }
}

#[rstest]
fn test_print_tree_recursive_data() {
    let expected = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/dot.envrc
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level1.env
    └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level2.env
        └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/a/level3.env
            └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/complex/level4.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/complex")).unwrap();
    assert_eq!(trees.len(), 1);
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr = Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                println!("{}", tree_str);
                assert_eq!(normalize_path_separator(&tree_str), normalize_path_separator(expected));
            }
        }
    }
}

#[rstest]
fn test_print_tree_recursive_parallel() {
    let expected = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/a_test.env
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/b_test.env
    └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/parallel/test.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/parallel")).unwrap();
    assert_eq!(trees.len(), 3);
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr = Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                println!("{}", tree_str);
                if tree_str.contains("test.env") {
                    assert_eq!(normalize_path_separator(&tree_str), normalize_path_separator(expected));
                }
            }
        }
    }
}

#[rstest]
fn test_print_tree_recursive_tree() {
    let expected = "/Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/root.env
├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level11.env
├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level12.env
│   ├── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level21.env
│   └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level22.env
│       └── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level32.env
└── /Users/Q187392/dev/s/public/rs-env/rsenv/tests/resources/environments/tree/level13.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/tree")).unwrap();
    assert_eq!(trees.len(), 1);
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr = Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                println!("{}", tree_str);
                assert_eq!(normalize_path_separator(&tree_str), normalize_path_separator(expected));
            }
        }
    }
}
