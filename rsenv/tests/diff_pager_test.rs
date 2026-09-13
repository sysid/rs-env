//! Tests for the pure decision logic behind `rsenv swap diff` rendering:
//! which pager to use, and what path a patch header carries.

use std::path::Path;

use rsenv::cli::diff_render::patch_label;
use rsenv::cli::pager::resolve_pager;

// ============================================================
// resolve_pager() - what `git var GIT_PAGER` gave us
// ============================================================

#[test]
fn given_configured_pager_when_resolve_then_returns_it() {
    // Arrange - what `git var GIT_PAGER` prints for a delta user
    let raw = Some("delta".to_string());

    // Act
    let pager = resolve_pager(raw);

    // Assert
    assert_eq!(pager.as_deref(), Some("delta"));
}

#[test]
fn given_pager_with_arguments_when_resolve_then_keeps_the_whole_command() {
    // Arrange - git returns a shell command, not a bare binary
    let raw = Some("less -FRX".to_string());

    // Act
    let pager = resolve_pager(raw);

    // Assert - it is handed to `sh -c`, so arguments must survive intact
    assert_eq!(pager.as_deref(), Some("less -FRX"));
}

#[test]
fn given_cat_as_pager_when_resolve_then_no_pager() {
    // Arrange - `cat` is git's idiom for "paging disabled"
    let raw = Some("cat".to_string());

    // Act
    let pager = resolve_pager(raw);

    // Assert
    assert!(pager.is_none(), "cat must disable paging, got {:?}", pager);
}

#[test]
fn given_blank_or_missing_pager_when_resolve_then_no_pager() {
    // Arrange / Act / Assert - git absent, or an empty core.pager
    assert!(resolve_pager(None).is_none());
    assert!(resolve_pager(Some(String::new())).is_none());
    assert!(resolve_pager(Some("   \n".to_string())).is_none());
}

// ============================================================
// patch_label() - paths a diff viewer can actually resolve
// ============================================================

#[test]
fn given_file_inside_swapped_dir_when_label_then_path_is_project_relative() {
    // Arrange - the entry is `thoughts`, the change is relative to that entry
    let entry = Path::new("thoughts");
    let change = Path::new("research/notes.md");

    // Act
    let label = patch_label(entry, change);

    // Assert - delta resolves header paths against the cwd (the project root),
    // so the entry segment MUST be present or the hyperlink points nowhere.
    assert_eq!(label, "thoughts/research/notes.md");
}

#[test]
fn given_flat_file_entry_when_label_then_uses_the_entry_path_alone() {
    // Arrange - a swapped single file has an empty change path
    let entry = Path::new("CLAUDE.md");
    let change = Path::new("");

    // Act
    let label = patch_label(entry, change);

    // Assert
    assert_eq!(label, "CLAUDE.md");
}

#[test]
fn given_nested_entry_when_label_then_both_segments_are_joined() {
    // Arrange
    let entry = Path::new(".claude");
    let change = Path::new("settings.local.json");

    // Act
    let label = patch_label(entry, change);

    // Assert
    assert_eq!(label, ".claude/settings.local.json");
}
