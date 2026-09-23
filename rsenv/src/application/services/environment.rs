//! Environment hierarchy service
//!
//! Handles building merged environment variables from hierarchical env files.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use tracing::debug;

use crate::application::{ApplicationError, ApplicationResult};
use crate::domain::EnvFile;
use crate::infrastructure::traits::FileSystem;

/// Output from building an environment hierarchy.
#[derive(Debug, Clone)]
pub struct EnvOutput {
    /// Merged environment variables (parents overridden by children)
    pub variables: BTreeMap<String, String>,
    /// Keys to emit as shell literals (single-quoted in source). Merged with
    /// the same child-overrides-parent precedence as `variables`.
    pub literal_keys: BTreeSet<String>,
    /// Files in the hierarchy, in BFS order (roots first)
    pub files: Vec<EnvFile>,
}

/// An env file whose `# rsenv:` directive names several parents.
///
/// Legal for `env build`, which merges a DAG, but not for the tree commands.
#[derive(Debug, Clone)]
pub struct MultiParentFile {
    /// The file carrying the directive
    pub file: PathBuf,
    /// The parents it names, as written
    pub parents: Vec<String>,
}

/// Hierarchy information for a directory of env files.
#[derive(Debug, Clone)]
pub struct EnvHierarchy {
    /// All env files found in the directory
    pub files: Vec<EnvFile>,
}

/// Environments scaffolded into a vault's `envs/` directory. `none` comes first: it is the
/// hierarchy root that all others link to.
pub(crate) const DEFAULT_ENVS: [&str; 6] = ["none", "local", "test", "int", "e2e", "prod"];

/// Body of a default env file.
///
/// `none.env` is the hierarchy root and carries no parent link; every other env declares
/// `none.env` as its parent, so anything added to the root is inherited by all of them.
pub(crate) fn default_env_content(env: &str) -> String {
    if env == "none" {
        format!(
            "################################## {env}.env ##################################\n\
             export RUN_ENV={env}\n"
        )
    } else {
        format!(
            "################################## {env}.env ##################################\n\
             # rsenv: none.env\n\
             export RUN_ENV={env}\n"
        )
    }
}

/// Marker `init_files` inserts to mark a swept-aside file.
const BACKUP_MARKER: &str = "bkp";

/// Name `init_files` sweeps a file aside to: `local.env` -> `local.bkp.env`.
/// A file with no extension gets the marker appended: `NOTES` -> `NOTES.bkp`.
///
/// The marker sits *before* the extension so the backup keeps the extension of the original.
/// Both the vault's `*.env` gitignore and `sops.file_extensions_enc` key off that extension,
/// so a trailing `local.env.bkp` would be covered by neither - it would sit in `envs/` as
/// uncommittable, unencryptable plaintext and wedge `rsenv vault commit`, which refuses any
/// non-`.enc` file under `envs/`.
fn backup_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    match path.extension() {
        Some(ext) => {
            path.with_file_name(format!("{stem}.{BACKUP_MARKER}.{}", ext.to_string_lossy()))
        }
        None => path.with_file_name(format!("{stem}.{BACKUP_MARKER}")),
    }
}

/// True when `path` already names a backup, so sweeps never cascade.
///
/// Covers the current `local.bkp.env` shape, the extension-less `NOTES.bkp` shape, and the
/// superseded `local.env.bkp` shape - old backups are recognised so a sweep leaves them
/// alone rather than burying them one level deeper.
fn is_backup(path: &Path) -> bool {
    if path.extension().is_some_and(|e| e == BACKUP_MARKER) {
        return true;
    }
    path.file_stem()
        .is_some_and(|s| s.to_string_lossy().ends_with(&format!(".{BACKUP_MARKER}")))
}

/// Service for building hierarchical environment variables.
pub struct EnvironmentService {
    fs: Arc<dyn FileSystem>,
}

