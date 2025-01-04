use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use regex::Regex;
use tracing::instrument;
use walkdir::WalkDir;

use crate::errors::{TreeError, TreeResult};
use crate::arena::{TreeArena, NodeData};
use crate::util::path::PathExt;

pub struct TreeBuilder {
    relationship_cache: HashMap<PathBuf, Vec<PathBuf>>,
    visited_paths: HashSet<PathBuf>,
    parent_regex: Regex,
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
            parent_regex: Regex::new(r"# rsenv: (.+)").unwrap(),
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
                reason: "Not a directory".to_string()
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

            if entry.file_type().is_file() {
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
        let current_dir = abs_path.parent()
            .ok_or_else(|| TreeError::InvalidParent(path.to_path_buf()))?;

        for line in reader.lines() {
            let line = line.map_err(TreeError::FileReadError)?;
            if let Some(caps) = self.parent_regex.captures(&line) {
                let parent_relative = caps.get(1).unwrap().as_str();
                let parent_path = current_dir.join(parent_relative);
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
        self.relationship_cache
            .keys()
            .filter(|path| !self.relationship_cache.values().any(|v| v.contains(path)))
            .cloned()
            .collect()
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
                base_path: current_path.parent()
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