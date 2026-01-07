//! Tests for VaultService

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::VaultService;
use rsenv::config::Settings;
use rsenv::infrastructure::traits::RealFileSystem;

/// Helper to create a test Settings with custom vault_base_dir
fn test_settings(vault_base_dir: PathBuf) -> Settings {
    Settings {
        vault_base_dir,
        editor: "vim".to_string(),
        sops: Default::default(),
    }
}

// ============================================================
// init() tests
// ============================================================

#[test]
fn given_new_project_when_init_then_creates_vault_directory() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::create_dir_all(&vault_base).unwrap();
    // Canonicalize vault_base for comparison (handles /var -> /private/var on macOS)
    let vault_base_canonical = std::fs::canonicalize(&vault_base).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - vault directory was created
    assert!(vault.path.exists());
    assert!(vault.path.starts_with(&vault_base_canonical));
}

#[test]
fn given_new_project_when_init_then_vault_has_sentinel_id() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - sentinel_id contains project name
    assert!(vault.sentinel_id.starts_with("myproject-"));
    assert!(vault.sentinel_id.len() > "myproject-".len()); // Has hash suffix
}

#[test]
fn given_new_project_when_init_then_creates_envrc_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let _vault = service.init(&project_dir, false).unwrap();

    // Assert - .envrc symlink exists in project
    let envrc_path = project_dir.join(".envrc");
    assert!(envrc_path.is_symlink());

    // Assert - symlink points to dot.envrc in vault
    let target = std::fs::read_link(&envrc_path).unwrap();
    assert!(target.ends_with("dot.envrc"));
}

#[test]
fn given_new_project_when_init_then_creates_dot_envrc_in_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - dot.envrc exists in vault
    let dot_envrc = vault.path.join("dot.envrc");
    assert!(dot_envrc.exists());

    // Assert - contains rsenv section
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(content.contains("# config.version = 2"));
}

#[test]
fn given_new_project_when_init_then_creates_subdirectories() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - subdirectories exist
    assert!(vault.path.join("guarded").exists());
    assert!(vault.path.join("swap").exists());
    assert!(vault.path.join("envs").exists());
}

#[test]
fn given_new_project_when_init_then_creates_default_env_files() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - env files exist
    let envs_dir = vault.path.join("envs");
    assert!(envs_dir.join("local.env").exists());
    assert!(envs_dir.join("test.env").exists());
    assert!(envs_dir.join("int.env").exists());
    assert!(envs_dir.join("prod.env").exists());
}

#[test]
fn given_new_project_when_init_then_env_files_have_correct_content() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - env files have correct content
    let envs_dir = vault.path.join("envs");

    let local_content = std::fs::read_to_string(envs_dir.join("local.env")).unwrap();
    assert_eq!(local_content, "export RUN_ENV=\"local\"\n");

    let prod_content = std::fs::read_to_string(envs_dir.join("prod.env")).unwrap();
    assert_eq!(prod_content, "export RUN_ENV=\"prod\"\n");
}

#[test]
fn given_already_initialized_project_when_init_then_returns_existing_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act - init twice
    let vault1 = service.init(&project_dir, false).unwrap();
    let vault2 = service.init(&project_dir, false).unwrap();

    // Assert - same vault returned
    assert_eq!(vault1.sentinel_id, vault2.sentinel_id);
    assert_eq!(vault1.path, vault2.path);
}

// ============================================================
// get() tests
// ============================================================

#[test]
fn given_initialized_project_when_get_then_returns_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    let vault = service.init(&project_dir, false).unwrap();

    // Act
    let found = service.get(&project_dir).unwrap();

    // Assert
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.sentinel_id, vault.sentinel_id);
}

#[test]
fn given_uninitialized_project_when_get_then_returns_none() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act - don't init, just get
    let found = service.get(&project_dir).unwrap();

    // Assert
    assert!(found.is_none());
}

// ============================================================
// guard() tests
// ============================================================

#[test]
fn given_file_in_project_when_guard_then_moves_to_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a sensitive file
    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "secret: password123\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let vault = service.init(&project_dir, false).unwrap();

    // Act
    let guarded = service.guard(&config_file, false).unwrap();

    // Assert - file moved to vault
    assert!(guarded.vault_path.exists());
    assert!(guarded.vault_path.starts_with(&vault.path));
    let content = std::fs::read_to_string(&guarded.vault_path).unwrap();
    assert!(content.contains("secret: password123"));
}

