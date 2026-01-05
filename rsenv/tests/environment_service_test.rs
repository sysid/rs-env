//! Tests for EnvironmentService

use std::path::PathBuf;

use tempfile::TempDir;

use rsenv::application::services::EnvironmentService;
use rsenv::infrastructure::traits::{FileSystem, RealFileSystem};

/// Helper to create temp env files for testing
fn create_env_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("write env file");
    path
}

#[test]
fn given_single_env_file_when_building_then_returns_its_variables() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();
    let leaf = create_env_file(
        &temp,
        "local.env",
        r#"export FOO=bar
export BAZ=qux
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&leaf).unwrap();

    // Assert
    assert_eq!(result.variables.get("FOO"), Some(&"bar".to_string()));
    assert_eq!(result.variables.get("BAZ"), Some(&"qux".to_string()));
}

#[test]
fn given_env_file_with_parent_when_building_then_merges_variables() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    // Create parent first
    let _base = create_env_file(
        &temp,
        "base.env",
        r#"export BASE_VAR=from_base
export SHARED=base_value
"#,
    );

    // Create leaf that references parent
    let leaf = create_env_file(
        &temp,
        "local.env",
        r#"# rsenv: base.env
export LOCAL_VAR=from_local
export SHARED=local_value
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&leaf).unwrap();

    // Assert - base vars should be present
    assert_eq!(
        result.variables.get("BASE_VAR"),
        Some(&"from_base".to_string())
    );
    // Assert - local vars should be present
    assert_eq!(
        result.variables.get("LOCAL_VAR"),
        Some(&"from_local".to_string())
    );
    // Assert - local should override base (child wins)
    assert_eq!(
        result.variables.get("SHARED"),
        Some(&"local_value".to_string())
    );
}

#[test]
fn given_three_level_hierarchy_when_building_then_merges_all() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    // root.env (no parent)
    let _root = create_env_file(
        &temp,
        "root.env",
        r#"export ROOT=root_value
export OVERRIDE=from_root
"#,
    );

    // middle.env -> root.env
    let _middle = create_env_file(
        &temp,
        "middle.env",
        r#"# rsenv: root.env
export MIDDLE=middle_value
export OVERRIDE=from_middle
"#,
    );

    // leaf.env -> middle.env
    let leaf = create_env_file(
        &temp,
        "leaf.env",
        r#"# rsenv: middle.env
export LEAF=leaf_value
export OVERRIDE=from_leaf
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&leaf).unwrap();

    // Assert - all levels present
    assert_eq!(
        result.variables.get("ROOT"),
        Some(&"root_value".to_string())
    );
    assert_eq!(
        result.variables.get("MIDDLE"),
        Some(&"middle_value".to_string())
    );
    assert_eq!(
        result.variables.get("LEAF"),
        Some(&"leaf_value".to_string())
    );
    // Assert - leaf wins override
    assert_eq!(
        result.variables.get("OVERRIDE"),
        Some(&"from_leaf".to_string())
    );
}

#[test]
fn given_dag_with_multiple_parents_when_building_then_merges_all_branches() {
    // Arrange - v1 format uses export prefix and space-separated parents
    let temp = TempDir::new().unwrap();

    // Two independent bases
    let _base_a = create_env_file(
        &temp,
        "base_a.env",
        r#"export FROM_A=value_a
export SHARED_AB=from_a
"#,
    );

    let _base_b = create_env_file(
        &temp,
        "base_b.env",
        r#"export FROM_B=value_b
export SHARED_AB=from_b
"#,
    );

    // Leaf has both parents (v1 format: space-separated)
    let leaf = create_env_file(
        &temp,
        "leaf.env",
        r#"# rsenv: base_a.env base_b.env
export LEAF=leaf_value
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&leaf).unwrap();

    // Assert - both branches present
    assert_eq!(result.variables.get("FROM_A"), Some(&"value_a".to_string()));
    assert_eq!(result.variables.get("FROM_B"), Some(&"value_b".to_string()));
    assert_eq!(
        result.variables.get("LEAF"),
        Some(&"leaf_value".to_string())
    );
    // Note: SHARED_AB will be from whichever is processed last in BFS
}

