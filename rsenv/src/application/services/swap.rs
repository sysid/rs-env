//! File swap-in/swap-out service
//!
//! Manages atomic file swapping between project and vault versions
//! with hostname-based conflict detection and state tracking.
//!
//! ## File State Diagrams
//!
//! ```text
//! SWAPPED OUT (normal state):
//! project/                          vault/swap/
//!   config.yml <- original            config.yml <- override version
//!
//! SWAPPED IN:
//! project/                          vault/swap/
//!   config.yml <- override (moved)    config.yml.<host>.rsenv_active <- sentinel
//!                                     config.yml.rsenv_original <- backup
//!                                     (config.yml GONE - moved to project)
//! ```
//!
//! ## Dot-file Handling
//!
//! Dot-files (`.gitignore`, `.envrc`, etc.) are neutralized in the vault to
//! prevent them from having active effects. The naming convention is:
//! - `.gitignore` → `dot.gitignore` (neutralized)
//! - `.hidden/` → `dot.hidden/` (neutralized directory)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::debug;
use walkdir::WalkDir;

use crate::application::dotfile::{is_dotfile, neutralize_name, neutralize_path, restore_name};
use crate::application::services::VaultService;
use crate::application::{ApplicationError, ApplicationResult, IoResultExt};
use crate::config::Settings;
use crate::domain::{SwapFile, SwapState};
use crate::infrastructure::traits::FileSystem;

/// File swap-in/swap-out service.
pub struct SwapService {
    fs: Arc<dyn FileSystem>,
    vault_service: Arc<VaultService>,
    #[allow(dead_code)] // May be used for future configuration
    settings: Arc<Settings>,
}

