//! Configuration management with layered loading
//!
//! Precedence (lowest to highest):
//! 1. Compiled defaults
//! 2. Global config: `$XDG_CONFIG_HOME/rsenv/rsenv.toml`
//! 3. Local config: `<vault_dir>/.rsenv.toml` (vault directory, not project)
//! 4. Environment variables: `RSENV_*` prefix

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use config::{Config, ConfigError, Environment, File};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::application::ApplicationError;
use crate::domain::expand_env_vars;

/// SOPS encryption configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SopsConfig {
    /// GPG key fingerprint for encryption
    pub gpg_key: Option<String>,
    /// Age public key (alternative to GPG)
    pub age_key: Option<String>,
    /// File extensions to encrypt (e.g., ["envrc", "env"])
    pub file_extensions_enc: Vec<String>,
    /// Exact filenames to encrypt (e.g., ["dot_pypirc", "dot_pgpass"])
    pub file_names_enc: Vec<String>,
    /// File extensions to decrypt (typically ["enc"])
    pub file_extensions_dec: Vec<String>,
    /// Exact filenames to decrypt (usually empty)
    pub file_names_dec: Vec<String>,
}

impl Default for SopsConfig {
    fn default() -> Self {
        Self {
            gpg_key: None,
            age_key: None,
            file_extensions_enc: vec!["envrc".into(), "env".into()],
            file_names_enc: vec![],
            file_extensions_dec: vec!["enc".into()],
            file_names_dec: vec![],
        }
    }
}

/// Raw SOPS config for intermediate parsing (arrays are Option to detect "not specified").
///
/// Used during layered config merging to distinguish between:
/// - `None` → field not specified, inherit from base
/// - `Some([])` → explicit empty array
/// - `Some([...])` → explicit values to merge
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RawSopsConfig {
    pub gpg_key: Option<String>,
    pub age_key: Option<String>,
    pub file_extensions_enc: Option<Vec<String>>,
    pub file_names_enc: Option<Vec<String>>,
    pub file_extensions_dec: Option<Vec<String>>,
    pub file_names_dec: Option<Vec<String>>,
}

/// Raw settings for intermediate parsing.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RawSettings {
    pub base_dir: Option<PathBuf>,
    pub editor: Option<String>,
    #[serde(default)]
    pub sops: RawSopsConfig,
}

impl SopsConfig {
    /// Merge arrays with union semantics and negation support.
    ///
    /// - Items from overlay are added to base
    /// - Items prefixed with `!` remove the corresponding item from the result
    /// - Duplicates are de-duplicated
    ///
    /// # Examples
    /// ```ignore
    /// merge_array(&["a", "b"], &["c"])       // → ["a", "b", "c"]
    /// merge_array(&["a", "b"], &["!a", "c"]) // → ["b", "c"]
    /// ```
    pub fn merge_array(base: &[String], overlay: &[String]) -> Vec<String> {
        let mut result: HashSet<String> = base.iter().cloned().collect();

        for pattern in overlay {
            if let Some(negated) = pattern.strip_prefix('!') {
                result.remove(negated);
            } else {
                result.insert(pattern.clone());
            }
        }

        // Convert to sorted Vec for deterministic output
        let mut vec: Vec<String> = result.into_iter().collect();
        vec.sort();
        vec
    }

    /// Merge overlay config onto self (base).
    ///
    /// - Scalar options: overlay wins if Some, otherwise keep base
    /// - Arrays: union merge with negation support (if overlay specified)
    pub fn merge(&self, overlay: &RawSopsConfig) -> Self {
        Self {
            gpg_key: overlay.gpg_key.clone().or_else(|| self.gpg_key.clone()),
            age_key: overlay.age_key.clone().or_else(|| self.age_key.clone()),
            file_extensions_enc: overlay
                .file_extensions_enc
                .as_ref()
                .map(|o| Self::merge_array(&self.file_extensions_enc, o))
                .unwrap_or_else(|| self.file_extensions_enc.clone()),
            file_names_enc: overlay
                .file_names_enc
                .as_ref()
                .map(|o| Self::merge_array(&self.file_names_enc, o))
                .unwrap_or_else(|| self.file_names_enc.clone()),
            file_extensions_dec: overlay
                .file_extensions_dec
                .as_ref()
                .map(|o| Self::merge_array(&self.file_extensions_dec, o))
                .unwrap_or_else(|| self.file_extensions_dec.clone()),
            file_names_dec: overlay
                .file_names_dec
                .as_ref()
                .map(|o| Self::merge_array(&self.file_names_dec, o))
                .unwrap_or_else(|| self.file_names_dec.clone()),
        }
    }

