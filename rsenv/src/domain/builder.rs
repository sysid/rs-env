//! Tree builder for scanning directories and building environment hierarchies.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

use crate::domain::arena::{NodeData, TreeArena};
use crate::domain::error::DomainError;
use crate::domain::expand_env_vars;

/// Result type for tree operations.
pub type TreeResult<T> = Result<T, DomainError>;

/// Constructs hierarchical trees from environment files.
pub struct TreeBuilder {
    relationship_cache: HashMap<PathBuf, Vec<PathBuf>>,
    visited_paths: HashSet<PathBuf>,
    parent_regex: Regex,
    all_files: HashSet<PathBuf>,
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self {
            relationship_cache: HashMap::new(),
            visited_paths: HashSet::new(),
            parent_regex: Regex::new(r"# rsenv:\s*(.+)").unwrap(),
            all_files: HashSet::new(),
        }
    }

    /// Build trees from all .env files in directory.
    pub fn build_from_directory(&mut self, directory_path: &Path) -> TreeResult<Vec<TreeArena>> {
        if !directory_path.exists() {
            return Err(DomainError::FileNotFound(directory_path.to_path_buf()));
        }
        if !directory_path.is_dir() {
            return Err(DomainError::InvalidEnvFile {
                path: directory_path.to_path_buf(),
                message: "Not a directory".to_string(),
            });
        }

        // Reset state for fresh scan
        self.relationship_cache.clear();
        self.visited_paths.clear();
        self.all_files.clear();

        // Scan directory and build relationship cache
        self.scan_directory(directory_path)?;

        // Find root nodes
        let root_files = self.find_root_nodes();

        // Cycle detection: if we have relationships but no root nodes, there's a cycle
        if root_files.is_empty() && !self.relationship_cache.is_empty() {
            // Find one file involved in the cycle for the error message
            let cycle_file = self.relationship_cache.keys().next().unwrap().clone();
            return Err(DomainError::CycleDetected(cycle_file));
        }

        // Build trees
        let mut trees = Vec::new();
        for root in root_files {
            self.visited_paths.clear(); // Reset for each tree
            let tree = self.build_tree(&root)?;
            trees.push(tree);
        }

        Ok(trees)
    }

    fn scan_directory(&mut self, directory_path: &Path) -> TreeResult<()> {
        for entry in WalkDir::new(directory_path) {
            let entry = entry.map_err(|e| DomainError::InvalidEnvFile {
                path: directory_path.to_path_buf(),
                message: e.to_string(),
            })?;

            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "env" {
                        let abs_path = entry
                            .path()
                            .canonicalize()
                            .map_err(|_| DomainError::InvalidParent(entry.path().to_path_buf()))?;
                        self.all_files.insert(abs_path.clone());
                        self.process_file(entry.path())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn process_file(&mut self, path: &Path) -> TreeResult<()> {
        let file = File::open(path).map_err(|_| DomainError::FileNotFound(path.to_path_buf()))?;
        let reader = BufReader::new(file);
        let abs_path = path
            .canonicalize()
            .map_err(|_| DomainError::InvalidParent(path.to_path_buf()))?;
        let current_dir = abs_path
            .parent()
            .ok_or_else(|| DomainError::InvalidParent(path.to_path_buf()))?;

        for line in reader.lines() {
            let line = line.map_err(|_| DomainError::FileNotFound(path.to_path_buf()))?;
            if let Some(caps) = self.parent_regex.captures(&line) {
                // v1: space-separated parents
                for parent_str in caps.get(1).unwrap().as_str().split_whitespace() {
                    let expanded_path = expand_env_vars(parent_str);
                    let parent_path = current_dir.join(&expanded_path);
                    if let Ok(parent_canonical) = parent_path.canonicalize() {
                        self.relationship_cache
                            .entry(parent_canonical)
                            .or_default()
                            .push(abs_path.clone());
                    }
                }
            }
        }
        Ok(())
    }

    fn find_root_nodes(&self) -> Vec<PathBuf> {
        let mut root_nodes = Vec::new();

        // Files that are parents but not children
        for path in self.relationship_cache.keys() {
            if !self.relationship_cache.values().any(|v| v.contains(path)) {
                root_nodes.push(path.clone());
            }
        }

        // Standalone files (not in any relationship)
        for file_path in &self.all_files {
            let is_parent = self.relationship_cache.contains_key(file_path);
            let is_child = self
                .relationship_cache
                .values()
                .any(|v| v.contains(file_path));

            if !is_parent && !is_child {
                root_nodes.push(file_path.clone());
            }
        }

        root_nodes
    }

    fn build_tree(&mut self, root_path: &Path) -> TreeResult<TreeArena> {
        let mut tree = TreeArena::new();
        let mut stack = vec![(root_path.to_path_buf(), None)];

        while let Some((current_path, parent_idx)) = stack.pop() {
            // Cycle detection
            if !self.visited_paths.insert(current_path.clone()) {
                return Err(DomainError::CycleDetected(current_path));
            }

            let node_data = NodeData {
                base_path: current_path
                    .parent()
                    .ok_or_else(|| DomainError::InvalidParent(current_path.clone()))?
                    .to_path_buf(),
                file_path: current_path.clone(),
            };

            let current_idx = tree.insert_node(node_data, parent_idx);

            // Add children to stack
            if let Some(children) = self.relationship_cache.get(&current_path) {
                for child in children {
                    stack.push((child.clone(), Some(current_idx)));
                }
            }
        }

        Ok(tree)
    }
}
