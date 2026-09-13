//! Tests for VaultCommitService (`rsenv swap commit`)
//!
//! `swap commit` swaps this project's data out of the project and into its vault,
//! then makes ONE commit scoped to that project's vault directory, whose message
//! carries the project's HEAD commit hash as the join key back to the project.
//!
//! These tests drive REAL git in tempdirs: there is no MockCommandRunner in this
//! repo (see TODO.md), and real git is the authority on git behaviour anyway.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::{CommitOptions, SwapService, VaultCommitService, VaultService};
use rsenv::application::ApplicationError;
use rsenv::config::Settings;
use rsenv::infrastructure::traits::{RealCommandRunner, RealFileSystem};

// ============================================================
// Fixtures
// ============================================================

/// Layout mirrors production: a git repo at the vault base (`~/.rsenv`)
/// containing `vaults/<id>/`, and a separate git repo for the project.
struct Fixture {
    _temp: TempDir,
    vault_base: PathBuf,
    project_dir: PathBuf,
    vault_path: PathBuf,
    service: VaultCommitService,
    swap: Arc<SwapService>,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {:?}: {}", args, e));
    assert!(
        out.status.success(),
        "git {:?} in {} failed: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_init(dir: &Path) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "-b", "main"])
        .output()
        .unwrap();
    assert!(out.status.success(), "git init failed in {}", dir.display());
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test User"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn test_settings(base_dir: PathBuf) -> Settings {
    Settings {
        base_dir,
        editor: "vim".to_string(),
        sops: Default::default(),
    }
}

/// Build a project + vault, both under git, with the vault base repo holding
/// one empty initial commit so HEAD exists.
fn setup(project_is_git_repo: bool) -> Fixture {
    let temp = TempDir::new().unwrap();
    let vault_base = temp.path().join("rsenv");
    let project_dir = temp.path().join("proj");
    std::fs::create_dir_all(&vault_base).unwrap();
    std::fs::create_dir_all(&project_dir).unwrap();

    if project_is_git_repo {
        git_init(&project_dir);
        // rsenv drops a .envrc symlink into the project; keep the project tree clean
        // so "dirty" in a commit message means real project changes.
        std::fs::write(project_dir.join(".gitignore"), ".envrc\nthoughts/\n").unwrap();
        git(&project_dir, &["add", ".gitignore"]);
        git(&project_dir, &["commit", "-m", "initial"]);
    }

    let settings = Arc::new(test_settings(vault_base.clone()));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let vault = vault_service.init(&project_dir, false).unwrap();

    git_init(&vault_base);
    // Mirrors the real ~/.rsenv/.gitignore: encrypted files are committable,
    // plaintext env files are not.
    std::fs::write(vault_base.join(".gitignore"), "!*.enc\n*.env\n*.envrc\n").unwrap();
    git(&vault_base, &["add", ".gitignore"]);
    git(&vault_base, &["commit", "-m", "init vault"]);

    let swap = Arc::new(SwapService::new(
        fs,
        vault_service.clone(),
        settings.clone(),
    ));
    let service = VaultCommitService::new(swap.clone(), vault_service, Arc::new(RealCommandRunner));

    Fixture {
        _temp: temp,
        vault_base,
        project_dir,
        vault_path: vault.path,
        service,
        swap,
    }
}

/// Put `thoughts/` under swap management and swap it in, then edit it in the project.
/// This is the real-world shape: live work sits in the project, the vault holds a
/// frozen sentinel.
fn swap_in_thoughts_with_edit(fx: &Fixture) {
    let thoughts = fx.project_dir.join("thoughts");
    std::fs::create_dir_all(&thoughts).unwrap();
    std::fs::write(thoughts.join("notes.md"), "original\n").unwrap();

    fx.swap
        .swap_init(&fx.project_dir, std::slice::from_ref(&thoughts))
        .unwrap();
    fx.swap
        .swap_in(&fx.project_dir, std::slice::from_ref(&thoughts))
        .unwrap();

    // Live edit that exists ONLY in the project directory until swap-out.
    std::fs::write(thoughts.join("notes.md"), "edited in project\n").unwrap();
}

fn auto() -> CommitOptions {
    CommitOptions {
        auto: true,
        push: false,
    }
}

fn head_message(repo: &Path) -> String {
    git(repo, &["log", "-1", "--format=%B"])
}

fn commit_count(repo: &Path) -> usize {
    git(repo, &["rev-list", "--count", "HEAD"]).parse().unwrap()
}

// ============================================================
// commit() — the happy path
// ============================================================

#[test]
fn given_swapped_in_changes_when_commit_auto_then_vault_repo_gains_one_commit() {
    // Arrange
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    let before = commit_count(&fx.vault_base);

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    assert_eq!(commit_count(&fx.vault_base), before + 1);
    assert!(outcome.commit.is_some(), "expected a commit sha");

    let files = git(
        &fx.vault_base,
        &["show", "--name-only", "--format=", "HEAD"],
    );
    assert!(!files.is_empty(), "commit touched no files");
    let vault_id = fx.vault_path.file_name().unwrap().to_str().unwrap();
    for line in files.lines() {
        assert!(
            line.starts_with(&format!("vaults/{}/", vault_id)),
            "commit escaped this project's vault: {}",
            line
        );
    }
}

#[test]
fn given_swapped_in_changes_when_commit_auto_then_live_edit_reaches_the_vault() {
    // Arrange — the whole point: live work is in NO repo until swap-out.
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);