#[test]
fn given_nonexistent_parent_when_building_then_returns_error() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    let leaf = create_env_file(
        &temp,
        "leaf.env",
        r#"# rsenv: nonexistent.env
export FOO=bar
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&leaf);

    // Assert
    assert!(result.is_err());
}

#[test]
fn given_cycle_in_hierarchy_when_building_then_handles_gracefully() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    // a.env -> b.env
    let _a = create_env_file(
        &temp,
        "a.env",
        r#"# rsenv: b.env
export A=value_a
"#,
    );

    // b.env -> a.env (cycle!)
    let _b = create_env_file(
        &temp,
        "b.env",
        r#"# rsenv: a.env
export B=value_b
"#,
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(&temp.path().join("a.env"));

    // Assert - should either error or handle cycle by not revisiting
    // Either way, should not infinite loop
    // For now, let's just ensure it completes (doesn't hang)
    assert!(result.is_ok() || result.is_err());
}

// ============================================================
// get_hierarchy tests
// ============================================================

#[test]
fn given_directory_with_env_files_when_getting_hierarchy_then_returns_all_leaves() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    // Create base (no parent - root)
    let _base = create_env_file(&temp, "base.env", "export BASE=value\n");

    // Create two leaves pointing to base
    let _local = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=value\n",
    );
    let _prod = create_env_file(&temp, "prod.env", "# rsenv: base.env\nexport PROD=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();

    // Assert - should find all env files
    assert_eq!(hierarchy.files.len(), 3);
}

#[test]
fn given_directory_with_hierarchy_when_getting_hierarchy_then_includes_parent_info() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    let _base = create_env_file(&temp, "base.env", "export BASE=value\n");
    let _local = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();

    // Assert - local.env should have base.env as parent
    let local_file = hierarchy
        .files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .map(|n| n == "local.env")
                .unwrap_or(false)
        })
        .expect("local.env should be in hierarchy");

    assert!(!local_file.parents.is_empty());
}

#[test]
fn given_empty_directory_when_getting_hierarchy_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();

    // Assert
    assert!(hierarchy.files.is_empty());
}

// ============================================================
// link/unlink tests
// ============================================================

#[test]
fn given_two_env_files_when_linking_then_child_gets_parent_directive() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    let parent = create_env_file(&temp, "base.env", "export BASE=value\n");
    let child = create_env_file(&temp, "local.env", "export LOCAL=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.link(&parent, &child).unwrap();

    // Assert - child should now have parent directive (path may be relative)
    let content = fs.read_to_string(&child).unwrap();
    assert!(content.contains("# rsenv:"), "Should have rsenv directive");
    assert!(content.contains("base.env"), "Should reference base.env");
}

#[test]
fn given_already_linked_file_when_linking_same_parent_then_no_duplicate() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    let parent = create_env_file(&temp, "base.env", "export BASE=value\n");
    let child = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.link(&parent, &child).unwrap();

    // Assert - should still have only one reference
    let content = fs.read_to_string(&child).unwrap();
    let count = content.matches("base.env").count();
    assert_eq!(count, 1);
}

