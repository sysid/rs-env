//! Tests for SwapService
//!
//! These tests verify the correct swap behavior matching rplc:
//! - Sentinel files go to vault: `vault/swap/<rel_path>.<hostname>.rsenv_active`
//! - Backup files go to vault: `vault/swap/<rel_path>.rsenv_original`
//! - Sentinel contains copy of vault content (not empty)
//! - swap_in MOVES vault to project (vault file removed)
//! - swap_out MOVES modifications back to vault

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::{SwapService, VaultService};
use rsenv::config::Settings;
use rsenv::domain::SwapState;
use rsenv::infrastructure::traits::RealFileSystem;

/// Helper to create test settings with custom base_dir
fn test_settings(base_dir: PathBuf) -> Settings {
    Settings {
        base_dir,
        editor: "vim".to_string(),
        sops: Default::default(),
    }
}

/// Helper to set up a project with vault
fn setup_project(temp: &TempDir) -> (PathBuf, PathBuf, Arc<Settings>) {
    let base_dir = temp.path().to_path_buf();
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(base_dir));
    let fs = Arc::new(RealFileSystem);
    let vault_service = VaultService::new(fs, settings.clone());
    let vault = vault_service.init(&project_dir, false).unwrap();

    (project_dir, vault.path, settings)
}

/// Get the current hostname
fn get_hostname() -> String {
    hostname::get().unwrap().to_string_lossy().to_string()
}

// ============================================================
// swap_in() tests
// ============================================================

#[test]
fn given_file_with_vault_override_when_swap_in_then_replaces_project_file() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create original file in project
    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    // Create override in vault's swap directory
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let vault_file = swap_dir.join("config.yml");
    std::fs::write(&vault_file, "override: dev-value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let swapped = service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);
    assert_eq!(
        swapped[0].state,
        SwapState::In {
            hostname: hostname.clone()
        }
    );

    // Project file should have override content
    let content = std::fs::read_to_string(&project_file).unwrap();
    assert!(
        content.contains("override: dev-value"),
        "project file should have override content"
    );

    // Vault file should be GONE (moved, not copied)
    assert!(
        !vault_file.exists(),
        "vault file should be moved, not copied"
    );

    // Backup should be in VAULT, not project
    let backup_in_vault = swap_dir.join("config.yml.rsenv_original");
    assert!(backup_in_vault.exists(), "backup should be in vault");
    let backup_content = std::fs::read_to_string(&backup_in_vault).unwrap();
    assert!(
        backup_content.contains("original: value"),
        "backup should contain original content"
    );

    // Sentinel should be in VAULT with content (copy of vault before move)
    let sentinel_in_vault = swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname));
    assert!(sentinel_in_vault.exists(), "sentinel should be in vault");
    let sentinel_content = std::fs::read_to_string(&sentinel_in_vault).unwrap();
    assert!(
        sentinel_content.contains("override: dev-value"),
        "sentinel should be copy of vault content"
    );

    // Old wrong locations should NOT exist
    let wrong_backup = project_dir.join("config.yml.rsenv_original");
    assert!(
        !wrong_backup.exists(),
        "backup should NOT be in project dir"
    );
    let wrong_sentinel = project_dir.join(format!("config.yml@@{}@@rsenv_active", hostname));
    assert!(
        !wrong_sentinel.exists(),
        "sentinel should NOT be in project dir"
    );
}

#[test]
fn given_already_swapped_file_when_swap_in_then_succeeds_idempotently() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in twice
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();
    let result = service.swap_in(&project_dir, &[project_file]);

    // Assert - second swap should succeed (idempotent) with empty result
    assert!(result.is_ok());
    assert!(
        result.unwrap().is_empty(),
        "no files should be swapped on second call"
    );
}

#[test]
fn given_swapped_by_different_host_when_swap_in_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    // Create sentinel file from different host IN VAULT (correct location)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let sentinel = swap_dir.join("config.yml@@other-host@@rsenv_active");
    std::fs::write(&sentinel, "override content").unwrap();

    // Also need vault file for swap_in to find
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_in(&project_dir, &[project_file]);

    // Assert - should fail due to different host
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("other-host"),
        "error should mention the other host"
    );
}

// ============================================================
// swap_out() tests
// ============================================================

#[test]
fn given_swapped_file_when_swap_out_then_restores_original() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in first
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Act - swap out
    let swapped = service
        .swap_out(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);
    assert_eq!(swapped[0].state, SwapState::Out);

    // Project file should have original content
    let content = std::fs::read_to_string(&project_file).unwrap();
    assert!(
        content.contains("original: value"),
        "project file should have original content restored"
    );

    // Vault file should be restored
    let vault_file = swap_dir.join("config.yml");
    assert!(vault_file.exists(), "vault file should be restored");

    // Backup should be removed from vault
    let backup = swap_dir.join("config.yml.rsenv_original");
    assert!(!backup.exists(), "backup should be removed after swap-out");

    // Sentinel should be removed from vault
    let sentinel = swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname));
    assert!(
        !sentinel.exists(),
        "sentinel should be removed after swap-out"
    );
}

#[test]
fn given_modified_file_when_swap_out_then_modifications_captured_in_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in first
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // MODIFY the swapped-in file (this should be captured on swap-out!)
    std::fs::write(&project_file, "override: MODIFIED!\n").unwrap();

    // Act - swap out
    service
        .swap_out(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - vault file should have MODIFIED content (captured changes!)
    let vault_file = swap_dir.join("config.yml");
    let vault_content = std::fs::read_to_string(&vault_file).unwrap();
    assert!(
        vault_content.contains("MODIFIED"),
        "vault should capture modifications from project"
    );

    // Project file should have ORIGINAL content restored
    let project_content = std::fs::read_to_string(&project_file).unwrap();
    assert!(
        project_content.contains("original: value"),
        "project should have original restored"
    );
}

#[test]
fn given_not_swapped_file_when_swap_out_then_succeeds_idempotently() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    // Vault has file but no sentinel (not swapped in)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap out without swapping in
    let result = service.swap_out(&project_dir, &[project_file]);

    // Assert - should succeed (idempotent) with empty result
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty(), "no files should be swapped out");
}

// ============================================================
// swap_init() tests
// ============================================================

#[test]
fn given_project_file_without_vault_when_swap_init_then_moves_to_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    let project_file = project_dir.join("new_config.yml");
    std::fs::write(&project_file, "new: content\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - initialize (move project to vault)
    let initialized = service
        .swap_init(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(initialized.len(), 1);

    // Project file should be GONE
    assert!(
        !project_file.exists(),
        "project file should be moved to vault"
    );

    // Vault file should have the content
    let vault_file = vault_path.join("swap/new_config.yml");
    assert!(vault_file.exists(), "vault should have the file");
    let content = std::fs::read_to_string(&vault_file).unwrap();
    assert!(content.contains("new: content"));

    // No sentinel or backup created for init
    let sentinel = vault_path.join(format!("swap/new_config.yml@@{}@@rsenv_active", hostname));
    let backup = vault_path.join("swap/new_config.yml.rsenv_original");
    assert!(!sentinel.exists(), "no sentinel for init");
    assert!(!backup.exists(), "no backup for init");
}

#[test]
fn given_vault_already_has_file_when_swap_init_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "project content\n").unwrap();

    // Vault already has this file
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "vault content\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_init(&project_dir, &[project_file]);

    // Assert - should fail because vault already has file
    assert!(result.is_err());
}

#[test]
fn given_project_file_not_exists_when_swap_init_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, _, settings) = setup_project(&temp);

    let project_file = project_dir.join("nonexistent.yml");
    // Don't create the file

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_init(&project_dir, &[project_file]);

    // Assert - should fail because project file doesn't exist
    assert!(result.is_err());
}

// ============================================================
// status() tests
// ============================================================

#[test]
fn given_sentinel_in_vault_when_status_then_shows_swapped_in() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create vault file
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs.clone(), vault_service, settings);

    // Swap in to create sentinel in vault
    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original").unwrap();
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Verify sentinel is in vault (not project)
    let sentinel_in_vault = swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname));
    assert!(
        sentinel_in_vault.exists(),
        "sentinel should be in vault for status test"
    );

    // Act
    let status = service.status(&project_dir).unwrap();

    // Assert - should find the swapped-in file
    assert_eq!(status.len(), 1);
    assert!(matches!(status[0].state, SwapState::In { .. }));
}

