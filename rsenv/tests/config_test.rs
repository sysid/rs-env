//! Integration tests for Settings config loading with layered merge semantics.
//!
//! Merge Semantics:
//! - Defaults → Global: REPLACE (global defines the real baseline)
//! - Global → Vault: UNION with negation support (vault adds project-specific patterns)
//! - Any → Env vars: REPLACE (explicit user override)
//!
//! Note: These tests run without a global config (temp directories only),
//! so they effectively test vault config merging with defaults.

use std::fs;

use tempfile::TempDir;

use rsenv::config::Settings;

// ============================================================
// Settings::load() vault config union merge tests
// ============================================================

/// Test that vault config arrays UNION with current config (defaults when no global).
///
/// When no global config exists, vault config unions with compiled defaults.
/// (If a global config existed, vault would union with global instead.)
#[test]
fn given_vault_config_with_extensions_when_load_then_unions_with_current() {
    // Arrange: Create a vault with local config (no global config)
    let vault_dir = TempDir::new().unwrap();
    let vault_path = vault_dir.path();

    // Vault config adds "yaml" to file_extensions_enc
    // Current (defaults): ["envrc", "env"]
    let vault_config = r#"
[sops]
file_extensions_enc = ["yaml"]
"#;
    fs::write(vault_path.join(".rsenv.toml"), vault_config).unwrap();

    // Act: Load settings with vault
    let settings = Settings::load(Some(vault_path)).expect("load settings");

    // Assert: Should have current (defaults) PLUS vault additions (union merge)
    // Current: ["envrc", "env"], Vault adds: ["yaml"]
    // Expected: ["env", "envrc", "yaml"] (sorted)
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"env".to_string()),
        "Should contain default 'env'"
    );
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"envrc".to_string()),
        "Should contain default 'envrc'"
    );
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"yaml".to_string()),
        "Should contain vault-added 'yaml'"
    );
    assert_eq!(
        settings.sops.file_extensions_enc.len(),
        3,
        "Should have exactly 3 extensions"
    );
}

/// Test that negation prefix removes items from the current config.
#[test]
fn given_vault_config_with_negation_when_load_then_removes_negated_item() {
    // Arrange: Vault config removes "env" from current config (defaults when no global)
    let vault_dir = TempDir::new().unwrap();
    let vault_path = vault_dir.path();

    let vault_config = r#"
[sops]
file_extensions_enc = ["yaml", "!env"]
"#;
    fs::write(vault_path.join(".rsenv.toml"), vault_config).unwrap();

    // Act
    let settings = Settings::load(Some(vault_path)).expect("load settings");

    // Assert: "env" should be removed, "yaml" added
    // Current (defaults): ["envrc", "env"], Vault: ["yaml", "!env"]
    // Expected: ["envrc", "yaml"]
    assert!(
        !settings
            .sops
            .file_extensions_enc
            .contains(&"env".to_string()),
        "'env' should be removed by !env"
    );
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"envrc".to_string()),
        "Should keep 'envrc'"
    );
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"yaml".to_string()),
        "Should add 'yaml'"
    );
    assert_eq!(
        settings.sops.file_extensions_enc.len(),
        2,
        "Should have exactly 2 extensions"
    );
}

/// Test that scalar values from vault config override defaults.
#[test]
fn given_vault_config_with_scalars_when_load_then_overrides_scalars() {
    // Arrange
    let vault_dir = TempDir::new().unwrap();
    let vault_path = vault_dir.path();

    let vault_config = r#"
editor = "nvim"

[sops]
gpg_key = "VAULT-GPG-KEY"
"#;
    fs::write(vault_path.join(".rsenv.toml"), vault_config).unwrap();

    // Act
    let settings = Settings::load(Some(vault_path)).expect("load settings");

    // Assert: Scalars should be replaced
    assert_eq!(settings.editor, "nvim");
    assert_eq!(settings.sops.gpg_key, Some("VAULT-GPG-KEY".to_string()));
}

/// Test that unspecified arrays in vault config inherit from current config.
#[test]
fn given_vault_config_without_array_when_load_then_inherits_current() {
    // Arrange: Vault config specifies gpg_key but NOT arrays
    let vault_dir = TempDir::new().unwrap();
    let vault_path = vault_dir.path();

    let vault_config = r#"
[sops]
gpg_key = "TEST-KEY"
"#;
    fs::write(vault_path.join(".rsenv.toml"), vault_config).unwrap();

    // Act
    let settings = Settings::load(Some(vault_path)).expect("load settings");

    // Assert: Arrays should be inherited from current config (defaults when no global)
    assert!(settings
        .sops
        .file_extensions_enc
        .contains(&"env".to_string()));
    assert!(settings
        .sops
        .file_extensions_enc
        .contains(&"envrc".to_string()));
    assert_eq!(settings.sops.file_extensions_enc.len(), 2);
    // But gpg_key should be from vault
    assert_eq!(settings.sops.gpg_key, Some("TEST-KEY".to_string()));
}

/// Test multiple array fields merge independently.
#[test]
fn given_vault_config_with_multiple_arrays_when_load_then_merges_each_independently() {
    // Arrange
    let vault_dir = TempDir::new().unwrap();
    let vault_path = vault_dir.path();

    let vault_config = r#"
[sops]
file_extensions_enc = ["yaml"]
file_names_enc = ["secrets.txt", "credentials.json"]
"#;
    fs::write(vault_path.join(".rsenv.toml"), vault_config).unwrap();

    // Act
    let settings = Settings::load(Some(vault_path)).expect("load settings");

    // Assert: file_extensions_enc merged (defaults + vault values)
    // Note: may also include values from user's global config
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"yaml".to_string()),
        "Should contain vault-added 'yaml'"
    );
    assert!(
        settings
            .sops
            .file_extensions_enc
            .contains(&"env".to_string()),
        "Should contain default 'env'"
    );
    assert!(
        settings.sops.file_extensions_enc.len() >= 3,
        "Should have at least 3 extensions (defaults + yaml)"
    );

    // Assert: file_names_enc contains vault values (may also have global config values)
    assert!(
        settings
            .sops
            .file_names_enc
            .contains(&"secrets.txt".to_string()),
        "Should contain vault-added 'secrets.txt'"
    );
    assert!(
        settings
            .sops
            .file_names_enc
            .contains(&"credentials.json".to_string()),
        "Should contain vault-added 'credentials.json'"
    );
    assert!(
        settings.sops.file_names_enc.len() >= 2,
        "Should have at least 2 file names from vault"
    );
}
