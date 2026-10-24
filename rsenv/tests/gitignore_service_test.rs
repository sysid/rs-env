//! Tests for GitignoreService
//!
//! Tests the two-tier gitignore management:
//! - Global gitignore at base_dir
//! - Per-vault gitignore (only if vault has local config)

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::GitignoreService;
use rsenv::config::{Settings, SopsConfig};
use rsenv::infrastructure::traits::RealFileSystem;

/// Helper to create test settings with custom SOPS config.
fn test_settings(base_dir: PathBuf, sops: SopsConfig) -> Settings {
    Settings {
        base_dir,
        editor: "vim".to_string(),
        sops,
    }
}

/// Helper to create a default SopsConfig for testing.
fn test_sops_config() -> SopsConfig {
    SopsConfig {
        gpg_key: Some("test-key".to_string()),
        age_key: None,
        file_extensions_enc: vec!["env".into(), "envrc".into()],
        file_names_enc: vec!["dot_pypirc".into()],
        file_extensions_dec: vec!["enc".into()],
        file_names_dec: vec![],
        encrypt_on_commit: true,
    }
}

// ============================================================
// Pattern extraction tests
// ============================================================

#[test]
fn given_sops_config_when_getting_global_patterns_then_returns_expected() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let patterns = service.global_patterns();

    // Assert
    assert!(patterns.contains("*.env"));
    assert!(patterns.contains("*.envrc"));
    assert!(patterns.contains("dot_pypirc"));
    assert_eq!(patterns.len(), 3);
}

#[test]
fn given_empty_sops_config_when_getting_patterns_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let sops = SopsConfig {
        gpg_key: None,
        age_key: None,
        file_extensions_enc: vec![],
        file_names_enc: vec![],
        file_extensions_dec: vec![],
        file_names_dec: vec![],
        encrypt_on_commit: true,
    };
    let settings = test_settings(temp.path().to_path_buf(), sops);
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let patterns = service.global_patterns();

    // Assert
    assert!(patterns.is_empty());
}

// ============================================================
// Status tests
// ============================================================

#[test]
fn given_no_gitignore_when_status_then_shows_patterns_to_add() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let status = service.status(None).unwrap();

    // Assert
    assert!(!status.global_diff.in_sync);
    assert_eq!(status.global_diff.to_add.len(), 3);
    assert!(status.global_diff.to_remove.is_empty());
}

#[test]
fn given_synced_gitignore_when_status_then_shows_in_sync() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# rsenv-managed start
# Source: global config
*.env
*.envrc
dot_pypirc
# rsenv-managed end"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let status = service.status(None).unwrap();

    // Assert
    assert!(status.global_diff.in_sync);
    assert!(status.global_diff.to_add.is_empty());
    assert!(status.global_diff.to_remove.is_empty());
}

#[test]
fn given_partial_gitignore_when_status_then_shows_diff() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# rsenv-managed start
*.env
old_pattern
# rsenv-managed end"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let status = service.status(None).unwrap();

    // Assert
    assert!(!status.global_diff.in_sync);
    // Missing: *.envrc, dot_pypirc
    assert_eq!(status.global_diff.to_add.len(), 2);
    assert!(status.global_diff.to_add.contains(&"*.envrc".to_string()));
    assert!(status
        .global_diff
        .to_add
        .contains(&"dot_pypirc".to_string()));
    // Extra: old_pattern
    assert_eq!(status.global_diff.to_remove.len(), 1);
    assert!(status
        .global_diff
        .to_remove
        .contains(&"old_pattern".to_string()));
}

// ============================================================
// Sync tests
// ============================================================

#[test]
fn given_no_gitignore_when_sync_global_then_creates_file() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let diff = service.sync_global().unwrap();

    // Assert
    assert!(!diff.in_sync); // Was out of sync before
    assert!(gitignore_path.exists());
    let content = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("# rsenv-managed start"));
    assert!(content.contains("*.env"));
    assert!(content.contains("*.envrc"));
    assert!(content.contains("dot_pypirc"));
    assert!(content.contains("# rsenv-managed end"));
    assert!(content.contains("!*.enc")); // Negation for encrypted files
}

