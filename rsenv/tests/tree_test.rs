//! Tests for TreeBuilder using v1 test fixtures

use std::path::Path;

use rsenv::domain::{create_branches, TreeBuilder};

// ============================================================
// Invalid Parent Tests
// ============================================================

#[test]
fn given_invalid_parent_path_when_building_trees_then_skips_unresolvable_parent() {
    // TreeBuilder silently skips parents that can't be resolved (canonicalize fails).
    // This is different from EnvironmentService which errors on missing parents.
    // The fail/ directory has level11.env -> not-existing.env, but TreeBuilder
    // will just treat level11.env as a standalone file since the parent doesn't resolve.
    let mut builder = TreeBuilder::new();
    let result = builder.build_from_directory(Path::new("tests/resources/environments/fail"));

    // TreeBuilder succeeds - it just ignores unresolvable parents
    assert!(
        result.is_ok(),
        "TreeBuilder should succeed, skipping missing parents: {:?}",
        result.err()
    );
    let trees = result.unwrap();
    // Should have 2 standalone trees since the parent reference doesn't resolve
    assert_eq!(trees.len(), 2);
}

// ============================================================
// Complex Hierarchy Tests
// ============================================================

#[test]
fn given_complex_hierarchy_when_building_trees_then_returns_correct_depth() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/complex"))
        .unwrap();

    // complex/ has a 5-level hierarchy (dot.envrc -> level1 -> level2 -> level3 -> level4)
    // plus a standalone result.env
    assert_eq!(trees.len(), 2);

    for tree in &trees {
        let depth = tree.depth();
        // Either depth 5 (the hierarchy) or depth 1 (standalone result.env)
        assert!(
            depth == 5 || depth == 1,
            "Expected depth 5 or 1, got {}",
            depth
        );
    }
}

#[test]
fn given_complex_hierarchy_when_building_trees_then_each_tree_has_one_leaf() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/complex"))
        .unwrap();

    for tree in &trees {
        let leaf_nodes = tree.leaf_nodes();
        assert_eq!(
            leaf_nodes.len(),
            1,
            "Each tree should have exactly one leaf"
        );
        // Leaf should be either level4.env or result.env
        let leaf = &leaf_nodes[0];
        assert!(
            leaf.ends_with("level4.env") || leaf.ends_with("result.env"),
            "Leaf should be level4.env or result.env, got {}",
            leaf
        );
    }
}

// ============================================================
// Tree Structure Tests
// ============================================================

#[test]
fn given_tree_structure_when_building_trees_then_returns_correct_hierarchy() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/tree"))
        .unwrap();

    // tree/ has a single tree structure with branching
    assert_eq!(trees.len(), 1);

    let tree = &trees[0];
    assert_eq!(tree.depth(), 4, "Tree should have depth 4");

    let mut leaf_nodes = tree.leaf_nodes();
    leaf_nodes.sort();
    // Should have 4 leaves: level11.env, level13.env, level21.env, level32.env
    assert_eq!(leaf_nodes.len(), 4, "Should have 4 leaf nodes");
    assert!(leaf_nodes[0].ends_with("level11.env"));
}

// ============================================================
// Parallel Structure Tests
// ============================================================

#[test]
fn given_parallel_structure_when_building_trees_then_returns_three_trees() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/parallel"))
        .unwrap();

    assert_eq!(trees.len(), 3, "Should find 3 independent trees");
}

#[test]
fn given_parallel_structure_when_getting_leaves_then_returns_correct_leaves() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/parallel"))
        .unwrap();

    let mut all_leaves: Vec<String> = trees.iter().flat_map(|t| t.leaf_nodes()).collect();
    all_leaves.sort();

    // Should have exactly 3 leaves: test.env, int.env, prod.env
    assert_eq!(all_leaves.len(), 3);
    assert!(all_leaves.iter().any(|l| l.ends_with("test.env")));
    assert!(all_leaves.iter().any(|l| l.ends_with("int.env")));
    assert!(all_leaves.iter().any(|l| l.ends_with("prod.env")));

    // These should NOT be leaves (they are intermediate nodes)
    assert!(!all_leaves.iter().any(|l| l.ends_with("a_test.env")));
    assert!(!all_leaves.iter().any(|l| l.ends_with("b_test.env")));
}

// ============================================================
// Subdirectory Tests
// ============================================================

#[test]
fn given_tree2_confguard_when_building_trees_then_returns_correct_hierarchy() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/tree2/confguard"))
        .unwrap();

    assert_eq!(trees.len(), 1);
    let tree = &trees[0];
    assert_eq!(tree.depth(), 4, "Tree should have depth 4");

    let mut leaf_nodes = tree.leaf_nodes();
    leaf_nodes.sort();

    // Should have 4 leaves including one in subdir
    assert_eq!(leaf_nodes.len(), 4);
    assert!(leaf_nodes.iter().any(|l| l.contains("subdir")));
}