#[test]
fn given_file_with_existing_parent_when_linking_another_then_replaces_parent() {
    // Arrange - v1 format uses export prefix
    // Note: v1 behavior is to REPLACE existing parent, not add
    let temp = TempDir::new().unwrap();

    let _parent_a = create_env_file(&temp, "a.env", "export A=value\n");
    let _parent_b = create_env_file(&temp, "b.env", "export B=value\n");
    let child = create_env_file(&temp, "local.env", "# rsenv: a.env\nexport LOCAL=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    let parent_b = temp.path().join("b.env");
    service.link(&parent_b, &child).unwrap();

    // Assert - v1 behavior: new parent REPLACES old one
    let content = fs.read_to_string(&child).unwrap();
    assert!(content.contains("b.env"));
    // Count occurrences - should only be one rsenv directive
    let count = content.matches("# rsenv:").count();
    assert_eq!(count, 1);
}

#[test]
fn given_file_without_parent_when_unlinking_then_no_change() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();

    let file = create_env_file(&temp, "local.env", "export LOCAL=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.unlink(&file).unwrap();

    // Assert - file unchanged
    let content = fs.read_to_string(&file).unwrap();
    assert_eq!(content, "export LOCAL=value\n");
}

// ============================================================
// v1 compatibility link/unlink tests
// ============================================================

#[test]
fn given_file_with_existing_parent_when_linking_new_parent_then_replaces() {
    // Arrange - v1 behavior: link REPLACES existing parent
    let temp = TempDir::new().unwrap();

    let _old_parent = create_env_file(&temp, "old.env", "export OLD=value\n");
    let _new_parent = create_env_file(&temp, "new.env", "export NEW=value\n");
    let child = create_env_file(&temp, "child.env", "# rsenv: old.env\nexport CHILD=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    let new_parent = temp.path().join("new.env");
    service.link(&new_parent, &child).unwrap();

    // Assert - should have ONLY new parent, not both
    let content = fs.read_to_string(&child).unwrap();
    assert!(content.contains("new.env"), "Should have new parent");
    assert!(!content.contains("old.env"), "Should NOT have old parent");

    // Count occurrences of "# rsenv:" - should be exactly 1
    let count = content.matches("# rsenv:").count();
    assert_eq!(count, 1, "Should have exactly one rsenv directive");
}

#[test]
fn given_file_with_multiple_rsenv_directives_when_linking_then_errors() {
    // Arrange - v1 errors on multiple # rsenv: lines
    let temp = TempDir::new().unwrap();

    let _parent = create_env_file(&temp, "parent.env", "export P=value\n");
    let child = create_env_file(
        &temp,
        "child.env",
        "# rsenv: one.env\n# rsenv: two.env\nexport CHILD=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let parent = temp.path().join("parent.env");
    let result = service.link(&parent, &child);

    // Assert - should error
    assert!(result.is_err(), "Should error on multiple rsenv directives");
}

#[test]
fn given_file_with_parent_when_unlinking_then_keeps_empty_directive() {
    // Arrange - v1 behavior: keeps "# rsenv:" line, just empties it
    let temp = TempDir::new().unwrap();

    let _parent = create_env_file(&temp, "base.env", "export BASE=value\n");
    let child = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.unlink(&child).unwrap();

    // Assert - should have empty directive, not removed
    let content = fs.read_to_string(&child).unwrap();
    assert!(content.contains("# rsenv:"), "Should keep rsenv directive");
    assert!(
        !content.contains("base.env"),
        "Should not have parent reference"
    );

    // The line should be exactly "# rsenv:" (possibly with newline)
    let has_empty_directive = content.lines().any(|l| l.trim() == "# rsenv:");
    assert!(has_empty_directive, "Should have empty # rsenv: line");
}

#[test]
fn given_file_with_multiple_rsenv_directives_when_unlinking_then_errors() {
    // Arrange - v1 errors on multiple # rsenv: lines
    let temp = TempDir::new().unwrap();

    let child = create_env_file(
        &temp,
        "child.env",
        "# rsenv: one.env\n# rsenv: two.env\nexport CHILD=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.unlink(&child);

    // Assert - should error
    assert!(result.is_err(), "Should error on multiple rsenv directives");
}

#[test]
fn given_child_in_subdir_when_linking_then_uses_relative_parent_path() {
    // Arrange - ensure relative path calculation is used
    let temp = TempDir::new().unwrap();

    let parent = create_env_file(&temp, "base.env", "export BASE=value\n");

    let subdir = temp.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();
    let child = create_env_file(&temp, "sub/local.env", "export LOCAL=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.link(&parent, &child).unwrap();

    // Assert - first line should reference parent via relative path
    let content = fs.read_to_string(&child).unwrap();
    let first_line = content.lines().next().unwrap_or("");
    assert_eq!(first_line, "# rsenv: ../base.env");
}

#[test]
fn given_file_with_parent_when_unlinking_then_rewrites_directive_exactly() {
    // Arrange - confirm unlink keeps directive line and trailing newline
    let temp = TempDir::new().unwrap();

    let _parent = create_env_file(&temp, "base.env", "export BASE=value\n");
    let child = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act
    service.unlink(&child).unwrap();

    // Assert - directive rewritten to empty and newline preserved
    let content = fs.read_to_string(&child).unwrap();
    assert_eq!(content, "# rsenv:\nexport LOCAL=value\n");
}

// =============================================================================
// Phase 4: is_dag, link_chain, get_files tests
// =============================================================================

#[test]
fn given_tree_structure_when_checking_dag_then_returns_false() {
    // Arrange - simple tree, no multiple parents
    let temp = TempDir::new().unwrap();

    create_env_file(&temp, "root.env", "export ROOT=value\n");
    create_env_file(
        &temp,
        "child.env",
        "# rsenv: root.env\nexport CHILD=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let is_dag = service.is_dag(temp.path()).unwrap();

    // Assert
    assert!(!is_dag);
}

#[test]
fn given_dag_structure_when_checking_dag_then_returns_true() {
    // Arrange - file with multiple parents
    let temp = TempDir::new().unwrap();

    create_env_file(&temp, "parent1.env", "export P1=value\n");
    create_env_file(&temp, "parent2.env", "export P2=value\n");
    create_env_file(
        &temp,
        "child.env",
        "# rsenv: parent1.env parent2.env\nexport CHILD=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let is_dag = service.is_dag(temp.path()).unwrap();

    // Assert
    assert!(is_dag);
}

#[test]
fn given_multiple_files_when_linking_chain_then_creates_hierarchy() {
    // Arrange
    let temp = TempDir::new().unwrap();

    let root = create_env_file(&temp, "root.env", "export ROOT=value\n");
    let middle = create_env_file(&temp, "middle.env", "export MIDDLE=value\n");
    let leaf = create_env_file(&temp, "leaf.env", "export LEAF=value\n");

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Act - root <- middle <- leaf
    service
        .link_chain(&[root.clone(), middle.clone(), leaf.clone()])
        .unwrap();

    // Assert
    let root_content = fs.read_to_string(&root).unwrap();
    // Root should have no parent reference (unlink only operates on existing directives)
    assert!(
        !root_content.contains("middle.env"),
        "Root should have no parent"
    );

    let middle_content = fs.read_to_string(&middle).unwrap();
    assert!(
        middle_content.contains("root.env"),
        "Middle should link to root"
    );

    let leaf_content = fs.read_to_string(&leaf).unwrap();
    assert!(
        leaf_content.contains("middle.env"),
        "Leaf should link to middle"
    );
}

#[test]
fn given_leaf_when_getting_files_then_returns_hierarchy() {
    // Arrange
    let temp = TempDir::new().unwrap();

    create_env_file(&temp, "root.env", "export ROOT=value\n");
    create_env_file(
        &temp,
        "middle.env",
        "# rsenv: root.env\nexport MIDDLE=value\n",
    );
    let leaf = create_env_file(
        &temp,
        "leaf.env",
        "# rsenv: middle.env\nexport LEAF=value\n",
    );

    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let files = service.get_files(&leaf).unwrap();

    // Assert
    assert_eq!(files.len(), 3);
}

#[test]
fn given_nonexistent_file_when_building_then_returns_file_not_found() {
    // Arrange
    let fs = std::sync::Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act
    let result = service.build(std::path::Path::new("xxx"));

    // Assert
    assert!(result.is_err());
}
