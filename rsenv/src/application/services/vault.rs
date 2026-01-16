//! Vault (project-associated directory) management service
//!
//! Handles creation and management of vaults - secure storage locations
//! for project-specific sensitive files and configurations.

use std::path::Path;
use std::sync::Arc;

use tracing::debug;

use crate::application::dotfile::neutralize_path;
use crate::application::{ApplicationError, ApplicationResult, IoResultExt};
use crate::config::Settings;
use crate::domain::{GuardedFile, Vault};
use crate::infrastructure::traits::FileSystem;

/// Vault management service.
pub struct VaultService {
    fs: Arc<dyn FileSystem>,
    settings: Arc<Settings>,
}

impl VaultService {
    /// Create a new vault service.
    pub fn new(fs: Arc<dyn FileSystem>, settings: Arc<Settings>) -> Self {
        Self { fs, settings }
    }

    /// Discover vault path from project's .envrc symlink.
    ///
    /// This is a standalone function that doesn't require Settings,
    /// enabling vault discovery before full config loading.
    ///
    /// # Arguments
    /// * `fs` - FileSystem trait for file operations
    /// * `project_dir` - Path to the project directory
    ///
    /// # Returns
    /// Some(vault_path) if project is initialized, None otherwise
    pub fn discover_vault_path(
        fs: &dyn FileSystem,
        project_dir: &Path,
    ) -> ApplicationResult<Option<std::path::PathBuf>> {
        // Canonicalize project_dir for consistent path handling
        let project_dir = match fs.canonicalize(project_dir) {
            Ok(p) => p,
            Err(_) => return Ok(None), // Project dir doesn't exist
        };
        let envrc_path = project_dir.join(".envrc");

        // Check if .envrc exists and is a symlink
        if !fs.exists(&envrc_path) || !fs.is_symlink(&envrc_path) {
            return Ok(None);
        }

        // Read symlink target
        let target = match fs.read_link(&envrc_path) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        // Target should be dot.envrc in a vault directory
        if !target.ends_with("dot.envrc") {
            return Ok(None);
        }

        // Resolve relative symlink target to absolute path
        let resolved_target = if target.is_relative() {
            project_dir.join(&target)
        } else {
            target.clone()
        };

        // Canonicalize to get clean absolute path
        let resolved_target = match fs.canonicalize(&resolved_target) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Get vault directory (parent of dot.envrc)
        Ok(resolved_target.parent().map(|p| p.to_path_buf()))
    }

