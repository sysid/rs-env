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

// ============================================================
// sops.encrypt_on_commit
// ============================================================

/// `rsenv vault commit` re-encrypts the vault before committing so the `.enc` files
/// it stages match the plaintext. On by default: committing stale ciphertext is the
/// failure mode this key exists to prevent.
#[test]
fn given_no_config_when_load_then_encrypt_on_commit_is_enabled() {
    let vault_dir = TempDir::new().unwrap();

    let settings = Settings::load(Some(vault_dir.path())).expect("load settings");

    assert!(
        settings.sops.encrypt_on_commit,
        "encrypt_on_commit must default to true"
    );
}

#[test]
fn given_vault_config_disabling_encrypt_on_commit_when_load_then_disabled() {
    let vault_dir = TempDir::new().unwrap();
    let vault_config = r#"
[sops]
encrypt_on_commit = false
"#;
    fs::write(vault_dir.path().join(".rsenv.toml"), vault_config).unwrap();

    let settings = Settings::load(Some(vault_dir.path())).expect("load settings");

    assert!(
        !settings.sops.encrypt_on_commit,
        "vault config must be able to turn encrypt_on_commit off"
    );
}

#[test]
fn given_vault_config_without_encrypt_on_commit_when_load_then_keeps_default() {
    let vault_dir = TempDir::new().unwrap();
    let vault_config = r#"
[sops]
file_extensions_enc = ["yaml"]
"#;
    fs::write(vault_dir.path().join(".rsenv.toml"), vault_config).unwrap();

    let settings = Settings::load(Some(vault_dir.path())).expect("load settings");

    assert!(
        settings.sops.encrypt_on_commit,
        "an unrelated vault key must not disturb encrypt_on_commit"
    );
}

// ============================================================
// Editor resolution
//
// One place decides which editor rsenv spawns: Settings. Every command reads
// settings.editor, so a config file beats the environment and nothing resolves
// the editor on its own. $VISUAL wins over $EDITOR, per the usual convention
// that VISUAL names the full-screen editor.
// ============================================================

/// Serialised by the mandatory `--test-threads=1`; these mutate process env.
fn with_editor_env<T>(visual: Option<&str>, editor: Option<&str>, body: impl FnOnce() -> T) -> T {
    let prev_visual = std::env::var("VISUAL").ok();
    let prev_editor = std::env::var("EDITOR").ok();

    match visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match editor {
        Some(e) => std::env::set_var("EDITOR", e),
        None => std::env::remove_var("EDITOR"),
    }

    let result = body();

    match prev_visual {
        Some(v) => std::env::set_var("VISUAL", v),
        None => std::env::remove_var("VISUAL"),
    }
    match prev_editor {
        Some(e) => std::env::set_var("EDITOR", e),
        None => std::env::remove_var("EDITOR"),
    }

    result
}

#[test]
fn given_visual_and_editor_when_defaulting_then_visual_wins() {
    let settings = with_editor_env(Some("nvim"), Some("vim"), Settings::default);

    assert_eq!(settings.editor, "nvim");
}

#[test]
fn given_only_editor_when_defaulting_then_editor_is_used() {
    let settings = with_editor_env(None, Some("hx"), Settings::default);

    assert_eq!(settings.editor, "hx");
}

#[test]
fn given_neither_when_defaulting_then_falls_back_to_vim() {
    let settings = with_editor_env(None, None, Settings::default);

    assert_eq!(settings.editor, "vim");
}

#[test]
fn given_config_file_when_loaded_then_it_beats_the_environment() {
    let vault_dir = TempDir::new().unwrap();
    fs::write(vault_dir.path().join(".rsenv.toml"), "editor = \"nvim\"\n").unwrap();

    let settings = with_editor_env(Some("emacs"), Some("emacs"), || {
        Settings::load(Some(vault_dir.path())).expect("load settings")
    });

    assert_eq!(
        settings.editor, "nvim",
        "an explicit config must outrank an inherited environment"
    );
}

// ============================================================
// RSENV__* environment overrides
// ============================================================

/// Sets the given env vars for the duration of `body`, then restores them.
/// Serialised by the mandatory `--test-threads=1`; these mutate process env.
fn with_env<T>(vars: &[(&str, &str)], body: impl FnOnce() -> T) -> T {
    let previous: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
        .collect();
    for (k, v) in vars {
        std::env::set_var(k, v);
    }
    let result = body();
    for (k, prev) in previous {
        match prev {
            Some(v) => std::env::set_var(&k, v),
            None => std::env::remove_var(&k),
        }
    }
    result
}

#[test]
fn given_comma_separated_file_names_env_when_load_then_replaces_list() {
    let settings = with_env(
        &[("RSENV__SOPS__FILE_NAMES_ENC", "restic.pw,kube_config")],
        || Settings::load(None).expect("load settings"),
    );

    assert_eq!(
        settings.sops.file_names_enc,
        vec!["restic.pw".to_string(), "kube_config".to_string()]
    );
}

#[test]
fn given_single_value_extensions_env_when_load_then_list_of_one() {
    let settings = with_env(&[("RSENV__SOPS__FILE_EXTENSIONS_ENC", "yaml")], || {
        Settings::load(None).expect("load settings")
    });

    assert_eq!(settings.sops.file_extensions_enc, vec!["yaml".to_string()]);
}

#[test]
fn given_decrypt_list_envs_when_load_then_both_replaced() {
    let settings = with_env(
        &[
            ("RSENV__SOPS__FILE_EXTENSIONS_DEC", "enc,sops"),
            ("RSENV__SOPS__FILE_NAMES_DEC", "secret.bin"),
        ],
        || Settings::load(None).expect("load settings"),
    );

    assert_eq!(
        settings.sops.file_extensions_dec,
        vec!["enc".to_string(), "sops".to_string()]
    );
    assert_eq!(settings.sops.file_names_dec, vec!["secret.bin".to_string()]);
}

/// Enabling list parsing must not turn scalar settings into lists:
/// a comma inside a scalar value is part of the value, not a separator.
#[test]
fn given_scalar_env_with_comma_alongside_list_env_when_load_then_scalar_kept_verbatim() {
    let settings = with_env(
        &[
            ("RSENV__EDITOR", "code --wait,--new-window"),
            ("RSENV__SOPS__GPG_KEY", "ABC123"),
            ("RSENV__SOPS__FILE_NAMES_ENC", "restic.pw"),
        ],
        || Settings::load(None).expect("load settings"),
    );

    assert_eq!(settings.editor, "code --wait,--new-window");
    assert_eq!(settings.sops.gpg_key.as_deref(), Some("ABC123"));
    assert_eq!(settings.sops.file_names_enc, vec!["restic.pw".to_string()]);
}

/// An empty value is an explicit "no patterns", and whitespace around items is ignored.
#[test]
fn given_empty_and_padded_list_envs_when_load_then_empty_and_trimmed() {
    let settings = with_env(
        &[
            ("RSENV__SOPS__FILE_NAMES_ENC", ""),
            ("RSENV__SOPS__FILE_EXTENSIONS_ENC", " env , envrc "),
        ],
        || Settings::load(None).expect("load settings"),
    );

    assert!(settings.sops.file_names_enc.is_empty());
    assert_eq!(
        settings.sops.file_extensions_enc,
        vec!["env".to_string(), "envrc".to_string()]
    );
}

// ============================================================
// Settings::template()
// ============================================================

/// `config init`/`config edit` write the template and later load it as config.
#[test]
fn given_template_when_parsed_then_it_is_valid_toml() {
    let template = Settings::template();

    toml::from_str::<toml::Value>(&template).expect("template must be valid TOML");
}