    /// Apply global config onto defaults.
    ///
    /// Unlike `merge()` which uses union semantics for arrays, this method
    /// uses REPLACE semantics: if global config specifies an array, it completely
    /// replaces the default array.
    ///
    /// Rationale: Defaults are just examples. Global config defines the real
    /// baseline for the user/organization. Vault config (using `merge()`) then
    /// adds project-specific patterns on top.
    pub fn apply_global(&self, global: &RawSopsConfig) -> Self {
        Self {
            gpg_key: global.gpg_key.clone().or_else(|| self.gpg_key.clone()),
            age_key: global.age_key.clone().or_else(|| self.age_key.clone()),
            // Arrays: if global specifies them, REPLACE entirely (not union)
            file_extensions_enc: global
                .file_extensions_enc
                .clone()
                .unwrap_or_else(|| self.file_extensions_enc.clone()),
            file_names_enc: global
                .file_names_enc
                .clone()
                .unwrap_or_else(|| self.file_names_enc.clone()),
            file_extensions_dec: global
                .file_extensions_dec
                .clone()
                .unwrap_or_else(|| self.file_extensions_dec.clone()),
            file_names_dec: global
                .file_names_dec
                .clone()
                .unwrap_or_else(|| self.file_names_dec.clone()),
        }
    }
}

/// Unified configuration for rsenv.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Base directory for rsenv (default: ~/.rsenv)
    pub base_dir: PathBuf,
    /// Editor command (default: $EDITOR or "vim")
    pub editor: String,
    /// SOPS encryption settings
    pub sops: SopsConfig,
}

impl Default for Settings {
    fn default() -> Self {
        // Try $EDITOR, fall back to vim
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());

        // Default base directory
        let base_dir = dirs_default_base_dir();

        Self {
            base_dir,
            editor,
            sops: SopsConfig::default(),
        }
    }
}

/// Get the default base directory (~/.rsenv).
fn dirs_default_base_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".rsenv"))
        .unwrap_or_else(|| PathBuf::from("~/.rsenv"))
}

/// Get the XDG config directory for rsenv.
pub fn global_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "rsenv").map(|dirs| dirs.config_dir().to_path_buf())
}

/// Get the path to the global config file.
pub fn global_config_path() -> Option<PathBuf> {
    global_config_dir().map(|dir| dir.join("rsenv.toml"))
}

/// Get the path to the local config file in a vault directory.
pub fn vault_config_path(vault_dir: &Path) -> PathBuf {
    vault_dir.join(".rsenv.toml")
}

/// Load a TOML file into RawSettings for manual merging.
fn load_raw_settings(path: &Path) -> Result<RawSettings, ApplicationError> {
    let content = std::fs::read_to_string(path).map_err(|e| ApplicationError::Config {
        message: format!("read {}: {}", path.display(), e),
    })?;
    toml::from_str(&content).map_err(|e| ApplicationError::Config {
        message: format!("parse {}: {}", path.display(), e),
    })
}

impl Settings {
    /// Get the vaults directory (base_dir/vaults).
    pub fn vaults_dir(&self) -> PathBuf {
        self.base_dir.join("vaults")
    }

    /// Expand shell variables and tilde in path-like fields.
    ///
    /// Handles `~`, `$VAR`, and `${VAR}` syntax.
    fn expand_paths(&mut self) {
        // Expand base_dir
        let expanded = expand_env_vars(self.base_dir.to_string_lossy().as_ref());
        self.base_dir = PathBuf::from(expanded);

        // Expand editor (may contain path like ~/bin/myeditor)
        self.editor = expand_env_vars(&self.editor);
    }

    /// Merge overlay config onto self (base) with union semantics for arrays.
    ///
    /// - Scalar options: overlay wins if Some, otherwise keep base
    /// - Arrays (SOPS config): union merge with negation support
    fn merge_with(&self, overlay: &RawSettings) -> Self {
        Self {
            base_dir: overlay
                .base_dir
                .clone()
                .unwrap_or_else(|| self.base_dir.clone()),
            editor: overlay
                .editor
                .clone()
                .unwrap_or_else(|| self.editor.clone()),
            sops: self.sops.merge(&overlay.sops),
        }
    }