    // Act
    fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    let vault_id = fx.vault_path.file_name().unwrap().to_str().unwrap();
    let spec = format!("HEAD:vaults/{}/swap/thoughts/notes.md", vault_id);
    let committed = git(&fx.vault_base, &["show", &spec]);
    assert_eq!(committed, "edited in project");
}

#[test]
fn given_clean_project_when_commit_auto_then_message_carries_project_head_hash() {
    // Arrange
    let fx = setup(true);
    let project_head = git(&fx.project_dir, &["rev-parse", "HEAD"]);
    swap_in_thoughts_with_edit(&fx);

    // Act
    fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    let msg = head_message(&fx.vault_base);
    assert!(
        msg.contains(&project_head),
        "project HEAD {} missing from vault commit message:\n{}",
        project_head,
        msg
    );
    assert!(msg.contains("(main)"), "branch missing:\n{}", msg);
    assert!(
        !msg.contains("dirty"),
        "clean project marked dirty:\n{}",
        msg
    );
}

#[test]
fn given_dirty_project_when_commit_auto_then_message_marks_it_dirty() {
    // Arrange
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    std::fs::write(
        fx.project_dir.join(".gitignore"),
        ".envrc\nthoughts/\nextra\n",
    )
    .unwrap();

    // Act
    fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    let msg = head_message(&fx.vault_base);
    assert!(
        msg.contains("dirty"),
        "dirty project not marked in message:\n{}",
        msg
    );
}

#[test]
fn given_project_without_git_when_commit_auto_then_commits_and_says_so() {
    // Arrange
    let fx = setup(false);
    swap_in_thoughts_with_edit(&fx);

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    assert!(
        outcome.commit.is_some(),
        "should still commit without project git"
    );
    let msg = head_message(&fx.vault_base);
    assert!(
        msg.contains("not a git repository"),
        "absence of project repo not stated:\n{}",
        msg
    );
}

#[test]
fn given_swapped_in_entries_when_commit_then_they_end_up_swapped_out() {
    // Arrange
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert — Tom's chosen post-condition: stay swapped out.
    assert_eq!(outcome.swapped_out.len(), 1);
    let status = fx.swap.status(&fx.project_dir).unwrap();
    assert!(
        status
            .iter()
            .all(|s| matches!(s.state, rsenv::domain::SwapState::Out)),
        "something is still swapped in: {:?}",
        status
    );
    assert!(
        fx.vault_path.join("swap/thoughts/notes.md").exists(),
        "vault override not restored"
    );
}

#[test]
fn given_nothing_changed_when_commit_auto_then_no_commit_is_created() {
    // Arrange — a freshly initialised vault holds nothing committable
    // (empty dirs, and dot.envrc is gitignored). Nothing swapped in, no edits.
    let fx = setup(true);
    let before = commit_count(&fx.vault_base);

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    assert!(outcome.commit.is_none(), "committed with nothing to commit");
    assert_eq!(commit_count(&fx.vault_base), before);
}

// ============================================================
// The core promise: one commit, one project
// ============================================================

#[test]
fn given_another_dirty_vault_when_commit_then_that_vault_stays_uncommitted() {
    // Arrange — a second project's vault with pending changes, as on a real machine.
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);

    let other_vault = fx.vault_base.join("vaults/other-deadbeef");
    std::fs::create_dir_all(other_vault.join("swap")).unwrap();
    std::fs::write(other_vault.join("swap/OTHER.md"), "do not commit me\n").unwrap();

    // Act
    fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    let files = git(
        &fx.vault_base,
        &["show", "--name-only", "--format=", "HEAD"],
    );
    assert!(
        !files.contains("other-deadbeef"),
        "swept in an unrelated vault:\n{}",
        files
    );
    let untracked = git(&fx.vault_base, &["status", "--porcelain"]);
    assert!(
        untracked.contains("other-deadbeef"),
        "other vault should still be pending:\n{}",
        untracked
    );
}

// ============================================================
// Safety gate
// ============================================================