#[test]
fn given_mixed_swap_states_when_status_then_categorizes_correctly() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // File 1: will be swapped in
    let file1 = project_dir.join("config1.yml");
    std::fs::write(&file1, "value1").unwrap();

    // File 2: not swapped (vault has override but not applied)
    let file2 = project_dir.join("config2.yml");
    std::fs::write(&file2, "value2").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config1.yml"), "override1").unwrap();
    std::fs::write(swap_dir.join("config2.yml"), "override2").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in file1
    service.swap_in(&project_dir, &[file1]).unwrap();

    // Act
    let status = service.status(&project_dir).unwrap();

    // Assert
    assert_eq!(status.len(), 2);
    let swapped_in: Vec<_> = status
        .iter()
        .filter(|s| matches!(s.state, SwapState::In { .. }))
        .collect();
    let swapped_out: Vec<_> = status
        .iter()
        .filter(|s| matches!(s.state, SwapState::Out))
        .collect();
    assert_eq!(swapped_in.len(), 1);
    assert_eq!(swapped_out.len(), 1);
}

#[test]
fn given_no_swap_files_when_status_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, _, settings) = setup_project(&temp);

    // Just a regular file, no vault override
    std::fs::write(project_dir.join("config.yml"), "value").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let status = service.status(&project_dir).unwrap();

    // Assert
    assert!(status.is_empty());
}

// ============================================================
// Nested path tests
// ============================================================

#[test]
fn given_nested_path_when_swap_in_then_sentinel_preserves_structure() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create nested structure in project
    let nested_dir = project_dir.join("src/main/resources");
    std::fs::create_dir_all(&nested_dir).unwrap();
    let project_file = nested_dir.join("application.yml");
    std::fs::write(&project_file, "original: nested\n").unwrap();

    // Create override in vault's swap directory with same structure
    let swap_dir = vault_path.join("swap/src/main/resources");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let vault_file = swap_dir.join("application.yml");
    std::fs::write(&vault_file, "override: nested\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let swapped = service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);

    // Sentinel should be at: vault/swap/src/main/resources/application.yml.<host>.rsenv_active
    let sentinel = swap_dir.join(format!("application.yml@@{}@@rsenv_active", hostname));
    assert!(
        sentinel.exists(),
        "sentinel should preserve nested structure in vault"
    );

    // Backup should be at: vault/swap/src/main/resources/application.yml.rsenv_original
    let backup = swap_dir.join("application.yml.rsenv_original");
    assert!(
        backup.exists(),
        "backup should preserve nested structure in vault"
    );
}

// ============================================================
// Directory swap tests
// ============================================================

#[test]
fn given_directory_in_vault_when_swap_in_then_swaps_entire_directory() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create original directory in project
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "original: app\n").unwrap();
    std::fs::create_dir_all(project_subdir.join("nested")).unwrap();
    std::fs::write(project_subdir.join("nested/db.yml"), "original: db\n").unwrap();

    // Create override directory in vault's swap directory
    let swap_dir = vault_path.join("swap");
    let vault_config_dir = swap_dir.join("config");
    std::fs::create_dir_all(&vault_config_dir).unwrap();
    std::fs::write(vault_config_dir.join("app.yml"), "override: app\n").unwrap();
    std::fs::create_dir_all(vault_config_dir.join("nested")).unwrap();
    std::fs::write(vault_config_dir.join("nested/db.yml"), "override: db\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let swapped = service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);
    assert_eq!(
        swapped[0].state,
        SwapState::In {
            hostname: hostname.clone()
        }
    );

    // Project directory should have override content
    let app_content = std::fs::read_to_string(project_subdir.join("app.yml")).unwrap();
    assert!(
        app_content.contains("override: app"),
        "project app.yml should have override content"
    );
    let db_content = std::fs::read_to_string(project_subdir.join("nested/db.yml")).unwrap();
    assert!(
        db_content.contains("override: db"),
        "project nested/db.yml should have override content"
    );

    // Vault directory should be GONE (moved, not copied)
    assert!(
        !vault_config_dir.exists(),
        "vault config dir should be moved, not copied"
    );

    // Backup should be in VAULT
    let backup_dir = swap_dir.join("config.rsenv_original");
    assert!(backup_dir.exists(), "backup directory should be in vault");
    assert!(
        backup_dir.join("app.yml").exists(),
        "backup should have app.yml"
    );
    assert!(
        backup_dir.join("nested/db.yml").exists(),
        "backup should have nested/db.yml"
    );

    // Sentinel directory should be in VAULT with content (copy of vault before move)
    let sentinel_dir = swap_dir.join(format!("config@@{}@@rsenv_active", hostname));
    assert!(
        sentinel_dir.exists(),
        "sentinel directory should be in vault"
    );
    assert!(
        sentinel_dir.join("app.yml").exists(),
        "sentinel should have app.yml"
    );
    let sentinel_content = std::fs::read_to_string(sentinel_dir.join("app.yml")).unwrap();
    assert!(
        sentinel_content.contains("override: app"),
        "sentinel should be copy of vault content"
    );
}

#[test]
fn given_directory_swapped_in_when_swap_out_then_restores_original() {
    // Arrange: setup project with directory swapped in
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create original directory in project
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "original: app\n").unwrap();
    std::fs::create_dir_all(project_subdir.join("nested")).unwrap();
    std::fs::write(project_subdir.join("nested/db.yml"), "original: db\n").unwrap();

    // Create override directory in vault's swap directory
    let swap_dir = vault_path.join("swap");
    let vault_config_dir = swap_dir.join("config");
    std::fs::create_dir_all(&vault_config_dir).unwrap();
    std::fs::write(vault_config_dir.join("app.yml"), "override: app\n").unwrap();
    std::fs::create_dir_all(vault_config_dir.join("nested")).unwrap();
    std::fs::write(vault_config_dir.join("nested/db.yml"), "override: db\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // First swap in
    service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Verify swapped in state - check sentinel exists
    let sentinel_dir = swap_dir.join(format!("config@@{}@@rsenv_active", hostname));
    assert!(sentinel_dir.exists(), "sentinel should exist after swap in");

    // Act: swap out
    let swapped = service
        .swap_out(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);
    assert_eq!(swapped[0].state, SwapState::Out);

    // Project directory should have original content restored
    let app_content = std::fs::read_to_string(project_subdir.join("app.yml")).unwrap();
    assert!(
        app_content.contains("original: app"),
        "project app.yml should have original content after swap out"
    );
    let db_content = std::fs::read_to_string(project_subdir.join("nested/db.yml")).unwrap();
    assert!(
        db_content.contains("original: db"),
        "project nested/db.yml should have original content after swap out"
    );

    // Vault should have override content back
    assert!(
        vault_config_dir.exists(),
        "vault config dir should be restored"
    );
    let vault_app = std::fs::read_to_string(vault_config_dir.join("app.yml")).unwrap();
    assert!(
        vault_app.contains("override: app"),
        "vault app.yml should have override content"
    );

    // Sentinel and backup should be cleaned up
    assert!(
        !sentinel_dir.exists(),
        "sentinel should be removed after swap out"
    );
    let backup_dir = swap_dir.join("config.rsenv_original");
    assert!(
        !backup_dir.exists(),
        "backup should be removed after swap out"
    );
}

#[test]
fn given_directory_swapped_in_when_status_then_shows_swapped_in() {
    // Arrange: setup project with directory swapped in
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create original directory in project
    let project_subdir = project_dir.join("thoughts");
    std::fs::create_dir_all(project_subdir.join("nested")).unwrap();
    std::fs::write(project_subdir.join("nested/note.md"), "original\n").unwrap();

    // Create override directory in vault's swap directory
    let swap_dir = vault_path.join("swap");
    let vault_thoughts = swap_dir.join("thoughts");
    std::fs::create_dir_all(vault_thoughts.join("nested")).unwrap();
    std::fs::write(vault_thoughts.join("nested/note.md"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in the directory
    service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Act: check status
    let status = service.status(&project_dir).unwrap();

    // Assert: should show directory as swapped in
    assert_eq!(status.len(), 1, "should have exactly one swap entry");
    assert_eq!(status[0].project_path, project_subdir);
    assert!(
        matches!(&status[0].state, SwapState::In { hostname: h } if h == &hostname),
        "directory should be shown as swapped in by current host"
    );
}

// ============================================================
// RSENV_SWAPPED marker tests
// ============================================================

#[test]
fn given_swap_in_when_successful_then_adds_marker_to_dot_envrc() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - dot.envrc should contain marker
    let dot_envrc = vault_path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("export RSENV_SWAPPED=1"),
        "dot.envrc should contain RSENV_SWAPPED marker after swap_in"
    );
}

