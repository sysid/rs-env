//! Tests for the pre-commit hook script rsenv installs.
//!
//! Unlike tests/hook_test.rs these need no `.git/hooks` directory: they check the
//! script itself, which is what decides what a commit is checked against.

use std::io::Write;
use std::process::{Command, Stdio};

use rsenv::cli::hook::{pre_commit_hook, HookScope};

fn is_valid_bash(script: &str) -> bool {
    let mut child = Command::new("bash")
        .arg("-n")
        .stdin(Stdio::piped())
        .spawn()
        .expect("run bash");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    child.wait().unwrap().success()
}

#[test]
fn given_base_dir_scope_when_rendering_hook_then_checks_all_vaults() {
    // Act
    let script = pre_commit_hook(HookScope::Vaults);

    // Assert
    assert!(script.contains("# rsenv pre-commit hook"));
    assert!(script.contains("rsenv sops status --global --check"));
    assert!(is_valid_bash(&script), "{script}");
}

/// A hook installed in another repo (e.g. a secrets store) must check that repo,
/// not the vaults under the rsenv base dir.
#[test]
fn given_dir_scope_when_rendering_hook_then_checks_the_repo_it_runs_in() {
    // Act
    let script = pre_commit_hook(HookScope::Dir);

    // Assert
    assert!(script.contains("# rsenv pre-commit hook"));
    assert!(
        script.contains(r#"rsenv sops status --dir "$(git rev-parse --show-toplevel)" --check"#),
        "{script}"
    );
    assert!(!script.contains("--global"), "{script}");
    assert!(
        script.contains("rsenv sops encrypt --dir"),
        "remedy names --dir: {script}"
    );
    assert!(is_valid_bash(&script), "{script}");
}