#[test]
fn given_existing_gitignore_when_sync_then_preserves_user_content() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let user_content = "# My custom ignores\n.idea/\nnode_modules/\n";
    std::fs::write(&gitignore_path, user_content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    service.sync_global().unwrap();

    // Assert
    let content = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("# My custom ignores"));
    assert!(content.contains(".idea/"));
    assert!(content.contains("node_modules/"));
    assert!(content.contains("# rsenv-managed start"));
    assert!(content.contains("*.env"));
}

#[test]
fn given_gitignore_with_managed_section_when_sync_then_updates_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# User content
.idea/

# rsenv-managed start
old_pattern
# rsenv-managed end

# More user content
.DS_Store"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    service.sync_global().unwrap();

    // Assert
    let new_content = std::fs::read_to_string(&gitignore_path).unwrap();
    // User content preserved
    assert!(new_content.contains("# User content"));
    assert!(new_content.contains(".idea/"));
    assert!(new_content.contains(".DS_Store"));
    // Old pattern removed, new patterns added
    assert!(!new_content.contains("old_pattern"));
    assert!(new_content.contains("*.env"));
    assert!(new_content.contains("*.envrc"));
    assert!(new_content.contains("dot_pypirc"));
}

#[test]
fn given_already_synced_when_sync_then_no_changes() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# rsenv-managed start
# Source: global config
*.env
*.envrc
dot_pypirc
# rsenv-managed end"#;
    std::fs::write(&gitignore_path, content).unwrap();
    let original_mtime = std::fs::metadata(&gitignore_path)
        .unwrap()
        .modified()
        .unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let diff = service.sync_global().unwrap();

    // Assert
    assert!(diff.in_sync);
    // File shouldn't have been rewritten (mtime unchanged)
    let new_mtime = std::fs::metadata(&gitignore_path)
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(original_mtime, new_mtime);
}

// ============================================================
// Vault-local config tests
// ============================================================

#[test]
fn given_vault_without_local_config_when_vault_patterns_then_returns_none() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let patterns = service.vault_patterns(&vault_dir).unwrap();

    // Assert
    assert!(patterns.is_none());
}

#[test]
fn given_vault_with_local_config_when_vault_patterns_then_returns_patterns() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();

    // Create vault-local config
    let vault_config = vault_dir.join(".rsenv.toml");
    let config_content = r#"
[sops]
file_extensions_enc = ["yaml", "json"]
file_names_enc = ["secrets.txt"]
"#;
    std::fs::write(&vault_config, config_content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let patterns = service.vault_patterns(&vault_dir).unwrap();

    // Assert
    assert!(patterns.is_some());
    let patterns = patterns.unwrap();
    assert!(patterns.contains("*.yaml"));
    assert!(patterns.contains("*.json"));
    assert!(patterns.contains("secrets.txt"));
}

#[test]
fn given_vault_without_local_config_when_sync_vault_then_returns_none() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let diff = service.sync_vault(&vault_dir).unwrap();

    // Assert
    assert!(diff.is_none());
    // No .gitignore should be created in vault
    assert!(!vault_dir.join(".gitignore").exists());
}

#[test]
fn given_vault_with_local_config_when_sync_vault_then_creates_gitignore() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();

    // Create vault-local config
    let vault_config = vault_dir.join(".rsenv.toml");
    let config_content = r#"
[sops]
file_extensions_enc = ["yaml"]
file_names_enc = ["secrets.txt"]
"#;
    std::fs::write(&vault_config, config_content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let diff = service.sync_vault(&vault_dir).unwrap();

    // Assert
    assert!(diff.is_some());
    let diff = diff.unwrap();
    assert!(!diff.in_sync); // Was out of sync before

    let gitignore_path = vault_dir.join(".gitignore");
    assert!(gitignore_path.exists());
    let content = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("*.yaml"));
    assert!(content.contains("secrets.txt"));
}

