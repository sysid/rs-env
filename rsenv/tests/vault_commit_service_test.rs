//! Tests for VaultCommitService (`rsenv vault commit`)
//!
//! The command does NOT swap: it requires the project to be swapped out already and
//! refuses otherwise. It then makes ONE commit scoped to that project's vault directory,
//! whose message carries the project's HEAD commit hash as the join key back to the
//! project.
//!
//! These tests drive REAL git in tempdirs: there is no MockCommandRunner in this
//! repo (see TODO.md), and real git is the authority on git behaviour anyway.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::{
    CommitOptions, SopsService, SwapService, VaultCommitService, VaultService,
};
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

fn test_settings(base_dir: PathBuf, encrypt_on_commit: bool) -> Settings {
    Settings {
        base_dir,
        editor: "vim".to_string(),
        sops: rsenv::config::SopsConfig {
            encrypt_on_commit,
            ..Default::default()
        },
    }
}

/// Build a project + vault, both under git, with the vault base repo holding
/// one empty initial commit so HEAD exists.
fn setup(project_is_git_repo: bool) -> Fixture {
    // These tests are about git/commit semantics; encryption has its own tests below
    // and would need a real SOPS key here.
    setup_with(project_is_git_repo, false)
}

fn setup_with(project_is_git_repo: bool, encrypt_on_commit: bool) -> Fixture {
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

    let settings = Arc::new(test_settings(vault_base.clone(), encrypt_on_commit));
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
    let cmd = Arc::new(RealCommandRunner);
    let sops = Arc::new(SopsService::new(
        Arc::new(RealFileSystem),
        cmd.clone(),
        settings.clone(),
    ));
    let service = VaultCommitService::new(swap.clone(), vault_service, sops, cmd, settings);

    Fixture {
        _temp: temp,
        vault_base,
        project_dir,
        vault_path: vault.path,
        service,
        swap,
    }
}

/// Put `thoughts/` under swap management, swap it in, edit it in the project, swap out.
///
/// This leaves the state `vault commit` requires: the live edit has landed in the vault
/// and the project is swapped out. Swapping out is the USER's job now, so the tests do it
/// explicitly rather than relying on the command to do it for them.
fn swap_in_edit_then_out(fx: &Fixture) {
    swap_in_thoughts_with_edit(fx);

    fx.swap
        .swap_out_vault(&fx.project_dir)
        .expect("swap out before committing");
}

/// Swap `thoughts/` in and edit it, leaving the project SWAPPED IN.
///
/// Only the refusal test stops here; everything else continues with
/// `swap_in_edit_then_out`.
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
        no_encrypt: false,
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
    swap_in_edit_then_out(&fx);
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
    swap_in_edit_then_out(&fx);

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
    swap_in_edit_then_out(&fx);

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
    swap_in_edit_then_out(&fx);
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
    swap_in_edit_then_out(&fx);

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
    swap_in_edit_then_out(&fx);

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
    swap_in_edit_then_out(&fx);
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
    swap_in_edit_then_out(&fx);
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
    swap_in_edit_then_out(&fx);
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
    let settings = Arc::new(test_settings(temp.path().join("rsenv"), false));
    let fs = Arc::new(RealFileSystem);
    let vault_service = Arc::new(VaultService::new(fs.clone(), settings.clone()));
    let swap = Arc::new(SwapService::new(
        fs.clone(),
        vault_service.clone(),
        settings.clone(),
    ));
    let cmd = Arc::new(RealCommandRunner);
    let sops = Arc::new(SopsService::new(fs, cmd.clone(), settings.clone()));
    let service = VaultCommitService::new(swap, vault_service, sops, cmd, settings);

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

    swap_in_edit_then_out(&fx);

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: true,
                push: true,
                no_encrypt: false,
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
    swap_in_edit_then_out(&fx);
    std::env::set_var("GIT_EDITOR", "true");

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: false,
                push: false,
                no_encrypt: false,
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
    swap_in_edit_then_out(&fx);
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
                no_encrypt: false,
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

// ============================================================
// sops.encrypt_on_commit
//
// Without this, `vault commit` stages whatever `.enc` happens to exist, so a plaintext
// edit made while swapped in is committed as stale ciphertext.
// ============================================================