#[test]
fn given_swap_out_all_when_successful_then_removes_marker() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original: value\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in first (adds marker)
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Verify marker exists
    let dot_envrc = vault_path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(content.contains("export RSENV_SWAPPED=1"));

    // Act - swap out all
    service
        .swap_out(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - marker should be removed
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        !content.contains("export RSENV_SWAPPED=1"),
        "dot.envrc should NOT contain RSENV_SWAPPED marker after all files swapped out"
    );
}

#[test]
fn given_swap_out_partial_when_files_remain_then_keeps_marker() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Two files
    let file1 = project_dir.join("config1.yml");
    let file2 = project_dir.join("config2.yml");
    std::fs::write(&file1, "original1\n").unwrap();
    std::fs::write(&file2, "original2\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config1.yml"), "override1\n").unwrap();
    std::fs::write(swap_dir.join("config2.yml"), "override2\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in both files
    service.swap_in(&project_dir, &[file1.clone()]).unwrap();
    service.swap_in(&project_dir, &[file2.clone()]).unwrap();

    // Act - swap out only file1
    service.swap_out(&project_dir, &[file1.clone()]).unwrap();

    // Assert - marker should remain (file2 still swapped in)
    let dot_envrc = vault_path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("export RSENV_SWAPPED=1"),
        "marker should remain when files still swapped in"
    );

    // Swap out file2
    service.swap_out(&project_dir, &[file2.clone()]).unwrap();

    // Assert - now marker should be gone
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        !content.contains("export RSENV_SWAPPED=1"),
        "marker should be removed when all files swapped out"
    );
}

#[test]
fn given_marker_already_exists_when_swap_in_then_no_duplicate() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Two files
    let file1 = project_dir.join("config1.yml");
    let file2 = project_dir.join("config2.yml");
    std::fs::write(&file1, "original1\n").unwrap();
    std::fs::write(&file2, "original2\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config1.yml"), "override1\n").unwrap();
    std::fs::write(swap_dir.join("config2.yml"), "override2\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in file1 (adds marker)
    service.swap_in(&project_dir, &[file1.clone()]).unwrap();

    // Act - swap in file2 (marker already exists)
    service.swap_in(&project_dir, &[file2.clone()]).unwrap();

    // Assert - should have exactly one marker line
    let dot_envrc = vault_path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    let marker_count = content
        .lines()
        .filter(|line| line.trim() == "export RSENV_SWAPPED=1")
        .count();
    assert_eq!(marker_count, 1, "should have exactly one marker line");
}

// ============================================================
// Dot-file neutralization tests
// ============================================================

#[test]
fn given_directory_with_gitignore_when_swap_init_then_gitignore_neutralized() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create directory with .gitignore in project
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "app: config\n").unwrap();
    std::fs::write(project_subdir.join(".gitignore"), "*.local\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - init (move project dir to vault)
    service
        .swap_init(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert - .gitignore should be neutralized in vault
    let swap_dir = vault_path.join("swap");
    let vault_config = swap_dir.join("config");

    // .gitignore should NOT exist (neutralized)
    assert!(
        !vault_config.join(".gitignore").exists(),
        ".gitignore should be neutralized in vault"
    );

    // Neutralized form should exist
    assert!(
        vault_config.join("dot.gitignore").exists(),
        "dot.gitignore should exist in vault"
    );

    // Content should be preserved
    let content = std::fs::read_to_string(vault_config.join("dot.gitignore")).unwrap();
    assert!(content.contains("*.local"), "content should be preserved");
}

#[test]
fn given_directory_with_nested_gitignore_when_swap_out_then_gitignore_neutralized() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create original directory in project (no .gitignore initially)
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(project_subdir.join("nested")).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "original: app\n").unwrap();
    std::fs::write(project_subdir.join("nested/db.yml"), "original: db\n").unwrap();

    // Create override directory in vault's swap directory
    let swap_dir = vault_path.join("swap");
    let vault_config_dir = swap_dir.join("config");
    std::fs::create_dir_all(vault_config_dir.join("nested")).unwrap();
    std::fs::write(vault_config_dir.join("app.yml"), "override: app\n").unwrap();
    std::fs::write(vault_config_dir.join("nested/db.yml"), "override: db\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in
    service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Add .gitignore files while swapped in (simulating user adding them)
    std::fs::write(project_subdir.join(".gitignore"), "root-ignore\n").unwrap();
    std::fs::write(project_subdir.join("nested/.gitignore"), "nested-ignore\n").unwrap();

    // Act - swap out (should neutralize .gitignore in vault)
    service
        .swap_out(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert - .gitignore should be neutralized in vault
    let vault_config = swap_dir.join("config");

    // Root .gitignore neutralized
    assert!(
        !vault_config.join(".gitignore").exists(),
        "root .gitignore should be neutralized"
    );
    assert!(
        vault_config.join("dot.gitignore").exists(),
        "root dot.gitignore should exist"
    );

    // Nested .gitignore neutralized
    assert!(
        !vault_config.join("nested/.gitignore").exists(),
        "nested .gitignore should be neutralized"
    );
    assert!(
        vault_config.join("nested/dot.gitignore").exists(),
        "nested dot.gitignore should exist"
    );
}

#[test]
fn given_neutralized_gitignore_in_vault_when_swap_in_then_gitignore_restored() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create original directory in project
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "original: app\n").unwrap();

    // Create override directory in vault with NEUTRALIZED .gitignore
    let swap_dir = vault_path.join("swap");
    let vault_config_dir = swap_dir.join("config");
    std::fs::create_dir_all(&vault_config_dir).unwrap();
    std::fs::write(vault_config_dir.join("app.yml"), "override: app\n").unwrap();
    // Note: dot.gitignore (not .gitignore)
    std::fs::write(vault_config_dir.join("dot.gitignore"), "*.local\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in (should restore .gitignore)
    service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert - .gitignore should be restored in project
    assert!(
        project_subdir.join(".gitignore").exists(),
        ".gitignore should be restored in project"
    );
    assert!(
        !project_subdir.join("dot.gitignore").exists(),
        "dot.gitignore should NOT exist in project"
    );

    // Content preserved
    let content = std::fs::read_to_string(project_subdir.join(".gitignore")).unwrap();
    assert!(content.contains("*.local"), "content should be preserved");
}

#[test]
fn given_standalone_gitignore_when_swap_init_then_neutralized() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create standalone .gitignore in project
    let project_gitignore = project_dir.join("subdir/.gitignore");
    std::fs::create_dir_all(project_dir.join("subdir")).unwrap();
    std::fs::write(&project_gitignore, "*.tmp\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - init standalone .gitignore
    service
        .swap_init(&project_dir, &[project_gitignore.clone()])
        .unwrap();

    // Assert - should be neutralized in vault
    let swap_dir = vault_path.join("swap/subdir");

    assert!(
        !swap_dir.join(".gitignore").exists(),
        "bare .gitignore should NOT exist in vault"
    );
    assert!(
        swap_dir.join("dot.gitignore").exists(),
        "dot.gitignore should exist in vault"
    );
}

#[test]
fn given_standalone_neutralized_gitignore_when_swap_in_then_gitignore_restored() {
    // This is the bug scenario: after swap_init, standalone .gitignore becomes
    // dot.gitignore in vault. swap_in should find it and restore it.
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create original .gitignore in project (for backup)
    let project_gitignore = project_dir.join("subdir/.gitignore");
    std::fs::create_dir_all(project_dir.join("subdir")).unwrap();
    std::fs::write(&project_gitignore, "original\n").unwrap();

    // Create NEUTRALIZED .gitignore in vault (simulating after swap_init)
    let swap_dir = vault_path.join("swap/subdir");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("dot.gitignore"), "override content\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in standalone .gitignore (neutralized form)
    let result = service.swap_in(&project_dir, &[project_gitignore.clone()]);

    // Assert - should succeed and .gitignore should exist in project with vault content
    assert!(result.is_ok(), "swap_in should succeed: {:?}", result.err());
    assert!(
        project_gitignore.exists(),
        ".gitignore should exist in project"
    );
    let content = std::fs::read_to_string(&project_gitignore).unwrap();
    assert!(
        content.contains("override"),
        "should have vault content, got: {}",
        content
    );
}

