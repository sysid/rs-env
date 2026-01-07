//! I/O boundary traits for testability
//!
//! These traits abstract external I/O operations, allowing services
//! to be tested with mock implementations.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Filesystem abstraction for testability.
pub trait FileSystem: Send + Sync {
    /// Read file contents to string.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;

    /// Write string content to file.
    fn write(&self, path: &Path, content: &str) -> io::Result<()>;

    /// Check if path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Check if path is a file.
    fn is_file(&self, path: &Path) -> bool;

    /// Check if path is a directory.
    fn is_dir(&self, path: &Path) -> bool;

    /// Create directory and all parent directories.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Rename/move a file.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Remove a file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Remove a directory and all its contents.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Create a symbolic link.
    fn symlink(&self, original: &Path, link: &Path) -> io::Result<()>;

    /// Create symlink with relative path (from link's parent to original).
    fn symlink_relative(&self, original: &Path, link: &Path) -> io::Result<()>;

    /// Read the target of a symbolic link.
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;

    /// Check if path is a symbolic link.
    fn is_symlink(&self, path: &Path) -> bool;

    /// Canonicalize path (resolve symlinks, make absolute).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// Copy file from source to destination.
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64>;

    /// Copy directory recursively from source to destination.
    fn copy_dir(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Copy file or directory (auto-detect).
    fn copy_any(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Remove file or directory (auto-detect).
    fn remove_any(&self, path: &Path) -> io::Result<()>;

    /// Create parent directories if needed.
    fn ensure_parent(&self, path: &Path) -> io::Result<()>;

    /// Move file or directory, with fallback for cross-device moves.
    ///
    /// Tries atomic rename first. If that fails with EXDEV (cross-device link),
    /// falls back to copy + delete. This matches rplc's `_move_path` behavior.
    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()>;
}

/// External command runner abstraction.
pub trait CommandRunner: Send + Sync {
    /// Run a command with arguments.
    fn run(&self, cmd: &str, args: &[&str]) -> io::Result<Output>;

    /// Run a command with arguments and capture combined output.
    fn run_with_stdin(&self, cmd: &str, args: &[&str], stdin: &str) -> io::Result<Output>;
}

/// Item for FZF-style selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionItem {
    /// Display text shown in selector
    pub display: String,
    /// Actual value (e.g., file path)
    pub value: String,
}

/// Interactive FZF-style selector abstraction.
pub trait Selector: Send + Sync {
    /// Present items to user and return selected one.
    /// Returns None if user cancels (Esc/Ctrl-C).
    fn select_one(
        &self,
        items: &[SelectionItem],
        prompt: &str,
    ) -> Result<Option<SelectionItem>, String>;
}

/// Editor abstraction for opening files.
pub trait Editor: Send + Sync {
    /// Open a file in the editor.
    /// Blocks until editor exits.
    fn open(&self, path: &Path) -> io::Result<()>;
}

// ============================================================
// REAL IMPLEMENTATIONS
// ============================================================

/// Real filesystem implementation.
#[derive(Debug, Default)]
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        std::fs::write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn symlink(&self, original: &Path, link: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(original, link)
        }
        #[cfg(windows)]
        {
            if original.is_dir() {
                std::os::windows::fs::symlink_dir(original, link)
            } else {
                std::os::windows::fs::symlink_file(original, link)
            }
        }
    }

    fn symlink_relative(&self, original: &Path, link: &Path) -> io::Result<()> {
        let link_parent = link.parent().unwrap_or(Path::new("."));
        let target = pathdiff::diff_paths(original, link_parent).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "cannot compute relative path from {} to {}",
                    link_parent.display(),
                    original.display()
                ),
            )
        })?;
        self.symlink(&target, link)
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(path)
    }

    fn is_symlink(&self, path: &Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        std::fs::copy(from, to)
    }

    fn copy_dir(&self, from: &Path, to: &Path) -> io::Result<()> {
        use walkdir::WalkDir;

        std::fs::create_dir_all(to)?;
        for entry in WalkDir::new(from).into_iter().filter_map(|e| e.ok()) {
            let rel_path = entry
                .path()
                .strip_prefix(from)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
            let target = to.join(rel_path);

            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target)?;
            } else {
                std::fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    }

    fn copy_any(&self, from: &Path, to: &Path) -> io::Result<()> {
        if from.is_dir() {
            self.copy_dir(from, to)
        } else {
            self.copy(from, to).map(|_| ())
        }
    }

    fn remove_any(&self, path: &Path) -> io::Result<()> {
        if path.is_dir() {
            self.remove_dir_all(path)
        } else {
            self.remove_file(path)
        }
    }

    fn ensure_parent(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                self.create_dir_all(parent)?;
            }
        }
        Ok(())
    }

    fn move_path(&self, from: &Path, to: &Path) -> io::Result<()> {
        match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(e) => {
                // EXDEV = 18 on Unix (cross-device link not permitted)
                #[cfg(unix)]
                const EXDEV: i32 = 18;
                #[cfg(windows)]
                const EXDEV: i32 = 17; // ERROR_NOT_SAME_DEVICE

                if e.raw_os_error() == Some(EXDEV) {
                    // Fallback: copy then delete (not atomic, but works cross-device)
                    self.copy_any(from, to)?;
                    self.remove_any(from)?;
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}

/// Real command runner implementation.
#[derive(Debug, Default)]
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, cmd: &str, args: &[&str]) -> io::Result<Output> {
        std::process::Command::new(cmd).args(args).output()
    }

    fn run_with_stdin(&self, cmd: &str, args: &[&str], stdin: &str) -> io::Result<Output> {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = std::process::Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(stdin.as_bytes())?;
        }

        child.wait_with_output()
    }
}

/// Real selector implementation using skim (FZF-like).
#[derive(Debug, Default)]
pub struct SkimSelector;

/// Real editor implementation using $EDITOR, $VISUAL, or vim.
#[derive(Debug, Default)]
pub struct EnvironmentEditor;

impl Selector for SkimSelector {
    fn select_one(
        &self,
        items: &[SelectionItem],
        prompt: &str,
    ) -> Result<Option<SelectionItem>, String> {
        use skim::prelude::*;
        use std::io::Cursor;

        if items.is_empty() {
            return Ok(None);
        }

        // Build input as newline-separated display strings
        let input = items
            .iter()
            .map(|i| i.display.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let options = SkimOptionsBuilder::default()
            .prompt(Some(prompt))
            .height(Some("50%"))
            .multi(false)
            .build()
            .map_err(|e| format!("failed to build skim options: {e}"))?;

        let item_reader = SkimItemReader::default();
        let items_arc = item_reader.of_bufread(Cursor::new(input));

        let output = Skim::run_with(&options, Some(items_arc));

        match output {
            Some(out) if out.is_abort => Ok(None),
            Some(out) => {
                if let Some(selected) = out.selected_items.first() {
                    let display = selected.output().to_string();
                    // Find the matching item
                    let item = items.iter().find(|i| i.display == display).cloned();
                    Ok(item)
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
}

impl Editor for EnvironmentEditor {
    fn open(&self, path: &Path) -> io::Result<()> {
        use std::process::Command;

        // Determine editor: $VISUAL > $EDITOR > vim
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vim".to_string());

        let status = Command::new(&editor).arg(path).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("editor exited with status: {}", status),
            ))
        }
    }
}
