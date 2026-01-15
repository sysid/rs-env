//! Gitignore management service
//!
//! Two-tier gitignore system:
//! - Tier 1: Global gitignore at `<base_dir>/.gitignore`
//! - Tier 2: Per-vault gitignore at `<vault>/.gitignore` (only if vault has local config)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::debug;

use crate::application::{ApplicationError, ApplicationResult};
use crate::config::{vault_config_path, Settings, SopsConfig};
use crate::infrastructure::traits::FileSystem;

const START_MARKER: &str = "# rsenv-managed start";
const END_MARKER: &str = "# rsenv-managed end";

/// Difference between config patterns and gitignore patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitignoreDiff {
    /// Patterns in config but not in gitignore (would be added)
    pub to_add: Vec<String>,
    /// Patterns in gitignore but not in config (would be removed)
    pub to_remove: Vec<String>,
    /// Whether config and gitignore are in sync
    pub in_sync: bool,
}

/// Status of global and per-vault gitignore.
#[derive(Debug, Clone)]
pub struct GitignoreStatus {
    /// Global gitignore path
    pub global_path: PathBuf,
    /// Global gitignore diff
    pub global_diff: GitignoreDiff,
    /// Per-vault gitignore (only if vault has local config)
    pub vault: Option<VaultGitignoreStatus>,
}

/// Status of per-vault gitignore.
#[derive(Debug, Clone)]
pub struct VaultGitignoreStatus {
    /// Vault gitignore path
    pub path: PathBuf,
    /// Vault gitignore diff
    pub diff: GitignoreDiff,
}

/// Gitignore management service.
pub struct GitignoreService {
    fs: Arc<dyn FileSystem>,
    /// Global config (defaults + XDG config only)
    global_settings: Settings,
}

impl GitignoreService {
    /// Create a new gitignore service.
    ///
    /// # Arguments
    /// * `fs` - Filesystem abstraction
    /// * `global_settings` - Global config loaded via `Settings::load_global_only()`
    pub fn new(fs: Arc<dyn FileSystem>, global_settings: Settings) -> Self {
        Self {
            fs,
            global_settings,
        }
    }

    /// Get patterns from a SopsConfig.
    fn patterns_from_config(sops: &SopsConfig) -> BTreeSet<String> {
        let mut patterns = BTreeSet::new();
        for ext in &sops.file_extensions_enc {
            patterns.insert(format!("*.{}", ext));
        }
        for name in &sops.file_names_enc {
            patterns.insert(name.clone());
        }
        patterns
    }

    /// Get patterns that should be in global gitignore.
    pub fn global_patterns(&self) -> BTreeSet<String> {
        Self::patterns_from_config(&self.global_settings.sops)
    }

    /// Get patterns that should be in per-vault gitignore.
    ///
    /// Returns None if vault has no local config.
    pub fn vault_patterns(&self, vault_dir: &Path) -> ApplicationResult<Option<BTreeSet<String>>> {
        let local_config_path = vault_config_path(vault_dir);
        if !self.fs.exists(&local_config_path) {
            return Ok(None);
        }

        let vault_settings = Settings::load_vault_only(vault_dir)?;
        Ok(vault_settings.map(|s| Self::patterns_from_config(&s.sops)))
    }

    /// Extract patterns currently in a gitignore's managed section.
    fn current_patterns(&self, gitignore_path: &Path) -> ApplicationResult<BTreeSet<String>> {
        if !self.fs.exists(gitignore_path) {
            return Ok(BTreeSet::new());
        }

        let content = self.fs.read_to_string(gitignore_path).map_err(|e| {
            ApplicationError::OperationFailed {
                context: format!("read .gitignore: {}", gitignore_path.display()),
                source: Box::new(e),
            }
        })?;

        Ok(Self::extract_managed_patterns(&content))
    }