/// The vault tier holds only what the vault file itself lists: arrays the vault
/// does not set are inherited through the parent `.gitignore`, not copied from
/// compiled defaults.
#[test]
fn given_vault_config_setting_only_file_names_when_vault_patterns_then_no_default_extensions() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::write(
        vault_dir.join(".rsenv.toml"),
        "[sops]\nfile_names_enc = [\"xxx\"]\n",
    )
    .unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    let patterns = service.vault_patterns(&vault_dir).unwrap().unwrap();

    // Assert
    let expected: std::collections::BTreeSet<String> = ["xxx".to_string()].into();
    assert_eq!(
        patterns, expected,
        "no *.env / *.envrc from compiled defaults"
    );
}

/// `!x` in the vault config removes an inherited item. The parent `.gitignore`
/// still ignores it, so the vault must un-ignore it explicitly with git's `!`.
#[test]
fn given_vault_negations_when_vault_patterns_then_git_unignore_lines() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::write(
        vault_dir.join(".rsenv.toml"),
        r#"
[sops]
file_extensions_enc = ["yaml", "!env"]
file_names_enc = ["!dot_pypirc"]
"#,
    )
    .unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    let patterns = service.vault_patterns(&vault_dir).unwrap().unwrap();

    // Assert
    let expected: std::collections::BTreeSet<String> = [
        "*.yaml".to_string(),
        "!*.env".to_string(),
        "!dot_pypirc".to_string(),
    ]
    .into();
    assert_eq!(patterns, expected);
}

#[test]
fn given_vault_negation_when_sync_vault_then_gitignore_unignores_instead_of_literal() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::write(
        vault_dir.join(".rsenv.toml"),
        "[sops]\nfile_extensions_enc = [\"!env\"]\n",
    )
    .unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    service.sync_vault(&vault_dir).unwrap();

    // Assert
    let content = std::fs::read_to_string(vault_dir.join(".gitignore")).unwrap();
    assert!(
        content.lines().any(|l| l == "!*.env"),
        "content:\n{content}"
    );
    assert!(!content.contains("*.!env"), "content:\n{content}");
}

/// A reader of the vault `.gitignore` must learn that it is only the vault's
/// additions and that the rest lives in the parent `.gitignore`.
#[test]
fn given_vault_config_when_sync_vault_then_header_points_to_parent_gitignore() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::write(
        vault_dir.join(".rsenv.toml"),
        "[sops]\nfile_names_enc = [\"xxx\"]\n",
    )
    .unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    service.sync_vault(&vault_dir).unwrap();

    // Assert
    let content = std::fs::read_to_string(vault_dir.join(".gitignore")).unwrap();
    assert!(
        content.contains(
            "# Source: vault-local config only; global patterns come from the rsenv base-dir .gitignore"
        ),
        "content:\n{content}"
    );
}

// ============================================================
// sync_all tests
// ============================================================

#[test]
fn given_vault_when_sync_all_then_syncs_global_and_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_dir = temp.path().join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();

    // Create vault-local config
    let vault_config = vault_dir.join(".rsenv.toml");
    let config_content = r#"
[sops]
file_extensions_enc = ["yaml"]
"#;
    std::fs::write(&vault_config, config_content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let (global_diff, vault_diff) = service.sync_all(Some(&vault_dir)).unwrap();

    // Assert
    assert!(!global_diff.in_sync); // Global was out of sync
    assert!(vault_diff.is_some());
    assert!(!vault_diff.unwrap().in_sync); // Vault was out of sync

    // Both files created
    assert!(temp.path().join(".gitignore").exists());
    assert!(vault_dir.join(".gitignore").exists());
}

// ============================================================
// Clean tests
// ============================================================

#[test]
fn given_gitignore_with_managed_section_when_clean_then_removes_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# User content
.idea/

# rsenv-managed start
*.env
*.envrc
# rsenv-managed end