#[test]
fn given_plaintext_in_guarded_when_commit_then_aborts_without_committing() {
    // Arrange — guarded/ is NOT covered by the global vault .gitignore; only
    // per-file entries protect it. A stale entry must not leak plaintext.
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    std::fs::create_dir_all(fx.vault_path.join("guarded")).unwrap();
    std::fs::write(fx.vault_path.join("guarded/id_rsa"), "PRIVATE KEY\n").unwrap();
    let before = commit_count(&fx.vault_base);

    // Act
    let err = fx.service.commit(&fx.project_dir, &auto()).unwrap_err();

    // Assert
    assert!(
        err.to_string().contains("id_rsa"),
        "error must name the offending path: {}",
        err
    );
    assert_eq!(
        commit_count(&fx.vault_base),
        before,
        "aborted run must not commit"
    );
}

#[test]
fn given_encrypted_file_in_guarded_when_commit_then_it_is_allowed() {
    // Arrange — .enc files are the whole point of the vault being committable.
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    std::fs::create_dir_all(fx.vault_path.join("guarded")).unwrap();
    std::fs::write(
        fx.vault_path.join("guarded/id_rsa.a1b2c3d4.enc"),
        "cipher\n",
    )
    .unwrap();

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    assert!(outcome.commit.is_some());
    let files = git(
        &fx.vault_base,
        &["show", "--name-only", "--format=", "HEAD"],
    );
    assert!(files.contains("id_rsa.a1b2c3d4.enc"), "missing:\n{}", files);
}

// ============================================================
// Failure modes
// ============================================================

#[test]
fn given_vault_base_not_a_git_repo_when_commit_then_git_failed_error() {
    // Arrange
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    std::fs::remove_dir_all(fx.vault_base.join(".git")).unwrap();

    // Act
    let err = fx.service.commit(&fx.project_dir, &auto()).unwrap_err();

    // Assert
    assert!(
        matches!(err, ApplicationError::GitFailed { .. }),
        "expected GitFailed, got: {:?}",
        err
    );
}

#[test]
fn given_uninitialized_project_when_commit_then_vault_not_initialized_error() {
    // Arrange
    let temp = TempDir::new().unwrap();
    let project_dir = temp.path().join("bare");
    std::fs::create_dir_all(&project_dir).unwrap();
    let settings = Arc::new(test_settings(temp.path().join("rsenv")));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let swap = Arc::new(SwapService::new(fs, vault_service.clone(), settings));
    let service = VaultCommitService::new(swap, vault_service, Arc::new(RealCommandRunner));

    // Act
    let err = service.commit(&project_dir, &auto()).unwrap_err();

    // Assert
    assert!(
        matches!(err, ApplicationError::VaultNotInitialized(_)),
        "expected VaultNotInitialized, got: {:?}",
        err
    );
}

// ============================================================
// --push
// ============================================================

#[test]
fn given_push_flag_when_commit_then_remote_receives_the_commit() {
    // Arrange
    let fx = setup(true);
    let remote = fx.vault_base.parent().unwrap().join("remote.git");
    let out = Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(&remote)
        .output()
        .unwrap();
    assert!(out.status.success());
    git(
        &fx.vault_base,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&fx.vault_base, &["push", "-u", "origin", "main"]);

    swap_in_thoughts_with_edit(&fx);

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: true,
                push: true,
            },
        )
        .unwrap();

    // Assert
    assert!(outcome.pushed);
    let local = git(&fx.vault_base, &["rev-parse", "HEAD"]);
    let pushed = git(&remote, &["rev-parse", "main"]);
    assert_eq!(local, pushed, "remote did not receive the commit");
}

// ============================================================
// Interactive (non-auto) path
// ============================================================

#[test]
fn given_interactive_mode_when_editor_accepts_then_prefilled_message_is_committed() {
    // Arrange — `true` accepts the prefilled message unchanged.
    let fx = setup(true);
    let project_head = git(&fx.project_dir, &["rev-parse", "HEAD"]);
    swap_in_thoughts_with_edit(&fx);
    std::env::set_var("GIT_EDITOR", "true");

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: false,
                push: false,
            },
        )
        .unwrap();

    // Assert
    std::env::remove_var("GIT_EDITOR");
    assert!(
        outcome.commit.is_some(),
        "editor accepted but nothing committed"
    );
    assert!(
        head_message(&fx.vault_base).contains(&project_head),
        "prefill lost the project hash"
    );
}

#[test]
fn given_interactive_mode_when_editor_aborts_then_nothing_is_committed() {
    // Arrange — `false` exits non-zero, which git treats as an aborted commit.
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    let before = commit_count(&fx.vault_base);
    std::env::set_var("GIT_EDITOR", "false");

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: false,
                push: false,
            },
        )
        .unwrap();

    // Assert
    std::env::remove_var("GIT_EDITOR");
    assert!(
        outcome.commit.is_none(),
        "aborted editor still produced a commit"
    );
    assert_eq!(commit_count(&fx.vault_base), before);
}