    /// Extract patterns from managed section of content.
    fn extract_managed_patterns(content: &str) -> BTreeSet<String> {
        let mut patterns = BTreeSet::new();
        let mut in_managed = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == START_MARKER {
                in_managed = true;
                continue;
            }
            if trimmed == END_MARKER {
                in_managed = false;
                continue;
            }
            if in_managed {
                // Skip comments and empty lines within managed section
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    patterns.insert(trimmed.to_string());
                }
            }
        }

        patterns
    }

    /// Compute diff between config patterns and gitignore patterns.
    fn compute_diff(config: &BTreeSet<String>, current: &BTreeSet<String>) -> GitignoreDiff {
        let to_add: Vec<String> = config.difference(current).cloned().collect();
        let to_remove: Vec<String> = current.difference(config).cloned().collect();
        let in_sync = to_add.is_empty() && to_remove.is_empty();

        GitignoreDiff {
            to_add,
            to_remove,
            in_sync,
        }
    }

    /// Get global gitignore path.
    pub fn global_gitignore_path(&self) -> PathBuf {
        self.global_settings.base_dir.join(".gitignore")
    }

    /// Get per-vault gitignore path.
    pub fn vault_gitignore_path(&self, vault_dir: &Path) -> PathBuf {
        vault_dir.join(".gitignore")
    }

    /// Get sync status for global and per-vault gitignore.
    ///
    /// # Arguments
    /// * `vault_dir` - Optional vault directory to also check per-vault status
    pub fn status(&self, vault_dir: Option<&Path>) -> ApplicationResult<GitignoreStatus> {
        debug!(
            "status: vault_dir={}",
            vault_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".into())
        );
        let global_path = self.global_gitignore_path();
        let global_config_patterns = self.global_patterns();
        let global_current_patterns = self.current_patterns(&global_path)?;
        let global_diff = Self::compute_diff(&global_config_patterns, &global_current_patterns);

        let vault = if let Some(vd) = vault_dir {
            if let Some(vault_config_patterns) = self.vault_patterns(vd)? {
                let vault_path = self.vault_gitignore_path(vd);
                let vault_current_patterns = self.current_patterns(&vault_path)?;
                let vault_diff =
                    Self::compute_diff(&vault_config_patterns, &vault_current_patterns);
                Some(VaultGitignoreStatus {
                    path: vault_path,
                    diff: vault_diff,
                })
            } else {
                None
            }
        } else {
            None
        };

        debug!(
            "status: global_in_sync={}, vault_in_sync={}",
            global_diff.in_sync,
            vault.as_ref().map(|v| v.diff.in_sync).unwrap_or(true)
        );
        Ok(GitignoreStatus {
            global_path,
            global_diff,
            vault,
        })
    }

    /// Check if global gitignore is in sync.
    pub fn is_global_synced(&self) -> ApplicationResult<bool> {
        let global_path = self.global_gitignore_path();
        let config_patterns = self.global_patterns();
        let current_patterns = self.current_patterns(&global_path)?;
        Ok(config_patterns == current_patterns)
    }

    /// Check if all gitignores are in sync (global + vault if applicable).
    pub fn is_synced(&self, vault_dir: Option<&Path>) -> ApplicationResult<bool> {
        debug!(
            "is_synced: vault_dir={}",
            vault_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".into())
        );
        let status = self.status(vault_dir)?;
        let global_synced = status.global_diff.in_sync;
        let vault_synced = status
            .vault
            .as_ref()
            .map(|v| v.diff.in_sync)
            .unwrap_or(true);
        let result = global_synced && vault_synced;
        debug!("is_synced: result={}", result);
        Ok(result)
    }

    /// Update a gitignore file with patterns.
    fn update_gitignore_file(
        &self,
        gitignore_path: &Path,
        patterns: &BTreeSet<String>,
        source_label: &str,
    ) -> ApplicationResult<()> {
        // Read existing content
        let existing = if self.fs.exists(gitignore_path) {
            self.fs.read_to_string(gitignore_path).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("read .gitignore: {}", gitignore_path.display()),
                    source: Box::new(e),
                }
            })?
        } else {
            String::new()
        };

        // Remove existing managed section
        let cleaned = Self::remove_managed_section(&existing);

        // Build new managed section
        let managed_section = if patterns.is_empty() {
            String::new()
        } else {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let pattern_lines: Vec<&str> = patterns.iter().map(|s| s.as_str()).collect();
            format!(
                "{}\n# Source: {}\n# Updated: {}\n{}\n{}\n",
                START_MARKER,
                source_label,
                timestamp,
                pattern_lines.join("\n"),
                END_MARKER
            )
        };

        // Write back
        let new_content = if cleaned.trim().is_empty() {
            // New file or empty after cleaning
            if managed_section.is_empty() {
                String::new()
            } else {
                format!(
                    "{}\n# Encrypted files are safe to commit\n!*.enc\n",
                    managed_section
                )
            }
        } else {
            // Append to existing content
            format!(
                "{}{}",
                cleaned.trim_end(),
                if managed_section.is_empty() {
                    String::new()
                } else {
                    format!("\n\n{}", managed_section)
                }
            )
        };

        // Create parent directory if needed
        if let Some(parent) = gitignore_path.parent() {
            if !self.fs.exists(parent) {
                self.fs
                    .create_dir_all(parent)
                    .map_err(|e| ApplicationError::OperationFailed {
                        context: format!("create directory: {}", parent.display()),
                        source: Box::new(e),
                    })?;
            }
        }

        self.fs
            .write(gitignore_path, new_content.trim())
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("write .gitignore: {}", gitignore_path.display()),
                source: Box::new(e),
            })?;

        Ok(())
    }

    /// Sync global gitignore.
    pub fn sync_global(&self) -> ApplicationResult<GitignoreDiff> {
        debug!(
            "sync_global: path={}",
            self.global_gitignore_path().display()
        );
        let global_path = self.global_gitignore_path();
        let patterns = self.global_patterns();
        let current = self.current_patterns(&global_path)?;
        let diff = Self::compute_diff(&patterns, &current);

        if !diff.in_sync {
            debug!(
                "sync_global: updating, to_add={}, to_remove={}",
                diff.to_add.len(),
                diff.to_remove.len()
            );
            self.update_gitignore_file(&global_path, &patterns, "global config")?;
        } else {
            debug!("sync_global: already in sync");
        }

        Ok(diff)
    }

    /// Sync per-vault gitignore (only if vault has local config).
    pub fn sync_vault(&self, vault_dir: &Path) -> ApplicationResult<Option<GitignoreDiff>> {
        debug!("sync_vault: vault_dir={}", vault_dir.display());
        if let Some(patterns) = self.vault_patterns(vault_dir)? {
            let vault_path = self.vault_gitignore_path(vault_dir);
            let current = self.current_patterns(&vault_path)?;
            let diff = Self::compute_diff(&patterns, &current);

            if !diff.in_sync {
                debug!(
                    "sync_vault: updating, to_add={}, to_remove={}",
                    diff.to_add.len(),
                    diff.to_remove.len()
                );
                self.update_gitignore_file(&vault_path, &patterns, "vault-local config")?;
            } else {
                debug!("sync_vault: already in sync");
            }

            Ok(Some(diff))
        } else {
            debug!("sync_vault: no local config, skipping");
            Ok(None)
        }
    }

    /// Sync both global and per-vault gitignore.
    ///
    /// # Arguments
    /// * `vault_dir` - Optional vault directory
    ///
    /// # Returns
    /// Tuple of (global_diff, optional_vault_diff)
    pub fn sync_all(
        &self,
        vault_dir: Option<&Path>,
    ) -> ApplicationResult<(GitignoreDiff, Option<GitignoreDiff>)> {
        debug!(
            "sync_all: vault_dir={}",
            vault_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".into())
        );
        let global_diff = self.sync_global()?;
        let vault_diff = if let Some(vd) = vault_dir {
            self.sync_vault(vd)?
        } else {
            None
        };
        Ok((global_diff, vault_diff))
    }

    /// Remove managed section from a gitignore file.
    fn clean_gitignore_file(&self, gitignore_path: &Path) -> ApplicationResult<bool> {
        if !self.fs.exists(gitignore_path) {
            return Ok(false);
        }

        let content = self.fs.read_to_string(gitignore_path).map_err(|e| {
            ApplicationError::OperationFailed {
                context: format!("read .gitignore: {}", gitignore_path.display()),
                source: Box::new(e),
            }
        })?;

        let cleaned = Self::remove_managed_section(&content);

        // Only write if something changed
        if cleaned.trim() != content.trim() {
            self.fs.write(gitignore_path, cleaned.trim()).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("write .gitignore: {}", gitignore_path.display()),
                    source: Box::new(e),
                }
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clean managed section from global gitignore.
    pub fn clean_global(&self) -> ApplicationResult<bool> {
        debug!(
            "clean_global: path={}",
            self.global_gitignore_path().display()
        );
        let global_path = self.global_gitignore_path();
        let result = self.clean_gitignore_file(&global_path)?;
        debug!("clean_global: section_removed={}", result);
        Ok(result)
    }

    /// Clean managed section from per-vault gitignore.
    pub fn clean_vault(&self, vault_dir: &Path) -> ApplicationResult<bool> {
        debug!("clean_vault: vault_dir={}", vault_dir.display());
        let vault_path = self.vault_gitignore_path(vault_dir);
        let result = self.clean_gitignore_file(&vault_path)?;
        debug!("clean_vault: section_removed={}", result);
        Ok(result)
    }

    /// Clean managed section from all gitignores.
    pub fn clean_all(&self, vault_dir: Option<&Path>) -> ApplicationResult<(bool, bool)> {
        debug!(
            "clean_all: vault_dir={}",
            vault_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none".into())
        );
        let global_cleaned = self.clean_global()?;
        let vault_cleaned = if let Some(vd) = vault_dir {
            self.clean_vault(vd)?
        } else {
            false
        };
        debug!(
            "clean_all: global_cleaned={}, vault_cleaned={}",
            global_cleaned, vault_cleaned
        );
        Ok((global_cleaned, vault_cleaned))
    }

    /// Remove managed section from content.
    fn remove_managed_section(content: &str) -> String {
        let mut result = String::new();
        let mut in_managed = false;

        for line in content.lines() {
            if line.trim() == START_MARKER {
                in_managed = true;
                continue;
            }
            if line.trim() == END_MARKER {
                in_managed = false;
                continue;
            }
            if !in_managed {
                result.push_str(line);
                result.push('\n');
            }
        }

        result
    }
}
