//! Tests for envrc module

use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

use rsenv::application::envrc::{
    delete_section, update_dot_envrc, END_SECTION_DELIMITER, START_SECTION_DELIMITER,
};
use rsenv::application::services::EnvironmentService;
use rsenv::infrastructure::traits::{FileSystem, RealFileSystem};

#[test]
fn given_empty_envrc_when_updating_then_adds_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");
    std::fs::write(&envrc_path, "").unwrap();

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let data = "export FOO=bar\nexport BAZ=qux\n";

    // Act
    update_dot_envrc(&fs, &envrc_path, data).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(content.contains(START_SECTION_DELIMITER));
    assert!(content.contains(END_SECTION_DELIMITER));
    assert!(content.contains(data));
}

#[test]
fn given_envrc_with_section_when_deleting_then_removes_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");

    let initial_content = format!(
        "# pre-existing content\n{}\nexport FOO=bar\n{}\n# more content\n",
        START_SECTION_DELIMITER, END_SECTION_DELIMITER
    );
    std::fs::write(&envrc_path, &initial_content).unwrap();

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);

    // Act
    delete_section(&fs, &envrc_path).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(!content.contains(START_SECTION_DELIMITER));
    assert!(!content.contains(END_SECTION_DELIMITER));
    assert!(content.contains("pre-existing"));
}

#[test]
fn given_envrc_with_section_when_updating_then_replaces_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");

    let initial_content = format!(
        "# pre-existing\n{}\nexport OLD=value\n{}\n",
        START_SECTION_DELIMITER, END_SECTION_DELIMITER
    );
    std::fs::write(&envrc_path, &initial_content).unwrap();

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let new_data = "export NEW=value\n";

    // Act
    update_dot_envrc(&fs, &envrc_path, new_data).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(content.contains("NEW=value"));
    assert!(!content.contains("OLD=value"));
    // Should have exactly one pair of delimiters
    assert_eq!(content.matches(START_SECTION_DELIMITER).count(), 1);
    assert_eq!(content.matches(END_SECTION_DELIMITER).count(), 1);
}

#[test]
fn given_nonexistent_envrc_when_updating_then_creates_file() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let data = "export FOO=bar\n";

    // Act
    update_dot_envrc(&fs, &envrc_path, data).unwrap();

    // Assert
    assert!(envrc_path.exists());
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(content.contains(data));
}

// ============================================================
// Integration Tests (using EnvironmentService like v1's build_env_vars)
// ============================================================

/// Helper to format variables as export statements (v1 format)
fn format_exports(vars: &std::collections::BTreeMap<String, String>) -> String {
    vars.iter()
        .map(|(k, v)| format!("export {}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn given_v1_complex_hierarchy_when_updating_envrc_then_adds_merged_exports() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");
    std::fs::write(&envrc_path, "# pre-existing content\n").unwrap();

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // Build merged variables from v1 test fixture
    let result = service
        .build(Path::new("tests/resources/environments/complex/level4.env"))
        .unwrap();
    let data = format_exports(&result.variables);

    // Act
    update_dot_envrc(&fs, &envrc_path, &data).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(content.contains(START_SECTION_DELIMITER));
    assert!(content.contains(END_SECTION_DELIMITER));
    assert!(content.contains("export VAR_6=var_64"));
    assert!(content.contains("export VAR_7=var_74"));
    assert!(content.contains("pre-existing")); // Original content preserved
}

#[test]
fn given_v1_envrc_with_section_when_deleting_then_removes_merged_exports() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // First, build and add the section
    let result = service
        .build(Path::new("tests/resources/environments/complex/level4.env"))
        .unwrap();
    let data = format_exports(&result.variables);

    std::fs::write(
        &envrc_path,
        format!(
            "# header\n{}\n{}\n{}\n# footer\n",
            START_SECTION_DELIMITER, data, END_SECTION_DELIMITER
        ),
    )
    .unwrap();

    // Act
    delete_section(&fs, &envrc_path).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(!content.contains(START_SECTION_DELIMITER));
    assert!(!content.contains("VAR_6"));
    assert!(content.contains("header"));
    assert!(content.contains("footer"));
}

#[test]
fn given_v1_multiple_updates_when_updating_envrc_then_maintains_single_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let envrc_path = temp.path().join(".envrc");
    std::fs::write(&envrc_path, "").unwrap();

    let fs: Arc<dyn FileSystem> = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs.clone());

    // First update with level4
    let result1 = service
        .build(Path::new("tests/resources/environments/complex/level4.env"))
        .unwrap();
    let data1 = format_exports(&result1.variables);
    update_dot_envrc(&fs, &envrc_path, &data1).unwrap();

    // Second update with level3 (should replace first)
    let result2 = service
        .build(Path::new(
            "tests/resources/environments/complex/a/level3.env",
        ))
        .unwrap();
    let data2 = format_exports(&result2.variables);
    update_dot_envrc(&fs, &envrc_path, &data2).unwrap();

    // Assert
    let content = std::fs::read_to_string(&envrc_path).unwrap();

    // Should contain level3's data, not level4's
    // Level4 has VAR_6=var_64, VAR_7=var_74
    // Level3 has different values
    assert!(content.contains(&data2), "Should contain level3 data");

    // Should only have one set of delimiters
    assert_eq!(content.matches(START_SECTION_DELIMITER).count(), 1);
    assert_eq!(content.matches(END_SECTION_DELIMITER).count(), 1);
}