impl SwapService {
    /// Create a new swap service.
    pub fn new(
        fs: Arc<dyn FileSystem>,
        vault_service: Arc<VaultService>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            fs,
            vault_service,
            settings,
        }
    }

    // ============================================================
    // Helper methods for vault paths
    // ============================================================

    /// Get sentinel path in vault for a file.
    /// Format: `vault/swap/<rel_path>.<hostname>.rsenv_active`
    fn get_sentinel_path(swap_dir: &Path, relative: &Path, hostname: &str) -> PathBuf {
        let parent = swap_dir
            .join(relative)
            .parent()
            .unwrap_or(swap_dir)
            .to_path_buf();
        let file_name = relative
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        parent.join(format!("{}.{}.rsenv_active", file_name, hostname))
    }

    /// Get backup path in vault for a file.
    /// Format: `vault/swap/<rel_path>.rsenv_original`
    fn get_backup_path(swap_dir: &Path, relative: &Path) -> PathBuf {
        let parent = swap_dir
            .join(relative)
            .parent()
            .unwrap_or(swap_dir)
            .to_path_buf();
        let file_name = relative
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        parent.join(format!("{}.rsenv_original", file_name))
    }

    /// Find any sentinel for a file (from any host) in vault.
    /// Returns `(sentinel_path, hostname)` if found.
    fn find_any_sentinel(&self, swap_dir: &Path, relative: &Path) -> Option<(PathBuf, String)> {
        let sentinel_dir = swap_dir.join(relative).parent()?.to_path_buf();
        let base_name = relative.file_name()?.to_string_lossy().to_string();

        if !self.fs.exists(&sentinel_dir) {
            return None;
        }

        // Look for pattern: <filename>.<hostname>.rsenv_active
        // Entry can be either a file or directory (for directory swapping)
        for entry in WalkDir::new(&sentinel_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.depth() > 0)
        // skip root, allow files AND directories
        {
            let name = entry.file_name().to_string_lossy().to_string();

            // Pattern: {base_name}.{hostname}.rsenv_active
            if name.ends_with(".rsenv_active") && name.starts_with(&format!("{}.", base_name)) {
                // Extract hostname: remove base_name + "." prefix and ".rsenv_active" suffix
                let prefix_len = base_name.len() + 1; // "config.yml."
                let suffix_len = ".rsenv_active".len();

                if name.len() > prefix_len + suffix_len {
                    let hostname = name[prefix_len..name.len() - suffix_len].to_string();
                    return Some((entry.path().to_path_buf(), hostname));
                }
            }
        }
        None
    }

    // ============================================================
    // Dot-file neutralization helpers
    // ============================================================

    /// Neutralize dot-files in path: `.foo` → `dot.foo`
    ///
    /// For directories: recursively finds and renames all dot-files and dot-directories.
    /// For standalone dot-files: renames the file directly.
    ///
    /// Prevents dot-files in vault from having active effects (e.g., .gitignore
    /// affecting git's view of vault, .envrc being sourced by direnv).
    fn neutralize_dotfiles(&self, path: &Path) -> ApplicationResult<()> {
        if self.fs.is_dir(path) {
            // For directories, we need to handle this carefully:
            // 1. First collect all dot-files/dirs to rename (to avoid iterator invalidation)
            // 2. Process deepest paths first (so we don't invalidate parent paths)
            let mut to_rename: Vec<PathBuf> = Vec::new();

            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                if let Some(name) = entry.path().file_name() {
                    if is_dotfile(name) {
                        to_rename.push(entry.path().to_path_buf());
                    }
                }
            }

            // Sort by depth (deepest first) to avoid invalidating parent paths
            to_rename.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

            for dotpath in to_rename {
                if let Some(name) = dotpath.file_name() {
                    let name_str = name.to_string_lossy();
                    let neutralized_name = neutralize_name(&name_str);
                    if neutralized_name != name_str {
                        let neutralized = dotpath.with_file_name(&neutralized_name);
                        debug!(
                            "neutralize_dotfile: {} -> {}",
                            dotpath.display(),
                            neutralized.display()
                        );
                        self.fs
                            .rename(&dotpath, &neutralized)
                            .with_path_context("neutralize dot-file", &dotpath)?;
                    }
                }
            }
        } else if let Some(name) = path.file_name() {
            // Standalone dot-file
            if is_dotfile(name) && self.fs.exists(path) {
                let name_str = name.to_string_lossy();
                let neutralized_name = neutralize_name(&name_str);
                let neutralized = path.with_file_name(&neutralized_name);
                debug!(
                    "neutralize_dotfile: {} -> {}",
                    path.display(),
                    neutralized.display()
                );
                self.fs
                    .rename(path, &neutralized)
                    .with_path_context("neutralize dot-file", path)?;
            }
        }
        Ok(())
    }

    /// Restore neutralized dot-files in path: `dot.foo` → `.foo`
    ///
    /// For directories: recursively finds and restores all neutralized dot-files.
    /// For standalone dot-file paths: checks if the neutralized form exists and restores it.
    ///
    /// Restores dot-files when content is swapped back into the project.
    fn restore_dotfiles(&self, path: &Path) -> ApplicationResult<()> {
        if self.fs.is_dir(path) {
            // Collect all neutralized files to rename
            let mut to_rename: Vec<PathBuf> = Vec::new();

            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                if let Some(name) = entry.path().file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("dot.") {
                        to_rename.push(entry.path().to_path_buf());
                    }
                }
            }

            // Sort by depth (deepest first) to avoid invalidating parent paths
            to_rename.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

            for neutralized_path in to_rename {
                if let Some(name) = neutralized_path.file_name() {
                    let name_str = name.to_string_lossy();
                    let restored_name = restore_name(&name_str);
                    if restored_name != name_str {
                        let restored = neutralized_path.with_file_name(&restored_name);
                        debug!(
                            "restore_dotfile: {} -> {}",
                            neutralized_path.display(),
                            restored.display()
                        );
                        self.fs
                            .rename(&neutralized_path, &restored)
                            .with_path_context("restore dot-file", &neutralized_path)?;
                    }
                }
            }
        } else if let Some(name) = path.file_name() {
            // Standalone: check if neutralized form exists
            if is_dotfile(name) {
                let name_str = name.to_string_lossy();
                let neutralized_name = neutralize_name(&name_str);
                let neutralized = path.with_file_name(&neutralized_name);
                if self.fs.exists(&neutralized) {
                    debug!(
                        "restore_dotfile: {} -> {}",
                        neutralized.display(),
                        path.display()
                    );
                    self.fs
                        .rename(&neutralized, path)
                        .with_path_context("restore dot-file", &neutralized)?;
                }
            }
        }
        Ok(())
    }

    /// Find bare (non-neutralized) dot-files in vault path.
    ///
    /// Returns a list of paths to dot-files that should be neutralized.
    fn find_bare_dotfiles(&self, path: &Path) -> Vec<PathBuf> {
        let mut bare = Vec::new();

        if !self.fs.exists(path) {
            return bare;
        }

        if self.fs.is_dir(path) {
            // Directory: recursively find all bare dot-files
            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                if let Some(name) = entry.path().file_name() {
                    if is_dotfile(name) {
                        bare.push(entry.path().to_path_buf());
                    }
                }
            }
        } else if let Some(name) = path.file_name() {
            if is_dotfile(name) {
                bare.push(path.to_path_buf());
            }
        }

        bare
    }

    // ============================================================
    // Public API
    // ============================================================

    /// Swap files in (replace project files with vault overrides).
    ///
    /// For each file:
    /// 1. Check for existing swap (sentinel in vault)
    /// 2. Create sentinel as COPY of vault content (before move)
    /// 3. Backup original to vault: `<rel_path>.rsenv_original`
    /// 4. MOVE vault content to project
    ///
    /// # Arguments
    /// * `project_dir` - Project directory
    /// * `files` - Files to swap in (relative or absolute paths)
    ///
    /// # Returns
    /// Vec of swapped files with their new state
    pub fn swap_in(
        &self,
        project_dir: &Path,
        files: &[PathBuf],
    ) -> ApplicationResult<Vec<SwapFile>> {
        let vault = self
            .vault_service
            .get(project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.to_path_buf()))?;

        let hostname = Self::get_hostname()?;
        let swap_dir = vault.path.join("swap");
        debug!(
            "swap_in: project_dir={}, swap_dir={}, hostname={}",
            project_dir.display(),
            swap_dir.display(),
            hostname
        );

        let mut swapped = Vec::new();

        for file in files {
            // Normalize to absolute project path
            let project_file = if file.is_absolute() {
                file.clone()
            } else {
                project_dir.join(file)
            };

            // Get relative path for vault lookup
            let relative = project_file.strip_prefix(project_dir).map_err(|_| {
                ApplicationError::OperationFailed {
                    context: format!(
                        "file {} is not within project {}",
                        project_file.display(),
                        project_dir.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "file not in project",
                    )),
                }
            })?;

            // Compute vault path - first try neutralized form (expected after init/swap_out)
            let neutralized_relative = neutralize_path(relative);
            let vault_file_neutralized = swap_dir.join(&neutralized_relative);
            let vault_file_original = swap_dir.join(relative);

            debug!(
                "swap_in: checking vault_file_neutralized={}, exists={}",
                vault_file_neutralized.display(),
                self.fs.exists(&vault_file_neutralized)
            );
            debug!(
                "swap_in: checking vault_file_original={}, exists={}",
                vault_file_original.display(),
                self.fs.exists(&vault_file_original)
            );

            // Determine which vault file to use:
            // 1. Prefer neutralized form (dot.xxx) - this is the expected form after init
            // 2. Fall back to original form if neutralized doesn't exist
            //    (this allows the bare dotfile check below to catch and reject it)
            let vault_file = if self.fs.exists(&vault_file_neutralized) {
                vault_file_neutralized
            } else if self.fs.exists(&vault_file_original) {
                vault_file_original
            } else {
                // Neither exists - will fail with "no vault override" error below
                vault_file_neutralized // Use neutralized form in error message
            };

            // 1. Check for existing swap (sentinel in VAULT)
            if let Some((_, existing_host)) = self.find_any_sentinel(&swap_dir, relative) {
                if existing_host == hostname {
                    // Idempotent: already swapped in by same host, skip
                    eprintln!("{} already swapped in, skipping", project_file.display());
                    continue;
                } else {
                    return Err(ApplicationError::OperationFailed {
                        context: format!(
                            "{} is swapped in by host '{}', cannot swap from '{}'",
                            project_file.display(),
                            existing_host,
                            hostname
                        ),
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "swapped by different host",
                        )),
                    });
                }
            }

            // 2. Check vault override exists
            if !self.fs.exists(&vault_file) {
                return Err(ApplicationError::OperationFailed {
                    context: format!("no vault override for {}", project_file.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "vault override not found",
                    )),
                });
            }

            // 2b. Safety check: reject if bare dot-files exist in vault
            let bare_dotfiles = self.find_bare_dotfiles(&vault_file);
            if !bare_dotfiles.is_empty() {
                let expected_renames: Vec<String> = bare_dotfiles
                    .iter()
                    .map(|p| {
                        let name = p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                        let neutralized = neutralize_name(&name);
                        format!(
                            "  {} → {}",
                            p.display(),
                            p.with_file_name(&neutralized).display()
                        )
                    })
                    .collect();
                return Err(ApplicationError::OperationFailed {
                    context: format!(
                        "vault contains bare dot-files that should be neutralized.\n\
                         Please rename:\n{}",
                        expected_renames.join("\n")
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "bare dot-file in vault",
                    )),
                });
            }

            // 3. Create sentinel as COPY of vault content (before move)
            let sentinel_path = Self::get_sentinel_path(&swap_dir, relative, &hostname);
            debug!("swap_in: creating sentinel at {}", sentinel_path.display());
            self.fs
                .ensure_parent(&sentinel_path)
                .with_path_context("create sentinel parent for", &sentinel_path)?;
            self.fs
                .copy_any(&vault_file, &sentinel_path)
                .with_path_context("create sentinel copy of", &vault_file)?;

            // 4. Backup original to VAULT (if exists)
            // Use move_path for cross-device support (vault may be on different FS)
            let backup_path = Self::get_backup_path(&swap_dir, relative);
            if self.fs.exists(&project_file) {
                debug!(
                    "swap_in: backing up {} to {}",
                    project_file.display(),
                    backup_path.display()
                );
                self.fs
                    .move_path(&project_file, &backup_path)
                    .map_err(|e| {
                        // Cleanup sentinel on failure
                        let _ = self.fs.remove_any(&sentinel_path);
                        ApplicationError::OperationFailed {
                            context: format!(
                                "backup {} to {}",
                                project_file.display(),
                                backup_path.display()
                            ),
                            source: Box::new(e),
                        }
                    })?;
            }

            // 5. MOVE vault content to project (vault file removed)
            debug!(
                "swap_in: moving {} to {}",
                vault_file.display(),
                project_file.display()
            );
            self.fs.move_path(&vault_file, &project_file).map_err(|e| {
                // Rollback: restore backup, remove sentinel
                if self.fs.exists(&backup_path) {
                    let _ = self.fs.move_path(&backup_path, &project_file);
                }
                let _ = self.fs.remove_any(&sentinel_path);
                ApplicationError::OperationFailed {
                    context: format!(
                        "move {} to {}",
                        vault_file.display(),
                        project_file.display()
                    ),
                    source: Box::new(e),
                }
            })?;

            // 6. Restore any neutralized dot-files in project
            debug!(
                "swap_in: restoring dot-files in {}",
                project_file.display()
            );
            self.restore_dotfiles(&project_file)?;

            swapped.push(SwapFile {
                project_path: project_file,
                vault_path: vault_file,
                state: SwapState::In {
                    hostname: hostname.clone(),
                },
            });
        }

        // Add RSENV_SWAPPED marker to dot.envrc
        if !swapped.is_empty() {
            let dot_envrc = vault.path.join("dot.envrc");
            crate::application::envrc::add_swapped_marker(&self.fs, &dot_envrc)?;
        }

        Ok(swapped)
    }

    /// Swap files out (restore original project files).
    ///
    /// For each file:
    /// 1. Find sentinel in vault
    /// 2. MOVE modified project content back to vault (captures changes!)
    /// 3. Restore original from backup in vault
    /// 4. Remove sentinel
    ///
    /// # Arguments
    /// * `project_dir` - Project directory
    /// * `files` - Files to swap out
    ///
    /// # Returns
    /// Vec of swapped files with their new state
    pub fn swap_out(
        &self,
        project_dir: &Path,
        files: &[PathBuf],
    ) -> ApplicationResult<Vec<SwapFile>> {
        let vault = self
            .vault_service
            .get(project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.to_path_buf()))?;

        let hostname = Self::get_hostname()?;
        let swap_dir = vault.path.join("swap");
        debug!(
            "swap_out: project_dir={}, swap_dir={}, files={:?}",
            project_dir.display(),
            swap_dir.display(),
            files
        );

        let mut swapped = Vec::new();

        for file in files {
            let project_file = if file.is_absolute() {
                file.clone()
            } else {
                project_dir.join(file)
            };

            let relative = project_file.strip_prefix(project_dir).map_err(|_| {
                ApplicationError::OperationFailed {
                    context: format!(
                        "file {} is not within project {}",
                        project_file.display(),
                        project_dir.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "file not in project",
                    )),
                }
            })?;

            let vault_file = swap_dir.join(relative);
            let backup_path = Self::get_backup_path(&swap_dir, relative);

            // Find sentinel in VAULT
            match self.find_any_sentinel(&swap_dir, relative) {
                Some((sentinel_path, sentinel_host)) => {
                    debug!(
                        "swap_out: found sentinel for {} (host={})",
                        project_file.display(),
                        sentinel_host
                    );
                    // Normal swap-out case: sentinel exists
                    if sentinel_host != hostname {
                        return Err(ApplicationError::OperationFailed {
                            context: format!(
                                "{} is swapped in by host '{}', cannot swap out from '{}'",
                                project_file.display(),
                                sentinel_host,
                                hostname
                            ),
                            source: Box::new(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                "swapped by different host",
                            )),
                        });
                    }

                    // 1. MOVE modified project content back to vault (captures changes!)
                    // Use move_path for cross-device support (vault may be on different FS)
                    let project_existed = self.fs.exists(&project_file);
                    if project_existed {
                        debug!(
                            "swap_out: moving {} to vault at {}",
                            project_file.display(),
                            vault_file.display()
                        );
                        self.fs
                            .ensure_parent(&vault_file)
                            .with_path_context("create vault parent for", &vault_file)?;
                        self.fs.move_path(&project_file, &vault_file).map_err(|e| {
                            ApplicationError::OperationFailed {
                                context: format!(
                                    "move {} to {}",
                                    project_file.display(),
                                    vault_file.display()
                                ),
                                source: Box::new(e),
                            }
                        })?;

                        // Neutralize any dot-files in vault
                        self.neutralize_dotfiles(&vault_file)?;
                    }

                    // 2. Restore original from backup in VAULT
                    if self.fs.exists(&backup_path) {
                        debug!(
                            "swap_out: restoring original from {}",
                            backup_path.display()
                        );
                        if let Err(e) = self.fs.move_path(&backup_path, &project_file) {
                            // Rollback: move vault content back to project
                            if project_existed {
                                let _ = self.fs.move_path(&vault_file, &project_file);
                            }
                            return Err(ApplicationError::OperationFailed {
                                context: format!(
                                    "restore {} from {}",
                                    project_file.display(),
                                    backup_path.display()
                                ),
                                source: Box::new(e),
                            });
                        }
                    }

                    // 3. Remove sentinel from VAULT (file or directory)
                    debug!("swap_out: removing sentinel {}", sentinel_path.display());
                    self.fs
                        .remove_any(&sentinel_path)
                        .with_path_context("remove sentinel", &sentinel_path)?;
                }
                None => {
                    // Idempotent: not swapped in, skip
                    eprintln!("{} not swapped in, skipping", project_file.display());
                    continue;
                }
            }

            swapped.push(SwapFile {
                project_path: project_file,
                vault_path: vault_file,
                state: SwapState::Out,
            });
        }

        // Remove RSENV_SWAPPED marker if no files remain swapped in
        if !swapped.is_empty() {
            let remaining = self.status(project_dir)?;
            let any_swapped = remaining
                .iter()
                .any(|s| matches!(s.state, SwapState::In { .. }));

            if !any_swapped {
                let dot_envrc = vault.path.join("dot.envrc");
                crate::application::envrc::remove_swapped_marker(&self.fs, &dot_envrc)?;
            }
        }

        Ok(swapped)
    }

    /// Initialize files in vault (move from project to vault).
    ///
    /// This is for first-time setup: moves project files to vault
    /// so they can later be swapped in/out.
    ///
    /// # Arguments
    /// * `project_dir` - Project directory
    /// * `files` - Files to initialize (must exist in project)
    ///
    /// # Returns
    /// Vec of initialized files
    pub fn swap_init(
        &self,
        project_dir: &Path,
        files: &[PathBuf],
    ) -> ApplicationResult<Vec<SwapFile>> {
        let vault = self
            .vault_service
            .get(project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.to_path_buf()))?;

        let swap_dir = vault.path.join("swap");
        debug!(
            "swap_init: project_dir={}, swap_dir={}, files={:?}",
            project_dir.display(),
            swap_dir.display(),
            files
        );
        let mut initialized = Vec::new();

        for file in files {
            let project_file = if file.is_absolute() {
                file.clone()
            } else {
                project_dir.join(file)
            };

            let relative = project_file.strip_prefix(project_dir).map_err(|_| {
                ApplicationError::OperationFailed {
                    context: format!(
                        "file {} is not within project {}",
                        project_file.display(),
                        project_dir.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "file not in project",
                    )),
                }
            })?;

            let vault_file = swap_dir.join(relative);

            // Check project file exists
            if !self.fs.exists(&project_file) {
                return Err(ApplicationError::OperationFailed {
                    context: format!("project file does not exist: {}", project_file.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file not found",
                    )),
                });
            }

            // Check vault file does NOT exist
            if self.fs.exists(&vault_file) {
                return Err(ApplicationError::OperationFailed {
                    context: format!("vault already has file: {}", vault_file.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "already initialized",
                    )),
                });
            }

            // Create parent dirs in vault
            self.fs
                .ensure_parent(&vault_file)
                .with_path_context("create vault parent for", &vault_file)?;

            // Move project to vault (no sentinel, no backup)
            // Use move_path for cross-device support (vault may be on different FS)
            debug!(
                "swap_init: moving {} to vault at {}",
                project_file.display(),
                vault_file.display()
            );
            self.fs.move_path(&project_file, &vault_file).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!(
                        "move {} to {}",
                        project_file.display(),
                        vault_file.display()
                    ),
                    source: Box::new(e),
                }
            })?;

            // Neutralize any dot-files in vault
            self.neutralize_dotfiles(&vault_file)?;

            initialized.push(SwapFile {
                project_path: project_file,
                vault_path: vault_file,
                state: SwapState::Out,
            });
        }

        Ok(initialized)
    }

    /// Get swap status for all swappable files in a project.
    ///
    /// Finds all files that have vault overrides (swapped out) or sentinels (swapped in).
    ///
    /// # Arguments
    /// * `project_dir` - Project directory
    ///
    /// # Returns
    /// Vec of SwapFile with current states
    pub fn status(&self, project_dir: &Path) -> ApplicationResult<Vec<SwapFile>> {
        debug!("status: project_dir={}", project_dir.display());
        let vault = match self.vault_service.get(project_dir)? {
            Some(v) => v,
            None => {
                debug!("status: no vault found");
                return Ok(vec![]);
            }
        };

        let swap_dir = vault.path.join("swap");
        if !self.fs.exists(&swap_dir) {
            debug!("status: swap_dir does not exist");
            return Ok(vec![]);
        }

        let mut files = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        // Helper to check if a path is inside a sentinel/backup directory
        let is_inside_special_dir = |path: &Path| -> bool {
            path.ancestors().skip(1).any(|ancestor| {
                ancestor
                    .file_name()
                    .map(|n| {
                        let s = n.to_string_lossy();
                        s.ends_with(".rsenv_active") || s.ends_with(".rsenv_original")
                    })
                    .unwrap_or(false)
            })
        };

        // Walk vault swap directory (files AND directories)
        for entry in WalkDir::new(&swap_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.depth() > 0)
        // skip root
        {
            let vault_path = entry.path().to_path_buf();

            // Skip entries inside sentinel/backup directories (they're atomic units)
            if is_inside_special_dir(&vault_path) {
                continue;
            }

            let name = vault_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip backup files/directories
            if name.ends_with(".rsenv_original") {
                continue;
            }

            // Handle sentinel files/directories: extract the base name
            if name.ends_with(".rsenv_active") {
                // Sentinel format: <filename>.<hostname>.rsenv_active
                // Extract <filename> by removing .<hostname>.rsenv_active suffix
                let suffix_start = name
                    .rfind('.')
                    .and_then(|last_dot| name[..last_dot].rfind('.'));

                if let Some(base_end) = suffix_start {
                    let base_name = &name[..base_end];
                    let parent = vault_path.parent().unwrap_or(&swap_dir);
                    let base_vault_path = parent.join(base_name);

                    let relative = base_vault_path.strip_prefix(&swap_dir).map_err(|_| {
                        ApplicationError::OperationFailed {
                            context: format!("strip prefix from {}", base_vault_path.display()),
                            source: Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "path error",
                            )),
                        }
                    })?;

                    // Only add if we haven't seen this path yet
                    if !seen_paths.contains(relative) {
                        seen_paths.insert(relative.to_path_buf());
                        let project_path = project_dir.join(relative);
                        let state = self.get_swap_state(&swap_dir, relative)?;

                        files.push(SwapFile {
                            project_path,
                            vault_path: base_vault_path,
                            state,
                        });
                    }
                }
                continue;
            }

            // Regular vault file/directory (swapped out)
            let relative = vault_path.strip_prefix(&swap_dir).map_err(|_| {
                ApplicationError::OperationFailed {
                    context: format!("strip prefix from {}", vault_path.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "path error",
                    )),
                }
            })?;

            // Only add if we haven't seen this path yet
            if !seen_paths.contains(relative) {
                seen_paths.insert(relative.to_path_buf());
                let project_path = project_dir.join(relative);
                let state = self.get_swap_state(&swap_dir, relative)?;

                files.push(SwapFile {
                    project_path,
                    vault_path,
                    state,
                });
            }
        }

        debug!("status: found {} swap files", files.len());
        Ok(files)
    }

    /// Swap out all projects under a base directory.
    ///
    /// Finds all projects with vaults and swaps out all active swaps.
    ///
    /// # Arguments
    /// * `base_dir` - Directory to search for projects
    ///
    /// # Returns
    /// Vec of project directories that were processed
    pub fn swap_out_all(&self, base_dir: &Path) -> ApplicationResult<Vec<PathBuf>> {
        debug!("swap_out_all: base_dir={}", base_dir.display());
        let mut processed = Vec::new();

        // Walk directory looking for .envrc symlinks (indicating vault)
        for entry in WalkDir::new(base_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == ".envrc" && self.fs.is_symlink(e.path()))
        {
            if let Some(project_dir) = entry.path().parent() {
                // Get status and swap out any active swaps
                let status = self.status(project_dir)?;
                let swapped_in: Vec<_> = status
                    .iter()
                    .filter(|s| matches!(s.state, SwapState::In { .. }))
                    .map(|s| s.project_path.clone())
                    .collect();

                if !swapped_in.is_empty() {
                    debug!(
                        "swap_out_all: processing {} with {} swapped files",
                        project_dir.display(),
                        swapped_in.len()
                    );
                    self.swap_out(project_dir, &swapped_in)?;
                    processed.push(project_dir.to_path_buf());
                }
            }
        }

        debug!("swap_out_all: processed {} projects", processed.len());
        Ok(processed)
    }

    /// Delete swap files from vault (remove override and backup).
    ///
    /// Removes files from swap management entirely. This deletes:
    /// - The vault override file: `vault/swap/<rel_path>`
    /// - The backup file if present: `vault/swap/<rel_path>.rsenv_original`
    ///
    /// # Safety
    /// - Fails if ANY targeted file is swapped in (has sentinel)
    /// - All-or-nothing: if any file is swapped in, no deletions occur
    /// - Project files are NOT deleted, only vault artifacts
    pub fn delete(
        &self,
        project_dir: &Path,
        files: &[PathBuf],
    ) -> ApplicationResult<Vec<SwapFile>> {
        let vault = self
            .vault_service
            .get(project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.to_path_buf()))?;

        let swap_dir = vault.path.join("swap");
        debug!(
            "delete: project_dir={}, files={:?}",
            project_dir.display(),
            files
        );

        // PHASE 1: Validate ALL files first (all-or-nothing safety)
        let mut to_delete: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new(); // (project_file, vault_file, relative)

        for file in files {
            let project_file = if file.is_absolute() {
                file.clone()
            } else {
                project_dir.join(file)
            };

            let relative = project_file
                .strip_prefix(project_dir)
                .map_err(|_| ApplicationError::OperationFailed {
                    context: format!(
                        "file {} is not within project {}",
                        project_file.display(),
                        project_dir.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "file not in project",
                    )),
                })?
                .to_path_buf();

            // Check if swapped in (fail if any sentinel exists)
            if let Some((_, hostname)) = self.find_any_sentinel(&swap_dir, &relative) {
                return Err(ApplicationError::OperationFailed {
                    context: format!(
                        "cannot delete {}: swapped in by host '{}', swap out first",
                        project_file.display(),
                        hostname
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "file is swapped in",
                    )),
                });
            }

            let vault_file = swap_dir.join(&relative);
            to_delete.push((project_file, vault_file, relative));
        }

        // PHASE 2: Delete vault artifacts (only if all validations passed)
        let mut deleted = Vec::new();

        for (project_file, vault_file, relative) in to_delete {
            // Delete vault override file (if exists - idempotent)
            if self.fs.exists(&vault_file) {
                debug!("delete: removing vault file {}", vault_file.display());
                self.fs
                    .remove_any(&vault_file)
                    .with_path_context("delete vault file", &vault_file)?;
            }

            // Delete backup file (if exists - idempotent)
            let backup_path = Self::get_backup_path(&swap_dir, &relative);
            if self.fs.exists(&backup_path) {
                debug!("delete: removing backup {}", backup_path.display());
                self.fs
                    .remove_any(&backup_path)
                    .with_path_context("delete backup", &backup_path)?;
            }

            deleted.push(SwapFile {
                project_path: project_file,
                vault_path: vault_file,
                state: SwapState::Out,
            });
        }

        Ok(deleted)
    }

    /// Get the current swap state of a file by checking vault for sentinel.
    fn get_swap_state(&self, swap_dir: &Path, relative: &Path) -> ApplicationResult<SwapState> {
        if let Some((_, hostname)) = self.find_any_sentinel(swap_dir, relative) {
            Ok(SwapState::In { hostname })
        } else {
            Ok(SwapState::Out)
        }
    }

    /// Get current hostname.
    fn get_hostname() -> ApplicationResult<String> {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .map_err(|e| ApplicationError::OperationFailed {
                context: "get hostname".to_string(),
                source: Box::new(e),
            })
    }
}
