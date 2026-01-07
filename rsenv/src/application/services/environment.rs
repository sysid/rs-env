//! Environment hierarchy service
//!
//! Handles building merged environment variables from hierarchical env files.

use std::collections::{BTreeMap, HashSet, VecDeque};
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
    /// Files in the hierarchy, in BFS order (roots first)
    pub files: Vec<EnvFile>,
}

/// Hierarchy information for a directory of env files.
#[derive(Debug, Clone)]
pub struct EnvHierarchy {
    /// All env files found in the directory
    pub files: Vec<EnvFile>,
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
        for file in files.iter().rev() {
            for (key, value) in &file.variables {
                variables.insert(key.clone(), value.clone());
            }
        }

        Ok(EnvOutput { variables, files })
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

    /// Check if directory contains DAG structure (files with multiple parents).
    pub fn is_dag(&self, dir: &Path) -> ApplicationResult<bool> {
        debug!("is_dag: dir={}", dir.display());
        let re = Regex::new(r"# rsenv:\s*(.+)").map_err(|e| ApplicationError::OperationFailed {
            context: "compile regex".to_string(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        })?;

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(content) = self.fs.read_to_string(entry.path()) {
                    for line in content.lines() {
                        if let Some(caps) = re.captures(line) {
                            let parents: Vec<&str> = caps[1].split_whitespace().collect();
                            if parents.len() > 1 {
                                debug!(
                                    "is_dag: found multi-parent file {}",
                                    entry.path().display()
                                );
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }
        debug!("is_dag: no DAG structure found");
        Ok(false)
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