    /// Apply global config onto defaults with REPLACE semantics for arrays.
    ///
    /// Unlike `merge_with()` which uses union semantics, this method replaces
    /// arrays entirely if the global config specifies them.
    ///
    /// Rationale: Defaults are just examples. Global config defines the real
    /// baseline for the user/organization.
    fn apply_global(&self, global: &RawSettings) -> Self {
        Self {
            base_dir: global
                .base_dir
                .clone()
                .unwrap_or_else(|| self.base_dir.clone()),
            editor: global.editor.clone().unwrap_or_else(|| self.editor.clone()),
            sops: self.sops.apply_global(&global.sops),
        }
    }

    /// Load settings with layered precedence.
    ///
    /// # Arguments
    /// * `vault_dir` - Optional vault directory for local config
    ///
    /// # Precedence (lowest to highest)
    /// 1. Compiled defaults (examples only)
    /// 2. Global config: `$XDG_CONFIG_HOME/rsenv/rsenv.toml` (arrays REPLACE defaults)
    /// 3. Local config: `<vault_dir>/.rsenv.toml` (arrays UNION with global)
    /// 4. Environment variables: `RSENV_*` prefix (REPLACES - explicit override)
    ///
    /// # Array Merge Semantics
    /// - Defaults → Global: REPLACE (global defines the real baseline)
    /// - Global → Vault: UNION with negation support (vault adds project-specific patterns)
    /// - Any → Env vars: REPLACE (explicit user override)
    pub fn load(vault_dir: Option<&Path>) -> Result<Self, ApplicationError> {
        // 1. Start with defaults
        let mut current = Self::default();

        // 2. Load global config (REPLACES defaults - global defines the real baseline)
        if let Some(global_path) = global_config_path() {
            if global_path.exists() {
                let raw = load_raw_settings(&global_path)?;
                current = current.apply_global(&raw);
            }
        }

        // 3. Load and merge vault config (UNION with global - adds project-specific patterns)
        if let Some(vault) = vault_dir {
            let local_path = vault_config_path(vault);
            if local_path.exists() {
                let raw = load_raw_settings(&local_path)?;
                current = current.merge_with(&raw);
            }
        }

        // 4. Apply environment variables (replaces - explicit override)
        // This still uses the config crate for env var parsing
        current = Self::apply_env_overrides(current)?;

        // Expand ~ and $VAR in path-like fields
        current.expand_paths();

        Ok(current)
    }

    /// Apply RSENV_* environment variables as explicit overrides.
    ///
    /// Env vars replace values (not merge) - they are explicit user overrides.
    fn apply_env_overrides(mut settings: Self) -> Result<Self, ApplicationError> {
        // Use config crate just for env var parsing
        let builder = Config::builder().add_source(
            Environment::with_prefix("RSENV")
                .separator("__")
                .list_separator(","),
        );

        let config = builder.build().map_err(config_err)?;

        // Apply individual env vars if set (they replace, not merge)
        if let Ok(val) = config.get_string("base_dir") {
            settings.base_dir = PathBuf::from(val);
        }
        if let Ok(val) = config.get_string("editor") {
            settings.editor = val;
        }
        if let Ok(val) = config.get_string("sops.gpg_key") {
            settings.sops.gpg_key = Some(val);
        }
        if let Ok(val) = config.get_string("sops.age_key") {
            settings.sops.age_key = Some(val);
        }
        if let Ok(val) = config.get::<Vec<String>>("sops.file_extensions_enc") {
            settings.sops.file_extensions_enc = val;
        }
        if let Ok(val) = config.get::<Vec<String>>("sops.file_names_enc") {
            settings.sops.file_names_enc = val;
        }
        if let Ok(val) = config.get::<Vec<String>>("sops.file_extensions_dec") {
            settings.sops.file_extensions_dec = val;
        }
        if let Ok(val) = config.get::<Vec<String>>("sops.file_names_dec") {
            settings.sops.file_names_dec = val;
        }

        Ok(settings)
    }