#[test]
fn given_bare_gitignore_in_vault_when_swap_in_then_rejects_with_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create project file
    let project_file = project_dir.join("config/.gitignore");
    std::fs::create_dir_all(project_dir.join("config")).unwrap();
    std::fs::write(&project_file, "original\n").unwrap();

    // Create vault with BARE .gitignore (not neutralized - user mistake)
    let swap_dir = vault_path.join("swap/config");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join(".gitignore"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in should be rejected
    let result = service.swap_in(&project_dir, &[project_file.clone()]);

    // Assert - should fail with informative error
    assert!(result.is_err(), "swap_in should reject bare .gitignore");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains(".gitignore") && err.contains("dot.gitignore"),
        "error should mention expected rename: {}",
        err
    );
}

#[test]
fn given_bare_gitignore_in_vault_dir_when_swap_in_then_rejects_with_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create project directory
    let project_subdir = project_dir.join("config");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "original\n").unwrap();

    // Create vault directory with BARE .gitignore (not neutralized)
    let swap_dir = vault_path.join("swap/config");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("app.yml"), "override\n").unwrap();
    std::fs::write(swap_dir.join(".gitignore"), "should-be-disabled\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in should be rejected
    let result = service.swap_in(&project_dir, &[project_subdir.clone()]);

    // Assert - should fail with informative error
    assert!(
        result.is_err(),
        "swap_in should reject bare .gitignore in directory"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains(".gitignore"),
        "error should mention .gitignore: {}",
        err
    );
}

#[test]
fn given_gitignore_full_cycle_when_swap_in_out_then_content_preserved() {
    // Full cycle: init (neutralize) -> swap_in (restore) -> modify -> swap_out (neutralize)
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create directory with .gitignore in project
    let project_subdir = project_dir.join("myconfig");
    std::fs::create_dir_all(&project_subdir).unwrap();
    std::fs::write(project_subdir.join("app.yml"), "app: original\n").unwrap();
    std::fs::write(project_subdir.join(".gitignore"), "*.local\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Step 1: Init (moves to vault, neutralizes .gitignore)
    service
        .swap_init(&project_dir, &[project_subdir.clone()])
        .unwrap();

    let swap_dir = vault_path.join("swap/myconfig");
    assert!(
        swap_dir.join("dot.gitignore").exists(),
        "after init: .gitignore should be neutralized"
    );

    // Step 2: Swap in (restores .gitignore to project)
    service
        .swap_in(&project_dir, &[project_subdir.clone()])
        .unwrap();

    assert!(
        project_subdir.join(".gitignore").exists(),
        "after swap_in: .gitignore should be restored in project"
    );
    let content = std::fs::read_to_string(project_subdir.join(".gitignore")).unwrap();
    assert!(
        content.contains("*.local"),
        "content preserved after swap_in"
    );

    // Step 3: Modify .gitignore while swapped in
    std::fs::write(project_subdir.join(".gitignore"), "*.local\n*.tmp\n").unwrap();

    // Step 4: Swap out (neutralizes modified .gitignore back to vault)
    service
        .swap_out(&project_dir, &[project_subdir.clone()])
        .unwrap();

    assert!(
        swap_dir.join("dot.gitignore").exists(),
        "after swap_out: .gitignore should be neutralized again"
    );

    // Verify modifications were captured
    let vault_content = std::fs::read_to_string(swap_dir.join("dot.gitignore")).unwrap();
    assert!(
        vault_content.contains("*.tmp"),
        "modifications should be captured in vault"
    );
}

#[test]
fn given_standalone_gitignore_full_cycle_when_init_swap_in_swap_out_then_works() {
    // Full cycle for STANDALONE .gitignore file (not inside a directory)
    // This is the exact scenario that was broken: init -> swap_in -> modify -> swap_out
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create standalone .gitignore in project subdirectory
    let project_gitignore = project_dir.join("subdir/.gitignore");
    std::fs::create_dir_all(project_dir.join("subdir")).unwrap();
    std::fs::write(&project_gitignore, "*.local\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Step 1: Init (moves standalone .gitignore to vault, neutralizes it)
    service
        .swap_init(&project_dir, &[project_gitignore.clone()])
        .unwrap();

    let swap_dir = vault_path.join("swap/subdir");
    assert!(
        !swap_dir.join(".gitignore").exists(),
        "after init: bare .gitignore should NOT exist in vault"
    );
    assert!(
        swap_dir.join("dot.gitignore").exists(),
        "after init: .gitignore should be neutralized in vault"
    );
    assert!(
        !project_gitignore.exists(),
        "after init: .gitignore should be gone from project"
    );

    // Step 2: Swap in (finds neutralized form and restores .gitignore to project)
    service
        .swap_in(&project_dir, &[project_gitignore.clone()])
        .unwrap();

    assert!(
        project_gitignore.exists(),
        "after swap_in: .gitignore should exist in project"
    );
    let content = std::fs::read_to_string(&project_gitignore).unwrap();
    assert!(
        content.contains("*.local"),
        "after swap_in: content should be preserved"
    );

    // Step 3: Modify while swapped in
    std::fs::write(&project_gitignore, "*.local\n*.tmp\n").unwrap();

    // Step 4: Swap out (moves modified .gitignore back to vault, neutralizes it)
    service
        .swap_out(&project_dir, &[project_gitignore.clone()])
        .unwrap();

    assert!(
        swap_dir.join("dot.gitignore").exists(),
        "after swap_out: .gitignore should be neutralized in vault"
    );
    assert!(
        !swap_dir.join(".gitignore").exists(),
        "after swap_out: bare .gitignore should NOT exist in vault"
    );

    // Verify modifications were captured
    let vault_content = std::fs::read_to_string(swap_dir.join("dot.gitignore")).unwrap();
    assert!(
        vault_content.contains("*.tmp"),
        "modifications should be captured in vault"
    );
}

// ============================================================
// delete() tests
// ============================================================

#[test]
fn given_swapped_out_file_when_delete_then_removes_vault_artifacts() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "project content\n").unwrap();

    // Create vault override
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let vault_file = swap_dir.join("config.yml");
    std::fs::write(&vault_file, "vault override\n").unwrap();

    // Create backup (simulate previous swap-in/out cycle)
    let backup_file = swap_dir.join("config.yml.rsenv_original");
    std::fs::write(&backup_file, "backup content\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let deleted = service
        .delete(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(deleted.len(), 1);
    assert!(!vault_file.exists(), "vault override should be deleted");
    assert!(!backup_file.exists(), "backup should be deleted");
    assert!(project_file.exists(), "project file should NOT be touched");
}

#[test]
fn given_swapped_in_file_when_delete_then_fails_with_hostname() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in first
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Act
    let result = service.delete(&project_dir, &[project_file]);

    // Assert
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("swapped in"),
        "error should mention swapped in"
    );
    assert!(err.contains(&hostname), "error should mention hostname");
}

#[test]
fn given_swapped_in_by_other_host_when_delete_then_fails_with_that_hostname() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");

    // Create sentinel from different host manually
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let sentinel = swap_dir.join("config.yml@@other-workstation@@rsenv_active");
    std::fs::write(&sentinel, "sentinel content").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.delete(&project_dir, &[project_file]);

    // Assert
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("other-workstation"),
        "error should mention the other host"
    );
}

#[test]
fn given_multiple_files_one_swapped_in_when_delete_then_no_deletions_occur() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let file1 = project_dir.join("config1.yml");
    let file2 = project_dir.join("config2.yml");
    std::fs::write(&file1, "original1\n").unwrap();
    std::fs::write(&file2, "original2\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config1.yml"), "override1\n").unwrap();
    std::fs::write(swap_dir.join("config2.yml"), "override2\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in only file2
    service.swap_in(&project_dir, &[file2.clone()]).unwrap();

    // Act - try to delete both
    let result = service.delete(&project_dir, &[file1.clone(), file2.clone()]);

    // Assert - should fail, AND file1's vault file should NOT be deleted
    assert!(result.is_err());
    assert!(
        swap_dir.join("config1.yml").exists(),
        "file1 vault should NOT be deleted due to all-or-nothing"
    );
}