// ============================================================
// Max Prefix Tests
// ============================================================

#[test]
fn given_max_prefix_structure_when_building_trees_then_handles_prefix_correctly() {
    let mut builder = TreeBuilder::new();
    let result = builder.build_from_directory(Path::new(
        "tests/resources/environments/max_prefix/confguard/xxx",
    ));

    // Should successfully build (path prefix handling)
    assert!(
        result.is_ok(),
        "Should handle max_prefix structure: {:?}",
        result.err()
    );
    let trees = result.unwrap();
    assert_eq!(trees.len(), 1);
}

// ============================================================
// Graph Tests (DAG structure)
// ============================================================

#[test]
fn given_graph_structure_when_building_trees_then_reports_cycle_due_to_dag() {
    // TreeBuilder is designed for tree structures (single parent per node).
    // DAG structures (multiple parents) cause cycle detection to trigger
    // because nodes get visited from multiple paths.
    // For DAG support, use EnvironmentService.build() which handles this correctly.
    let mut builder = TreeBuilder::new();
    let result = builder.build_from_directory(Path::new("tests/resources/environments/graph"));

    // TreeBuilder detects DAG as a cycle (expected behavior)
    assert!(result.is_err(), "TreeBuilder should detect DAG as cycle");
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("cycle") || err_msg.contains("Cycle"),
        "Error should mention cycle: {}",
        err_msg
    );
}

// ============================================================
// Iterator Tests
// ============================================================

#[test]
fn given_tree_when_iterating_then_visits_all_nodes() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/complex"))
        .unwrap();

    for tree in &trees {
        let mut count = 0;
        for (idx, node) in tree.iter() {
            count += 1;
            assert!(tree.get_node(idx).is_some());
            assert!(!node.data.file_path.to_string_lossy().is_empty());
        }
        assert!(count > 0, "Iterator should visit at least one node");
    }
}

#[test]
fn given_tree_when_postorder_iterating_then_visits_leaves_first() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/tree"))
        .unwrap();

    let tree = &trees[0];
    let mut paths: Vec<String> = Vec::new();

    for (_idx, node) in tree.iter_postorder() {
        paths.push(node.data.file_path.to_string_lossy().to_string());
    }

    // In postorder, leaves should come before root
    let leaf_nodes = tree.leaf_nodes();
    let root_path = tree
        .root()
        .and_then(|r| tree.get_node(r))
        .map(|n| n.data.file_path.to_string_lossy().to_string());

    if let Some(root) = root_path {
        let root_pos = paths.iter().position(|p| p == &root);
        for leaf in &leaf_nodes {
            let leaf_pos = paths.iter().position(|p| p == leaf);
            if let (Some(r), Some(l)) = (root_pos, leaf_pos) {
                assert!(l < r, "Leaf {} should come before root in postorder", leaf);
            }
        }
    }
}

// ============================================================
// Branch Tests (v1 test_edit.rs equivalent)
// ============================================================

/// Helper to extract just filenames from branches for comparison
fn branch_filenames(branches: Vec<Vec<std::path::PathBuf>>) -> Vec<Vec<String>> {
    branches
        .into_iter()
        .map(|branch| {
            branch
                .into_iter()
                .map(|p| {
                    p.file_name()
                        .expect("Invalid path")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .collect()
}

#[test]
fn given_tree_structure_when_creating_branches_then_returns_correct_branch_paths() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/tree"))
        .unwrap();

    let mut result = branch_filenames(create_branches(&trees));
    result.sort();

    let mut expected = vec![
        vec!["level11.env", "root.env"],
        vec!["level13.env", "root.env"],
        vec!["level32.env", "level22.env", "level12.env", "root.env"],
        vec!["level21.env", "level12.env", "root.env"],
    ];
    expected.sort();

    assert_eq!(result, expected);
}

#[test]
fn given_parallel_structure_when_creating_branches_then_returns_correct_paths() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/parallel"))
        .unwrap();

    let mut result = branch_filenames(create_branches(&trees));
    result.sort();

    let mut expected = vec![
        vec!["int.env", "b_int.env", "a_int.env"],
        vec!["prod.env", "b_prod.env", "a_prod.env"],
        vec!["test.env", "b_test.env", "a_test.env"],
    ];
    expected.sort();

    assert_eq!(result, expected);
}

#[test]
fn given_complex_structure_when_creating_branches_then_returns_correct_hierarchy() {
    let mut builder = TreeBuilder::new();
    let trees = builder
        .build_from_directory(Path::new("tests/resources/environments/complex"))
        .unwrap();

    let mut result = branch_filenames(create_branches(&trees));
    result.sort();

    let mut expected = vec![
        vec![
            "level4.env",
            "level3.env",
            "level2.env",
            "level1.env",
            "dot.envrc",
        ],
        vec!["result.env"], // result.env is a standalone file
    ];
    expected.sort();

    assert_eq!(result, expected);
}