    /// Load ONLY global config (defaults + XDG config file).
    ///
    /// Does NOT include vault-local config or environment variables.
    /// Used to determine which patterns belong in the global .gitignore.
    pub fn load_global_only() -> Result<Self, ApplicationError> {
        let mut builder = Config::builder();

        // 1. Start with defaults
        let defaults = Settings::default();
        builder = builder
            .set_default("base_dir", defaults.base_dir.to_string_lossy().to_string())
            .map_err(config_err)?
            .set_default("editor", defaults.editor.clone())
            .map_err(config_err)?
            .set_default(
                "sops.file_extensions_enc",
                defaults.sops.file_extensions_enc.clone(),
            )
            .map_err(config_err)?
            .set_default(
                "sops.file_extensions_dec",
                defaults.sops.file_extensions_dec.clone(),
            )
            .map_err(config_err)?
            .set_default("sops.file_names_enc", defaults.sops.file_names_enc.clone())
            .map_err(config_err)?
            .set_default("sops.file_names_dec", defaults.sops.file_names_dec.clone())
            .map_err(config_err)?;

        // 2. Global config only (no vault-local, no env vars)
        if let Some(global_path) = global_config_path() {
            if global_path.exists() {
                builder = builder.add_source(File::from(global_path).required(false));
            }
        }

        // Build and deserialize
        let config = builder.build().map_err(config_err)?;
        let mut settings: Self = config.try_deserialize().map_err(config_err)?;

        // Expand ~ and $VAR in path-like fields
        settings.expand_paths();

        Ok(settings)
    }

    /// Load ONLY vault-local config if it exists.
    ///
    /// Returns None if no vault-local config file exists.
    /// Used to determine which patterns belong in the per-vault .gitignore.
    pub fn load_vault_only(vault_dir: &Path) -> Result<Option<Self>, ApplicationError> {
        let local_path = vault_config_path(vault_dir);
        if !local_path.exists() {
            return Ok(None);
        }

        let mut builder = Config::builder();

        // Start with defaults (needed for deserialization)
        let defaults = Settings::default();
        builder = builder
            .set_default("base_dir", defaults.base_dir.to_string_lossy().to_string())
            .map_err(config_err)?
            .set_default("editor", defaults.editor.clone())
            .map_err(config_err)?
            .set_default(
                "sops.file_extensions_enc",
                defaults.sops.file_extensions_enc.clone(),
            )
            .map_err(config_err)?
            .set_default(
                "sops.file_extensions_dec",
                defaults.sops.file_extensions_dec.clone(),
            )
            .map_err(config_err)?
            .set_default("sops.file_names_enc", defaults.sops.file_names_enc.clone())
            .map_err(config_err)?
            .set_default("sops.file_names_dec", defaults.sops.file_names_dec.clone())
            .map_err(config_err)?;

        // Only vault-local config
        builder = builder.add_source(File::from(local_path).required(true));

        // Build and deserialize
        let config = builder.build().map_err(config_err)?;
        let mut settings: Self = config.try_deserialize().map_err(config_err)?;

        // Expand ~ and $VAR in path-like fields
        settings.expand_paths();

        Ok(Some(settings))
    }

    /// Show the effective configuration as TOML.
    pub fn to_toml(&self) -> Result<String, ApplicationError> {
        toml::to_string_pretty(self).map_err(|e| ApplicationError::Config {
            message: format!("serialize config: {e}"),
        })
    }

    /// Generate a template config file.
    pub fn template() -> String {
        r#"# rsenv configuration
#
# Locations (by precedence, lowest to highest):
#   Global: ~/.config/rsenv/rsenv.toml  (defines your baseline)
#   Local:  <vault_dir>/.rsenv.toml     (project-specific additions)
#   Env:    RSENV_* environment variables (explicit overrides)
#
# Array Merge Semantics:
#   Global config REPLACES compiled defaults (defaults are just examples).
#   Local/vault config UNIONS with global (adds project-specific patterns).
#   Use "!pattern" in local config to REMOVE an inherited item:
#     file_extensions_enc = ["yaml", "!env"]  # adds yaml, removes env from global

# Base directory for rsenv (vaults stored in base_dir/vaults)
# base_dir = "~/.rsenv"

# Editor for editing env files
# editor = "vim"

[sops]
# GPG key fingerprint for SOPS encryption
# gpg_key = "60A4127E82E218297532FAB6D750B66AE08F3B90"

# Age public key (alternative to GPG)
# age_key = "age1..."

# File extensions to encrypt (merged with global config, use !ext to remove)
# file_extensions_enc = ["envrc", "env"]

# Exact filenames to encrypt (merged with global config, use !name to remove)
# file_names_enc = ["dot_pypirc", "dot_pgpass", "kube_config"]

# File extensions to decrypt (typically just .enc)
# file_extensions_dec = ["enc"]
"#
        .to_string()
    }
}

