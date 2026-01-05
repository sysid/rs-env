//! Configuration management with layered loading
//!
//! Precedence (lowest to highest):
//! 1. Compiled defaults
//! 2. Global config: `$XDG_CONFIG_HOME/rsenv/rsenv.toml`
//! 3. Local config: `<vault_dir>/.rsenv.toml` (vault directory, not project)
//! 4. Environment variables: `RSENV_*` prefix

use std::path::{Path, PathBuf};

use config::{Config, ConfigError, Environment, File};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::application::ApplicationError;

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

/// Unified configuration for rsenv.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Base directory for vaults (default: ~/.rsenv/vaults)
    pub vault_base_dir: PathBuf,
    /// Editor command (default: $EDITOR or "vim")
    pub editor: String,
    /// SOPS encryption settings
    pub sops: SopsConfig,
}

impl Default for Settings {
    fn default() -> Self {
        // Try $EDITOR, fall back to vim
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());

        // Default vault directory
        let vault_base_dir = dirs_default_vault_dir();

        Self {
            vault_base_dir,
            editor,
            sops: SopsConfig::default(),
        }
    }
}

/// Get the default vault directory (~/.rsenv/vaults).
fn dirs_default_vault_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".rsenv").join("vaults"))
        .unwrap_or_else(|| PathBuf::from("~/.rsenv/vaults"))
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

impl Settings {
    /// Load settings with layered precedence.
    ///
    /// # Arguments
    /// * `vault_dir` - Optional vault directory for local config
    ///
    /// # Precedence (lowest to highest)
    /// 1. Compiled defaults
    /// 2. Global config: `$XDG_CONFIG_HOME/rsenv/rsenv.toml`
    /// 3. Local config: `<vault_dir>/.rsenv.toml`
    /// 4. Environment variables: `RSENV_*` prefix
    pub fn load(vault_dir: Option<&Path>) -> Result<Self, ApplicationError> {
        let mut builder = Config::builder();

        // 1. Start with defaults
        let defaults = Settings::default();
        builder = builder
            .set_default(
                "vault_base_dir",
                defaults.vault_base_dir.to_string_lossy().to_string(),
            )
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

        // 2. Global config (optional)
        if let Some(global_path) = global_config_path() {
            if global_path.exists() {
                builder = builder.add_source(File::from(global_path).required(false));
            }
        }

        // 3. Local config from vault (optional)
        if let Some(vault) = vault_dir {
            let local_path = vault_config_path(vault);
            if local_path.exists() {
                builder = builder.add_source(File::from(local_path).required(false));
            }
        }

        // 4. Environment variables with RSENV_ prefix
        // Uses separator "__" for nested keys: RSENV_SOPS__GPG_KEY -> sops.gpg_key
        builder = builder.add_source(
            Environment::with_prefix("RSENV")
                .separator("__")
                .list_separator(","),
        );

        // Build and deserialize
        let config = builder.build().map_err(config_err)?;
        config.try_deserialize().map_err(config_err)
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
#   Global: ~/.config/rsenv/rsenv.toml
#   Local:  <vault_dir>/.rsenv.toml (project-specific, in vault)
#   Env:    RSENV_* environment variables

# Base directory for vaults
# vault_base_dir = "~/.rsenv/vaults"

# Editor for editing env files
# editor = "vim"

[sops]
# GPG key fingerprint for SOPS encryption
# gpg_key = "60A4127E82E218297532FAB6D750B66AE08F3B90"

# Age public key (alternative to GPG)
# age_key = "age1..."

# File extensions to encrypt
# file_extensions_enc = ["envrc", "env"]

# Exact filenames to encrypt
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
        assert!(settings
            .vault_base_dir
            .to_string_lossy()
            .contains(".rsenv/vaults"));
        assert!(!settings.editor.is_empty());
    }

    #[test]
    fn given_default_sops_config_when_created_then_has_expected_extensions() {
        let sops = SopsConfig::default();
        assert!(sops.file_extensions_enc.contains(&"env".to_string()));
        assert!(sops.file_extensions_dec.contains(&"enc".to_string()));
    }
}
