//! Tests for TreeBuilder

use std::path::PathBuf;
use tempfile::TempDir;

use rsenv::domain::TreeBuilder;

fn create_env_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, content).expect("write env file");
    path
}

#[test]
fn given_directory_with_hierarchy_when_building_then_creates_tree() {
    // Arrange
    let temp = TempDir::new().unwrap();

    create_env_file(&temp, "root.env", "export ROOT=value\n");
    create_env_file(
        &temp,
        "child.env",
        "# rsenv: root.env\nexport CHILD=value\n",
    );

    // Act
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(temp.path()).unwrap();

    // Assert
    assert_eq!(trees.len(), 1);
    assert!(trees[0].root().is_some());
}

#[test]
fn given_directory_with_multiple_trees_when_building_then_creates_all() {
    // Arrange
    let temp = TempDir::new().unwrap();

    // Tree 1
    create_env_file(&temp, "root1.env", "export ROOT1=value\n");
    create_env_file(
        &temp,
        "child1.env",
        "# rsenv: root1.env\nexport CHILD1=value\n",
    );

    // Tree 2 (standalone)
    create_env_file(&temp, "standalone.env", "export STANDALONE=value\n");

    // Act
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(temp.path()).unwrap();

    // Assert
    assert_eq!(trees.len(), 2);
}

#[test]
fn given_directory_with_cycle_when_building_then_errors() {
    // Arrange
    let temp = TempDir::new().unwrap();

    create_env_file(&temp, "a.env", "# rsenv: b.env\nexport A=value\n");
    create_env_file(&temp, "b.env", "# rsenv: a.env\nexport B=value\n");

    // Act
    let mut builder = TreeBuilder::new();
    let result = builder.build_from_directory(temp.path());

    // Assert
    assert!(result.is_err());
}

#[test]
fn given_nonexistent_directory_when_building_then_errors() {
    // Arrange
    let mut builder = TreeBuilder::new();

    // Act
    let result = builder.build_from_directory(&PathBuf::from("/nonexistent/path"));

    // Assert
    assert!(result.is_err());
}