# More user content
.DS_Store"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let cleaned = service.clean_global().unwrap();

    // Assert
    assert!(cleaned);
    let new_content = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(new_content.contains("# User content"));
    assert!(new_content.contains(".idea/"));
    assert!(new_content.contains(".DS_Store"));
    assert!(!new_content.contains("# rsenv-managed start"));
    assert!(!new_content.contains("*.env"));
    assert!(!new_content.contains("# rsenv-managed end"));
}

#[test]
fn given_gitignore_without_managed_section_when_clean_then_no_changes() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = "# User content\n.idea/\n";
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let cleaned = service.clean_global().unwrap();

    // Assert
    assert!(!cleaned);
}

#[test]
fn given_no_gitignore_when_clean_then_returns_false() {
    // Arrange
    let temp = TempDir::new().unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let cleaned = service.clean_global().unwrap();

    // Assert
    assert!(!cleaned);
}

// ============================================================
// is_synced tests
// ============================================================

#[test]
fn given_synced_global_when_is_global_synced_then_returns_true() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# rsenv-managed start
*.env
*.envrc
dot_pypirc
# rsenv-managed end"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let synced = service.is_global_synced().unwrap();

    // Assert
    assert!(synced);
}

#[test]
fn given_unsynced_global_when_is_global_synced_then_returns_false() {
    // Arrange
    let temp = TempDir::new().unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let synced = service.is_global_synced().unwrap();

    // Assert
    assert!(!synced);
}

// ============================================================
// Edge case tests
// ============================================================

#[test]
fn given_gitignore_with_comments_in_managed_section_when_extract_then_ignores_comments() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let gitignore_path = temp.path().join(".gitignore");
    let content = r#"# rsenv-managed start
# This is a comment
*.env
# Another comment
*.envrc
# rsenv-managed end"#;
    std::fs::write(&gitignore_path, content).unwrap();

    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    let status = service.status(None).unwrap();

    // Assert - Should only see the two patterns, not comments
    // Missing: dot_pypirc
    assert!(!status.global_diff.in_sync);
    assert_eq!(status.global_diff.to_add.len(), 1);
    assert!(status
        .global_diff
        .to_add
        .contains(&"dot_pypirc".to_string()));
}

#[test]
fn given_patterns_are_sorted_when_sync_then_output_is_deterministic() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let settings = test_settings(temp.path().to_path_buf(), test_sops_config());
    let fs = Arc::new(RealFileSystem);
    let service = GitignoreService::new(fs, settings);

    // Act
    service.sync_global().unwrap();

    // Assert - patterns should be alphabetically sorted
    let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    // Find the pattern lines (after start marker, before end marker)
    let start_idx = lines
        .iter()
        .position(|l| l.contains("rsenv-managed start"))
        .unwrap();
    let end_idx = lines
        .iter()
        .position(|l| l.contains("rsenv-managed end"))
        .unwrap();

    // Get pattern lines (skip comments)
    let pattern_lines: Vec<&&str> = lines[start_idx + 1..end_idx]
        .iter()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();

    // Should be sorted
    assert_eq!(*pattern_lines[0], "*.env");
    assert_eq!(*pattern_lines[1], "*.envrc");
    assert_eq!(*pattern_lines[2], "dot_pypirc");
}

// ============================================================
// Plain-directory gitignore (`--dir`): a separate repo outside base_dir,
// so it needs ALL global patterns - no parent .gitignore covers it.
// ============================================================

#[test]
fn given_plain_dir_when_sync_dir_then_writes_all_global_patterns() {
    // Arrange
    let base = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    let diff = service.sync_dir(store.path()).unwrap();

    // Assert
    assert!(!diff.in_sync, "was out of sync before");
    let content = std::fs::read_to_string(store.path().join(".gitignore")).unwrap();
    for pattern in ["*.env", "*.envrc", "dot_pypirc"] {
        assert!(
            content.lines().any(|l| l == pattern),
            "{pattern} missing:\n{content}"
        );
    }
    assert!(
        !base.path().join(".gitignore").exists(),
        "base-dir .gitignore untouched"
    );
}