#[test]
fn given_file_in_project_when_guard_then_creates_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "secret: password123\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Act
    let guarded = service.guard(&config_file, false).unwrap();

    // Assert - project path is now a symlink
    assert!(guarded.project_path.is_symlink());
    let target = std::fs::read_link(&guarded.project_path).unwrap();
    // With relative symlinks, target is relative path, so resolve and compare
    let resolved = guarded.project_path.parent().unwrap().join(&target);
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(&guarded.vault_path).unwrap()
    );
}

#[test]
fn given_file_in_subdir_when_guard_then_preserves_structure() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    let subdir = project_dir.join("config").join("secrets");
    std::fs::create_dir_all(&subdir).unwrap();

    let config_file = subdir.join("api.key");
    std::fs::write(&config_file, "sk-12345\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Act
    let guarded = service.guard(&config_file, false).unwrap();

    // Assert - vault path preserves relative structure
    assert!(guarded.vault_path.to_string_lossy().contains("config"));
    assert!(guarded.vault_path.to_string_lossy().contains("secrets"));
}

#[test]
fn given_uninitialized_project_when_guard_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "secret: password123\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    // Note: NOT calling init()

    // Act
    let result = service.guard(&config_file, false);

    // Assert
    assert!(result.is_err());
}

// ============================================================
// unguard() tests
// ============================================================

#[test]
fn given_guarded_file_when_unguard_then_restores_original() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "secret: password123\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();
    let guarded = service.guard(&config_file, false).unwrap();

    // Act
    service.unguard(&guarded.project_path).unwrap();

    // Assert - symlink replaced with real file
    assert!(!config_file.is_symlink());
    assert!(config_file.is_file());
    let content = std::fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("secret: password123"));
}

#[test]
fn given_guarded_file_when_unguard_then_removes_from_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "secret: password123\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();
    let guarded = service.guard(&config_file, false).unwrap();
    let vault_path = guarded.vault_path.clone();

    // Act
    service.unguard(&guarded.project_path).unwrap();

    // Assert - file removed from vault
    assert!(!vault_path.exists());
}

#[test]
fn given_non_symlink_when_unguard_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_file = project_dir.join("config.yml");
    std::fs::write(&config_file, "not guarded\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();
    // Note: NOT calling guard()

    // Act
    let result = service.unguard(&config_file);

    // Assert
    assert!(result.is_err());
}

// ============================================================
// dot.envrc section tests (confguard-style format)
// ============================================================

#[test]
fn given_new_project_when_init_then_dot_envrc_has_rsenv_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - dot.envrc has rsenv section with start/end delimiters
    let dot_envrc = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains(
            "#------------------------------- rsenv start --------------------------------"
        ),
        "Missing rsenv start delimiter"
    );
    assert!(
        content.contains(
            "#-------------------------------- rsenv end ---------------------------------"
        ),
        "Missing rsenv end delimiter"
    );
}

#[test]
fn given_init_with_relative_when_checking_dot_envrc_then_config_relative_is_true() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act - init with relative symlinks (absolute=false)
    let vault = service.init(&project_dir, false).unwrap();

    // Assert
    let dot_envrc = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("# config.relative = true"),
        "Expected config.relative = true for relative symlinks"
    );
}

#[test]
fn given_init_with_absolute_when_checking_dot_envrc_then_config_relative_is_false() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act - init with absolute symlinks (absolute=true)
    let vault = service.init(&project_dir, true).unwrap();

    // Assert
    let dot_envrc = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("# config.relative = false"),
        "Expected config.relative = false for absolute symlinks"
    );
}

#[test]
fn given_init_when_checking_dot_envrc_then_has_rsenv_vault_export() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - has RSENV_VAULT export pointing to vault root
    let dot_envrc = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("export RSENV_VAULT="),
        "Missing RSENV_VAULT export"
    );
    // RSENV_VAULT should NOT contain /guarded (points to vault root)
    assert!(
        !content.contains("RSENV_VAULT=") || !content.contains("/guarded"),
        "RSENV_VAULT should point to vault root, not /guarded subdirectory"
    );
}

