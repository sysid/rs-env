#![allow(unused_imports)]

use anyhow::Result;
use fs_extra::{copy_items, dir};
use generational_arena::Index;
use rsenv::arena::TreeArena;
use rsenv::builder::TreeBuilder;
use rsenv::util::path;
use rsenv::util::path::normalize_path_separator;
use rsenv::{
    build_env, build_env_vars, extract_env, link, link_all, print_files, tree_traits, unlink,
};
use rstest::rstest;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::{env, fs};
use termtree::Tree;

#[rstest]
fn given_invalid_parent_path_when_building_trees_then_returns_error() -> Result<()> {
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
fn given_complex_hierarchy_when_building_trees_then_returns_correct_depth_and_leaves() -> Result<()>
{
    let mut builder = TreeBuilder::new();
    let trees =
        builder.build_from_directory(Path::new("./tests/resources/environments/complex"))?;
    println!("trees: {:#?}", trees);
    for tree in &trees {
        println!("Depth of tree: {}", tree.depth());
        // The first tree should be the hierarchy with depth 5, the second should be the standalone file with depth 1
        assert!(tree.depth() == 5 || tree.depth() == 1);
    }
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Leaf nodes:");
        for leaf in &leaf_nodes {
            println!("{}", leaf);
        }
        assert_eq!(leaf_nodes.len(), 1);
        // Each tree should have exactly one leaf: either level4.env or result.env
        assert!(leaf_nodes[0].ends_with("level4.env") || leaf_nodes[0].ends_with("result.env"));
    }
    Ok(())
}

#[rstest]
fn given_tree_structure_when_building_trees_then_returns_correct_hierarchy() -> Result<()> {
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
fn given_partial_root_match_when_printing_leaf_paths_then_handles_prefix_correctly() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new(
        "./tests/resources/environments/max_prefix/confguard/xxx",
    ))?;
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
fn given_non_root_location_when_printing_leaf_paths_then_resolves_paths_correctly() -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/tree2/confguard"))?;
    assert_eq!(trees.len(), 1);

    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        println!("Tree paths:");
        assert_eq!(tree.depth(), 4);
        for path in &leaf_nodes {
            println!("{}", path);
        }

        let mut leaf_nodes = path::relativize_paths(leaf_nodes, "tests/");

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
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/complex"))
        .unwrap();
    for tree in &trees {
        for (idx, node) in tree.iter() {
            println!("{:?}: {}", idx, node.data.file_path.display());
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
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/complex"))
        .unwrap();
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
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/parallel"))
        .unwrap();
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr =
                    Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                println!("{}", tree_repr);
            }
        }
    }
}

#[rstest]
fn given_complex_structure_when_printing_tree_then_shows_nested_hierarchy() {
    let expected = "tests/resources/environments/complex/dot.envrc
└── tests/resources/environments/complex/level1.env
    └── tests/resources/environments/complex/level2.env
        └── tests/resources/environments/complex/a/level3.env
            └── tests/resources/environments/complex/level4.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/complex"))
        .unwrap();
    assert_eq!(trees.len(), 2); // One hierarchy tree + one standalone file (result.env)
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr =
                    Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                // Convert absolute paths to relative using path helper
                let relative_str = path::relativize_tree_str(&tree_str, "tests/");
                println!("{}", relative_str);
                // Only check the hierarchical tree, not the standalone result.env tree
                if relative_str.contains("dot.envrc") {
                    assert_eq!(
                        normalize_path_separator(&relative_str),
                        normalize_path_separator(expected)
                    );
                }
            }
        }
    }
}

#[rstest]
fn given_parallel_structure_when_printing_tree_then_shows_correct_hierarchy() {
    let expected = "tests/resources/environments/parallel/a_test.env
└── tests/resources/environments/parallel/b_test.env
    └── tests/resources/environments/parallel/test.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/parallel"))
        .unwrap();
    assert_eq!(trees.len(), 3);
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr =
                    Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                let relative_str = path::relativize_tree_str(&tree_str, "tests/");
                println!("{}", relative_str);
                if relative_str.contains("test.env") {
                    assert_eq!(
                        normalize_path_separator(&relative_str),
                        normalize_path_separator(expected)
                    );
                }
            }
        }
    }
}

#[rstest]
fn given_tree_structure_when_printing_complete_tree_then_shows_all_branches() {
    let expected = "tests/resources/environments/tree/root.env
├── tests/resources/environments/tree/level11.env
├── tests/resources/environments/tree/level12.env
│   ├── tests/resources/environments/tree/level21.env
│   └── tests/resources/environments/tree/level22.env
│       └── tests/resources/environments/tree/level32.env
└── tests/resources/environments/tree/level13.env\n";

    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("./tests/resources/environments/tree"))
        .unwrap();
    assert_eq!(trees.len(), 1);
    for tree in &trees {
        if let Some(root_idx) = tree.root() {
            if let Some(root_node) = tree.get_node(root_idx) {
                let mut tree_repr =
                    Tree::new(root_node.data.file_path.to_string_lossy().to_string());
                tree_traits::build_tree_representation(tree, root_idx, &mut tree_repr);
                let tree_str = tree_repr.to_string();
                let relative_str = path::relativize_tree_str(&tree_str, "tests/");
                println!("{}", relative_str);
                assert_eq!(
                    normalize_path_separator(&relative_str),
                    normalize_path_separator(expected)
                );
            }
        }
    }
}


#[rstest]
fn given_mixed_standalone_and_hierarchical_files_when_getting_leaves_then_returns_correct_leaves(
) -> Result<()> {
    let mut builder = TreeBuilder::new();
    let trees =
        builder.build_from_directory(Path::new("./tests/resources/environments/parallel"))?;

    let mut all_leaves = Vec::new();
    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        all_leaves.extend(leaf_nodes);
    }

    // Should return only the leaf nodes from hierarchical trees (test.env, int.env, prod.env)
    // but not the standalone files that are part of hierarchies (a_test.env, b_test.env, etc.)
    assert_eq!(all_leaves.len(), 3);
    assert!(all_leaves.iter().any(|leaf| leaf.ends_with("test.env")));
    assert!(all_leaves.iter().any(|leaf| leaf.ends_with("int.env")));
    assert!(all_leaves.iter().any(|leaf| leaf.ends_with("prod.env")));

    // These should NOT be leaves as they are part of hierarchies
    assert!(!all_leaves.iter().any(|leaf| leaf.ends_with("a_test.env")));
    assert!(!all_leaves.iter().any(|leaf| leaf.ends_with("b_test.env")));

    Ok(())
}
