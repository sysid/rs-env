//! Tests for dotfile module - unified dot-file handling

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use rsenv::application::dotfile::{
    is_dotfile, neutralize_name, neutralize_path, restore_name, restore_path,
};

// =============================================================================
// is_dotfile tests
// =============================================================================

#[test]
fn given_dotfile_name_when_is_dotfile_then_returns_true() {
    assert!(is_dotfile(OsStr::new(".gitignore")));
    assert!(is_dotfile(OsStr::new(".envrc")));
    assert!(is_dotfile(OsStr::new(".hidden")));
    assert!(is_dotfile(OsStr::new(".eslintrc")));
}

#[test]
fn given_regular_name_when_is_dotfile_then_returns_false() {
    assert!(!is_dotfile(OsStr::new("config.yml")));
    assert!(!is_dotfile(OsStr::new("README.md")));
    assert!(!is_dotfile(OsStr::new("dot.gitignore"))); // already neutralized
}

#[test]
fn given_single_dot_when_is_dotfile_then_returns_false() {
    // "." is current directory, not a dotfile
    assert!(!is_dotfile(OsStr::new(".")));
}

#[test]
fn given_double_dot_when_is_dotfile_then_returns_false() {
    // ".." is parent directory, not a dotfile
    assert!(!is_dotfile(OsStr::new("..")));
}

// =============================================================================
// neutralize_name tests
// =============================================================================

#[test]
fn given_dotfile_name_when_neutralize_then_adds_dot_prefix() {
    assert_eq!(neutralize_name(".gitignore"), "dot.gitignore");
    assert_eq!(neutralize_name(".envrc"), "dot.envrc");
    assert_eq!(neutralize_name(".hidden"), "dot.hidden");
    assert_eq!(neutralize_name(".eslintrc.json"), "dot.eslintrc.json");
}

#[test]
fn given_regular_name_when_neutralize_then_unchanged() {
    assert_eq!(neutralize_name("config.yml"), "config.yml");
    assert_eq!(neutralize_name("README.md"), "README.md");
}

#[test]
fn given_already_neutralized_when_neutralize_then_unchanged() {
    // dot.gitignore should not become dot.dot.gitignore
    assert_eq!(neutralize_name("dot.gitignore"), "dot.gitignore");
}

#[test]
fn given_single_dot_when_neutralize_then_unchanged() {
    assert_eq!(neutralize_name("."), ".");
}

#[test]
fn given_double_dot_when_neutralize_then_unchanged() {
    assert_eq!(neutralize_name(".."), "..");
}

// =============================================================================
// restore_name tests
// =============================================================================

#[test]
fn given_neutralized_name_when_restore_then_restores_dot() {
    assert_eq!(restore_name("dot.gitignore"), ".gitignore");
    assert_eq!(restore_name("dot.envrc"), ".envrc");
    assert_eq!(restore_name("dot.hidden"), ".hidden");
    assert_eq!(restore_name("dot.eslintrc.json"), ".eslintrc.json");
}

#[test]
fn given_regular_name_when_restore_then_unchanged() {
    assert_eq!(restore_name("config.yml"), "config.yml");
    assert_eq!(restore_name("README.md"), "README.md");
}

#[test]
fn given_dotfile_name_when_restore_then_unchanged() {
    // .gitignore should not become anything else
    assert_eq!(restore_name(".gitignore"), ".gitignore");
}

#[test]
fn given_just_dot_prefix_when_restore_then_unchanged() {
    // "dot." alone (with nothing after) should stay as is
    assert_eq!(restore_name("dot."), "dot.");
}

// =============================================================================
// neutralize_path tests
// =============================================================================

#[test]
fn given_path_with_dotfile_when_neutralize_path_then_transforms_filename() {
    let path = Path::new("config/.gitignore");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("config/dot.gitignore"));
}

#[test]
fn given_path_with_dot_directory_when_neutralize_path_then_transforms_dirname() {
    let path = Path::new(".hidden/file.txt");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("dot.hidden/file.txt"));
}

#[test]
fn given_path_with_multiple_dot_components_when_neutralize_path_then_transforms_all() {
    let path = Path::new(".hidden/.secret/file.txt");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("dot.hidden/dot.secret/file.txt"));
}

#[test]
fn given_path_with_mixed_components_when_neutralize_path_then_transforms_only_dotfiles() {
    let path = Path::new("src/.config/settings.json");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("src/dot.config/settings.json"));
}

#[test]
fn given_standalone_dotfile_when_neutralize_path_then_transforms() {
    let path = Path::new(".gitignore");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("dot.gitignore"));
}

#[test]
fn given_regular_path_when_neutralize_path_then_unchanged() {
    let path = Path::new("src/config/settings.json");
    let result = neutralize_path(path);
    assert_eq!(result, PathBuf::from("src/config/settings.json"));
}

// =============================================================================
// restore_path tests
// =============================================================================

#[test]
fn given_path_with_neutralized_file_when_restore_path_then_restores() {
    let path = Path::new("config/dot.gitignore");
    let result = restore_path(path);
    assert_eq!(result, PathBuf::from("config/.gitignore"));
}

#[test]
fn given_path_with_neutralized_directory_when_restore_path_then_restores() {
    let path = Path::new("dot.hidden/file.txt");
    let result = restore_path(path);
    assert_eq!(result, PathBuf::from(".hidden/file.txt"));
}

#[test]
fn given_path_with_multiple_neutralized_when_restore_path_then_restores_all() {
    let path = Path::new("dot.hidden/dot.secret/file.txt");
    let result = restore_path(path);
    assert_eq!(result, PathBuf::from(".hidden/.secret/file.txt"));
}

#[test]
fn given_regular_path_when_restore_path_then_unchanged() {
    let path = Path::new("src/config/settings.json");
    let result = restore_path(path);
    assert_eq!(result, PathBuf::from("src/config/settings.json"));
}

// =============================================================================
// Round-trip tests
// =============================================================================

#[test]
fn given_dotfile_path_when_neutralize_then_restore_then_original() {
    let original = Path::new(".hidden/.secret/.gitignore");
    let neutralized = neutralize_path(original);
    let restored = restore_path(&neutralized);
    assert_eq!(restored, original);
}

#[test]
fn given_mixed_path_when_neutralize_then_restore_then_original() {
    let original = Path::new("src/.config/db/.credentials");
    let neutralized = neutralize_path(original);
    let restored = restore_path(&neutralized);
    assert_eq!(restored, original);
}