#[test]
fn given_nonexistent_vault_file_when_delete_then_succeeds_idempotently() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_file = project_dir.join("config.yml");
    // Don't create vault file - it doesn't exist

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    // swap_dir exists but config.yml doesn't

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - delete nonexistent file
    let deleted = service
        .delete(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - should succeed (idempotent)
    assert_eq!(deleted.len(), 1);
}

#[test]
fn given_directory_in_vault_when_delete_then_removes_entire_directory() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let project_subdir = project_dir.join("config");

    // Create vault directory with contents
    let swap_dir = vault_path.join("swap");
    let vault_config = swap_dir.join("config");
    std::fs::create_dir_all(vault_config.join("nested")).unwrap();
    std::fs::write(vault_config.join("app.yml"), "app config").unwrap();
    std::fs::write(vault_config.join("nested/db.yml"), "db config").unwrap();

    // Also create backup directory
    let backup_dir = swap_dir.join("config.rsenv_original");
    std::fs::create_dir_all(&backup_dir).unwrap();
    std::fs::write(backup_dir.join("app.yml"), "original app").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let deleted = service
        .delete(&project_dir, &[project_subdir.clone()])
        .unwrap();

    // Assert
    assert_eq!(deleted.len(), 1);
    assert!(!vault_config.exists(), "vault config dir should be deleted");
    assert!(!backup_dir.exists(), "backup dir should be deleted");
}

// ============================================================
// move_path() tests (FileSystem trait)
// ============================================================

#[test]
fn given_file_when_move_path_then_moves_atomically() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("dest.txt");
    std::fs::write(&src, "test content").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.move_path(&src, &dst).unwrap();

    // Assert
    assert!(!src.exists(), "source should be removed");
    assert!(dst.exists(), "destination should exist");
    assert_eq!(
        std::fs::read_to_string(&dst).unwrap(),
        "test content",
        "content should be preserved"
    );
}

#[test]
fn given_directory_when_move_path_then_moves_entire_tree() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("srcdir");
    let dst = temp.path().join("dstdir");
    std::fs::create_dir_all(src.join("nested")).unwrap();
    std::fs::write(src.join("file.txt"), "root file").unwrap();
    std::fs::write(src.join("nested/inner.txt"), "nested file").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.move_path(&src, &dst).unwrap();

    // Assert
    assert!(!src.exists(), "source dir should be removed");
    assert!(dst.exists(), "destination dir should exist");
    assert_eq!(
        std::fs::read_to_string(dst.join("file.txt")).unwrap(),
        "root file"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("nested/inner.txt")).unwrap(),
        "nested file"
    );
}

use rsenv::infrastructure::traits::FileSystem;

// ============================================================
// Hostname with dots tests (FQDN support)
// ============================================================

#[test]
fn given_sentinel_with_dotted_hostname_when_status_then_parses_correctly() {
    // Arrange - simulates hostname like "MacBookAir.fritz.box"
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    let dotted_hostname = "MacBookAir.fritz.box";

    // Create sentinel with NEW @@ format: {base_name}@@{hostname}@@rsenv_active
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();

    // Create sentinel as directory (simulating swapped-in directory)
    let sentinel = swap_dir.join(format!("thoughts@@{}@@rsenv_active", dotted_hostname));
    std::fs::create_dir_all(&sentinel).unwrap();
    std::fs::write(sentinel.join("test.txt"), "sentinel content").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let status = service.status(&project_dir).unwrap();

    // Assert - should find exactly one swapped item with correct base_name
    assert_eq!(status.len(), 1, "should find exactly one swap entry");

    let entry = &status[0];
    assert_eq!(
        entry.project_path,
        project_dir.join("thoughts"),
        "base_name should be 'thoughts', not 'thoughts.MacBookAir.fritz'"
    );

    // Verify the state contains the full hostname
    match &entry.state {
        SwapState::In { hostname } => {
            assert_eq!(
                hostname, dotted_hostname,
                "hostname should be full FQDN '{}'",
                dotted_hostname
            );
        }
        other => panic!("expected SwapState::In, got {:?}", other),
    }
}

#[test]
fn given_dotted_hostname_when_swap_in_then_creates_correct_sentinel() {
    // This test verifies that swap_in creates sentinels with @@ delimiters
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create vault file to swap in
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let project_file = project_dir.join("config.yml");
    service.swap_in(&project_dir, &[project_file]).unwrap();

    // Assert - sentinel should use @@ format
    let hostname = get_hostname();
    let expected_sentinel = swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname));

    assert!(
        expected_sentinel.exists(),
        "sentinel should exist at {:?}",
        expected_sentinel
    );

    // Old format should NOT exist
    let old_format_sentinel = swap_dir.join(format!("config.yml.{}.rsenv_active", hostname));
    assert!(
        !old_format_sentinel.exists(),
        "old format sentinel should NOT exist at {:?}",
        old_format_sentinel
    );
}

// ============================================================
// status_all_vaults() tests
// ============================================================

#[test]
fn given_no_vaults_when_status_all_vaults_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let statuses = service.status_all_vaults(&vaults_dir).unwrap();

    // Assert
    assert!(
        statuses.is_empty(),
        "should return empty when no vaults exist"
    );
}

#[test]
fn given_vault_with_no_active_swaps_when_status_all_vaults_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let vault_base = settings.vaults_dir();

    // Create vault file (but don't swap in - state is Out)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();

    // Project has original file
    std::fs::write(project_dir.join("config.yml"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let statuses = service.status_all_vaults(&vault_base).unwrap();

    // Assert - no active swaps (state is Out), so empty result
    assert!(
        statuses.is_empty(),
        "should return empty when no files are swapped in"
    );
}

#[test]
fn given_vault_with_active_swap_when_status_all_vaults_then_returns_that_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let vault_base = settings.vaults_dir();

    let project_file = project_dir.join("config.yml");
    std::fs::write(&project_file, "original\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in to create active swap
    service.swap_in(&project_dir, &[project_file]).unwrap();

    // Act
    let statuses = service.status_all_vaults(&vault_base).unwrap();

    // Assert
    assert_eq!(
        statuses.len(),
        1,
        "should return one vault with active swap"
    );
    assert_eq!(statuses[0].active_swaps.len(), 1);
    assert!(matches!(
        statuses[0].active_swaps[0].state,
        SwapState::In { .. }
    ));
}

#[test]
fn given_multiple_vaults_when_status_all_vaults_then_returns_only_those_with_swaps() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = VaultService::new(fs.clone(), settings.clone());

    // Create project 1 with active swap
    let project1 = temp.path().join("project1");
    std::fs::create_dir_all(&project1).unwrap();
    let vault1 = vault_service.init(&project1, false).unwrap();
    std::fs::write(project1.join("config.yml"), "original1\n").unwrap();
    let swap_dir1 = vault1.path.join("swap");
    std::fs::create_dir_all(&swap_dir1).unwrap();
    std::fs::write(swap_dir1.join("config.yml"), "override1\n").unwrap();

    // Create project 2 with NO active swap (file in vault but not swapped in)
    let project2 = temp.path().join("project2");
    std::fs::create_dir_all(&project2).unwrap();
    let vault2 = vault_service.init(&project2, false).unwrap();
    std::fs::write(project2.join("other.yml"), "original2\n").unwrap();
    let swap_dir2 = vault2.path.join("swap");
    std::fs::create_dir_all(&swap_dir2).unwrap();
    std::fs::write(swap_dir2.join("other.yml"), "override2\n").unwrap();

    let vault_service = Arc::new(vault_service);
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in only project1's file
    service
        .swap_in(&project1, &[project1.join("config.yml")])
        .unwrap();

    // Act
    let statuses = service.status_all_vaults(&vaults_dir).unwrap();

    // Assert - only project1 should be reported (it has active swap)
    assert_eq!(
        statuses.len(),
        1,
        "should return only vaults with active swaps"
    );
    assert!(
        statuses[0]
            .vault_path
            .to_string_lossy()
            .contains("project1"),
        "should be project1's vault"
    );
}