#[test]
fn given_init_when_checking_dot_envrc_then_has_state_metadata() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - has state metadata comments
    let dot_envrc = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc).unwrap();
    assert!(
        content.contains("# state.sentinel = 'myproject-"),
        "Missing state.sentinel"
    );
    assert!(
        content.contains("# state.timestamp = '"),
        "Missing state.timestamp"
    );
    assert!(
        content.contains("# state.sourceDir = '"),
        "Missing state.sourceDir"
    );
    assert!(
        content.contains("# config.version = 2"),
        "Missing config.version"
    );
}

// ============================================================
// reset() tests
// ============================================================

#[test]
fn given_initialized_project_when_reset_then_removes_envrc_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Verify .envrc is a symlink before reset
    let envrc_path = project_dir.join(".envrc");
    assert!(
        envrc_path.is_symlink(),
        ".envrc should be symlink before reset"
    );

    // Act
    service.reset(&project_dir).unwrap();

    // Assert - .envrc is no longer a symlink
    assert!(
        !envrc_path.is_symlink(),
        ".envrc should not be symlink after reset"
    );
}

#[test]
fn given_guarded_files_when_reset_then_restores_all_files() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create files to guard
    let file1 = project_dir.join("config.yml");
    let file2 = project_dir.join("secrets.env");
    std::fs::write(&file1, "config content").unwrap();
    std::fs::write(&file2, "secret content").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Guard both files
    service.guard(&file1, false).unwrap();
    service.guard(&file2, false).unwrap();

    // Verify they are symlinks
    assert!(file1.is_symlink());
    assert!(file2.is_symlink());

    // Act
    let restored_count = service.reset(&project_dir).unwrap();

    // Assert - both files restored
    assert_eq!(restored_count, 2);
    assert!(!file1.is_symlink());
    assert!(!file2.is_symlink());
    assert!(file1.is_file());
    assert!(file2.is_file());
    assert_eq!(std::fs::read_to_string(&file1).unwrap(), "config content");
    assert_eq!(std::fs::read_to_string(&file2).unwrap(), "secret content");
}

#[test]
fn given_existing_envrc_when_init_then_preserves_content_in_dot_envrc() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create existing .envrc with user content
    let envrc_path = project_dir.join(".envrc");
    std::fs::write(&envrc_path, "original .envrc content\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let vault = service.init(&project_dir, false).unwrap();

    // Assert - user content is preserved in dot.envrc (moved, not backed up)
    let dot_envrc_path = vault.path.join("dot.envrc");
    let content = std::fs::read_to_string(&dot_envrc_path).unwrap();
    assert!(
        content.contains("original .envrc content"),
        "User's original content should be preserved in dot.envrc"
    );
    // Also has rsenv section injected
    assert!(
        content.contains("export RSENV_VAULT="),
        "rsenv section should be injected"
    );

    // Act - reset should restore original content without rsenv section
    service.reset(&project_dir).unwrap();

    // Assert - original content restored, rsenv section removed
    let restored_content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(
        restored_content.contains("original .envrc content"),
        "Original content should be restored"
    );
    assert!(
        !restored_content.contains("export RSENV_VAULT="),
        "rsenv section should be removed after reset"
    );
}

#[test]
fn given_no_backup_when_reset_then_moves_dot_envrc_and_removes_section() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Act
    service.reset(&project_dir).unwrap();

    // Assert - .envrc exists and doesn't have rsenv section
    let envrc_path = project_dir.join(".envrc");
    assert!(envrc_path.exists());
    let content = std::fs::read_to_string(&envrc_path).unwrap();
    assert!(
        !content.contains("#------------------------------- rsenv start"),
        "rsenv section should be removed"
    );
    assert!(
        !content.contains("#-------------------------------- rsenv end"),
        "rsenv section should be removed"
    );
}

#[test]
fn given_reset_when_checking_vault_then_vault_still_exists() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let vault = service.init(&project_dir, false).unwrap();
    let vault_path = vault.path.clone();

    // Act
    service.reset(&project_dir).unwrap();

    // Assert - vault directory still exists
    assert!(vault_path.exists(), "vault should still exist after reset");
    assert!(vault_path.join("guarded").exists());
    assert!(vault_path.join("swap").exists());
    assert!(vault_path.join("envs").exists());
}

