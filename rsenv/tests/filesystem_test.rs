//! Tests for FileSystem trait unified methods
//!
//! TDD: These tests are written BEFORE implementation.

use rsenv::infrastructure::traits::{FileSystem, RealFileSystem};
use std::fs;
use tempfile::TempDir;

// ============================================================
// copy_any tests
// ============================================================

#[test]
fn given_file_when_copy_any_then_copies_file() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("dest.txt");
    fs::write(&src, "hello world").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.copy_any(&src, &dst).unwrap();

    // Assert
    assert!(dst.exists());
    assert_eq!(fs::read_to_string(&dst).unwrap(), "hello world");
    // Source should still exist (copy, not move)
    assert!(src.exists());
}

#[test]
fn given_directory_when_copy_any_then_copies_recursively() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let src_dir = temp.path().join("source");
    let dst_dir = temp.path().join("dest");

    fs::create_dir_all(src_dir.join("nested")).unwrap();
    fs::write(src_dir.join("file1.txt"), "content1").unwrap();
    fs::write(src_dir.join("nested/file2.txt"), "content2").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.copy_any(&src_dir, &dst_dir).unwrap();

    // Assert
    assert!(dst_dir.exists());
    assert!(dst_dir.join("file1.txt").exists());
    assert!(dst_dir.join("nested/file2.txt").exists());
    assert_eq!(
        fs::read_to_string(dst_dir.join("file1.txt")).unwrap(),
        "content1"
    );
    assert_eq!(
        fs::read_to_string(dst_dir.join("nested/file2.txt")).unwrap(),
        "content2"
    );
    // Source should still exist
    assert!(src_dir.exists());
}

#[test]
fn given_nonexistent_source_when_copy_any_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("nonexistent");
    let dst = temp.path().join("dest");

    let fs = RealFileSystem;

    // Act
    let result = fs.copy_any(&src, &dst);

    // Assert
    assert!(result.is_err());
}

#[test]
fn given_directory_with_broken_symlink_when_copy_any_then_preserves_symlink() {
    // Regression: copy_dir used to call std::fs::copy on symlinks, which follows
    // the link and fails when the target is unreachable in the copy's context.
    // The vault swap-in path hits exactly this: a relative symlink whose target
    // only resolves at the project location, not in the vault.
    let temp = TempDir::new().unwrap();
    let src_dir = temp.path().join("source");
    let dst_dir = temp.path().join("dest");
    fs::create_dir_all(&src_dir).unwrap();

    let link = src_dir.join("skills");
    let target = std::path::PathBuf::from("../.agents/skills"); // unresolvable from src_dir
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&target, &link).unwrap();

    let fs = RealFileSystem;

    fs.copy_any(&src_dir, &dst_dir).unwrap();

    let copied = dst_dir.join("skills");
    let meta = std::fs::symlink_metadata(&copied).unwrap();
    assert!(meta.file_type().is_symlink(), "entry should remain a symlink");
    assert_eq!(std::fs::read_link(&copied).unwrap(), target);
}

#[test]
fn given_directory_with_symlink_to_dir_when_copy_any_then_does_not_recurse() {
    // A symlink to a real directory must be copied as a symlink, not followed
    // and recursed into (which would duplicate content and could loop).
    let temp = TempDir::new().unwrap();
    let real_outside = temp.path().join("outside");
    fs::create_dir_all(&real_outside).unwrap();
    fs::write(real_outside.join("payload.txt"), "should-not-be-copied").unwrap();

    let src_dir = temp.path().join("source");
    let dst_dir = temp.path().join("dest");
    fs::create_dir_all(&src_dir).unwrap();

    let link = src_dir.join("link_to_outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_outside, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real_outside, &link).unwrap();

    let fs = RealFileSystem;

    fs.copy_any(&src_dir, &dst_dir).unwrap();

    let copied = dst_dir.join("link_to_outside");
    let meta = std::fs::symlink_metadata(&copied).unwrap();
    assert!(meta.file_type().is_symlink());
    assert!(
        !dst_dir.join("link_to_outside/payload.txt").exists()
            || std::fs::symlink_metadata(dst_dir.join("link_to_outside"))
                .unwrap()
                .file_type()
                .is_symlink(),
        "must not have materialised contents under the symlink as real files"
    );
}

// ============================================================
// remove_any tests
// ============================================================

#[test]
fn given_file_when_remove_any_then_removes_file() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("file.txt");
    fs::write(&file, "content").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.remove_any(&file).unwrap();

    // Assert
    assert!(!file.exists());
}

#[test]
fn given_directory_when_remove_any_then_removes_recursively() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("dir");
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("file.txt"), "content").unwrap();
    fs::write(dir.join("nested/deep.txt"), "deep").unwrap();

    let fs = RealFileSystem;

    // Act
    fs.remove_any(&dir).unwrap();

    // Assert
    assert!(!dir.exists());
}

#[test]
fn given_nonexistent_path_when_remove_any_then_returns_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("nonexistent");

    let fs = RealFileSystem;

    // Act
    let result = fs.remove_any(&path);

    // Assert
    assert!(result.is_err());
}

// ============================================================
// ensure_parent tests
// ============================================================

#[test]
fn given_nested_path_when_ensure_parent_then_creates_ancestors() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let nested = temp.path().join("a/b/c/file.txt");

    let fs = RealFileSystem;

    // Act
    fs.ensure_parent(&nested).unwrap();

    // Assert
    let parent = nested.parent().unwrap();
    assert!(parent.exists());
    assert!(parent.is_dir());
    // The file itself should NOT be created
    assert!(!nested.exists());
}

#[test]
fn given_existing_parent_when_ensure_parent_then_succeeds() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let file = temp.path().join("file.txt");
    // Parent already exists (temp.path())

    let fs = RealFileSystem;

    // Act
    let result = fs.ensure_parent(&file);

    // Assert
    assert!(result.is_ok());
}

#[test]
fn given_root_path_when_ensure_parent_then_succeeds() {
    // Arrange
    let fs = RealFileSystem;

    // Act - path with no parent (or empty parent)
    let result = fs.ensure_parent(std::path::Path::new("file.txt"));

    // Assert - should succeed (no-op)
    assert!(result.is_ok());
}

#[test]
fn given_path_with_no_parent_when_ensure_parent_then_succeeds() {
    // Arrange
    let fs = RealFileSystem;

    // Act - single component path
    let result = fs.ensure_parent(std::path::Path::new(""));

    // Assert - should succeed (no-op for empty path)
    assert!(result.is_ok());
}