impl EnvironmentService {
    /// Create a new environment service.
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self { fs }
    }

    /// Build merged environment variables from a leaf file.
    ///
    /// Performs BFS traversal from leaf to roots, then merges variables
    /// so that children override parents.
    pub fn build(&self, leaf: &Path) -> ApplicationResult<EnvOutput> {
        debug!("build: leaf={}", leaf.display());
        // v1 behavior: warn on symlinks
        self.warn_if_symlink(leaf);

        // Collect all files in hierarchy via BFS
        let files = self.collect_hierarchy(leaf)?;
        debug!("build: found {} files in hierarchy", files.len());

        // Merge variables: iterate in reverse (roots first) so children override
        let mut variables = BTreeMap::new();
        let mut literal_keys = BTreeSet::new();
        for file in files.iter().rev() {
            for (key, value) in &file.variables {
                variables.insert(key.clone(), value.clone());
                // The redefining file also decides the quote style for this key.
                if file.literal_keys.contains(key) {
                    literal_keys.insert(key.clone());
                } else {
                    literal_keys.remove(key);
                }
            }
        }

        Ok(EnvOutput {
            variables,
            literal_keys,
            files,
        })
    }

    /// Collect all files in the hierarchy via BFS traversal.
    ///
    /// Returns files in BFS order: leaf first, then parents, then grandparents, etc.
    fn collect_hierarchy(&self, leaf: &Path) -> ApplicationResult<Vec<EnvFile>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Start with the leaf
        queue.push_back(leaf.to_path_buf());

        while let Some(current_path) = queue.pop_front() {
            // Check file exists first - give clear error message
            if !self.fs.exists(&current_path) {
                return Err(ApplicationError::OperationFailed {
                    context: format!("file not found: {}", current_path.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file does not exist",
                    )),
                });
            }

            // Skip if already visited (handles cycles)
            let canonical = self.fs.canonicalize(&current_path).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("canonicalize {}", current_path.display()),
                    source: Box::new(e),
                }
            })?;

            if visited.contains(&canonical) {
                continue;
            }
            visited.insert(canonical.clone());

            // Read and parse the file
            let content = self.fs.read_to_string(&current_path).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("read env file {}", current_path.display()),
                    source: Box::new(e),
                }
            })?;

            let env_file = EnvFile::parse(&content, current_path.clone()).map_err(|e| {
                ApplicationError::OperationFailed {
                    context: format!("parse env file {}", current_path.display()),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.message,
                    )),
                }
            })?;

            // Add parents to queue in reverse order.
            // v1 behavior: "rightmost sibling wins" - rightmost parent should override leftmost.
            // BFS processes left-to-right, and merge reverses the order.
            // By reversing parents here, rightmost ends up being merged last (winning).
            for parent in env_file.parents.iter().rev() {
                if !visited.contains(parent) {
                    // Check parent exists
                    if !self.fs.exists(parent) {
                        return Err(ApplicationError::OperationFailed {
                            context: format!(
                                "parent file not found: {} (referenced from {})",
                                parent.display(),
                                current_path.display()
                            ),
                            source: Box::new(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "parent file not found",
                            )),
                        });
                    }
                    queue.push_back(parent.clone());
                }
            }

            result.push(env_file);
        }

        Ok(result)
    }

    /// Get hierarchy information for all env files in a directory.
    ///
    /// Scans the directory for `.env` files and parses them to extract
    /// their parent relationships.
    pub fn get_hierarchy(&self, dir: &Path) -> ApplicationResult<EnvHierarchy> {
        debug!("get_hierarchy: dir={}", dir.display());
        let mut files = Vec::new();

        // Scan directory for .env files
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip if not a file (use WalkDir entry method, not filesystem syscall)
            if !entry.file_type().is_file() {
                continue;
            }

            // Check for .env extension
            let is_env_file = path.extension().map(|ext| ext == "env").unwrap_or(false);

            if !is_env_file {
                continue;
            }

            // `init_files` backups keep the .env extension on purpose, so they are caught by
            // the vault's `*.env` gitignore and by SOPS. That puts them in front of this scan
            // too - exclude them, or every `env init` adds a duplicate node to `env tree` and
            // an extra candidate to `env select`.
            if is_backup(path) {
                continue;
            }

            // Parse the file
            let content = match self.fs.read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue, // Skip files we can't read
            };

            if let Ok(env_file) = EnvFile::parse(&content, path.to_path_buf()) {
                files.push(env_file);
            }
        }

        debug!("get_hierarchy: found {} env files", files.len());
        Ok(EnvHierarchy { files })
    }

    /// Regenerate the default env files in `envs_dir`.
    ///
    /// Every existing file is swept aside first: renamed by `backup_name`, or deleted when
    /// `clear` is set. Files that are already backups are left alone by the rename path so
    /// backups never cascade - a backup therefore holds the immediately previous version
    /// only, and is overwritten by a second call.
    ///
    /// Returns `(swept, created)`.
    pub fn init_files(&self, envs_dir: &Path, clear: bool) -> ApplicationResult<(usize, usize)> {
        debug!("init_files: envs_dir={} clear={}", envs_dir.display(), clear);

        // Tolerate a vault whose envs/ was deleted outright
        self.fs
            .create_dir_all(envs_dir)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("create envs directory: {}", envs_dir.display()),
                source: Box::new(e),
            })?;

        // Snapshot the listing before mutating it. The FileSystem trait has no read_dir, so
        // enumerate with WalkDir as get_hierarchy does.
        let existing: Vec<PathBuf> = walkdir::WalkDir::new(envs_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        let mut swept = 0;
        for path in existing {
            if clear {
                self.fs
                    .remove_file(&path)
                    .map_err(|e| ApplicationError::OperationFailed {
                        context: format!("remove env file: {}", path.display()),
                        source: Box::new(e),
                    })?;
                swept += 1;
                continue;
            }

            if is_backup(&path) {
                continue;
            }

            let backup = backup_name(&path);
            self.fs
                .rename(&path, &backup)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("back up env file: {} -> {}", path.display(), backup.display()),
                    source: Box::new(e),
                })?;
            swept += 1;
        }

        for env in DEFAULT_ENVS {
            let path = envs_dir.join(format!("{env}.env"));
            self.fs
                .write(&path, &default_env_content(env))
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("create env file: {}", path.display()),
                    source: Box::new(e),
                })?;
        }

        debug!("init_files: swept {} files", swept);
        Ok((swept, DEFAULT_ENVS.len()))
    }

    fn count_rsenv_directives(content: &str) -> usize {
        content
            .lines()
            .filter(|l| l.trim().starts_with("# rsenv:"))
            .count()
    }

    fn build_parent_reference(&self, parent: &Path, child: &Path) -> String {
        let fallback = || {
            parent
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| parent.to_string_lossy().to_string())
        };

        if let (Some(child_dir), Ok(parent_abs)) = (child.parent(), self.fs.canonicalize(parent)) {
            let child_dir_abs = self
                .fs
                .canonicalize(child_dir)
                .unwrap_or_else(|_| child_dir.to_path_buf());

            pathdiff::diff_paths(&parent_abs, child_dir_abs)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(fallback)
        } else {
            fallback()
        }
    }

    fn replace_rsenv_directive_lines(content: &str, replacement: &str) -> String {
        let new_lines: Vec<String> = content
            .lines()
            .map(|line| {
                if line.trim().starts_with("# rsenv:") {
                    replacement.to_string()
                } else {
                    line.to_string()
                }
            })
            .collect();

        new_lines.join("\n") + "\n"
    }

    /// Link a parent env file to a child.
    ///
    /// v1 behavior: REPLACES any existing parent (does not add).
    /// Uses relative path for the parent reference.
    /// Errors if the child has multiple `# rsenv:` directives.
    pub fn link(&self, parent: &Path, child: &Path) -> ApplicationResult<()> {
        debug!(
            "link: parent={}, child={}",
            parent.display(),
            child.display()
        );
        // Read the child file
        let content =
            self.fs
                .read_to_string(child)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("read child file {}", child.display()),
                    source: Box::new(e),
                })?;

        // Count rsenv directives - error if more than one
        let directive_count = Self::count_rsenv_directives(&content);
        if directive_count > 1 {
            return Err(ApplicationError::OperationFailed {
                context: format!(
                    "file {} has {} rsenv directives (expected at most 1)",
                    child.display(),
                    directive_count
                ),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "multiple rsenv directives not supported",
                )),
            });
        }

        // Get relative path from child to parent
        let parent_ref = self.build_parent_reference(parent, child);

        // Build new content - REPLACE existing directive or add new one
        let new_directive = format!("# rsenv: {}", parent_ref);

        let new_content = if directive_count == 0 {
            // No existing directive - add at top
            format!("{}\n{}", new_directive, content)
        } else {
            // Has existing directive - replace it
            Self::replace_rsenv_directive_lines(&content, &new_directive)
        };

        // Write back
        self.fs
            .write(child, &new_content)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("write child file {}", child.display()),
                source: Box::new(e),
            })?;

        Ok(())
    }

    /// Remove all parent links from an env file.
    ///
    /// v1 behavior: KEEPS the `# rsenv:` line but empties it (removes parent reference).
    /// Errors if the file has multiple `# rsenv:` directives.
    pub fn unlink(&self, file: &Path) -> ApplicationResult<()> {
        debug!("unlink: file={}", file.display());
        // Read the file
        let content =
            self.fs
                .read_to_string(file)
                .map_err(|e| ApplicationError::OperationFailed {
                    context: format!("read file {}", file.display()),
                    source: Box::new(e),
                })?;

        // Count rsenv directives - error if more than one
        let directive_count = Self::count_rsenv_directives(&content);
        if directive_count > 1 {
            return Err(ApplicationError::OperationFailed {
                context: format!(
                    "file {} has {} rsenv directives (expected at most 1)",
                    file.display(),
                    directive_count
                ),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "multiple rsenv directives not supported",
                )),
            });
        }

        // If no directive, nothing to do
        if directive_count == 0 {
            return Ok(());
        }

        // Replace directive with empty one (keep the line, remove parent)
        let new_content = Self::replace_rsenv_directive_lines(&content, "# rsenv:");

        // Write back
        self.fs
            .write(file, &new_content)
            .map_err(|e| ApplicationError::OperationFailed {
                context: format!("write file {}", file.display()),
                source: Box::new(e),
            })?;

        Ok(())
    }

    /// Env files that declare more than one parent - the shape tree commands cannot render.
    ///
    /// Returns every offender rather than a bare yes/no, so the caller can point at the
    /// exact `# rsenv:` lines to change. An empty result means the hierarchy is a tree.
    ///
    /// Only `*.env` files are inspected, matching `TreeBuilder::scan_directory`: a
    /// directive-shaped line anywhere else - a prose comment in `dot.envrc`, a swapped
    /// document - is not part of the hierarchy and must not be read as a declaration.
    pub fn multi_parent_files(&self, dir: &Path) -> ApplicationResult<Vec<MultiParentFile>> {
        debug!("multi_parent_files: dir={}", dir.display());
        let re = Regex::new(r"# rsenv:\s*(.+)").map_err(|e| ApplicationError::OperationFailed {
            context: "compile regex".to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        })?;

        let mut offenders = Vec::new();

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().is_none_or(|ext| ext != "env") {
                continue;
            }
            let Ok(content) = self.fs.read_to_string(entry.path()) else {
                continue;
            };

            for line in content.lines() {
                if let Some(caps) = re.captures(line) {
                    let parents: Vec<String> = caps[1]
                        .split_whitespace()
                        .map(|p| p.to_string())
                        .collect();
                    if parents.len() > 1 {
                        debug!(
                            "multi_parent_files: {} declares {} parents",
                            entry.path().display(),
                            parents.len()
                        );
                        offenders.push(MultiParentFile {
                            file: entry.path().to_path_buf(),
                            parents,
                        });
                    }
                }
            }
        }

        offenders.sort_by(|a, b| a.file.cmp(&b.file));
        Ok(offenders)
    }

    /// Link multiple files in a chain: files[0] <- files[1] <- files[2] <- ...
    /// First file becomes root (unlinked), each subsequent file links to previous.
    pub fn link_chain(&self, files: &[PathBuf]) -> ApplicationResult<()> {
        debug!("link_chain: {} files", files.len());
        if files.is_empty() {
            return Ok(());
        }

        let mut parent: Option<&PathBuf> = None;
        for file in files {
            if let Some(parent_path) = parent {
                self.link(parent_path, file)?;
            } else {
                // First file becomes root (unlink it)
                self.unlink(file)?;
            }
            parent = Some(file);
        }
        Ok(())
    }

    /// Get all files in hierarchy starting from leaf.
    pub fn get_files(&self, leaf: &Path) -> ApplicationResult<Vec<PathBuf>> {
        debug!("get_files: leaf={}", leaf.display());
        let output = self.build(leaf)?;
        debug!("get_files: found {} files in hierarchy", output.files.len());
        Ok(output.files.iter().map(|f| f.path.clone()).collect())
    }

    /// Warn if path is a symlink (v1 behavior).
    fn warn_if_symlink(&self, path: &Path) {
        if self.fs.is_symlink(path) {
            eprintln!("Warning: The file {} is a symbolic link.", path.display());
        }
    }
}