/// Hand-written rules and foreign generated sections (e.g. confguard's) stay as they are.
#[test]
fn given_dir_gitignore_with_foreign_lines_when_sync_dir_then_foreign_lines_kept() {
    // Arrange
    let base = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let foreign = "# over-rule global gitignore\n!.run\n\
        # ---------------------------------- confguard-start -----------------------------------\n\
        restic.pw  # sops-managed 2026-10-20 14:31:31\n\
        # ---------------------------------- confguard-end -----------------------------------\n";
    std::fs::write(store.path().join(".gitignore"), foreign).unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    service.sync_dir(store.path()).unwrap();

    // Assert
    let content = std::fs::read_to_string(store.path().join(".gitignore")).unwrap();
    assert!(
        content.starts_with(foreign.trim_end()),
        "foreign content kept in place:\n{content}"
    );
    assert!(content.contains("# rsenv-managed start"), "{content}");
}

#[test]
fn given_synced_dir_when_sync_dir_again_then_in_sync_and_file_unchanged() {
    // Arrange
    let base = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);
    service.sync_dir(store.path()).unwrap();
    let first = std::fs::read_to_string(store.path().join(".gitignore")).unwrap();

    // Act
    let diff = service.sync_dir(store.path()).unwrap();

    // Assert
    assert!(diff.in_sync);
    let second = std::fs::read_to_string(store.path().join(".gitignore")).unwrap();
    assert_eq!(first, second, "no rewrite (timestamp would churn)");
}

#[test]
fn given_dir_with_stale_section_when_status_dir_then_reports_diff_without_writing() {
    // Arrange
    let base = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    let stale = "# rsenv-managed start\n*.env\nold_name\n# rsenv-managed end\n";
    std::fs::write(store.path().join(".gitignore"), stale).unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    let diff = service.status_dir(store.path()).unwrap();

    // Assert
    assert!(!diff.in_sync);
    assert_eq!(
        diff.to_add,
        vec!["*.envrc".to_string(), "dot_pypirc".to_string()]
    );
    assert_eq!(diff.to_remove, vec!["old_name".to_string()]);
    assert_eq!(
        std::fs::read_to_string(store.path().join(".gitignore")).unwrap(),
        stale
    );
}

#[test]
fn given_dir_with_managed_section_when_clean_dir_then_only_section_removed() {
    // Arrange
    let base = TempDir::new().unwrap();
    let store = TempDir::new().unwrap();
    std::fs::write(
        store.path().join(".gitignore"),
        "!.run\n# rsenv-managed start\n*.env\n# rsenv-managed end\n",
    )
    .unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act
    let cleaned = service.clean_dir(store.path()).unwrap();

    // Assert
    assert!(cleaned);
    assert_eq!(
        std::fs::read_to_string(store.path().join(".gitignore")).unwrap(),
        "!.run"
    );
}

#[test]
fn given_dir_gitignore_when_has_managed_section_then_true_only_with_rsenv_markers() {
    // Arrange
    let base = TempDir::new().unwrap();
    let with = TempDir::new().unwrap();
    let without = TempDir::new().unwrap();
    let missing = TempDir::new().unwrap();
    std::fs::write(
        with.path().join(".gitignore"),
        "# rsenv-managed start\n*.env\n# rsenv-managed end\n",
    )
    .unwrap();
    std::fs::write(without.path().join(".gitignore"), "*.log\n").unwrap();
    let settings = test_settings(base.path().to_path_buf(), test_sops_config());
    let service = GitignoreService::new(Arc::new(RealFileSystem), settings);

    // Act / Assert
    assert!(service.has_managed_section(with.path()).unwrap());
    assert!(!service.has_managed_section(without.path()).unwrap());
    assert!(!service.has_managed_section(missing.path()).unwrap());
}