#[test]
fn given_vault_without_valid_dot_envrc_when_status_all_vaults_then_skips_gracefully() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    // Create a directory that looks like a vault but has invalid dot.envrc
    let fake_vault = vaults_dir.join("fake-vault-abc123");
    std::fs::create_dir_all(&fake_vault).unwrap();
    std::fs::write(fake_vault.join("dot.envrc"), "# not valid rsenv metadata\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - should not panic or error
    let result = service.status_all_vaults(&vaults_dir);

    // Assert - should succeed and return empty (invalid vault skipped)
    assert!(result.is_ok(), "should not error on invalid vault");
    assert!(
        result.unwrap().is_empty(),
        "should skip invalid vaults gracefully"
    );
}

// ============================================================
// swap_out_vault() tests
// ============================================================

#[test]
fn given_no_swapped_files_when_swap_out_vault_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create vault swap file but don't swap in (state is Out)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();
    std::fs::write(project_dir.join("config.yml"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_out_vault(&project_dir).unwrap();

    // Assert
    assert!(
        result.is_empty(),
        "should return empty when no files swapped in"
    );
}

#[test]
fn given_swapped_files_when_swap_out_vault_then_swaps_all_out() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create two files in vault swap dir
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override1\n").unwrap();
    std::fs::write(swap_dir.join("settings.yml"), "override2\n").unwrap();

    // Create originals in project
    std::fs::write(project_dir.join("config.yml"), "original1\n").unwrap();
    std::fs::write(project_dir.join("settings.yml"), "original2\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap both in
    service
        .swap_in(
            &project_dir,
            &[
                project_dir.join("config.yml"),
                project_dir.join("settings.yml"),
            ],
        )
        .unwrap();

    // Act - swap out ALL via vault-out
    let result = service.swap_out_vault(&project_dir).unwrap();

    // Assert
    assert_eq!(result.len(), 2, "should swap out both files");

    // Verify files are back to original
    let config_content = std::fs::read_to_string(project_dir.join("config.yml")).unwrap();
    let settings_content = std::fs::read_to_string(project_dir.join("settings.yml")).unwrap();
    assert_eq!(config_content, "original1\n");
    assert_eq!(settings_content, "original2\n");
}

#[test]
fn given_dotfile_swapped_in_when_swap_out_vault_then_restores_correctly() {
    // Regression test: status_impl must use restore_path() when building project_path
    // from neutralized vault paths. Without this, swap_out_vault would try to move
    // project/dot.claude instead of project/.claude
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create .claude/config.json in project (dotfile directory)
    let claude_dir = project_dir.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let project_file = claude_dir.join("config.json");
    std::fs::write(&project_file, "original\n").unwrap();

    // Create override at NEUTRALIZED path in vault (dot.claude)
    let swap_dir = vault_path.join("swap");
    let vault_claude_dir = swap_dir.join("dot.claude");
    std::fs::create_dir_all(&vault_claude_dir).unwrap();
    std::fs::write(vault_claude_dir.join("config.json"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in via explicit path
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Verify swap in worked
    let content = std::fs::read_to_string(&project_file).unwrap();
    assert!(content.contains("override"), "should have override content");

    // Act - swap out via vault-out (discovery path, not explicit)
    // This exercises status_impl -> swap_out with discovered paths
    let result = service.swap_out_vault(&project_dir).unwrap();

    // Assert
    assert_eq!(result.len(), 1, "should swap out one file");

    // The key assertion: project_path in result should be .claude, not dot.claude
    assert_eq!(
        result[0].project_path, project_file,
        "project_path should be .claude/config.json, not dot.claude/config.json"
    );

    // Verify file is back to original content
    let final_content = std::fs::read_to_string(&project_file).unwrap();
    assert_eq!(final_content, "original\n", "should restore original");
}

#[test]
fn given_no_vault_when_swap_out_vault_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().join("no-vault-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(temp.path().to_path_buf()));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_out_vault(&project_dir).unwrap();

    // Assert
    assert!(
        result.is_empty(),
        "should return empty for project without vault"
    );
}

// ============================================================
// swap_out_all_vaults() tests
// ============================================================

#[test]
fn given_no_vaults_when_swap_out_all_vaults_then_returns_empty() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let results = service.swap_out_all_vaults(&vaults_dir).unwrap();

    // Assert
    assert!(results.is_empty(), "should return empty when no vaults");
}

#[test]
fn given_vault_with_swapped_files_when_swap_out_all_vaults_then_swaps_out() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let vault_base = settings.vaults_dir();

    // Create and swap in a file
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();
    std::fs::write(project_dir.join("config.yml"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    service
        .swap_in(&project_dir, &[project_dir.join("config.yml")])
        .unwrap();

    // Verify it's swapped in
    let status_before = service.status_all_vaults(&vault_base).unwrap();
    assert_eq!(status_before.len(), 1, "should have one vault with swap");

    // Act
    let results = service.swap_out_all_vaults(&vault_base).unwrap();

    // Assert
    assert_eq!(results.len(), 1, "should process one vault");
    assert_eq!(
        results[0].active_swaps.len(),
        1,
        "should have swapped out one file"
    );

    // Verify it's now clean
    let status_after = service.status_all_vaults(&vault_base).unwrap();
    assert!(
        status_after.is_empty(),
        "should be clean after swap_out_all_vaults"
    );
}

#[test]
fn given_multiple_vaults_when_swap_out_all_vaults_then_swaps_out_all() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = VaultService::new(fs.clone(), settings.clone());

    // Create two projects with swapped files
    let project1 = temp.path().join("project1");
    std::fs::create_dir_all(&project1).unwrap();
    let vault1 = vault_service.init(&project1, false).unwrap();
    std::fs::write(project1.join("file1.yml"), "orig1\n").unwrap();
    let swap_dir1 = vault1.path.join("swap");
    std::fs::create_dir_all(&swap_dir1).unwrap();
    std::fs::write(swap_dir1.join("file1.yml"), "override1\n").unwrap();

    let project2 = temp.path().join("project2");
    std::fs::create_dir_all(&project2).unwrap();
    let vault2 = vault_service.init(&project2, false).unwrap();
    std::fs::write(project2.join("file2.yml"), "orig2\n").unwrap();
    let swap_dir2 = vault2.path.join("swap");
    std::fs::create_dir_all(&swap_dir2).unwrap();
    std::fs::write(swap_dir2.join("file2.yml"), "override2\n").unwrap();

    let vault_service = Arc::new(vault_service);
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in both
    service
        .swap_in(&project1, &[project1.join("file1.yml")])
        .unwrap();
    service
        .swap_in(&project2, &[project2.join("file2.yml")])
        .unwrap();

    // Verify both swapped
    let status_before = service.status_all_vaults(&vaults_dir).unwrap();
    assert_eq!(status_before.len(), 2, "should have two vaults with swaps");

    // Act
    let results = service.swap_out_all_vaults(&vaults_dir).unwrap();

    // Assert
    assert_eq!(results.len(), 2, "should process both vaults");

    // Verify all clean
    let status_after = service.status_all_vaults(&vaults_dir).unwrap();
    assert!(status_after.is_empty(), "should be all clean after");
}

// ============================================================
// Stale sourceDir tests (vault metadata points to non-existent project)
// ============================================================

/// Helper to create a valid dot.envrc with custom sourceDir
fn create_dot_envrc_content(source_dir: &str, sentinel_id: &str, vault_path: &str) -> String {
    format!(
        r#"#------------------------------- rsenv start --------------------------------
# config.relative = true
# config.version = 2
# state.sentinel = '{sentinel_id}'
# state.timestamp = '2024-01-01T00:00:00.000000+00:00'
# state.sourceDir = '{source_dir}'
export RSENV_VAULT={vault_path}
#-------------------------------- rsenv vars --------------------------------
#-------------------------------- rsenv end ---------------------------------"#,
        sentinel_id = sentinel_id,
        source_dir = source_dir,
        vault_path = vault_path,
    )
}

#[test]
fn given_vault_with_stale_source_dir_when_status_all_vaults_then_still_detects_swaps() {
    // Arrange
    // This tests the bug: status_all_vaults should detect swapped-in files
    // even when state.sourceDir in dot.envrc points to non-existent path
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    // Create a vault directory with swap files
    let vault_path = vaults_dir.join("test-vault-abc123");
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();

    // Create dot.envrc with sourceDir pointing to NON-EXISTENT path
    let non_existent_project = temp.path().join("this-project-does-not-exist");
    let dot_envrc_content = create_dot_envrc_content(
        &non_existent_project.to_string_lossy(),
        "test-vault-abc123",
        &vault_path.to_string_lossy(),
    );
    std::fs::write(vault_path.join("dot.envrc"), &dot_envrc_content).unwrap();

    // Create a swap file that is "swapped in" (has sentinel marker)
    let hostname = get_hostname();
    std::fs::write(
        swap_dir.join("config.yml.rsenv_original"),
        "original content\n",
    )
    .unwrap();
    std::fs::write(
        swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname)),
        "override content\n",
    )
    .unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let statuses = service.status_all_vaults(&vaults_dir).unwrap();

    // Assert - should detect the swap even though sourceDir doesn't exist
    assert_eq!(
        statuses.len(),
        1,
        "should detect vault with active swap despite stale sourceDir"
    );
    assert_eq!(
        statuses[0].active_swaps.len(),
        1,
        "should detect one active swap"
    );
    assert!(
        matches!(statuses[0].active_swaps[0].state, SwapState::In { .. }),
        "swap should be in 'In' state"
    );
}

