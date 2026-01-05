//! Tests for the Editor trait and edit workflow

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rsenv::infrastructure::traits::Editor;

/// Mock editor that records what file was opened
struct MockEditor {
    opened_files: Mutex<Vec<PathBuf>>,
    should_fail: bool,
}

impl MockEditor {
    fn new() -> Self {
        Self {
            opened_files: Mutex::new(Vec::new()),
            should_fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            opened_files: Mutex::new(Vec::new()),
            should_fail: true,
        }
    }

    fn opened_files(&self) -> Vec<PathBuf> {
        self.opened_files.lock().unwrap().clone()
    }
}

impl Editor for MockEditor {
    fn open(&self, path: &Path) -> io::Result<()> {
        if self.should_fail {
            return Err(io::Error::new(io::ErrorKind::Other, "editor failed"));
        }
        self.opened_files.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

#[test]
fn given_file_path_when_opening_in_editor_then_calls_editor() {
    // Arrange
    let editor = MockEditor::new();
    let file = PathBuf::from("/tmp/test.env");

    // Act
    editor.open(&file).unwrap();

    // Assert
    let opened = editor.opened_files();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0], file);
}

#[test]
fn given_editor_fails_when_opening_then_returns_error() {
    // Arrange
    let editor = MockEditor::failing();
    let file = PathBuf::from("/tmp/test.env");

    // Act
    let result = editor.open(&file);

    // Assert
    assert!(result.is_err());
}