    /// Initialize a vault for a project.
    ///
    /// Creates a new vault directory and links it to the project via .envrc symlink.
    /// If the project already has a vault, returns the existing one.
    ///
    /// # Arguments
    /// * `project_dir` - Path to the project directory
    /// * `absolute` - If true, use absolute paths for symlinks; otherwise use relative paths
    ///
    /// # Returns
    /// The created or existing Vault
    pub fn init(&self, project_dir: &Path, absolute: bool) -> ApplicationResult<Vault> {
        debug!(
            "init: project_dir={}, absolute={}",
            project_dir.display(),
            absolute
        );
        // Canonicalize project_dir for consistent path handling
        let project_dir =
            self.fs
                .canonicalize(project_dir)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("canonicalize project dir: {}", project_dir.display()),
                    source: Box::new(e),
                })?;

        // Check if already initialized
        if let Some(vault) = self.get(&project_dir)? {
            debug!("init: already initialized at {}", vault.path.display());
            return Ok(vault);
        }

        // Generate sentinel ID: projectname-shortid
        let project_name = project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        let short_id = Self::generate_short_id();
        let sentinel_id = format!("{}-{}", project_name, short_id);

        // Create vault directory
        let vault_path = self.settings.vaults_dir().join(&sentinel_id);
        debug!("init: creating vault at {}", vault_path.display());
        self.fs
            .create_dir_all(&vault_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("create vault directory: {}", vault_path.display()),
                source: Box::new(e),
            })?;

        // Canonicalize vault path for consistency with get()
        let vault_path =
            self.fs
                .canonicalize(&vault_path)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("canonicalize vault path: {}", vault_path.display()),
                    source: Box::new(e),
                })?;

        // Handle existing .envrc: move to vault as dot.envrc (preserving user content)
        let envrc_link = project_dir.join(".envrc");
        let dot_envrc_path = vault_path.join("dot.envrc");
        let relative = !absolute;

        if self.fs.exists(&envrc_link) {
            if self.fs.is_symlink(&envrc_link) {
                // Remove existing symlink, create empty dot.envrc
                self.fs.remove_file(&envrc_link).map_err(|e| {
                    ApplicationError::OperationFailed {
                        context: format!(
                            "remove existing .envrc symlink: {}",
                            envrc_link.display()
                        ),
                        source: Box::new(e),
                    }
                })?;
                self.fs.write(&dot_envrc_path, "").map_err(|e| {
                    ApplicationError::OperationFailed {
                        context: format!("create dot.envrc: {}", dot_envrc_path.display()),
                        source: Box::new(e),
                    }
                })?;
            } else {
                // MOVE existing .envrc to vault as dot.envrc (preserving user content!)
                self.fs.rename(&envrc_link, &dot_envrc_path).map_err(|e| {
                    ApplicationError::OperationFailed {
                        context: format!("move existing .envrc to {}", dot_envrc_path.display()),
                        source: Box::new(e),
                    }
                })?;
            }
        } else {
            // No existing .envrc - create empty dot.envrc
            self.fs
                .write(&dot_envrc_path, "")
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("create dot.envrc: {}", dot_envrc_path.display()),
                    source: Box::new(e),
                })?;
        }

        // Inject rsenv section into dot.envrc (preserves existing content)
        let section_data =
            Self::generate_rsenv_section(&sentinel_id, &project_dir, &vault_path, relative);
        crate::application::envrc::update_dot_envrc(&self.fs, &dot_envrc_path, &section_data)?;

        // Create vault subdirectories and default env files
        self.create_vault_subdirectories(&vault_path)?;
        self.create_default_env_files(&vault_path)?;

        // Create .envrc symlink in project pointing to vault's dot.envrc
        let symlink_result = if absolute {
            self.fs.symlink(&dot_envrc_path, &envrc_link)
        } else {
            self.fs.symlink_relative(&dot_envrc_path, &envrc_link)
        };
        symlink_result.map_err(|e| ApplicationError::OperationFailed {
            context: format!(
                "create .envrc symlink: {} -> {}",
                envrc_link.display(),
                dot_envrc_path.display()
            ),
            source: Box::new(e),
        })?;

        Ok(Vault {
            path: vault_path,
            sentinel_id,
        })
    }

    /// Get the vault for a project, if one exists.
    ///
    /// Checks if the project has a .envrc symlink pointing to a vault.
    ///
    /// # Arguments
    /// * `project_dir` - Path to the project directory
    ///
    /// # Returns
    /// Some(Vault) if initialized, None otherwise
    pub fn get(&self, project_dir: &Path) -> ApplicationResult<Option<Vault>> {
        debug!("get: project_dir={}", project_dir.display());
        // Canonicalize project_dir for consistent path handling
        let project_dir = match self.fs.canonicalize(project_dir) {
            Ok(p) => p,
            Err(_) => {
                debug!("get: project dir does not exist");
                return Ok(None);
            }
        };
        let envrc_path = project_dir.join(".envrc");

        // Check if .envrc exists and is a symlink
        if !self.fs.exists(&envrc_path) || !self.fs.is_symlink(&envrc_path) {
            debug!("get: no .envrc symlink found");
            return Ok(None);
        }

        // Read symlink target
        let target =
            self.fs
                .read_link(&envrc_path)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("read .envrc symlink: {}", envrc_path.display()),
                    source: Box::new(e),
                })?;

        // Target should be dot.envrc in a vault directory
        if !target.ends_with("dot.envrc") {
            return Ok(None);
        }

        // Resolve relative symlink target to absolute path
        let resolved_target = if target.is_relative() {
            project_dir.join(&target)
        } else {
            target.clone()
        };

        // Canonicalize to get clean absolute path
        let resolved_target = self.fs.canonicalize(&resolved_target).map_err(|e| {
            ApplicationError::OperationFailed {
                context: format!("resolve vault path: {}", resolved_target.display()),
                source: Box::new(e),
            }
        })?;

        // Get vault directory (parent of dot.envrc)
        let vault_path =
            resolved_target
                .parent()
                .ok_or_else(|| ApplicationError::OperationFailed {
                    context: format!("invalid vault path: {}", resolved_target.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "cannot determine vault directory",
                    )),
                })?;

        // Extract sentinel_id from vault directory name
        let sentinel_id = vault_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ApplicationError::OperationFailed {
                context: format!("invalid vault name: {}", vault_path.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "cannot determine sentinel ID",
                )),
            })?
            .to_string();

        debug!("get: found vault at {}", vault_path.display());
        Ok(Some(Vault {
            path: vault_path.to_path_buf(),
            sentinel_id,
        }))
    }

    /// Reconnect a project to its vault by re-creating the .envrc symlink.
    ///
    /// Use this when the .envrc symlink is deleted or when the project has moved.
    /// Updates state.sourceDir in dot.envrc if the project path has changed.
    ///
    /// # Arguments
    /// * `dot_envrc_path` - Path to the dot.envrc file in the vault
    /// * `project_dir` - Path to the project directory (may be new location)
    ///
    /// # Returns
    /// The Vault that was reconnected
    pub fn reconnect(&self, dot_envrc_path: &Path, project_dir: &Path) -> ApplicationResult<Vault> {
        debug!(
            "reconnect: dot_envrc={}, project_dir={}",
            dot_envrc_path.display(),
            project_dir.display()
        );
        // Verify dot.envrc exists
        if !self.fs.exists(dot_envrc_path) {
            return Err(ApplicationError::OperationFailed {
                context: format!("dot.envrc not found: {}", dot_envrc_path.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                )),
            });
        }

        // Read and parse metadata
        let content = self.fs.read_to_string(dot_envrc_path).map_err(|e| {
            ApplicationError::OperationFailed {
                context: format!("read dot.envrc: {}", dot_envrc_path.display()),
                source: Box::new(e),
            }
        })?;

        let metadata =
            crate::application::envrc::parse_rsenv_metadata(&content).ok_or_else(|| {
                ApplicationError::OperationFailed {
                    context: format!("no rsenv section in: {}", dot_envrc_path.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "not an rsenv-managed file",
                    )),
                }
            })?;

        // Canonicalize project_dir
        let project_dir =
            self.fs
                .canonicalize(project_dir)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("canonicalize project dir: {}", project_dir.display()),
                    source: Box::new(e),
                })?;

        // Get vault path (parent of dot.envrc)
        let dot_envrc_path = self.fs.canonicalize(dot_envrc_path).map_err(|e| {
            ApplicationError::OperationFailed {
                context: format!("canonicalize dot.envrc: {}", dot_envrc_path.display()),
                source: Box::new(e),
            }
        })?;

        let vault_path =
            dot_envrc_path
                .parent()
                .ok_or_else(|| ApplicationError::OperationFailed {
                    context: format!("invalid vault path: {}", dot_envrc_path.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "cannot determine vault directory",
                    )),
                })?;

        // Check if .envrc already exists in project
        let envrc_link = project_dir.join(".envrc");
        if self.fs.exists(&envrc_link) {
            if self.fs.is_symlink(&envrc_link) {
                // Check if it already points to the correct target
                if let Ok(target) = self.fs.read_link(&envrc_link) {
                    let resolved = if target.is_relative() {
                        project_dir.join(&target)
                    } else {
                        target
                    };
                    if let Ok(resolved) = self.fs.canonicalize(&resolved) {
                        if resolved == dot_envrc_path {
                            // Already correctly linked - idempotent success
                            return Ok(Vault {
                                path: vault_path.to_path_buf(),
                                sentinel_id: metadata.sentinel,
                            });
                        }
                    }
                }
                // Symlink exists but points elsewhere - error
                return Err(ApplicationError::OperationFailed {
                    context: format!(
                        ".envrc symlink exists but points elsewhere: {}",
                        envrc_link.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "symlink points to different target",
                    )),
                });
            } else {
                // Regular file exists - cannot overwrite
                return Err(ApplicationError::OperationFailed {
                    context: format!(
                        "cannot overwrite existing .envrc file: {}",
                        envrc_link.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "regular file exists",
                    )),
                });
            }
        }

        // Update state.sourceDir if project path changed
        let project_dir_str = project_dir.to_string_lossy().to_string();
        if metadata.source_dir != project_dir_str {
            crate::application::envrc::update_source_dir(
                &self.fs,
                &dot_envrc_path,
                &project_dir_str,
            )?;
        }

        // Create .envrc symlink (relative or absolute based on metadata)
        let symlink_result = if metadata.relative {
            self.fs.symlink_relative(&dot_envrc_path, &envrc_link)
        } else {
            self.fs.symlink(&dot_envrc_path, &envrc_link)
        };
        symlink_result.map_err(|e| ApplicationError::OperationFailed {
            context: format!(
                "create .envrc symlink: {} -> {}",
                envrc_link.display(),
                dot_envrc_path.display()
            ),
            source: Box::new(e),
        })?;

        Ok(Vault {
            path: vault_path.to_path_buf(),
            sentinel_id: metadata.sentinel,
        })
    }

    /// Generate a short ID for the vault (8 hex characters).
    fn generate_short_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{:08x}", (timestamp as u32) ^ std::process::id())
    }

    /// Create vault subdirectories (guarded, swap, envs).
    fn create_vault_subdirectories(&self, vault_dir: &Path) -> ApplicationResult<()> {
        for subdir in ["guarded", "swap", "envs"] {
            self.fs
                .create_dir_all(&vault_dir.join(subdir))
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!(
                        "create vault subdirectory: {}/{}",
                        vault_dir.display(),
                        subdir
                    ),
                    source: Box::new(e),
                })?;
        }
        Ok(())
    }

    /// Create default environment files in the envs subdirectory.
    fn create_default_env_files(&self, vault_dir: &Path) -> ApplicationResult<()> {
        let envs_dir = vault_dir.join("envs");
        for env in ["none", "local", "test", "int", "e2e", "prod"] {
            let path = envs_dir.join(format!("{}.env", env));
            let content = if env == "none" {
                // none.env is the root - no parent link
                format!(
                    "################################## {env}.env ##################################\n\
                     export RUN_ENV={env}\n"
                )
            } else {
                // All others link to none.env
                format!(
                    "################################## {env}.env ##################################\n\
                     # rsenv: none.env\n\
                     export RUN_ENV={env}\n"
                )
            };
            self.fs
                .write(&path, &content)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("create env file: {}", path.display()),
                    source: Box::new(e),
                })?;
        }
        Ok(())
    }

    /// Generate the rsenv section content (without delimiters).
    ///
    /// The content is meant to be injected via `update_dot_envrc()` which
    /// adds the section delimiters and handles existing content preservation.
    fn generate_rsenv_section(
        sentinel_id: &str,
        project_dir: &Path,
        vault_path: &Path,
        relative: bool,
    ) -> String {
        use chrono::Utc;

        let timestamp = Utc::now().to_rfc3339();

        // Convert paths to use $HOME for portability
        let home_dir = std::env::var("HOME").unwrap_or_default();
        let source_dir = if !home_dir.is_empty() {
            project_dir.to_string_lossy().replace(&home_dir, "$HOME")
        } else {
            project_dir.to_string_lossy().to_string()
        };
        let vault_var = if !home_dir.is_empty() {
            vault_path.to_string_lossy().replace(&home_dir, "$HOME")
        } else {
            vault_path.to_string_lossy().to_string()
        };

        format!(
            r#"# config.relative = {relative}
# config.version = 2
# state.sentinel = '{sentinel_id}'
# state.timestamp = '{timestamp}'
# state.sourceDir = '{source_dir}'
export RSENV_VAULT={vault_var}
{vars_marker}"#,
            sentinel_id = sentinel_id,
            relative = relative,
            timestamp = timestamp,
            source_dir = source_dir,
            vault_var = vault_var,
            vars_marker = crate::application::envrc::VARS_SECTION_DELIMITER,
        )
    }

    /// Guard a file by moving it to the vault and creating a symlink.
    ///
    /// # Arguments
    /// * `file` - Path to the file to guard (must be within an initialized project)
    /// * `absolute` - If true, use absolute paths for symlinks; otherwise use relative paths
    ///
    /// # Returns
    /// GuardedFile with project and vault paths
    pub fn guard(&self, file: &Path, absolute: bool) -> ApplicationResult<GuardedFile> {
        debug!("guard: file={}, absolute={}", file.display(), absolute);
        // Check if file is already guarded (symlink to vault)
        if self.fs.is_symlink(file) {
            if let Ok(target) = self.fs.read_link(file) {
                if target.to_string_lossy().contains("/guarded/") {
                    return Err(ApplicationError::AlreadyGuarded(file.to_path_buf()));
                }
            }
        }

        // Canonicalize file path for consistent path handling
        let file = self
            .fs
            .canonicalize(file)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("canonicalize file: {}", file.display()),
                source: Box::new(e),
            })?;

        // Find the project directory by looking for .envrc symlink
        let project_dir = self.find_project_dir(&file)?;
        let vault = self
            .get(&project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.clone()))?;

        // Compute relative path within project
        let relative_path =
            file.strip_prefix(&project_dir)
                .map_err(|_| ApplicationError::OperationFailed {
                    context: format!(
                        "file {} is not within project {}",
                        file.display(),
                        project_dir.display()
                    ),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "file not in project",
                    )),
                })?;

        // Create vault path with same structure, but neutralize dotfiles
        // (.gitignore -> dot.gitignore) to prevent them from affecting vault
        let neutralized_relative = neutralize_path(relative_path);
        let vault_path = vault.path.join("guarded").join(&neutralized_relative);

        // Create parent directories in vault
        self.fs
            .ensure_parent(&vault_path)
            .with_path_context("create vault directory for", &vault_path)?;

        // Move file to vault
        debug!(
            "guard: moving {} to {}",
            file.display(),
            vault_path.display()
        );
        self.fs
            .rename(&file, &vault_path)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("move file {} to {}", file.display(), vault_path.display()),
                source: Box::new(e),
            })?;

        // Create symlink in project (relative by default)
        debug!("guard: creating symlink at {}", file.display());
        let symlink_result = if absolute {
            self.fs.symlink(&vault_path, &file)
        } else {
            self.fs.symlink_relative(&vault_path, &file)
        };
        symlink_result.map_err(|e| ApplicationError::OperationFailed {
            context: format!(
                "create symlink {} -> {}",
                file.display(),
                vault_path.display()
            ),
            source: Box::new(e),
        })?;

        Ok(GuardedFile {
            project_path: file,
            vault_path,
            encrypted: false,
        })
    }

    /// Unguard a file by restoring it from the vault.
    ///
    /// # Arguments
    /// * `file` - Path to the symlink in the project
    ///
    /// # Returns
    /// Ok(()) on success
    pub fn unguard(&self, file: &Path) -> ApplicationResult<()> {
        debug!("unguard: file={}", file.display());
        // Verify it's a symlink
        if !self.fs.is_symlink(file) {
            return Err(ApplicationError::OperationFailed {
                context: format!("not a guarded file (not a symlink): {}", file.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "not a symlink",
                )),
            });
        }

        // Read symlink target
        let link_target =
            self.fs
                .read_link(file)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("read symlink: {}", file.display()),
                    source: Box::new(e),
                })?;

        // Resolve relative symlink target to absolute path
        let vault_path = if link_target.is_relative() {
            let file_parent = file.parent().unwrap_or(Path::new("."));
            self.fs
                .canonicalize(&file_parent.join(&link_target))
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("resolve symlink target: {}", link_target.display()),
                    source: Box::new(e),
                })?
        } else {
            link_target
        };

        // Verify vault file exists
        if !self.fs.exists(&vault_path) {
            return Err(ApplicationError::OperationFailed {
                context: format!("vault file not found: {}", vault_path.display()),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "vault file missing",
                )),
            });
        }

        // Remove symlink
        self.fs
            .remove_file(file)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("remove symlink: {}", file.display()),
                source: Box::new(e),
            })?;

        // Move file from vault back to project
        debug!("unguard: restoring {} from vault", file.display());
        self.fs
            .rename(&vault_path, file)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!(
                    "restore file from {} to {}",
                    vault_path.display(),
                    file.display()
                ),
                source: Box::new(e),
            })?;

        Ok(())
    }

    /// Reset a vault: restore all guarded files and remove .envrc symlink.
    ///
    /// This operation:
    /// 1. Restores all guarded files from the vault back to the project
    /// 2. Removes the .envrc symlink from the project
    /// 3. Restores the original .envrc content (from backup or dot.envrc)
    ///
    /// The vault directory is NOT deleted - user must remove it manually.
    ///
    /// # Arguments
    /// * `project_dir` - Path to the project directory
    ///
    /// # Returns
    /// Number of files restored
    pub fn reset(&self, project_dir: &Path) -> ApplicationResult<usize> {
        debug!("reset: project_dir={}", project_dir.display());
        use walkdir::WalkDir;

        // Canonicalize project_dir for consistent path handling
        let project_dir =
            self.fs
                .canonicalize(project_dir)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("canonicalize project dir: {}", project_dir.display()),
                    source: Box::new(e),
                })?;

        // Get vault for project (error if not initialized)
        let vault = self
            .get(&project_dir)?
            .ok_or_else(|| ApplicationError::VaultNotInitialized(project_dir.clone()))?;

        // Collect and restore all guarded files
        let guarded_dir = vault.path.join("guarded");
        debug!("reset: guarded_dir={}", guarded_dir.display());
        let mut restored_count = 0;

        if self.fs.exists(&guarded_dir) {
            // Walk the guarded directory to find all files
            let guarded_files: Vec<_> = WalkDir::new(&guarded_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .collect();

            for vault_path in guarded_files {
                // Compute project path: strip guarded/ prefix and add project_dir
                if let Ok(relative) = vault_path.strip_prefix(&guarded_dir) {
                    let project_path = project_dir.join(relative);

                    // Only unguard if the project path is a symlink pointing to vault
                    if self.fs.is_symlink(&project_path) {
                        match self.unguard(&project_path) {
                            Ok(()) => restored_count += 1,
                            Err(e) => {
                                // Log warning but continue with other files
                                eprintln!(
                                    "Warning: Failed to restore {}: {}",
                                    project_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Handle .envrc restoration
        let envrc_path = project_dir.join(".envrc");
        let backup_path = vault.path.join("envrc.backup");
        let dot_envrc_path = vault.path.join("dot.envrc");

        // Remove .envrc symlink if it exists
        if self.fs.is_symlink(&envrc_path) {
            self.fs
                .remove_file(&envrc_path)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("remove .envrc symlink: {}", envrc_path.display()),
                    source: Box::new(e),
                })?;
        }

        // Restore .envrc content
        if self.fs.exists(&backup_path) {
            // If backup exists, restore it
            self.fs.rename(&backup_path, &envrc_path).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("restore .envrc backup from {}", backup_path.display()),
                    source: Box::new(e),
                }
            })?;
        } else if self.fs.exists(&dot_envrc_path) {
            // Move dot.envrc to project as .envrc
            self.fs.rename(&dot_envrc_path, &envrc_path).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("move dot.envrc to .envrc from {}", dot_envrc_path.display()),
                    source: Box::new(e),
                }
            })?;

            // Remove the rsenv section from the restored .envrc
            crate::application::envrc::delete_section(&self.fs, &envrc_path)?;
        }

        debug!("reset: restored {} files", restored_count);
        Ok(restored_count)
    }

    /// Find the project directory for a file by searching up for .envrc symlink.
    fn find_project_dir(&self, file: &Path) -> ApplicationResult<std::path::PathBuf> {
        let mut current = file.parent();
        while let Some(dir) = current {
            let envrc = dir.join(".envrc");
            if self.fs.is_symlink(&envrc) {
                return Ok(dir.to_path_buf());
            }
            current = dir.parent();
        }

        Err(ApplicationError::VaultNotInitialized(file.to_path_buf()))
    }
}
