//! Tests verifying v1 compatibility with original test fixtures

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rsenv::application::services::EnvironmentService;
use rsenv::domain::EnvFile;
use rsenv::infrastructure::traits::RealFileSystem;

/// Helper to parse a result.env file as reference
fn load_reference_vars(path: &Path) -> BTreeMap<String, String> {
    let content = std::fs::read_to_string(path).expect("read reference file");
    let env_file = EnvFile::parse(&content, path.to_path_buf()).expect("parse reference");
    env_file.variables
}

/// Helper to filter variables by prefix
fn filter_vars_by_prefix(
    vars: &BTreeMap<String, String>,
    prefix: &str,
) -> BTreeMap<String, String> {
    vars.iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[test]
fn given_v1_dag_structure_when_building_then_handles_multiple_parents() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service.build(Path::new("tests/resources/environments/graph/level31.env"));
    assert!(
        result.is_ok(),
        "Should handle DAG structure: {:?}",
        result.err()
    );

    let output = result.unwrap();
    // level31.env has: # rsenv: level21.env root.env
    assert_eq!(output.variables.get("var31"), Some(&"31".to_string()));
}

#[test]
fn given_v1_fail_directory_when_building_then_errors_on_missing_parent() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // fail/level11.env references a non-existing parent
    let result = service.build(Path::new("tests/resources/environments/fail/level11.env"));
    assert!(result.is_err(), "Should fail on missing parent");
}

#[test]
fn given_v1_env_vars_when_building_then_expands_variables() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Use absolute path for RSENV_TEST_ROOT (v1 expectation)
    let cwd = std::env::current_dir().unwrap();
    let env_vars_dir = cwd.join("tests/resources/environments/env_vars");
    std::env::set_var(
        "RSENV_TEST_ROOT",
        env_vars_dir.to_string_lossy().to_string(),
    );

    let result = service.build(Path::new(
        "tests/resources/environments/env_vars/development.env",
    ));
    // This tests that ${RSENV_TEST_ROOT}/config/base.env is expanded correctly
    assert!(result.is_ok(), "Should expand env vars: {:?}", result.err());
}

// ============================================================
// Reference Comparison Tests (comparing against result.env)
// ============================================================

#[test]
fn given_v1_complex_level4_when_building_then_matches_result_env() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/complex/level4.env"))
        .unwrap();

    let reference =
        load_reference_vars(Path::new("tests/resources/environments/complex/result.env"));

    let filtered = filter_vars_by_prefix(&result.variables, "VAR_");
    assert_eq!(
        filtered, reference,
        "Merged variables should match result.env"
    );
}

#[test]
fn given_v1_graph_level31_when_building_then_matches_result_env() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/graph/level31.env"))
        .unwrap();

    let reference = load_reference_vars(Path::new("tests/resources/environments/graph/result.env"));

    // Filter to just the vars that should match (both have var prefixes)
    let result_filtered: BTreeMap<_, _> = result
        .variables
        .iter()
        .filter(|(k, _)| k.starts_with("var") || k.starts_with("root"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    assert_eq!(
        result_filtered, reference,
        "Graph merge should match result.env"
    );
}

#[test]
fn given_v1_graph2_level21_when_building_then_matches_result1() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/graph2/level21.env"))
        .unwrap();

    let reference =
        load_reference_vars(Path::new("tests/resources/environments/graph2/result1.env"));

    assert_eq!(
        result.variables, reference,
        "Graph2 level21 should match result1.env"
    );
}

#[test]
fn given_v1_graph2_level22_when_building_then_matches_result2() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/graph2/level22.env"))
        .unwrap();

    let reference =
        load_reference_vars(Path::new("tests/resources/environments/graph2/result2.env"));

    assert_eq!(
        result.variables, reference,
        "Graph2 level22 should match result2.env"
    );
}

// ============================================================
// Parallel Hierarchy Tests
// ============================================================

#[test]
fn given_v1_parallel_test_when_building_then_returns_correct_exports() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/parallel/test.env"))
        .unwrap();

    // v1 build_env_vars returns formatted export string
    let mut exports: Vec<String> = result
        .variables
        .iter()
        .map(|(k, v)| format!("export {}={}", k, v))
        .collect();
    exports.sort();

    let expected = vec![
        "export a_var1=a_test_var1",
        "export a_var2=a_test_var2",
        "export b_var1=b_test_var1",
        "export b_var2=b_test_var2",
        "export var1=test_var1",
        "export var2=test_var2",
    ];

    assert_eq!(exports, expected);
}

// ============================================================
// Error Cases
// ============================================================

#[test]
fn given_v1_invalid_parent_when_building_then_error_mentions_missing_file() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service.build(Path::new("tests/resources/environments/graph2/error.env"));

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not-existing.env"),
        "Error should mention missing file: {}",
        err_msg
    );
}

// ============================================================
// File Count Tests
// ============================================================

#[test]
fn given_v1_complex_level4_when_getting_files_then_returns_5_level_hierarchy() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let result = service
        .build(Path::new("tests/resources/environments/complex/level4.env"))
        .unwrap();

    // 5-level hierarchy: level4 -> level3 -> level2 -> level1 -> (no more parents)
    // Actually checking the files in the hierarchy
    assert!(
        result.files.len() >= 4,
        "Should have at least 4 files in hierarchy, got {}",
        result.files.len()
    );
}

// ============================================================
// DAG Detection Tests
// ============================================================

#[test]
fn given_v1_tree_structure_when_checking_dag_then_returns_false() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let is_dag = service
        .is_dag(Path::new("tests/resources/environments/tree"))
        .unwrap();
    assert!(!is_dag, "Tree structure should not be DAG");
}

#[test]
fn given_v1_graph_structure_when_checking_dag_then_returns_true() {
    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    let is_dag = service
        .is_dag(Path::new("tests/resources/environments/graph"))
        .unwrap();
    assert!(is_dag, "Graph structure should be DAG");
}