#[test]
fn given_uninitialized_project_when_reset_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    // Note: NOT calling init()

    // Act
    let result = service.reset(&project_dir);

    // Assert
    assert!(result.is_err());
}

#[test]
fn given_nested_guarded_files_when_reset_then_preserves_directory_structure() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    let subdir = project_dir.join("config").join("secrets");
    std::fs::create_dir_all(&subdir).unwrap();

    let nested_file = subdir.join("api.key");
    std::fs::write(&nested_file, "secret-api-key").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Guard the nested file
    service.guard(&nested_file, false).unwrap();
    assert!(nested_file.is_symlink());

    // Act
    let restored_count = service.reset(&project_dir).unwrap();

    // Assert - file restored with correct content at correct path
    assert_eq!(restored_count, 1);
    assert!(!nested_file.is_symlink());
    assert!(nested_file.is_file());
    assert_eq!(
        std::fs::read_to_string(&nested_file).unwrap(),
        "secret-api-key"
    );
}

// ============================================================
// Directory guard/unguard tests
// ============================================================

#[test]
fn given_directory_when_guard_then_moves_to_vault_and_creates_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a directory with files
    let config_dir = project_dir.join("secrets");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("api.key"), "sk-12345\n").unwrap();
    std::fs::write(config_dir.join("db.key"), "postgres://secret\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let vault = service.init(&project_dir, false).unwrap();

    // Act
    let guarded = service.guard(&config_dir, false).unwrap();

    // Assert - directory moved to vault
    assert!(guarded.vault_path.exists());
    assert!(guarded.vault_path.is_dir());
    assert!(guarded.vault_path.starts_with(&vault.path));

    // Assert - all files preserved in vault
    let vault_api_key = guarded.vault_path.join("api.key");
    let vault_db_key = guarded.vault_path.join("db.key");
    assert!(vault_api_key.exists());
    assert!(vault_db_key.exists());
    assert_eq!(
        std::fs::read_to_string(&vault_api_key).unwrap(),
        "sk-12345\n"
    );
    assert_eq!(
        std::fs::read_to_string(&vault_db_key).unwrap(),
        "postgres://secret\n"
    );

    // Assert - project path is now a symlink
    assert!(guarded.project_path.is_symlink());
    let target = std::fs::read_link(&guarded.project_path).unwrap();
    let resolved = guarded.project_path.parent().unwrap().join(&target);
    assert_eq!(
        std::fs::canonicalize(&resolved).unwrap(),
        std::fs::canonicalize(&guarded.vault_path).unwrap()
    );
}

#[test]
fn given_guarded_directory_when_unguard_then_restores() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a directory with nested structure
    let config_dir = project_dir.join("secrets");
    std::fs::create_dir_all(config_dir.join("nested")).unwrap();
    std::fs::write(config_dir.join("api.key"), "sk-12345\n").unwrap();
    std::fs::write(config_dir.join("nested/deep.key"), "deep-secret\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();
    let guarded = service.guard(&config_dir, false).unwrap();
    let vault_path = guarded.vault_path.clone();

    // Act
    service.unguard(&guarded.project_path).unwrap();

    // Assert - symlink removed, directory restored
    assert!(!config_dir.is_symlink());
    assert!(config_dir.is_dir());

    // Assert - all files restored with correct content
    assert_eq!(
        std::fs::read_to_string(config_dir.join("api.key")).unwrap(),
        "sk-12345\n"
    );
    assert_eq!(
        std::fs::read_to_string(config_dir.join("nested/deep.key")).unwrap(),
        "deep-secret\n"
    );

    // Assert - removed from vault
    assert!(!vault_path.exists());
}