#[test]
fn given_vault_with_stale_source_dir_when_swap_out_all_vaults_then_warns_and_skips() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let base_dir = temp.path().to_path_buf();

    let settings = Arc::new(test_settings(base_dir));
    let vaults_dir = settings.vaults_dir();
    std::fs::create_dir_all(&vaults_dir).unwrap();

    let vault_path = vaults_dir.join("test-vault-abc123");
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();

    // Create dot.envrc with stale sourceDir
    let non_existent_project = temp.path().join("this-project-does-not-exist");
    let dot_envrc_content = create_dot_envrc_content(
        &non_existent_project.to_string_lossy(),
        "test-vault-abc123",
        &vault_path.to_string_lossy(),
    );
    std::fs::write(vault_path.join("dot.envrc"), &dot_envrc_content).unwrap();

    // Create "swapped in" file
    let hostname = get_hostname();
    std::fs::write(swap_dir.join("config.yml.rsenv_original"), "original\n").unwrap();
    std::fs::write(
        swap_dir.join(format!("config.yml@@{}@@rsenv_active", hostname)),
        "override\n",
    )
    .unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap_out_all_vaults should gracefully handle stale sourceDir
    let results = service.swap_out_all_vaults(&vaults_dir).unwrap();

    // Assert - should return empty (skipped because can't swap without valid project)
    // but should NOT error
    assert!(
        results.is_empty(),
        "should skip vaults with stale sourceDir (can't swap without valid project path)"
    );
}

// ============================================================
// Metadata validation tests
// ============================================================

#[test]
fn given_mismatched_source_dir_when_status_then_still_works() {
    // This tests that status() works even when vault metadata has wrong sourceDir
    // The warning is printed but operation succeeds
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create a swap file
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override\n").unwrap();
    std::fs::write(project_dir.join("config.yml"), "original\n").unwrap();

    // Manually corrupt the sourceDir in dot.envrc to simulate stale metadata
    let dot_envrc_path = vault_path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc_path).unwrap();
    let corrupted = content.replace(
        &project_dir.to_string_lossy().to_string(),
        "/some/nonexistent/path",
    );
    std::fs::write(&dot_envrc_path, corrupted).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - status should still work despite mismatched metadata
    let status = service.status(&project_dir).unwrap();

    // Assert - should find the swap file
    assert_eq!(
        status.len(),
        1,
        "should still find swap file despite metadata mismatch"
    );
}

// ============================================================
// Dotfile path neutralization tests
// ============================================================