#[test]
fn given_encrypt_on_commit_enabled_and_no_key_when_commit_then_fails_without_committing() {
    // Arrange
    let fx = setup_with(true, true);
    swap_in_edit_then_out(&fx);
    let before = commit_count(&fx.vault_base);

    // Act - the vault holds a plaintext dot.envrc, so encryption is attempted
    let err = fx
        .service
        .commit(&fx.project_dir, &auto())
        .expect_err("commit must fail when it cannot encrypt");

    // Assert - loud failure, not a commit of stale ciphertext
    assert!(
        matches!(err, ApplicationError::Config { .. }),
        "expected a config error about the missing key, got: {:?}",
        err
    );
    assert_eq!(
        commit_count(&fx.vault_base),
        before,
        "nothing may be committed when encryption fails"
    );
}

#[test]
fn given_encrypt_on_commit_disabled_when_commit_then_commits_without_encrypting() {
    // Arrange
    let fx = setup_with(true, false);
    swap_in_edit_then_out(&fx);
    let before = commit_count(&fx.vault_base);

    // Act
    let outcome = fx.service.commit(&fx.project_dir, &auto()).unwrap();

    // Assert
    assert!(
        outcome.commit.is_some(),
        "disabling encrypt_on_commit must leave committing untouched"
    );
    assert_eq!(commit_count(&fx.vault_base), before + 1);
}

#[test]
fn given_no_encrypt_flag_when_commit_then_skips_encryption_despite_config() {
    // Arrange - config says encrypt, but there is no key to encrypt with
    let fx = setup_with(true, true);
    swap_in_edit_then_out(&fx);
    let before = commit_count(&fx.vault_base);

    // Act
    let outcome = fx
        .service
        .commit(
            &fx.project_dir,
            &CommitOptions {
                auto: true,
                push: false,
                no_encrypt: true,
            },
        )
        .expect("--no-encrypt must bypass encryption entirely");

    // Assert - the same setup fails without the flag, so this proves the override
    assert!(outcome.encrypted.is_empty(), "nothing may be encrypted");
    assert!(outcome.commit.is_some(), "the commit must still happen");
    assert_eq!(commit_count(&fx.vault_base), before + 1);
}

// ============================================================
// Precondition: the project must already be swapped out
//
// While anything is swapped in, the live bytes are in the project and the vault holds
// only a frozen sentinel — so committing would record stale content. Swapping out is
// the user's job; this command only verifies it happened.
// ============================================================

#[test]
fn given_project_still_swapped_in_when_commit_then_refuses_and_commits_nothing() {
    // Arrange - swapped IN, deliberately not swapped back out
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);
    let before = commit_count(&fx.vault_base);

    // Act
    let err = fx
        .service
        .commit(&fx.project_dir, &auto())
        .expect_err("commit must refuse while the project is swapped in");

    // Assert
    match &err {
        ApplicationError::ProjectSwappedIn { paths } => {
            assert!(
                paths.iter().any(|p| p.ends_with("thoughts")),
                "the error must name the swapped-in path, got: {:?}",
                paths
            );
        }
        other => panic!("expected ProjectSwappedIn, got: {:?}", other),
    }
    assert_eq!(
        commit_count(&fx.vault_base),
        before,
        "nothing may be committed when the precondition fails"
    );
}

#[test]
fn given_project_still_swapped_in_when_commit_then_message_names_the_remedy() {
    // Arrange
    let fx = setup(true);
    swap_in_thoughts_with_edit(&fx);

    // Act
    let err = fx.service.commit(&fx.project_dir, &auto()).unwrap_err();

    // Assert - the user must be told what to do, not just what went wrong
    let message = err.to_string();
    assert!(
        message.contains("rsenv swap out"),
        "the error must tell the user to swap out, got: {}",
        message
    );
}

#[test]
fn given_entry_swapped_in_by_another_host_when_commit_then_it_proceeds() {
    // Arrange - a sentinel naming a DIFFERENT machine, as arrives via the vault repo.
    // `swap out` refuses to touch those, so blocking on them would leave no way forward;
    // `swap diff` already skips them for the same reason.
    let fx = setup(true);
    swap_in_edit_then_out(&fx);

    let foreign = fx
        .vault_path
        .join("swap")
        .join("thoughts@@otherhost@@rsenv_active");
    std::fs::create_dir_all(&foreign).unwrap();
    let before = commit_count(&fx.vault_base);

    // Act
    let outcome = fx
        .service
        .commit(&fx.project_dir, &auto())
        .expect("a foreign-host sentinel must not block this host's commit");

    // Assert
    assert!(outcome.commit.is_some(), "expected a commit sha");
    assert_eq!(commit_count(&fx.vault_base), before + 1);
}