#[test]
fn given_nested_directory_when_guard_then_preserves_structure_in_vault() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    let subdir = project_dir.join("config").join("env");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create nested directory
    let secrets_dir = subdir.join("secrets");
    std::fs::create_dir_all(&secrets_dir).unwrap();
    std::fs::write(secrets_dir.join("key.pem"), "-----BEGIN KEY-----\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);
    let _vault = service.init(&project_dir, false).unwrap();

    // Act
    let guarded = service.guard(&secrets_dir, false).unwrap();

    // Assert - vault path preserves relative structure
    let vault_path_str = guarded.vault_path.to_string_lossy();
    assert!(
        vault_path_str.contains("config"),
        "Should contain 'config' in path"
    );
    assert!(
        vault_path_str.contains("env"),
        "Should contain 'env' in path"
    );
    assert!(
        vault_path_str.contains("secrets"),
        "Should contain 'secrets' in path"
    );
}

// ============================================================
// reconnect() tests
// ============================================================

#[test]
fn given_deleted_symlink_when_reconnect_then_recreates_symlink() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Init project
    let vault = service.init(&project_dir, false).unwrap();
    let dot_envrc_path = vault.path.join("dot.envrc");

    // Delete the .envrc symlink
    let envrc_path = project_dir.join(".envrc");
    std::fs::remove_file(&envrc_path).unwrap();
    assert!(!envrc_path.exists());

    // Act
    let reconnected = service.reconnect(&dot_envrc_path, &project_dir).unwrap();

    // Assert
    assert!(envrc_path.is_symlink());
    assert_eq!(reconnected.sentinel_id, vault.sentinel_id);
}

#[test]
fn given_moved_project_when_reconnect_then_updates_source_dir() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let old_project_dir = temp.path().join("old_location");
    let new_project_dir = temp.path().join("new_location");
    std::fs::create_dir_all(&old_project_dir).unwrap();
    std::fs::create_dir_all(&new_project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Init at old location
    let vault = service.init(&old_project_dir, false).unwrap();
    let dot_envrc_path = vault.path.join("dot.envrc");

    // Delete the old symlink
    std::fs::remove_file(old_project_dir.join(".envrc")).unwrap();

    // Act - reconnect at new location
    let _reconnected = service
        .reconnect(&dot_envrc_path, &new_project_dir)
        .unwrap();

    // Assert - new symlink exists
    let new_envrc_path = new_project_dir.join(".envrc");
    assert!(new_envrc_path.is_symlink());

    // Assert - state.sourceDir was updated in dot.envrc
    let content = std::fs::read_to_string(&dot_envrc_path).unwrap();
    let new_dir_canonical = std::fs::canonicalize(&new_project_dir).unwrap();
    assert!(
        content.contains(&new_dir_canonical.to_string_lossy().to_string()),
        "dot.envrc should contain new project path"
    );
}

#[test]
fn given_already_linked_when_reconnect_then_returns_success() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Init project (symlink exists)
    let vault = service.init(&project_dir, false).unwrap();
    let dot_envrc_path = vault.path.join("dot.envrc");

    // Act - reconnect when already connected (idempotent)
    let result = service.reconnect(&dot_envrc_path, &project_dir);

    // Assert - should succeed
    assert!(result.is_ok());
    assert_eq!(result.unwrap().sentinel_id, vault.sentinel_id);
}

#[test]
fn given_not_rsenv_file_when_reconnect_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::create_dir_all(&vault_base).unwrap();

    // Create a file without rsenv section
    let fake_envrc = vault_base.join("not-rsenv.envrc");
    std::fs::write(&fake_envrc, "# just a regular file\nexport FOO=bar\n").unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Act
    let result = service.reconnect(&fake_envrc, &project_dir);

    // Assert
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("no rsenv section"));
}

#[test]
fn given_envrc_exists_as_file_when_reconnect_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("vaults");
    let project_dir = temp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    let settings = Arc::new(test_settings(vault_base));
    let fs = Arc::new(RealFileSystem);
    let service = VaultService::new(fs, settings);

    // Init project
    let vault = service.init(&project_dir, false).unwrap();
    let dot_envrc_path = vault.path.join("dot.envrc");

    // Replace symlink with regular file
    let envrc_path = project_dir.join(".envrc");
    std::fs::remove_file(&envrc_path).unwrap();
    std::fs::write(&envrc_path, "regular file").unwrap();

    // Act
    let result = service.reconnect(&dot_envrc_path, &project_dir);

    // Assert
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cannot overwrite"));
}
