use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tracing::instrument;
use walkdir::WalkDir;

use crate::arena::{NodeData, TreeArena};
use crate::errors::{TreeError, TreeResult};
use crate::util::path::PathExt;

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

    #[instrument(level = "debug", skip(self))]
    pub fn build_from_directory(&mut self, directory_path: &Path) -> TreeResult<Vec<TreeArena>> {
        if !directory_path.exists() {
            return Err(TreeError::FileNotFound(directory_path.to_path_buf()));
        }
        if !directory_path.is_dir() {
            return Err(TreeError::InvalidFormat {
                path: directory_path.to_path_buf(),
                reason: "Not a directory".to_string(),
            });
        }

        // Scan directory and build relationship cache
        self.scan_directory(directory_path)?;

        // Find root nodes
        let root_files = self.find_root_nodes();

        // Build trees
        let mut trees = Vec::new();
        for root in root_files {
            let tree = self.build_tree(&root)?;
            trees.push(tree);
        }

        Ok(trees)
    }

    #[instrument(level = "debug", skip(self))]
    fn scan_directory(&mut self, directory_path: &Path) -> TreeResult<()> {
        for entry in WalkDir::new(directory_path) {
            let entry = entry.map_err(|e| TreeError::PathResolution {
                path: directory_path.to_path_buf(),
                reason: e.to_string(),
            })?;

            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "env")
            {
                let abs_path = entry.path().to_canonical()?;
                self.all_files.insert(abs_path.clone());
                self.process_file(entry.path())?;
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    fn process_file(&mut self, path: &Path) -> TreeResult<()> {
        let file = File::open(path).map_err(TreeError::FileReadError)?;
        let reader = BufReader::new(file);
        let abs_path = path.to_canonical()?;
        let current_dir = abs_path
            .parent()
            .ok_or_else(|| TreeError::InvalidParent(path.to_path_buf()))?;

        for line in reader.lines() {
            let line = line.map_err(TreeError::FileReadError)?;
            if let Some(caps) = self.parent_regex.captures(&line) {
                let parent_relative = caps.get(1).unwrap().as_str();
                let expanded_path = crate::expand_env_vars(parent_relative);
                let parent_path = current_dir.join(expanded_path);
                let parent_canonical = parent_path.to_canonical()?;

                self.relationship_cache
                    .entry(parent_canonical)
                    .or_default()
                    .push(abs_path.clone());
            }
        }
        Ok(())
    }

    #[instrument(level = "debug", skip(self))]
    fn find_root_nodes(&self) -> Vec<PathBuf> {
        let mut root_nodes = Vec::new();

        // Find files that are parents but not children (traditional root nodes)
        for path in self.relationship_cache.keys() {
            if !self.relationship_cache.values().any(|v| v.contains(path)) {
                root_nodes.push(path.clone());
            }
        }

        // Find standalone files (files not in any relationship)
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

    #[instrument(level = "debug", skip(self))]
    fn build_tree(&mut self, root_path: &Path) -> TreeResult<TreeArena> {
        let mut tree = TreeArena::new();
        let mut stack = vec![(root_path.to_path_buf(), None)];
        self.visited_paths.clear();

        while let Some((current_path, parent_idx)) = stack.pop() {
            // Check for cycles
            if !self.visited_paths.insert(current_path.clone()) {
                return Err(TreeError::CycleDetected(current_path));
            }

            let node_data = NodeData {
                base_path: current_path
                    .parent()
                    .ok_or_else(|| TreeError::InvalidParent(current_path.clone()))?
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