#[test]
fn given_dotfile_path_when_swap_init_then_creates_neutralized_vault_path() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create .github/copilot-instructions.md in project
    let github_dir = project_dir.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    let project_file = github_dir.join("copilot-instructions.md");
    std::fs::write(&project_file, "# Instructions\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let initialized = service
        .swap_init(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(initialized.len(), 1);

    // Vault should have dot.github/copilot-instructions.md (neutralized)
    let swap_dir = vault_path.join("swap");
    let neutralized_path = swap_dir.join("dot.github").join("copilot-instructions.md");
    assert!(
        neutralized_path.exists(),
        "vault should have neutralized path: {}",
        neutralized_path.display()
    );

    // Non-neutralized path should NOT exist
    let non_neutralized = swap_dir.join(".github").join("copilot-instructions.md");
    assert!(
        !non_neutralized.exists(),
        "non-neutralized path should NOT exist: {}",
        non_neutralized.display()
    );

    // Project file should be removed
    assert!(
        !project_file.exists(),
        "project file should be moved to vault"
    );
}

#[test]
fn given_nested_dotfile_path_when_swap_init_then_neutralizes_all_segments() {
    // Test: .hidden/.secret/config.yml → dot.hidden/dot.secret/config.yml
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create nested dotfile path
    let nested_dir = project_dir.join(".hidden").join(".secret");
    std::fs::create_dir_all(&nested_dir).unwrap();
    let project_file = nested_dir.join("config.yml");
    std::fs::write(&project_file, "secret: value\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    service
        .swap_init(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - all dotfile segments should be neutralized
    let swap_dir = vault_path.join("swap");
    let neutralized_path = swap_dir
        .join("dot.hidden")
        .join("dot.secret")
        .join("config.yml");
    assert!(
        neutralized_path.exists(),
        "all dotfile segments should be neutralized: {}",
        neutralized_path.display()
    );
}

#[test]
fn given_dotfile_in_vault_when_swap_in_then_finds_neutralized_path() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let hostname = get_hostname();

    // Create original in project
    let github_dir = project_dir.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    let project_file = github_dir.join("config.md");
    std::fs::write(&project_file, "original\n").unwrap();

    // Create override at NEUTRALIZED path in vault
    let swap_dir = vault_path.join("swap");
    let vault_dotgithub = swap_dir.join("dot.github");
    std::fs::create_dir_all(&vault_dotgithub).unwrap();
    std::fs::write(vault_dotgithub.join("config.md"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let swapped = service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert
    assert_eq!(swapped.len(), 1);
    assert_eq!(
        swapped[0].state,
        SwapState::In {
            hostname: hostname.clone()
        }
    );

    // Project file should have override content
    let content = std::fs::read_to_string(&project_file).unwrap();
    assert!(content.contains("override"), "should have override content");

    // Sentinel should be at neutralized path
    let sentinel = vault_dotgithub.join(format!("config.md@@{}@@rsenv_active", hostname));
    assert!(
        sentinel.exists(),
        "sentinel should be at neutralized path: {}",
        sentinel.display()
    );
}

#[test]
fn given_dotfile_swapped_in_when_swap_out_then_restores_to_neutralized_path() {
    // Full round-trip test for dotfile paths
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create .github/config.md in project
    let github_dir = project_dir.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    let project_file = github_dir.join("config.md");
    std::fs::write(&project_file, "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Initialize (move to vault)
    service
        .swap_init(&project_dir, &[project_file.clone()])
        .unwrap();

    // Swap in
    service
        .swap_in(&project_dir, &[project_file.clone()])
        .unwrap();

    // Modify the file
    std::fs::write(&project_file, "modified\n").unwrap();

    // Act - swap out
    service
        .swap_out(&project_dir, &[project_file.clone()])
        .unwrap();

    // Assert - vault file should be at neutralized path with modifications
    let swap_dir = vault_path.join("swap");
    let vault_file = swap_dir.join("dot.github").join("config.md");
    assert!(
        vault_file.exists(),
        "vault file should be at neutralized path: {}",
        vault_file.display()
    );

    let vault_content = std::fs::read_to_string(&vault_file).unwrap();
    assert!(
        vault_content.contains("modified"),
        "vault should capture modifications"
    );

    // Non-neutralized path should NOT exist
    let wrong_path = swap_dir.join(".github").join("config.md");
    assert!(
        !wrong_path.exists(),
        "non-neutralized path should NOT exist"
    );
}

// ============================================================
// Broken symlink tests
// ============================================================

#[test]
fn given_broken_symlink_in_vault_when_swap_in_then_succeeds() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create original in project
    let project_file = project_dir.join("link.md");
    std::fs::write(&project_file, "original\n").unwrap();

    // Create a BROKEN symlink in vault (target doesn't exist)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    let vault_link = swap_dir.join("link.md");

    #[cfg(unix)]
    std::os::unix::fs::symlink("../nonexistent-target.md", &vault_link).unwrap();

    // Verify the symlink is broken (target doesn't exist)
    assert!(
        vault_link.symlink_metadata().is_ok(),
        "symlink should exist"
    );
    assert!(
        !vault_link.exists(),
        "symlink target should NOT exist (broken)"
    );

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act - swap in should work with broken symlink
    let result = service.swap_in(&project_dir, &[project_file.clone()]);

    // Assert - should succeed
    assert!(
        result.is_ok(),
        "swap_in should work with broken symlink: {:?}",
        result.err()
    );
    let swapped = result.unwrap();
    assert_eq!(swapped.len(), 1);

    // Project should now have the symlink
    assert!(
        project_file.symlink_metadata().is_ok(),
        "project should have the symlink"
    );
}

#[test]
fn given_broken_symlink_when_swap_init_then_preserves_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create a broken symlink in project
    let project_link = project_dir.join("broken-link.md");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../nonexistent.md", &project_link).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let result = service.swap_init(&project_dir, &[project_link.clone()]);

    // Assert - should succeed and preserve symlink
    assert!(result.is_ok(), "swap_init should work with broken symlink");

    // Vault should have the symlink
    let swap_dir = vault_path.join("swap");
    let vault_link = swap_dir.join("broken-link.md");
    assert!(
        vault_link.symlink_metadata().is_ok(),
        "vault should have the symlink"
    );

    // Should still be a symlink
    assert!(
        vault_link
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "vault file should be a symlink"
    );
}

#[test]
fn given_broken_symlink_with_dotpath_when_full_roundtrip_then_works() {
    // Full test: .github/instructions.md (symlink) → dot.github/instructions.md
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create .github directory and a broken symlink inside
    let github_dir = project_dir.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    let project_link = github_dir.join("instructions.md");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../AGENTS.md", &project_link).unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act 1: swap init
    let init_result = service.swap_init(&project_dir, &[project_link.clone()]);
    assert!(
        init_result.is_ok(),
        "swap_init failed: {:?}",
        init_result.err()
    );

    // Assert: vault has neutralized path with symlink
    let swap_dir = vault_path.join("swap");
    let vault_link = swap_dir.join("dot.github").join("instructions.md");
    assert!(
        vault_link.symlink_metadata().is_ok(),
        "vault should have symlink at neutralized path: {}",
        vault_link.display()
    );

    // Act 2: swap in
    let in_result = service.swap_in(&project_dir, &[project_link.clone()]);
    assert!(in_result.is_ok(), "swap_in failed: {:?}", in_result.err());

    // Assert: project has the symlink back
    assert!(
        project_link.symlink_metadata().is_ok(),
        "project should have symlink after swap_in"
    );

    // Act 3: swap out
    let out_result = service.swap_out(&project_dir, &[project_link.clone()]);
    assert!(
        out_result.is_ok(),
        "swap_out failed: {:?}",
        out_result.err()
    );

    // Assert: vault has symlink at neutralized path
    assert!(
        vault_link.symlink_metadata().is_ok(),
        "vault should have symlink at neutralized path after swap_out"
    );
}

// ============================================================
// status_quiet() tests — silent mode without metadata warnings
// ============================================================

#[test]
fn given_no_vault_when_status_quiet_then_returns_empty() {
    // Arrange: project dir with no vault (no .envrc symlink)
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().join("unmanaged");
    std::fs::create_dir_all(&project_dir).unwrap();

    let base_dir = temp.path().to_path_buf();
    let settings = Arc::new(test_settings(base_dir));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let status = service.status_quiet(&project_dir).unwrap();

    // Assert
    assert!(
        status.is_empty(),
        "unmanaged project should return empty status"
    );
}

#[test]
fn given_managed_project_when_status_quiet_then_returns_status_without_warnings() {
    // Arrange: managed project with a swap file in "out" state
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create vault swap file (not swapped in — state is Out)
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("config.yml"), "override content\n").unwrap();

    // Create project file
    std::fs::write(project_dir.join("config.yml"), "original content\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act — status_quiet should return status without calling validate_metadata
    let status = service.status_quiet(&project_dir).unwrap();

    // Assert: should find the swap file in "out" state
    assert_eq!(status.len(), 1, "should find one swappable file");
    assert!(
        matches!(status[0].state, SwapState::Out),
        "file should be in Out state"
    );
}

// ============================================================
// Silent mode exit code logic tests (project-level)
// These test the data conditions that drive CLI exit codes:
//   vault_service.get() = None  → exit 2 (unmanaged)
//   status_quiet() all Out/empty → exit 0 (clean)
//   status_quiet() any In        → exit 1 (dirty)
// ============================================================

#[test]
fn given_unmanaged_dir_when_silent_status_then_exit_2() {
    // Arrange: directory with no vault — simulates exit code 2
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().join("no_vault");
    std::fs::create_dir_all(&project_dir).unwrap();

    let base_dir = temp.path().to_path_buf();
    let settings = Arc::new(test_settings(base_dir));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));

    // Act: check vault existence (what CLI does before status_quiet)
    let vault = vault_service.get(&project_dir).unwrap();

    // Assert: no vault → CLI would exit 2
    assert!(vault.is_none(), "unmanaged dir should have no vault");
}

#[test]
fn given_managed_clean_when_silent_status_then_exit_0() {
    // Arrange: managed project, swap file in Out state → exit 0 (clean)
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Vault has swap file but NOT swapped in
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("app.conf"), "override\n").unwrap();
    std::fs::write(project_dir.join("app.conf"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act
    let status = service.status_quiet(&project_dir).unwrap();

    // Assert: no SwapState::In → CLI would exit 0
    let has_active = status
        .iter()
        .any(|f| matches!(&f.state, SwapState::In { .. }));
    assert!(!has_active, "clean project should have no active swaps");
}

#[test]
fn given_managed_dirty_when_silent_status_then_exit_1() {
    // Arrange: managed project, swap in a file → exit 1 (dirty)
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);

    // Create swap file in vault
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("settings.yml"), "override\n").unwrap();
    std::fs::write(project_dir.join("settings.yml"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in to make it dirty
    service
        .swap_in(&project_dir, &[project_dir.join("settings.yml")])
        .unwrap();

    // Act
    let status = service.status_quiet(&project_dir).unwrap();

    // Assert: has SwapState::In → CLI would exit 1
    let has_active = status
        .iter()
        .any(|f| matches!(&f.state, SwapState::In { .. }));
    assert!(has_active, "dirty project should have active swaps");
}

// ============================================================
// Global silent mode regression tests (--global --silent)
// These verify backward compatibility: exit 0 (no active swaps)
// and exit 1 (has active swaps) are preserved unchanged.
// ============================================================

#[test]
fn given_global_silent_no_swaps_when_status_then_exit_0() {
    // Arrange: managed project with swap file in Out state
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let vault_base = settings.vaults_dir();

    // Vault has swap file but NOT swapped in
    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("app.conf"), "override\n").unwrap();
    std::fs::write(project_dir.join("app.conf"), "original\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Act: same call as --global --silent CLI path
    let statuses = service.status_all_vaults(&vault_base).unwrap();
    let has_active = !statuses.is_empty();

    // Assert: no active swaps → CLI would exit 0
    assert!(
        !has_active,
        "clean global state should have no active swaps (exit 0)"
    );
}

#[test]
fn given_global_silent_active_swaps_when_status_then_exit_1() {
    // Arrange: managed project with a file swapped in
    let temp = TempDir::new().unwrap();
    let (project_dir, vault_path, settings) = setup_project(&temp);
    let vault_base = settings.vaults_dir();

    let project_file = project_dir.join("docker-compose.yml");
    std::fs::write(&project_file, "original\n").unwrap();

    let swap_dir = vault_path.join("swap");
    std::fs::create_dir_all(&swap_dir).unwrap();
    std::fs::write(swap_dir.join("docker-compose.yml"), "override\n").unwrap();

    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let service = SwapService::new(fs, vault_service, settings);

    // Swap in to create active state
    service.swap_in(&project_dir, &[project_file]).unwrap();

    // Act: same call as --global --silent CLI path
    let statuses = service.status_all_vaults(&vault_base).unwrap();
    let has_active = !statuses.is_empty();

    // Assert: has active swaps → CLI would exit 1
    assert!(
        has_active,
        "dirty global state should have active swaps (exit 1)"
    );
}
