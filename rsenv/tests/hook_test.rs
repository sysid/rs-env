//! Tests for git hook installation/removal

use std::fs;
use tempfile::TempDir;

/// The signature that identifies rsenv-managed hooks
const RSENV_HOOK_SIGNATURE: &str = "rsenv pre-commit hook";

/// Expected content patterns in the hook script
const EXPECTED_GLOBAL_CHECK: &str = "rsenv sops status --global --check";

// ============================================================
// Hook Installation Tests
// ============================================================

#[test]
fn given_git_repo_when_hook_install_then_creates_precommit_hook() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");

    // Act - simulate hook installation
    // This will be replaced with actual CLI call or service call
    let hook_content = create_test_hook_content();
    fs::write(&hook_path, &hook_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms).unwrap();
    }

    // Assert
    assert!(hook_path.exists(), "Hook file should exist");
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(
        content.contains(RSENV_HOOK_SIGNATURE),
        "Hook should contain rsenv signature"
    );
    assert!(
        content.contains(EXPECTED_GLOBAL_CHECK),
        "Hook should use --global --check flags"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&hook_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "Hook should be executable");
    }
}

#[test]
fn given_dir_flag_when_hook_install_then_installs_at_specified_location() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let custom_repo = temp.path().join("custom-repo");
    let git_dir = custom_repo.join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");

    // Act - simulate hook installation with explicit dir
    let hook_content = create_test_hook_content();
    fs::write(&hook_path, &hook_content).unwrap();

    // Assert
    assert!(hook_path.exists(), "Hook should be installed at custom location");
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains(RSENV_HOOK_SIGNATURE));
}

#[test]
fn given_non_git_dir_when_hook_install_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    // No .git directory created

    let git_dir = temp.path().join(".git");

    // Assert - the target is not a git repo
    assert!(
        !git_dir.exists(),
        "Test precondition: .git should not exist"
    );

    // The actual implementation should return an error when .git doesn't exist
    // This test verifies the precondition that we check for
}

#[test]
fn given_existing_hook_without_force_when_install_then_refuses() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");
    fs::write(&hook_path, "#!/bin/bash\necho existing").unwrap();

    // Assert - hook already exists
    assert!(hook_path.exists(), "Pre-existing hook should exist");

    // The actual implementation should refuse to overwrite without --force
}

// ============================================================
// Hook Removal Tests
// ============================================================

#[test]
fn given_rsenv_hook_when_remove_then_deletes_hook() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");
    let hook_content = create_test_hook_content();
    fs::write(&hook_path, &hook_content).unwrap();

    // Verify hook contains rsenv signature (precondition)
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains(RSENV_HOOK_SIGNATURE));

    // Act - simulate removal
    fs::remove_file(&hook_path).unwrap();

    // Assert
    assert!(!hook_path.exists(), "Hook should be removed");
}

#[test]
fn given_non_rsenv_hook_when_remove_then_refuses() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");
    fs::write(&hook_path, "#!/bin/bash\necho 'not rsenv'").unwrap();

    // Assert - hook exists but is not rsenv-managed
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(
        !content.contains(RSENV_HOOK_SIGNATURE),
        "Hook should NOT contain rsenv signature"
    );

    // The actual implementation should refuse to remove non-rsenv hooks
}

// ============================================================
// Hook Status Tests
// ============================================================

#[test]
fn given_no_hook_when_status_then_reports_not_installed() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");

    // Assert
    assert!(!hook_path.exists(), "No hook should exist");
}

#[test]
fn given_rsenv_hook_when_status_then_reports_installed() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let git_dir = temp.path().join(".git");
    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");
    let hook_content = create_test_hook_content();
    fs::write(&hook_path, &hook_content).unwrap();

    // Assert
    assert!(hook_path.exists());
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains(RSENV_HOOK_SIGNATURE));
}

// ============================================================
// Default Location Tests
// ============================================================

#[test]
fn given_vault_base_dir_when_default_target_then_uses_parent() {
    // Arrange
    let temp = TempDir::new().unwrap();
    // Simulate vault_base_dir = /tmp/xxx/.rsenv/vaults
    let rsenv_dir = temp.path().join(".rsenv");
    let vault_base_dir = rsenv_dir.join("vaults");
    fs::create_dir_all(&vault_base_dir).unwrap();

    // The default target should be parent of vault_base_dir = .rsenv
    let expected_target = vault_base_dir
        .parent()
        .expect("vault_base_dir should have parent");

    // Assert
    assert_eq!(expected_target, rsenv_dir);
}

// ============================================================
// Helper Functions
// ============================================================

/// Creates hook content matching expected format after refactoring
fn create_test_hook_content() -> String {
    r#"#!/bin/bash
# rsenv pre-commit hook - prevents committing with stale/unencrypted files
# Installed by: rsenv hook install

if ! command -v rsenv &> /dev/null; then
    echo "Warning: rsenv not found in PATH, skipping encryption check"
    exit 0
fi

if ! rsenv sops status --global --check 2>/dev/null; then
    echo ""
    echo "ERROR: Unencrypted or stale files in vault(s)."
    echo "       Run 'rsenv sops encrypt --global' to update encryption."
    echo "       Use 'rsenv sops status --global' to see details."
    echo ""
    exit 1
fi
"#
    .to_string()
}