fn config_err(e: ConfigError) -> ApplicationError {
    ApplicationError::Config {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_no_config_when_loading_then_uses_defaults() {
        let settings = Settings::load(None).expect("load defaults");
        assert!(settings.base_dir.to_string_lossy().contains(".rsenv"));
        assert!(!settings.base_dir.to_string_lossy().contains("vaults"));
        assert!(!settings.editor.is_empty());
    }

    #[test]
    fn given_default_sops_config_when_created_then_has_expected_extensions() {
        let sops = SopsConfig::default();
        assert!(sops.file_extensions_enc.contains(&"env".to_string()));
        assert!(sops.file_extensions_dec.contains(&"enc".to_string()));
    }

    #[test]
    fn given_tilde_in_base_dir_when_expand_paths_then_expands_to_home() {
        let mut settings = Settings {
            base_dir: PathBuf::from("~/.rsenv"),
            editor: "~/bin/myeditor".to_string(),
            sops: SopsConfig::default(),
        };

        settings.expand_paths();

        let home = std::env::var("HOME").expect("HOME should be set");
        let vault_str = settings.base_dir.to_string_lossy();
        assert!(
            vault_str.starts_with(&home),
            "base_dir should start with home dir: {}",
            vault_str
        );
        assert!(
            !vault_str.contains('~'),
            "base_dir should not contain tilde: {}",
            vault_str
        );
        assert!(
            settings.editor.starts_with(&home),
            "editor should start with home dir: {}",
            settings.editor
        );
    }

    #[test]
    fn given_env_var_in_path_when_expand_paths_then_expands_variable() {
        let mut settings = Settings {
            base_dir: PathBuf::from("$HOME/.rsenv"),
            editor: "${HOME}/bin/myeditor".to_string(),
            sops: SopsConfig::default(),
        };

        settings.expand_paths();

        let home = std::env::var("HOME").expect("HOME should be set");
        assert!(
            settings.base_dir.to_string_lossy().starts_with(&home),
            "base_dir should expand $HOME"
        );
        assert!(
            settings.editor.starts_with(&home),
            "editor should expand ${{HOME}}"
        );
    }

    // ========================================
    // Tests for merge_array union semantics
    // ========================================

    #[test]
    fn test_merge_array_union() {
        // Basic union: ["a", "b"] + ["c"] → ["a", "b", "c"]
        let base = vec!["a".to_string(), "b".to_string()];
        let overlay = vec!["c".to_string()];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(result.contains(&"a".to_string()));
        assert!(result.contains(&"b".to_string()));
        assert!(result.contains(&"c".to_string()));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_merge_array_negation() {
        // Negation: ["a", "b"] + ["!a", "c"] → ["b", "c"]
        let base = vec!["a".to_string(), "b".to_string()];
        let overlay = vec!["!a".to_string(), "c".to_string()];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(
            !result.contains(&"a".to_string()),
            "a should be removed by !a"
        );
        assert!(result.contains(&"b".to_string()));
        assert!(result.contains(&"c".to_string()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_array_negation_nonexistent() {
        // Noop for non-existent: ["a", "b"] + ["!x"] → ["a", "b"]
        let base = vec!["a".to_string(), "b".to_string()];
        let overlay = vec!["!x".to_string()];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(result.contains(&"a".to_string()));
        assert!(result.contains(&"b".to_string()));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_merge_array_empty_base() {
        // Empty base: [] + ["a"] → ["a"]
        let base: Vec<String> = vec![];
        let overlay = vec!["a".to_string()];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(result.contains(&"a".to_string()));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_merge_array_empty_overlay() {
        // Empty overlay: ["a"] + [] → ["a"] (no change)
        let base = vec!["a".to_string()];
        let overlay: Vec<String> = vec![];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(result.contains(&"a".to_string()));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_merge_array_duplicates() {
        // Duplicates should be de-duped: ["a", "b"] + ["a", "c"] → ["a", "b", "c"]
        let base = vec!["a".to_string(), "b".to_string()];
        let overlay = vec!["a".to_string(), "c".to_string()];
        let result = SopsConfig::merge_array(&base, &overlay);

        assert!(result.contains(&"a".to_string()));
        assert!(result.contains(&"b".to_string()));
        assert!(result.contains(&"c".to_string()));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_merge_sops_config() {
        let base = SopsConfig {
            gpg_key: Some("base-key".to_string()),
            age_key: None,
            file_extensions_enc: vec!["env".to_string(), "envrc".to_string()],
            file_names_enc: vec!["dot_pypirc".to_string()],
            file_extensions_dec: vec!["enc".to_string()],
            file_names_dec: vec![],
        };

        let overlay = RawSopsConfig {
            gpg_key: None,                            // Should NOT override base
            age_key: Some("overlay-age".to_string()), // Should be set
            file_extensions_enc: Some(vec!["yaml".to_string(), "!env".to_string()]), // Union with negation
            file_names_enc: Some(vec!["secrets.txt".to_string()]),                   // Union
            file_extensions_dec: None, // Should keep base
            file_names_dec: None,      // Should keep base
        };

        let result = base.merge(&overlay);

        // gpg_key: base wins (overlay is None)
        assert_eq!(result.gpg_key, Some("base-key".to_string()));
        // age_key: overlay wins
        assert_eq!(result.age_key, Some("overlay-age".to_string()));
        // file_extensions_enc: union with negation → ["envrc", "yaml"] (env removed)
        assert!(!result.file_extensions_enc.contains(&"env".to_string()));
        assert!(result.file_extensions_enc.contains(&"envrc".to_string()));
        assert!(result.file_extensions_enc.contains(&"yaml".to_string()));
        assert_eq!(result.file_extensions_enc.len(), 2);
        // file_names_enc: union → ["dot_pypirc", "secrets.txt"]
        assert!(result.file_names_enc.contains(&"dot_pypirc".to_string()));
        assert!(result.file_names_enc.contains(&"secrets.txt".to_string()));
        assert_eq!(result.file_names_enc.len(), 2);
        // file_extensions_dec: base (overlay is None)
        assert_eq!(result.file_extensions_dec, vec!["enc".to_string()]);
    }

    // ========================================
    // Tests for apply_global REPLACE semantics
    // ========================================

    #[test]
    fn test_apply_global_replaces_arrays() {
        // Global config should REPLACE base arrays, not union
        let base = SopsConfig {
            gpg_key: Some("base-key".to_string()),
            age_key: None,
            file_extensions_enc: vec!["env".to_string(), "envrc".to_string()],
            file_names_enc: vec!["dot_pypirc".to_string()],
            file_extensions_dec: vec!["enc".to_string()],
            file_names_dec: vec![],
        };

        let global = RawSopsConfig {
            gpg_key: None,
            age_key: Some("global-age".to_string()),
            file_extensions_enc: Some(vec!["yaml".to_string(), "json".to_string()]), // REPLACES base
            file_names_enc: Some(vec!["secrets.txt".to_string()]), // REPLACES base
            file_extensions_dec: None,                             // Keeps base
            file_names_dec: None,                                  // Keeps base
        };

        let result = base.apply_global(&global);

        // Scalars: global wins if specified
        assert_eq!(result.gpg_key, Some("base-key".to_string())); // base (global is None)
        assert_eq!(result.age_key, Some("global-age".to_string())); // global wins

        // Arrays: REPLACED by global (not union!)
        assert_eq!(
            result.file_extensions_enc,
            vec!["yaml".to_string(), "json".to_string()],
            "Global should REPLACE base file_extensions_enc"
        );
        assert_eq!(
            result.file_names_enc,
            vec!["secrets.txt".to_string()],
            "Global should REPLACE base file_names_enc"
        );

        // Unspecified arrays: keep base
        assert_eq!(result.file_extensions_dec, vec!["enc".to_string()]);
        assert!(result.file_names_dec.is_empty());
    }

    #[test]
    fn test_apply_global_keeps_base_when_not_specified() {
        // When global doesn't specify arrays, base is preserved
        let base = SopsConfig {
            gpg_key: None,
            age_key: None,
            file_extensions_enc: vec!["env".to_string(), "envrc".to_string()],
            file_names_enc: vec!["dot_pypirc".to_string()],
            file_extensions_dec: vec!["enc".to_string()],
            file_names_dec: vec![],
        };

        let global = RawSopsConfig {
            gpg_key: Some("global-gpg".to_string()),
            age_key: None,
            file_extensions_enc: None, // Not specified - keep base
            file_names_enc: None,
            file_extensions_dec: None,
            file_names_dec: None,
        };

        let result = base.apply_global(&global);

        // Scalar specified in global
        assert_eq!(result.gpg_key, Some("global-gpg".to_string()));

        // Arrays not specified in global - keep base
        assert_eq!(
            result.file_extensions_enc,
            vec!["env".to_string(), "envrc".to_string()],
            "Base arrays should be preserved when global doesn't specify"
        );
    }
}
