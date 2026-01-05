//! Tests for SopsService

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::SopsService;
use rsenv::config::{Settings, SopsConfig};
use rsenv::infrastructure::traits::{RealCommandRunner, RealFileSystem};

/// Helper to create test settings with custom SOPS config
fn test_settings(vault_base_dir: PathBuf, sops: SopsConfig) -> Settings {
    Settings {
        vault_base_dir,
        editor: "vim".to_string(),
        sops,
    }
}

/// Helper to create a default SopsConfig for testing
fn test_sops_config() -> SopsConfig {
    SopsConfig {
        gpg_key: Some("test-key".to_string()),
        age_key: None,
        file_extensions_enc: vec!["env".into(), "envrc".into()],
        file_names_enc: vec!["dot_pypirc".into()],
        file_extensions_dec: vec!["enc".into()],
        file_names_dec: vec![],
    }
}

// ============================================================
// collect_files() tests
// ============================================================

#[test]
fn given_files_matching_extension_when_collect_then_returns_matches() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Create files with various extensions
    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();
    std::fs::write(dir.join("local.envrc"), "export X=1").unwrap();
    std::fs::write(dir.join("readme.txt"), "ignored").unwrap();
    std::fs::write(dir.join("data.json"), "{}").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service
        .collect_files(dir, &["env".into(), "envrc".into()], &[])
        .unwrap();

    // Assert
    assert_eq!(files.len(), 2);
    let names: Vec<_> = files
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"config.env".to_string()));
    assert!(names.contains(&"local.envrc".to_string()));
}

#[test]
fn given_files_matching_exact_name_when_collect_then_returns_matches() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(dir.join("dot_pypirc"), "[pypi]").unwrap();
    std::fs::write(dir.join("dot_pgpass"), "localhost:5432").unwrap();
    std::fs::write(dir.join("other.txt"), "ignored").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service
        .collect_files(dir, &[], &["dot_pypirc".into(), "dot_pgpass".into()])
        .unwrap();

    // Assert
    assert_eq!(files.len(), 2);
}

#[test]
fn given_nested_files_when_collect_then_finds_recursively() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let subdir = dir.join("config").join("secrets");
    std::fs::create_dir_all(&subdir).unwrap();

    std::fs::write(dir.join("root.env"), "ROOT=1").unwrap();
    std::fs::write(subdir.join("nested.env"), "NESTED=1").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service.collect_files(dir, &["env".into()], &[]).unwrap();

    // Assert
    assert_eq!(files.len(), 2);
}

#[test]
fn given_empty_patterns_when_collect_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let files = service.collect_files(dir, &[], &[]).unwrap();

    // Assert
    assert!(files.is_empty());
}

// ============================================================
// status() tests
// ============================================================

#[test]
fn given_mixed_files_when_status_then_categorizes_correctly() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Pending encryption (matches enc patterns, not encrypted yet)
    std::fs::write(dir.join("config.env"), "KEY=value").unwrap();
    // Already encrypted
    std::fs::write(dir.join("secret.env.enc"), "encrypted-data").unwrap();
    // Not matching any pattern
    std::fs::write(dir.join("readme.txt"), "ignored").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert
    assert_eq!(status.pending_encrypt.len(), 1);
    assert_eq!(status.encrypted.len(), 1);
    // pending_clean shows plaintext files that have .enc counterpart
    // In this case config.env doesn't have config.env.enc, so pending_clean = 0
    // But secret.env would be pending_clean if it existed alongside secret.env.enc
}

#[test]
fn given_plaintext_with_enc_counterpart_when_status_then_marks_pending_clean() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    // Both plaintext and encrypted exist
    std::fs::write(dir.join("secret.env"), "KEY=plaintext").unwrap();
    std::fs::write(dir.join("secret.env.enc"), "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let status = service.status(Some(dir)).unwrap();

    // Assert - secret.env should be pending_clean since secret.env.enc exists
    assert_eq!(status.pending_clean.len(), 1);
    assert!(status.pending_clean[0].ends_with("secret.env"));
}

// ============================================================
// clean() tests
// ============================================================

#[test]
fn given_plaintext_with_enc_when_clean_then_deletes_plaintext() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    let plaintext = dir.join("secret.env");
    let encrypted = dir.join("secret.env.enc");
    std::fs::write(&plaintext, "KEY=plaintext").unwrap();
    std::fs::write(&encrypted, "encrypted-data").unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert
    assert_eq!(deleted.len(), 1);
    assert!(!plaintext.exists()); // Plaintext deleted
    assert!(encrypted.exists()); // Encrypted still exists
}

#[test]
fn given_plaintext_without_enc_when_clean_then_keeps_plaintext() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    let plaintext = dir.join("config.env");
    std::fs::write(&plaintext, "KEY=value").unwrap();
    // No .enc counterpart

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    let deleted = service.clean(Some(dir)).unwrap();

    // Assert
    assert!(deleted.is_empty());
    assert!(plaintext.exists()); // Plaintext preserved
}

// ============================================================
// gitignore tests
// ============================================================

#[test]
fn given_no_gitignore_when_update_then_creates_with_patterns() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    service.update_gitignore(dir).unwrap();

    // Assert
    let content = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(content.contains("# rsenv-managed start"));
    assert!(content.contains("*.env"));
    assert!(content.contains("*.envrc"));
    assert!(content.contains("dot_pypirc"));
    assert!(content.contains("# rsenv-managed end"));
}

#[test]
fn given_existing_gitignore_when_update_then_preserves_other_content() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(
        dir.join(".gitignore"),
        "# My gitignore\nnode_modules/\ntarget/\n",
    )
    .unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    service.update_gitignore(dir).unwrap();

    // Assert
    let content = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(content.contains("node_modules/"));
    assert!(content.contains("target/"));
    assert!(content.contains("# rsenv-managed start"));
}

#[test]
fn given_managed_section_when_clean_gitignore_then_removes_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    std::fs::write(
        dir.join(".gitignore"),
        "node_modules/\n\n# rsenv-managed start\n*.env\n# rsenv-managed end\ntarget/\n",
    )
    .unwrap();

    let settings = Arc::new(test_settings(dir.to_path_buf(), test_sops_config()));
    let fs = Arc::new(RealFileSystem);
    let cmd = Arc::new(RealCommandRunner);
    let service = SopsService::new(fs, cmd, settings);

    // Act
    service.clean_gitignore(dir).unwrap();

    // Assert
    let content = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(!content.contains("# rsenv-managed start"));
    assert!(!content.contains("*.env"));
    assert!(content.contains("node_modules/"));
    assert!(content.contains("target/"));
}
